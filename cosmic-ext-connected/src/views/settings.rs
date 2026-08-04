//! Settings view components.

use crate::app::{Message, SettingKey};
use crate::config::Config;
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
                .class(cosmic::theme::Button::Link)
                .on_press(Message::ToggleSettings),
            text::heading(fl!("notifications")),
        ]
        .spacing(sp.space_xxs)
        .align_y(Alignment::Center),
    );

    // KDE Connect duplicate-notification hint
    let hint = widget::container(
        widget::column::with_children(vec![
            text::caption(fl!("notification-duplicate-hint")).into(),
            widget::button::link(fl!("learn-more"))
                .on_press(Message::OpenUrl(
                    "https://github.com/nwxnw/cosmic-ext-connected#duplicate-notifications-with-kde-connect"
                        .to_string(),
                ))
                .into(),
        ])
            .spacing(sp.space_xxs),
    )
        .padding([sp.space_xxs, sp.space_s as u16])
        .width(Length::Fill)
        .class(cosmic::theme::Container::Card);

    // SMS notifications section
    let mut sms_section = settings::section().title(fl!("settings-sms-section")).add(
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
    let mut call_section = settings::section().title(fl!("settings-call-section")).add(
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
    let file_section = settings::section().title(fl!("settings-file-section")).add(
        settings::item::builder(fl!("settings-file-notifications"))
            .toggler(config.file_notifications, move |_| {
                Message::ToggleSetting(SettingKey::FileNotifications)
            }),
    );

    let sections = settings::view_column(vec![
        sms_section.into(),
        call_section.into(),
        file_section.into(),
    ]);

    let content = widget::column::with_children(vec![header.into(), hint.into(), sections.into()])
        .spacing(sp.space_xxs)
        .padding([0, sp.space_s as u16, sp.space_s as u16, sp.space_s as u16]);

    widget::container(widget::scrollable(content))
        .width(Length::Fill)
        .into()
}

/// Render the About sub-page
pub fn view_about() -> Element<'static, Message> {
    let sp = cosmic::theme::spacing();

    let back_btn = widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
        .class(cosmic::theme::Button::Link)
        .on_press(Message::BackFromAbout);

    let header = applet::padded_control(
        row![back_btn, text::heading(fl!("about"))]
            .spacing(sp.space_xxs)
            .align_y(Alignment::Center),
    );
    let about_icon = widget::icon::from_name("io.github.nwxnw.cosmic-ext-connected").size(64);
    let app_title = text::title3(fl!("app-title"));
    let about_summary = text::caption(fl!("about-summary"));
    let app_version = text::body(format!("v{}", env!("CARGO_PKG_VERSION")));
    let about_homepage = widget::button::link(fl!("about-homepage")).on_press(Message::OpenUrl(
        "https://github.com/nwxnw/cosmic-ext-connected".to_string(),
    ));
    let about_issues = widget::button::link(fl!("about-issues")).on_press(Message::OpenUrl(
        "https://github.com/nwxnw/cosmic-ext-connected/issues".to_string(),
    ));
    let license = text::caption("GPL-3.0");

    let identity = widget::column::with_children(vec![
        about_icon.into(),
        app_title.into(),
        about_summary.into(),
        app_version.into(),
    ])
    .align_x(Alignment::Center)
    .spacing(sp.space_xxxs);

    let links = widget::column::with_children(vec![about_homepage.into(), about_issues.into()])
        .align_x(Alignment::Center)
        .spacing(sp.space_xxxs);

    let body = widget::column::with_children(vec![identity.into(), links.into(), license.into()])
        .align_x(Alignment::Center)
        .spacing(sp.space_m)
        .width(Length::Fill)
        .padding([0, sp.space_s as u16, sp.space_s as u16, sp.space_s as u16]);

    let content =
        widget::column::with_children(vec![header.into(), body.into()]).spacing(sp.space_xxs);

    widget::container(widget::scrollable(content))
        .width(Length::Fill)
        .into()
}
