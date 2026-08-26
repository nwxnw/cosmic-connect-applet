//! D-Bus proxy for the clipboard plugin.
//!
//! Provides clipboard synchronization between devices.

use zbus::proxy;

/// Proxy for the clipboard plugin D-Bus interface.
///
/// The clipboard plugin allows sending clipboard content to connected devices.
/// Reception of clipboard content from devices is handled automatically by
/// the KDE Connect daemon - it updates the local clipboard when content arrives.
#[proxy(
    interface = "org.kde.kdeconnect.device.clipboard",
    default_service = "org.kde.kdeconnect"
)]
pub trait Clipboard {
    /// Send the current local clipboard content to the device.
    ///
    /// The one-argument `sendClipboard(content)` overload is deliberately
    /// unbound: it changed `QString` → `QVariant` in kdeconnect `v25.08.0`, so
    /// no single binding works on both older and newer daemons. Use
    /// `ShareProxy::share_text` to send specific text.
    #[zbus(name = "sendClipboard")]
    fn send_clipboard(&self) -> zbus::Result<()>;

    /// Check if automatic clipboard sharing is disabled.
    /// Returns true if auto-share is off, or if password sharing is disabled.
    #[zbus(property, name = "isAutoShareDisabled")]
    fn is_auto_share_disabled(&self) -> zbus::Result<bool>;

    /// Signal emitted when auto-share disabled state changes.
    #[zbus(signal, name = "autoShareDisabledChanged")]
    fn auto_share_disabled_changed(&self, disabled: bool) -> zbus::Result<()>;
}
