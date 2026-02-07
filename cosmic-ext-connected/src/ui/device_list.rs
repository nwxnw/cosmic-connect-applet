//! Device list view for the applet popup.

use crate::app::{DeviceInfo, Message};
use crate::config::Config;
use crate::fl;
use cosmic::applet;
use cosmic::iced::advanced::widget::text::Style as TextStyle;
use cosmic::iced::widget::{column, row};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, icon, text};
use cosmic::{theme, Element};

/// Render the device list view.
pub fn view<'a>(
    devices: &'a [DeviceInfo],
    config: &'a Config,
    status_message: Option<&'a str>,
) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();

    // Header with refresh and settings buttons
    let header = applet::padded_control(
        row![
            text::heading(fl!("devices")),
            widget::horizontal_space(),
            widget::button::icon(icon::from_name("view-refresh-symbolic"))
                .on_press(Message::RefreshDevices),
            widget::button::icon(icon::from_name("emblem-system-symbolic"))
                .on_press(Message::ToggleSettings),
        ]
        .spacing(sp.space_xxxs)
        .align_y(Alignment::Center),
    );

    // Filter devices based on config
    let filtered_devices: Vec<&DeviceInfo> = devices
        .iter()
        .filter(|d| {
            // Always show reachable devices
            if d.is_reachable {
                return true;
            }
            // Show offline paired devices only if config allows
            if d.is_paired && config.show_offline_devices {
                return true;
            }
            false
        })
        .collect();

    let device_rows: Vec<Element<Message>> = filtered_devices
        .iter()
        .map(|device| device_row(device, config))
        .collect();

    let mut content = column![header].spacing(sp.space_xxxs);

    // Status message bar (for feedback like "Ping sent!", "Sharing file...")
    if let Some(msg) = status_message {
        content = content.push(
            widget::container(text::caption(msg))
                .padding([sp.space_xxxs, sp.space_xxs])
                .width(Length::Fill)
                .class(cosmic::theme::Container::Card),
        );
    }

    if device_rows.is_empty() {
        content = content.push(
            widget::container(text::caption(fl!("no-devices")))
                .padding(sp.space_s)
                .width(Length::Fill),
        );
    } else {
        content = content.push(column(device_rows).spacing(sp.space_xxs));
    }

    widget::container(content.padding(sp.space_xxs)).into()
}

/// Render a single device row.
fn device_row<'a>(device: &'a DeviceInfo, config: &'a Config) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();

    let icon_name = match device.device_type.as_str() {
        "phone" | "smartphone" => "phone-symbolic",
        "tablet" => "tablet-symbolic",
        "desktop" => "computer-symbolic",
        "laptop" => "computer-laptop-symbolic",
        _ => "device-symbolic",
    };

    let (status_text, is_offline) = match (
        device.is_reachable,
        device.is_paired,
        device.is_pair_requested,
        device.is_pair_requested_by_peer,
    ) {
        (_, _, _, true) => (fl!("pairing-request"), false),
        (_, _, true, _) => (fl!("pairing"), false),
        (true, true, _, _) => (fl!("connected"), false),
        (false, true, _, _) => (fl!("offline"), true),
        (true, false, _, _) => (fl!("not-paired"), false),
        _ => (fl!("offline"), true),
    };

    // Apply warning color (yellow) to offline status text for better visual indication
    let status_widget: Element<Message> = if is_offline {
        fn warning_style(theme: &cosmic::Theme) -> TextStyle {
            let warning_color = theme.cosmic().warning.base;
            TextStyle {
                color: Some(warning_color.into()),
            }
        }
        text::caption(status_text)
            .class(theme::Text::Custom(warning_style))
            .into()
    } else {
        text::caption(status_text).into()
    };

    let mut row_content = row![
        icon::from_name(icon_name).size(24),
        column![text::body(device.name.clone()), status_widget,].spacing(2),
    ]
    .spacing(sp.space_xs)
    .align_y(Alignment::Center);

    // Add battery info if available and enabled in settings
    // KDE Connect returns -1 when battery level is unknown, so filter those out
    if config.show_battery_percentage {
        if let (Some(level), Some(charging)) = (device.battery_level, device.battery_charging) {
            if level >= 0 {
                let battery_text = if charging {
                    format!("{}%+", level)
                } else {
                    format!("{}%", level)
                };
                row_content = row_content.push(text::caption(battery_text));
            }
        }
    }

    // Add notification count badge if there are notifications and notifications are enabled
    if config.forward_notifications && !device.notifications.is_empty() {
        row_content = row_content.push(
            widget::container(text::caption(format!("{}", device.notifications.len())))
                .padding([2, sp.space_xxxs as u16 + 2])
                .class(cosmic::theme::Container::Card),
        );
    }

    // Add chevron indicator to show it's clickable
    row_content = row_content.push(widget::horizontal_space());
    row_content = row_content.push(icon::from_name("go-next-symbolic").size(16));

    applet::menu_button(row_content)
        .on_press(Message::SelectDevice(device.id.clone()))
        .into()
}
