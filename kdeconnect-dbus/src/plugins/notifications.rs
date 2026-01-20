//! D-Bus proxy for the notifications plugin.
//!
//! Provides access to notifications from the remote device.

use zbus::proxy;

/// Proxy for the notifications plugin D-Bus interface.
///
/// This interface manages the list of notifications from the device.
#[proxy(
    interface = "org.kde.kdeconnect.device.notifications",
    default_service = "org.kde.kdeconnect.daemon"
)]
pub trait Notifications {
    /// Get list of active notification IDs.
    #[zbus(name = "activeNotifications")]
    fn active_notifications(&self) -> zbus::Result<Vec<String>>;

    /// Send a reply to a notification.
    #[zbus(name = "sendReply")]
    fn send_reply(&self, reply_id: &str, message: &str) -> zbus::Result<()>;

    /// Signal emitted when a notification is posted.
    #[zbus(signal, name = "notificationPosted")]
    fn notification_posted(&self, public_id: String) -> zbus::Result<()>;

    /// Signal emitted when a notification is removed.
    #[zbus(signal, name = "notificationRemoved")]
    fn notification_removed(&self, public_id: String) -> zbus::Result<()>;

    /// Signal emitted when a notification is updated.
    #[zbus(signal, name = "notificationUpdated")]
    fn notification_updated(&self, public_id: String) -> zbus::Result<()>;

    /// Signal emitted when all notifications are removed.
    #[zbus(signal, name = "allNotificationsRemoved")]
    fn all_notifications_removed(&self) -> zbus::Result<()>;
}

/// Proxy for an individual notification D-Bus interface.
#[proxy(
    interface = "org.kde.kdeconnect.device.notifications.notification",
    default_service = "org.kde.kdeconnect.daemon"
)]
pub trait Notification {
    /// The internal notification ID.
    #[zbus(property, name = "internalId")]
    fn internal_id(&self) -> zbus::Result<String>;

    /// The application name that generated the notification.
    #[zbus(property, name = "appName")]
    fn app_name(&self) -> zbus::Result<String>;

    /// The notification ticker text.
    #[zbus(property, name = "ticker")]
    fn ticker(&self) -> zbus::Result<String>;

    /// The notification title.
    #[zbus(property, name = "title")]
    fn title(&self) -> zbus::Result<String>;

    /// The notification body text.
    #[zbus(property, name = "text")]
    fn text(&self) -> zbus::Result<String>;

    /// Path to the notification icon.
    #[zbus(property, name = "iconPath")]
    fn icon_path(&self) -> zbus::Result<String>;

    /// Whether the notification can be dismissed.
    #[zbus(property, name = "dismissable")]
    fn dismissable(&self) -> zbus::Result<bool>;

    /// Whether the notification has an icon.
    #[zbus(property, name = "hasIcon")]
    fn has_icon(&self) -> zbus::Result<bool>;

    /// Whether the notification is silent.
    #[zbus(property, name = "silent")]
    fn silent(&self) -> zbus::Result<bool>;

    /// The reply ID for replying to this notification.
    #[zbus(property, name = "replyId")]
    fn reply_id(&self) -> zbus::Result<String>;

    /// Dismiss/close this notification.
    #[zbus(name = "dismiss")]
    fn dismiss(&self) -> zbus::Result<()>;

    /// Send a reply to this notification.
    #[zbus(name = "sendReply")]
    fn send_reply(&self, message: &str) -> zbus::Result<()>;
}

/// Information about a notification.
#[derive(Debug, Clone)]
pub struct NotificationInfo {
    /// The notification ID.
    pub id: String,
    /// The application name.
    pub app_name: String,
    /// The notification title.
    pub title: String,
    /// The notification body text.
    pub text: String,
    /// Whether the notification can be dismissed.
    pub dismissable: bool,
    /// Whether the notification can be replied to.
    pub repliable: bool,
}
