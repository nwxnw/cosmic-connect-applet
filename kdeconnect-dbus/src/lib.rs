//! D-Bus client library for KDE Connect daemon.
//!
//! This crate provides Rust bindings to interact with the KDE Connect daemon
//! (`kdeconnectd`) via D-Bus. It abstracts the D-Bus interface into idiomatic
//! Rust types and async methods.
//!
//! # Example
//!
//! ```no_run
//! use kdeconnect_dbus::DaemonProxy;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let connection = zbus::Connection::session().await?;
//!     let daemon = DaemonProxy::new(&connection).await?;
//!
//!     let devices = daemon.devices().await?;
//!     println!("Connected devices: {:?}", devices);
//!
//!     Ok(())
//! }
//! ```

pub mod contacts;
pub mod daemon;
pub mod device;
pub mod plugins;

pub use contacts::{normalize_phone_number, phone_suffix, Contact, ContactLookup};
pub use daemon::DaemonProxy;
pub use device::DeviceProxy;

/// KDE Connect D-Bus service name. 'org.kde.kdeconnect' is the activatable name
/// while 'org.kde.kdeconnect.daemon' is the interface.
pub const SERVICE_NAME: &str = "org.kde.kdeconnect";

/// Base path for KDE Connect D-Bus objects
pub const BASE_PATH: &str = "/modules/kdeconnect";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{
        BatteryProxy, ClipboardProxy, ConversationsProxy, FindMyPhoneProxy, MprisRemoteProxy,
        NotificationProxy, NotificationsProxy, PingProxy, ShareProxy, SmsProxy, TelephonyProxy,
    };
    use zbus::proxy::Defaults;

    struct ProxyDefaults {
        label: &'static str,
        destination: Option<&'static str>,
        interface: Option<&'static str>,
    }

    /// Every `#[proxy]` in this crate. Add a row when you add a proxy -
    /// nothing else enumerates them.
    fn all_proxies() -> [ProxyDefaults; 13] {
        fn row<P: Defaults>(label: &'static str) -> ProxyDefaults {
            ProxyDefaults {
                label,
                destination: P::DESTINATION.as_ref().map(|n| n.as_str()),
                interface: P::INTERFACE.as_ref().map(|n| n.as_str()),
            }
        }

        [
            row::<DaemonProxy<'static>>("Daemon"),
            row::<DeviceProxy<'static>>("Device"),
            row::<BatteryProxy<'static>>("Battery"),
            row::<ClipboardProxy<'static>>("Clipboard"),
            row::<FindMyPhoneProxy<'static>>("FindMyPhone"),
            row::<MprisRemoteProxy<'static>>("MprisRemote"),
            row::<NotificationsProxy<'static>>("Notifications"),
            row::<NotificationProxy<'static>>("Notification"),
            row::<PingProxy<'static>>("Ping"),
            row::<ShareProxy<'static>>("Share"),
            row::<SmsProxy<'static>>("Sms"),
            row::<ConversationsProxy<'static>>("Conversations"),
            row::<TelephonyProxy<'static>>("Telephony"),
        ]
    }

    /// D.42 - every proxy must address the *activatable* name. Each attribute
    /// hardcodes it because `zbus_macros` parses `default_service` as a string
    /// literal and will not take `SERVICE_NAME`, so this is the only thing
    /// binding the 13 to the const.
    #[test]
    fn every_proxy_targets_the_activatable_name() {
        for p in all_proxies() {
            assert_eq!(
                p.destination,
                Some(SERVICE_NAME),
                "{}Proxy has the wrong destination",
                p.label
            );
        }
    }

    /// D.42 - `interface` and `default_service` sit on adjacent lines in every
    /// proxy attribute, and were the identical string in `daemon.rs` before the
    /// fix. An edit that lands on the wrong line is silent until a call fails.
    #[test]
    fn no_interface_was_overwritten_by_the_destination() {
        for p in all_proxies() {
            let interface = p
                .interface
                .unwrap_or_else(|| panic!("{}Proxy declares no interface", p.label));
            assert!(
                interface.starts_with("org.kde.kdeconnect."),
                "{}Proxy interface looks wrong: {interface}",
                p.label
            );
        }
    }
}
