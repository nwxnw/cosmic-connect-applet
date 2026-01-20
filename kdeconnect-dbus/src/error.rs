//! Error types for the kdeconnect-dbus crate.

use thiserror::Error;

/// Result type alias using our Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur when communicating with KDE Connect daemon.
#[derive(Debug, Error)]
pub enum Error {
    /// D-Bus communication error
    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    /// The KDE Connect daemon is not running
    #[error("KDE Connect daemon is not running")]
    DaemonNotRunning,

    /// Device not found
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Plugin not available on device
    #[error("Plugin '{plugin}' not available on device '{device}'")]
    PluginNotAvailable { device: String, plugin: String },

    /// Device is not reachable
    #[error("Device '{0}' is not reachable")]
    DeviceNotReachable(String),

    /// Device is not paired
    #[error("Device '{0}' is not paired")]
    DeviceNotPaired(String),
}
