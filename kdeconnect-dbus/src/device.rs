//! D-Bus proxy for KDE Connect devices.
//!
//! Each paired or discovered device has its own D-Bus object providing
//! access to device information and plugin functionality.

use crate::BASE_PATH;
use zbus::{proxy, Connection};

/// Proxy for a KDE Connect device D-Bus interface.
///
/// Provides access to device-specific operations like pairing, getting
/// device information, and accessing plugins.
#[proxy(
    interface = "org.kde.kdeconnect.device",
    default_service = "org.kde.kdeconnect.daemon"
)]
pub trait Device {
    /// Get the device's human-readable name.
    #[zbus(property, name = "name")]
    fn name(&self) -> zbus::Result<String>;

    /// Get the device type (e.g., "phone", "tablet", "desktop").
    #[zbus(property, name = "type")]
    fn device_type(&self) -> zbus::Result<String>;

    /// Check if the device is currently reachable on the network.
    #[zbus(property, name = "isReachable")]
    fn is_reachable(&self) -> zbus::Result<bool>;

    /// Check if the device is paired.
    #[zbus(property, name = "isPaired")]
    fn is_trusted(&self) -> zbus::Result<bool>;

    /// Check if we have requested pairing with this device.
    #[zbus(property, name = "isPairRequested")]
    fn is_pair_requested(&self) -> zbus::Result<bool>;

    /// Check if this device has requested pairing with us.
    #[zbus(property, name = "isPairRequestedByPeer")]
    fn is_pair_requested_by_peer(&self) -> zbus::Result<bool>;

    /// Get the list of supported plugin IDs.
    #[zbus(property, name = "supportedPlugins")]
    fn supported_plugins(&self) -> zbus::Result<Vec<String>>;

    /// Request pairing with this device.
    #[zbus(name = "requestPairing")]
    fn request_pair(&self) -> zbus::Result<()>;

    /// Unpair from this device.
    #[zbus(name = "unpair")]
    fn unpair(&self) -> zbus::Result<()>;

    /// Accept an incoming pairing request.
    #[zbus(name = "acceptPairing")]
    fn accept_pairing(&self) -> zbus::Result<()>;

    /// Reject an incoming pairing request.
    #[zbus(name = "cancelPairing")]
    fn reject_pairing(&self) -> zbus::Result<()>;

    /// Check if a specific plugin is enabled on this device.
    #[zbus(name = "hasPlugin")]
    fn has_plugin(&self, plugin: &str) -> zbus::Result<bool>;

    /// Signal emitted when the device's reachability changes.
    #[zbus(signal, name = "reachableChanged")]
    fn reachable_changed(&self, reachable: bool) -> zbus::Result<()>;

    /// Signal emitted when the device's pairing status changes.
    #[zbus(signal, name = "pairStateChanged")]
    fn pair_state_changed(&self, pair_state: i32) -> zbus::Result<()>;
}

impl DeviceProxy<'_> {
    /// Create a new device proxy for the given device ID.
    pub async fn for_device(connection: &Connection, device_id: &str) -> zbus::Result<Self> {
        let path = format!("{}/devices/{}", BASE_PATH, device_id);
        DeviceProxy::builder(connection).path(path)?.build().await
    }
}

/// Represents device type categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Phone,
    Tablet,
    Desktop,
    Laptop,
    Tv,
    Unknown,
}

impl From<&str> for DeviceType {
    fn from(s: &str) -> Self {
        match s {
            "phone" | "smartphone" => DeviceType::Phone,
            "tablet" => DeviceType::Tablet,
            "desktop" => DeviceType::Desktop,
            "laptop" => DeviceType::Laptop,
            "tv" => DeviceType::Tv,
            _ => DeviceType::Unknown,
        }
    }
}
