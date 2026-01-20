//! D-Bus proxy for the battery plugin.
//!
//! Provides access to the remote device's battery status.

use zbus::proxy;

/// Proxy for the battery plugin D-Bus interface.
#[proxy(
    interface = "org.kde.kdeconnect.device.battery",
    default_service = "org.kde.kdeconnect.daemon"
)]
pub trait Battery {
    /// Get the current battery charge percentage (0-100).
    #[zbus(property, name = "charge")]
    fn charge(&self) -> zbus::Result<i32>;

    /// Check if the device is currently charging.
    #[zbus(property, name = "isCharging")]
    fn is_charging(&self) -> zbus::Result<bool>;

    // Note: Signals removed due to naming conflict with property change receivers.
    // The `charge` property already generates `receive_charge_changed` for property
    // change notifications, which conflicts with a `charge_changed` signal.
    // Use property change streams instead for real-time updates.
}

/// Battery status information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryStatus {
    /// Charge percentage (0-100).
    pub charge: i32,
    /// Whether the device is charging.
    pub is_charging: bool,
}
