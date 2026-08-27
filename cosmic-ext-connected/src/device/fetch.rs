//! Device fetching and information retrieval.

use crate::app::{DaemonUnavailable, DeviceInfo, Message};
use crate::constants::dbus::ACTIVATION_RETRY_DELAYS_MS;
use kdeconnect_dbus::{
    plugins::{BatteryProxy, NotificationInfo, NotificationProxy, NotificationsProxy},
    DaemonProxy, DeviceProxy,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::Connection;

/// Classify a D-Bus failure by its error *name*.
///
/// A remote error arrives as `zbus::Error::MethodError(OwnedErrorName, _, _)`
/// (`zbus/src/error.rs:40`). `zbus::fdo::Error` is the *server*-side type and
/// never appears here.
fn classify(e: &zbus::Error) -> DaemonUnavailable {
    let zbus::Error::MethodError(name, _, _) = e else {
        return DaemonUnavailable::Other(e.to_string());
    };
    match name.as_str() {
        "org.freedesktop.DBus.Error.UnknownObject"
        | "org.freedesktop.DBus.Error.UnknownInterface" => DaemonUnavailable::Starting,
        "org.freedesktop.DBus.Error.ServiceUnknown" => DaemonUnavailable::NotInstalled,
        "org.freedesktop.DBus.Error.NoReply"
        | "org.freedesktop.DBus.Error.Timeout"
        | "org.freedesktop.DBus.Error.TimedOut" => DaemonUnavailable::NotResponding,
        // Spawn.ExecFailed, Spawn.ChildExited, Spawn.FileInvalid, ...
        n if n.starts_with("org.freedesktop.DBus.Error.Spawn.") => DaemonUnavailable::FailedToStart,
        _ => DaemonUnavailable::Other(e.to_string()),
    }
}

/// One attempt at the daemon's device list.
///
/// The connection mutex is acquired *inside* this fn so the guard is dropped
/// before the caller sleeps. Nineteen call sites share this
/// `Arc<Mutex<Connection>>` - SMS send/fetch, media, every device action - and
/// holding it across a backoff would stall all of them during a cold open,
/// which is exactly when the user is most likely to click something.
///
/// `DaemonProxy::new` does no round trip: zbus builds proxies with
/// `CacheProperties::Lazily` and `Daemon` declares no properties, so
/// `devices()` is the only call here that can fail against the bus.
async fn daemon_devices(conn: &Arc<Mutex<Connection>>) -> zbus::Result<Vec<String>> {
    let guard = conn.lock().await;
    DaemonProxy::new(&guard).await?.devices().await
}

/// Retry the daemon's device list across the activation window.
///
/// Returns `Err` on the first non-`Starting` classification. Those are permanent
/// until something changes outside the applet, and retrying only delays the
/// message the user is waiting for - `NoReply` in particular has already cost
/// zbus's call timeout, so a second attempt doubles it.
async fn fetch_device_ids(conn: &Arc<Mutex<Connection>>) -> Result<Vec<String>, DaemonUnavailable> {
    for (attempt, &delay_ms) in ACTIVATION_RETRY_DELAYS_MS.iter().enumerate() {
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        match daemon_devices(conn).await {
            Ok(ids) => return Ok(ids),
            Err(e) => match classify(&e) {
                DaemonUnavailable::Starting => tracing::debug!(
                    "Daemon still activating (attempt {}/{}): {}",
                    attempt + 1,
                    ACTIVATION_RETRY_DELAYS_MS.len(),
                    e
                ),
                other => {
                    tracing::warn!("Device fetch failed ({:?}), not retrying: {}", other, e);
                    return Err(other);
                }
            },
        }
    }
    // Every attempt saw `Starting`: the bus launched the daemon and it never
    // registered `/modules/kdeconnect` inside the budget.
    tracing::warn!("Daemon did not finish starting within the activation budget");
    Err(DaemonUnavailable::Starting)
}

/// Fetch all devices from the KDE Connect daemon via D-Bus.
pub async fn fetch_devices_async(conn: Arc<Mutex<Connection>>) -> Message {
    let device_ids = match fetch_device_ids(&conn).await {
        Ok(ids) => ids,
        Err(why) => return Message::DeviceFetchFailed(why),
    };

    tracing::debug!("Found {} device(s)", device_ids.len());

    // Re-acquire for the per-device pass; `fetch_device_info` takes the guard.
    let guard = conn.lock().await;
    let mut devices = Vec::new();
    for device_id in device_ids {
        match fetch_device_info(&guard, &device_id).await {
            Ok(info) => devices.push(info),
            Err(e) => {
                tracing::warn!("Failed to get info for device {}: {}", device_id, e);
            }
        }
    }

    Message::DevicesUpdated(devices)
}

/// Prod the daemon to re-broadcast on the network, then re-fetch devices state.
///
/// `forceOnNetworkChange` is fire-and-forget: it re-sends identify broadcasts,
/// but the phone's reconnect (and resulting reachableChanged signal) arrives
/// asynchronously and is picked up by the signal subscription, not this fetch
pub async fn force_network_change_async(conn: Arc<Mutex<Connection>>) -> Message {
    {
        let conn_guard = conn.lock().await;
        match DaemonProxy::new(&conn_guard).await {
            Ok(daemon) => {
                if let Err(e) = daemon.force_on_network_change().await {
                    //Best-effort: a failed prod shouldn't block the re-fetch.
                    tracing::warn!("forceOnNetworkChange failed: {}", e);
                }
            }
            Err(e) => tracing::warn!("Daemon proxy for network refresh failed: {}", e),
        }
    } // <-- guard MUST drop here; fetch_devices_async locks the same Mutex again
    fetch_devices_async(conn).await
}

/// Fetch information for a single device.
pub async fn fetch_device_info(conn: &Connection, device_id: &str) -> Result<DeviceInfo, String> {
    let device = DeviceProxy::for_device(conn, device_id)
        .await
        .map_err(|e| e.to_string())?;

    let id = device_id.to_string();
    let name = device.name().await.map_err(|e| e.to_string())?;
    let device_type = device
        .device_type()
        .await
        .unwrap_or_else(|_| "unknown".to_string());
    let is_reachable = device.is_reachable().await.unwrap_or(false);
    let is_paired = device.is_trusted().await.unwrap_or(false);
    let is_pair_requested = device.is_pair_requested().await.unwrap_or(false);
    let is_pair_requested_by_peer = device.is_pair_requested_by_peer().await.unwrap_or(false);

    // Try to get battery info if available
    let (battery_level, battery_charging) = if is_reachable && is_paired {
        fetch_battery_info(conn, device_id).await
    } else {
        (None, None)
    };

    // Fetch notifications if device is connected and paired
    let notifications = if is_reachable && is_paired {
        fetch_notifications(conn, device_id).await
    } else {
        Vec::new()
    };

    Ok(DeviceInfo {
        id,
        name,
        device_type,
        is_reachable,
        is_paired,
        is_pair_requested,
        is_pair_requested_by_peer,
        battery_level,
        battery_charging,
        notifications,
    })
}

/// Fetch battery information for a device.
pub async fn fetch_battery_info(conn: &Connection, device_id: &str) -> (Option<i32>, Option<bool>) {
    let path = format!(
        "{}/devices/{}/battery",
        kdeconnect_dbus::BASE_PATH,
        device_id
    );

    tracing::debug!("Fetching battery info from path: {}", path);

    let builder = match BatteryProxy::builder(conn).path(path.as_str()) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Failed to create battery proxy builder: {}", e);
            return (None, None);
        }
    };

    let battery = match builder.build().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Failed to build battery proxy: {}", e);
            return (None, None);
        }
    };

    let charge = match battery.charge().await {
        Ok(c) => {
            tracing::debug!("Battery charge: {}", c);
            Some(c)
        }
        Err(e) => {
            tracing::warn!("Failed to get battery charge: {}", e);
            None
        }
    };

    let is_charging = match battery.is_charging().await {
        Ok(c) => {
            tracing::debug!("Battery is_charging: {}", c);
            Some(c)
        }
        Err(e) => {
            tracing::warn!("Failed to get is_charging: {}", e);
            None
        }
    };

    (charge, is_charging)
}

/// Fetch notifications for a device.
pub async fn fetch_notifications(conn: &Connection, device_id: &str) -> Vec<NotificationInfo> {
    let notifications_path = format!(
        "{}/devices/{}/notifications",
        kdeconnect_dbus::BASE_PATH,
        device_id
    );

    // Get the notifications proxy
    let notifications_proxy = match NotificationsProxy::builder(conn)
        .path(notifications_path.as_str())
        .ok()
        .map(|b| b.build())
    {
        Some(fut) => match fut.await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Failed to create notifications proxy: {}", e);
                return Vec::new();
            }
        },
        None => {
            tracing::warn!("Failed to build notifications proxy path");
            return Vec::new();
        }
    };

    // Get list of active notification IDs
    let notification_ids = match notifications_proxy.active_notifications().await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!("Failed to get active notifications: {}", e);
            return Vec::new();
        }
    };

    tracing::debug!(
        "Found {} notifications for device {}",
        notification_ids.len(),
        device_id
    );

    // Fetch info for each notification
    let mut notifications = Vec::new();
    for notif_id in notification_ids {
        let notif_path = format!(
            "{}/devices/{}/notifications/{}",
            kdeconnect_dbus::BASE_PATH,
            device_id,
            notif_id
        );

        let notif_proxy = match NotificationProxy::builder(conn)
            .path(notif_path.as_str())
            .ok()
            .map(|b| b.build())
        {
            Some(fut) => match fut.await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "Failed to create notification proxy for {}: {}",
                        notif_id,
                        e
                    );
                    continue;
                }
            },
            None => continue,
        };

        let app_name = notif_proxy.app_name().await.unwrap_or_default();
        let title = notif_proxy.title().await.unwrap_or_default();
        let text = notif_proxy.text().await.unwrap_or_default();
        let dismissable = notif_proxy.dismissable().await.unwrap_or(false);
        let reply_id = notif_proxy.reply_id().await.unwrap_or_default();

        notifications.push(NotificationInfo {
            id: notif_id,
            app_name,
            title,
            text,
            dismissable,
            repliable: !reply_id.is_empty(),
        });
    }

    notifications
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method_error(name: &str) -> zbus::Error {
        let reply_to = zbus::Message::method_call("/modules/kdeconnect", "devices")
            .unwrap()
            .build(&())
            .unwrap();
        zbus::Error::MethodError(
            zbus::names::OwnedErrorName::try_from(name).unwrap(),
            None,
            reply_to,
        )
    }

    #[test]
    fn classify_maps_error_names_to_variants() {
        for (name, want) in [
            (
                "org.freedesktop.DBus.Error.UnknownObject",
                DaemonUnavailable::Starting,
            ),
            (
                "org.freedesktop.DBus.Error.UnknownInterface",
                DaemonUnavailable::Starting,
            ),
            (
                "org.freedesktop.DBus.Error.ServiceUnknown",
                DaemonUnavailable::NotInstalled,
            ),
            (
                "org.freedesktop.DBus.Error.Spawn.ExecFailed",
                DaemonUnavailable::FailedToStart,
            ),
            (
                "org.freedesktop.DBus.Error.Spawn.ChildExited",
                DaemonUnavailable::FailedToStart,
            ),
            (
                "org.freedesktop.DBus.Error.NoReply",
                DaemonUnavailable::NotResponding,
            ),
            (
                "org.freedesktop.DBus.Error.Timeout",
                DaemonUnavailable::NotResponding,
            ),
            (
                "org.freedesktop.DBus.Error.TimedOut",
                DaemonUnavailable::NotResponding,
            ),
        ] {
            assert_eq!(classify(&method_error(name)), want, "{name}");
        }
    }

    /// An unrecognised remote error and a non-`MethodError` both fall to `Other`.
    #[test]
    fn classify_falls_back_to_other() {
        assert!(matches!(
            classify(&method_error("org.kde.kdeconnect.Error.Whatever")),
            DaemonUnavailable::Other(_)
        ));
        assert!(matches!(
            classify(&zbus::Error::InvalidReply),
            DaemonUnavailable::Other(_)
        ));
    }
}
