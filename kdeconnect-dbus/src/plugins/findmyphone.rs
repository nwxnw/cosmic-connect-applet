//! D-Bus proxy for the findmyphone plugin.
//!
//! Allows triggering the phone to ring so the user can locate it.

use zbus::proxy;

/// Proxy for the findmyphone plugin D-Bus interface.
#[proxy(
    interface = "org.kde.kdeconnect.device.findmyphone",
    default_service = "org.kde.kdeconnect.daemon"
)]
pub trait FindMyPhone {
    /// Trigger the phone to ring.
    #[zbus(name = "ring")]
    fn ring(&self) -> zbus::Result<()>;
}
