//! Settings view components.

use crate::app::{Message, SettingKey};
use crate::config::Config;
use crate::constants::notifications::{MAX_TIMEOUT_SECS, MIN_TIMEOUT_SECS};
use crate::fl;
use cosmic::applet;
use cosmic::iced::widget::row;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, settings, text};
use cosmic::Element;

/// Render the main settings view (general settings + nav to notification settings).
pub fn view_settings(config: &Config) -> Element<'_, Message> {
    let sp = cosmic::theme::spacing();

    let header = applet::padded_control(
        row![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .on_press(Message::ToggleSettings),
            text::heading(fl!("settings")),
        ]
        .spacing(sp.space_xxs)
        .align_y(Alignment::Center),
    );

    // General section
    let general_section = settings::section()
        .add(
            settings::item::builder(fl!("settings-battery"))
                .toggler(config.show_battery_percentage, move |_| {
                    Message::ToggleSetting(SettingKey::ShowBatteryPercentage)
                }),
        )
        .add(
            settings::item::builder(fl!("settings-offline"))
                .toggler(config.show_offline_devices, move |_| {
                    Message::ToggleSetting(SettingKey::ShowOfflineDevices)
                }),
        )
        .add(
            settings::item::builder(fl!("settings-notifications"))
                .toggler(config.forward_notifications, move |_| {
                    Message::ToggleSetting(SettingKey::ForwardNotifications)
                }),
        );

    // Navigation to notification settings sub-page
    let notif_nav_row = row![
        widget::icon::from_name("preferences-system-notifications-symbolic").size(24),
        text::body(fl!("notification-settings")),
        widget::horizontal_space(),
        widget::icon::from_name("go-next-symbolic").size(16),
    ]
    .spacing(sp.space_xs)
    .align_y(Alignment::Center);

    let notif_nav_btn = applet::menu_button(notif_nav_row)
        .on_press(Message::OpenNotificationSettings);

    let sections = settings::view_column(vec![general_section.into()]);

    widget::container(
        widget::column::with_children(vec![
            header.into(),
            sections.into(),
            notif_nav_btn.into(),
        ])
        .spacing(sp.space_xxs),
    )
    .width(Length::Fill)
    .into()
}

/// Render the notification settings sub-page.
pub fn view_notification_settings(config: &Config) -> Element<'_, Message> {
    let sp = cosmic::theme::spacing();

    let back_btn = widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
        .on_press(Message::BackFromNotificationSettings);

    // SMS notifications section
    let mut sms_section = settings::section()
        .title(fl!("settings-sms-section"))
        .add(
            settings::item::builder(fl!("settings-sms-notifications"))
                .toggler(config.sms_notifications, move |_| {
                    Message::ToggleSetting(SettingKey::SmsNotifications)
                }),
        );

    if config.sms_notifications {
        sms_section = sms_section
            .add(
                settings::item::builder(fl!("settings-sms-show-sender"))
                    .toggler(config.sms_notification_show_sender, move |_| {
                        Message::ToggleSetting(SettingKey::SmsShowSender)
                    }),
            )
            .add(
                settings::item::builder(fl!("settings-sms-show-content"))
                    .toggler(config.sms_notification_show_content, move |_| {
                        Message::ToggleSetting(SettingKey::SmsShowContent)
                    }),
            );
    }

    // Call notifications section
    let mut call_section = settings::section()
        .title(fl!("settings-call-section"))
        .add(
            settings::item::builder(fl!("settings-call-notifications"))
                .toggler(config.call_notifications, move |_| {
                    Message::ToggleSetting(SettingKey::CallNotifications)
                }),
        );

    if config.call_notifications {
        call_section = call_section
            .add(
                settings::item::builder(fl!("settings-call-show-name"))
                    .toggler(config.call_notification_show_name, move |_| {
                        Message::ToggleSetting(SettingKey::CallShowName)
                    }),
            )
            .add(
                settings::item::builder(fl!("settings-call-show-number"))
                    .toggler(config.call_notification_show_number, move |_| {
                        Message::ToggleSetting(SettingKey::CallShowNumber)
                    }),
            );
    }

    // File notifications section
    let file_section = settings::section()
        .title(fl!("settings-file-section"))
        .add(
            settings::item::builder(fl!("settings-file-notifications"))
                .toggler(config.file_notifications, move |_| {
                    Message::ToggleSetting(SettingKey::FileNotifications)
                }),
        );

    // Notification timeout section
    let label = fl!(
        "notification-timeout-seconds",
        seconds = config.notification_timeout_secs.to_string()
    );
    let slider = widget::slider(
        MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS,
        config.notification_timeout_secs,
        Message::SetNotificationTimeout,
    );
    let slider_control = row![
        slider,
        widget::text::caption(label).width(Length::Fixed(36.0)),
    ]
    .spacing(sp.space_xxs)
    .align_y(Alignment::Center)
    .width(Length::Fixed(160.0));

    let timeout_section = settings::section()
        .title(fl!("settings-notification-timeout"))
        .add(settings::item::builder("").control(slider_control));

    let sections = settings::view_column(vec![
        sms_section.into(),
        call_section.into(),
        file_section.into(),
        timeout_section.into(),
    ]);

    let header = applet::padded_control(
        row![
            back_btn,
            text::heading(fl!("notification-settings")),
        ]
        .spacing(sp.space_xxs)
        .align_y(Alignment::Center),
    );

    let content = widget::column::with_children(vec![
        header.into(),
        sections.into(),
    ])
    .spacing(sp.space_xxs);

    widget::container(widget::scrollable(content))
        .width(Length::Fill)
        .into()
}
