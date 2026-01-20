//! D-Bus client library for KDE Connect daemon.
//!
//! This crate provides Rust bindings to interact with the KDE Connect daemon
//! (`kdeconnectd`) via D-Bus. It abstracts the D-Bus interface into idiomatic
//! Rust types and async methods.
//!
//! # Example
//!
//! ```no_run
//! use kdeconnect_dbus::DaemonProxy;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let connection = zbus::Connection::session().await?;
//!     let daemon = DaemonProxy::new(&connection).await?;
//!
//!     let devices = daemon.devices().await?;
//!     println!("Connected devices: {:?}", devices);
//!
//!     Ok(())
//! }
//! ```

pub mod contacts;
pub mod daemon;
pub mod device;
pub mod plugins;

mod error;

pub use contacts::{Contact, ContactLookup};
pub use daemon::DaemonProxy;
pub use device::DeviceProxy;
pub use error::{Error, Result};

/// KDE Connect D-Bus service name
pub const SERVICE_NAME: &str = "org.kde.kdeconnect.daemon";

/// Base path for KDE Connect D-Bus objects
pub const BASE_PATH: &str = "/modules/kdeconnect";
