//! Configuration management for the Connected applet.

use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// Application ID for configuration storage.
pub const APP_ID: &str = "io.github.nwxnw.cosmic-ext-connected";

/// Applet configuration stored in COSMIC's config system.
#[derive(Debug, Clone, Serialize, Deserialize, CosmicConfigEntry, PartialEq, Eq)]
#[version = 7]
pub struct Config {
    /// Whether the collapsible "offline" device group is expanded
    pub group_offline_expanded: bool,
    /// Enable desktop notifications for incoming SMS messages
    pub sms_notifications: bool,
    /// Show message content in SMS notifications (privacy)
    pub sms_notification_show_content: bool,
    /// Show sender name in SMS notifications (privacy)
    pub sms_notification_show_sender: bool,
    /// Merge iOS reaction-bucket sibling threads into one logical conversation.
    /// When off, each underlying thread renders separately and replies are sent
    /// against the user-displayed thread (which can produce duplicate delivery
    /// on the recipient side for symmetric merges — see v0.5.0 Topic 2).
    pub merge_reaction_threads: bool,
    /// Enable desktop notifications for incoming/missed calls
    pub call_notifications: bool,
    /// Show phone number in call notifications (privacy)
    pub call_notification_show_number: bool,
    /// Show contact name in call notifications (privacy)
    pub call_notification_show_name: bool,
    /// Enable desktop notifications for received files
    pub file_notifications: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            group_offline_expanded: false,
            sms_notifications: true,
            sms_notification_show_content: true,
            sms_notification_show_sender: true,
            merge_reaction_threads: true,
            call_notifications: true,
            call_notification_show_number: true,
            call_notification_show_name: true,
            file_notifications: true,
        }
    }
}

impl Config {
    /// Load configuration from disk, falling back to defaults if not found.
    pub fn load() -> Self {
        match cosmic_config::Config::new(APP_ID, Self::VERSION) {
            Ok(config_handler) => {
                let config = Self::get_entry(&config_handler).unwrap_or_else(|err| {
                    tracing::error!(?err, "Failed to load config, using defaults");
                    Self::default()
                });
                tracing::debug!("Loaded config: {:?}", config);
                config
            }
            Err(err) => {
                tracing::error!(?err, "Failed to create config handler, using defaults");
                Self::default()
            }
        }
    }

    /// Save configuration to disk.
    pub fn save(&self) -> Result<(), cosmic_config::Error> {
        let config_handler = cosmic_config::Config::new(APP_ID, Self::VERSION)?;
        self.write_entry(&config_handler)?;
        tracing::debug!("Saved config: {:?}", self);
        Ok(())
    }
}
