//! D-Bus proxy for the share plugin.
//!
//! Allows sharing files and text to devices, and receiving share notifications.

use zbus::proxy;

/// Proxy for the share plugin D-Bus interface.
#[proxy(
    interface = "org.kde.kdeconnect.device.share",
    default_service = "org.kde.kdeconnect.daemon"
)]
pub trait Share {
    /// Share a URL (including file:// URLs) to the device.
    #[zbus(name = "shareUrl")]
    fn share_url(&self, url: &str) -> zbus::Result<()>;

    /// Share text to the device's clipboard.
    #[zbus(name = "shareText")]
    fn share_text(&self, text: &str) -> zbus::Result<()>;

    /// Signal emitted when a file, URL, or text is received from a device.
    ///
    /// # Arguments
    /// * `url` - The file:// URL where the received file was saved,
    ///           or the URL/text that was received
    #[zbus(signal, name = "shareReceived")]
    fn share_received(&self, url: String);
}
