//! D-Bus proxy for the ping plugin.
//!
//! Allows sending ping messages to devices for testing connectivity.

use zbus::proxy;

/// Proxy for the ping plugin D-Bus interface.
#[proxy(
    interface = "org.kde.kdeconnect.device.ping",
    default_service = "org.kde.kdeconnect.daemon"
)]
pub trait Ping {
    /// Send a ping to the device.
    #[zbus(name = "sendPing")]
    fn send_ping(&self) -> zbus::Result<()>;

    /// Send a ping with a custom message.
    #[zbus(name = "sendPing")]
    fn send_ping_with_message(&self, custom_message: &str) -> zbus::Result<()>;
}
