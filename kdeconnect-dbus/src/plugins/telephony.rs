//! D-Bus proxy for the telephony plugin.
//!
//! Provides signals for incoming and missed phone calls.

use zbus::proxy;

/// Proxy for the telephony plugin D-Bus interface.
#[proxy(
    interface = "org.kde.kdeconnect.device.telephony",
    default_service = "org.kde.kdeconnect.daemon"
)]
pub trait Telephony {
    /// Signal emitted when a call is received or missed.
    ///
    /// # Arguments
    /// * `event` - "callReceived" for incoming call, "missedCall" for missed call
    /// * `phone_number` - The caller's phone number
    /// * `contact_name` - The contact name if available, otherwise the phone number
    #[zbus(signal, name = "callReceived")]
    fn call_received(&self, event: String, phone_number: String, contact_name: String);
}
