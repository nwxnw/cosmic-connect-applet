//! D-Bus proxy for the share plugin.
//!
//! Allows sharing files and text to devices.

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
}
