//! Settings view components.

use crate::app::{Message, SettingKey};
use crate::config::Config;
use crate::fl;
use cosmic::iced::widget::{column, row, text};
use cosmic::iced::{Alignment, Length};
use cosmic::widget;
use cosmic::Element;

/// Render the settings view.
pub fn view_settings(config: &Config) -> Element<'_, Message> {
    let back_btn = widget::button::text(fl!("back"))
        .leading_icon(widget::icon::from_name("go-previous-symbolic").size(16))
        .on_press(Message::ToggleSettings);

    let mut settings_col = column![
        back_btn,
        widget::divider::horizontal::default(),
        text(fl!("settings")).size(16),
        view_setting_toggle(
            fl!("settings-battery"),
            fl!("settings-battery-desc"),
            config.show_battery_percentage,
            SettingKey::ShowBatteryPercentage,
        ),
        view_setting_toggle(
            fl!("settings-offline"),
            fl!("settings-offline-desc"),
            config.show_offline_devices,
            SettingKey::ShowOfflineDevices,
        ),
        view_setting_toggle(
            fl!("settings-notifications"),
            fl!("settings-notifications-desc"),
            config.forward_notifications,
            SettingKey::ForwardNotifications,
        ),
        widget::divider::horizontal::default(),
        view_setting_toggle(
            fl!("settings-sms-notifications"),
            fl!("settings-sms-notifications-desc"),
            config.sms_notifications,
            SettingKey::SmsNotifications,
        ),
    ]
    .spacing(8)
    .padding(16);

    // Show sub-settings only when SMS notifications are enabled
    if config.sms_notifications {
        settings_col = settings_col
            .push(view_setting_toggle(
                fl!("settings-sms-show-sender"),
                fl!("settings-sms-show-sender-desc"),
                config.sms_notification_show_sender,
                SettingKey::SmsShowSender,
            ))
            .push(view_setting_toggle(
                fl!("settings-sms-show-content"),
                fl!("settings-sms-show-content-desc"),
                config.sms_notification_show_content,
                SettingKey::SmsShowContent,
            ));
    }

    // Call notifications section
    settings_col = settings_col
        .push(widget::divider::horizontal::default())
        .push(view_setting_toggle(
            fl!("settings-call-notifications"),
            fl!("settings-call-notifications-desc"),
            config.call_notifications,
            SettingKey::CallNotifications,
        ));

    // Show sub-settings only when call notifications are enabled
    if config.call_notifications {
        settings_col = settings_col
            .push(view_setting_toggle(
                fl!("settings-call-show-name"),
                fl!("settings-call-show-name-desc"),
                config.call_notification_show_name,
                SettingKey::CallShowName,
            ))
            .push(view_setting_toggle(
                fl!("settings-call-show-number"),
                fl!("settings-call-show-number-desc"),
                config.call_notification_show_number,
                SettingKey::CallShowNumber,
            ));
    }

    // File notifications section
    settings_col = settings_col
        .push(widget::divider::horizontal::default())
        .push(view_setting_toggle(
            fl!("settings-file-notifications"),
            fl!("settings-file-notifications-desc"),
            config.file_notifications,
            SettingKey::FileNotifications,
        ));

    widget::container(settings_col).width(Length::Fill).into()
}

/// Render a single setting toggle row.
pub fn view_setting_toggle(
    title: String,
    description: String,
    enabled: bool,
    key: SettingKey,
) -> Element<'static, Message> {
    let toggle = widget::toggler(enabled).on_toggle(move |_| Message::ToggleSetting(key.clone()));

    // Use width constraint on text column to ensure toggle alignment
    let text_col = column![
        text(title).size(14),
        text(description).size(11).wrapping(text::Wrapping::Word),
    ]
    .spacing(2)
    .width(Length::Fill);

    let setting_row = row![text_col, toggle,]
        .spacing(12)
        .align_y(Alignment::Center);

    widget::container(setting_row)
        .padding(12)
        .width(Length::Fill)
        .into()
}
