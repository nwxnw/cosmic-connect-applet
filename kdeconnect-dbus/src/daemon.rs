//! D-Bus proxy for the KDE Connect daemon.
//!
//! The daemon is the central service that manages device discovery,
//! pairing, and plugin management.

use zbus::{proxy, Connection};

/// Proxy for the KDE Connect daemon D-Bus interface.
///
/// This provides access to daemon-level operations like discovering devices,
/// announcing the local device, and managing the overall service.
#[proxy(
    interface = "org.kde.kdeconnect.daemon",
    default_service = "org.kde.kdeconnect.daemon",
    default_path = "/modules/kdeconnect"
)]
pub trait Daemon {
    /// Get a list of all known device IDs.
    #[zbus(name = "devices")]
    fn devices(&self) -> zbus::Result<Vec<String>>;

    /// Force a refresh of the device list.
    #[zbus(name = "forceOnNetworkChange")]
    fn force_on_network_change(&self) -> zbus::Result<()>;

    /// Get the local device ID.
    #[zbus(name = "selfId")]
    fn self_id(&self) -> zbus::Result<String>;

    /// Signal emitted when a new device is discovered.
    #[zbus(signal, name = "deviceAdded")]
    fn device_added(&self, id: &str) -> zbus::Result<()>;

    /// Signal emitted when a device is removed.
    #[zbus(signal, name = "deviceRemoved")]
    fn device_removed(&self, id: &str) -> zbus::Result<()>;

    /// Signal emitted when device visibility changes.
    #[zbus(signal, name = "deviceVisibilityChanged")]
    fn device_visibility_changed(&self, id: &str, visible: bool) -> zbus::Result<()>;
}

impl DaemonProxy<'_> {
    /// Check if the daemon is running and accessible.
    pub async fn is_running(connection: &Connection) -> bool {
        match Self::new(connection).await {
            Ok(proxy) => proxy.self_id().await.is_ok(),
            Err(_) => false,
        }
    }
}
