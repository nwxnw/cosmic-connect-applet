//! Individual device page view.
//!
//! Shows detailed information and actions for a specific device.

use crate::app::{DeviceInfo, Message};
use crate::fl;
use cosmic::applet;
use cosmic::iced::widget::{column, row, tooltip};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, icon, text};
use cosmic::Element;
use kdeconnect_dbus::plugins::NotificationInfo;

/// Render the device detail page.
pub fn view<'a>(device: &'a DeviceInfo, status_message: Option<&'a str>) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();

    // Back button
    let back_btn = widget::button::icon(icon::from_name("go-previous-symbolic"))
        .on_press(Message::BackToList);

    // Device icon based on type
    let icon_name = match device.device_type.as_str() {
        "phone" | "smartphone" => "phone-symbolic",
        "tablet" => "tablet-symbolic",
        "desktop" => "computer-symbolic",
        "laptop" => "computer-laptop-symbolic",
        _ => "device-symbolic",
    };

    // Build header row with device info and optional ping button
    let header: Element<Message> = {
        let mut header_row = row![
            back_btn,
            icon::from_name(icon_name).size(48),
            column![
                text::title4(device.name.clone()),
                text::caption(device.device_type.clone()),
            ]
            .spacing(sp.space_xxxs),
            widget::horizontal_space(),
        ]
        .spacing(sp.space_s)
        .align_y(Alignment::Center);

        // Add ping button if device is reachable and paired
        if device.is_reachable && device.is_paired {
            let device_id_for_ping = device.id.clone();
            let ping_btn =
                widget::button::icon(icon::from_name("emblem-synchronizing-symbolic").size(20))
                    .on_press(Message::SendPing(device_id_for_ping))
                    .padding(sp.space_xxs);
            let ping_with_tooltip = tooltip(
                ping_btn,
                text::caption(fl!("send-ping")),
                tooltip::Position::Bottom,
            )
            .gap(sp.space_xxxs)
            .padding(sp.space_xxs);
            header_row = header_row.push(ping_with_tooltip);
        }

        applet::padded_control(header_row).into()
    };

    // Build the combined status row with connected, paired, and battery
    let status_row = build_status_row(device);

    // Actions section - only available for connected and paired devices
    let actions: Element<Message> = if device.is_reachable && device.is_paired {
        let device_id_for_sms = device.id.clone();
        let device_id_for_sendto = device.id.clone();
        let device_type_for_sendto = device.device_type.clone();
        let device_id_for_media = device.id.clone();
        let device_id_for_find = device.id.clone();

        // SMS Messages action item
        let sms_row = row![
            icon::from_name("mail-message-new-symbolic").size(24),
            text::body(fl!("sms-messages")),
            widget::horizontal_space(),
            icon::from_name("go-next-symbolic").size(16),
        ]
        .spacing(sp.space_xs)
        .align_y(Alignment::Center);

        let sms_item = applet::menu_button(sms_row)
            .on_press(Message::OpenSmsView(device_id_for_sms));

        // Send to device action item
        let sendto_row = row![
            icon::from_name("document-send-symbolic").size(24),
            text::body(fl!("send-to", device = device.device_type.as_str())),
            widget::horizontal_space(),
            icon::from_name("go-next-symbolic").size(16),
        ]
        .spacing(sp.space_xs)
        .align_y(Alignment::Center);

        let sendto_item = applet::menu_button(sendto_row)
            .on_press(Message::OpenSendToView(
                device_id_for_sendto,
                device_type_for_sendto,
            ));

        // Media controls action item
        let media_row = row![
            icon::from_name("multimedia-player-symbolic").size(24),
            text::body(fl!("media-controls")),
            widget::horizontal_space(),
            icon::from_name("go-next-symbolic").size(16),
        ]
        .spacing(sp.space_xs)
        .align_y(Alignment::Center);

        let media_item = applet::menu_button(media_row)
            .on_press(Message::OpenMediaView(device_id_for_media));

        // Find Phone action item (no chevron - immediate action)
        let find_row = row![
            icon::from_name("audio-volume-high-symbolic").size(24),
            text::body(fl!("find-phone")),
            widget::horizontal_space(),
        ]
        .spacing(sp.space_xs)
        .align_y(Alignment::Center);

        let find_item = applet::menu_button(find_row)
            .on_press(Message::FindMyPhone(device_id_for_find));

        column![sms_item, sendto_item, media_item, find_item,]
            .spacing(sp.space_xxxs)
            .into()
    } else if !device.is_paired {
        // Not paired - show nothing (pairing section will be shown below)
        widget::Space::new(Length::Shrink, Length::Shrink).into()
    } else {
        text::caption(fl!("device-must-be-connected")).into()
    };

    // Pairing section
    let pairing_section: Element<Message> = build_pairing_section(device);

    // Notifications section
    let notifications_section: Element<Message> = build_notifications_section(device);

    // Build status message element if present
    let status_bar: Element<Message> = if let Some(msg) = status_message {
        widget::container(text::caption(msg))
            .padding([sp.space_xxxs, sp.space_xxs])
            .width(Length::Fill)
            .class(cosmic::theme::Container::Card)
            .into()
    } else {
        widget::Space::new(Length::Shrink, Length::Shrink).into()
    };

    let divider = || applet::padded_control(widget::divider::horizontal::default());

    let mut content = column![header, status_bar, status_row, divider(), actions,]
        .spacing(sp.space_xs);

    content = content.push(divider());
    content = content.push(pairing_section);

    if !device.notifications.is_empty() {
        content = content.push(divider());
        content = content.push(notifications_section);
    }

    widget::container(content).into()
}

/// Build the combined status row showing connected, paired, and battery status.
fn build_status_row<'a>(device: &'a DeviceInfo) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();

    // Connected status (left-aligned) - use icon to indicate status
    let connected_icon_name = if device.is_reachable {
        "emblem-ok-symbolic" // Green checkmark
    } else {
        "window-close-symbolic" // X mark
    };
    let connected_element = row![
        icon::from_name(connected_icon_name).size(16),
        text::caption(fl!("connected")),
    ]
    .spacing(sp.space_xxxs)
    .align_y(Alignment::Center);

    // Paired status (center-aligned) - use icon to indicate status
    let paired_icon_name = if device.is_paired {
        "emblem-ok-symbolic" // Green checkmark
    } else {
        "window-close-symbolic" // X mark
    };
    let paired_element = row![
        icon::from_name(paired_icon_name).size(16),
        text::caption(fl!("paired")),
    ]
    .spacing(sp.space_xxxs)
    .align_y(Alignment::Center);

    // Battery status (right-aligned) - percentage text + icon
    // KDE Connect returns -1 when battery level is unknown, so filter those out
    let battery_element: Element<Message> =
        if let (Some(level), Some(charging)) = (device.battery_level, device.battery_charging) {
            if level >= 0 {
                let battery_icon_name = get_battery_icon_name(level, charging);
                row![
                    text::caption(format!("{}%", level)),
                    icon::from_name(battery_icon_name).size(24),
                ]
                .spacing(sp.space_xxxs)
                .align_y(Alignment::Center)
                .into()
            } else {
                // Battery level is -1 (unknown) - don't show
                widget::Space::new(Length::Shrink, Length::Shrink).into()
            }
        } else {
            // No battery info available - empty space
            widget::Space::new(Length::Shrink, Length::Shrink).into()
        };

    row![
        connected_element,
        widget::horizontal_space(),
        paired_element,
        widget::horizontal_space(),
        battery_element,
    ]
    .align_y(Alignment::Center)
    .into()
}

/// Get the appropriate battery icon name based on level and charging state.
fn get_battery_icon_name(level: i32, charging: bool) -> &'static str {
    if charging {
        match level {
            0..=10 => "battery-level-10-charging-symbolic",
            11..=20 => "battery-level-20-charging-symbolic",
            21..=30 => "battery-level-30-charging-symbolic",
            31..=40 => "battery-level-40-charging-symbolic",
            41..=50 => "battery-level-50-charging-symbolic",
            51..=60 => "battery-level-60-charging-symbolic",
            61..=70 => "battery-level-70-charging-symbolic",
            71..=80 => "battery-level-80-charging-symbolic",
            81..=90 => "battery-level-90-charging-symbolic",
            _ => "battery-level-100-charging-symbolic",
        }
    } else {
        match level {
            0..=10 => "battery-level-10-symbolic",
            11..=20 => "battery-level-20-symbolic",
            21..=30 => "battery-level-30-symbolic",
            31..=40 => "battery-level-40-symbolic",
            41..=50 => "battery-level-50-symbolic",
            51..=60 => "battery-level-60-symbolic",
            61..=70 => "battery-level-70-symbolic",
            71..=80 => "battery-level-80-symbolic",
            81..=90 => "battery-level-90-symbolic",
            _ => "battery-level-100-symbolic",
        }
    }
}

/// Build the pairing section based on device state.
fn build_pairing_section<'a>(device: &'a DeviceInfo) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();
    let device_id = device.id.clone();

    // If peer requested pairing, show accept/reject buttons
    if device.is_pair_requested_by_peer {
        let accept_id = device_id.clone();
        let reject_id = device_id;
        return column![
            text::heading(fl!("pairing-request")),
            text::caption(fl!("device-wants-to-pair")),
            row![
                widget::button::suggested(fl!("accept"))
                    .leading_icon(icon::from_name("emblem-ok-symbolic").size(16))
                    .on_press(Message::AcceptPairing(accept_id)),
                widget::button::destructive(fl!("reject"))
                    .leading_icon(icon::from_name("window-close-symbolic").size(16))
                    .on_press(Message::RejectPairing(reject_id)),
            ]
            .spacing(sp.space_xxs),
        ]
        .spacing(sp.space_xxs)
        .into();
    }

    // If we requested pairing, show cancel button
    if device.is_pair_requested {
        return column![
            text::heading(fl!("pairing")),
            text::caption(fl!("waiting-for-device")),
            widget::button::standard(fl!("cancel")).on_press(Message::RejectPairing(device_id)),
        ]
        .spacing(sp.space_xxs)
        .into();
    }

    // If paired, show unpair button
    if device.is_paired {
        return column![
            text::heading(fl!("pairing")),
            widget::button::destructive(fl!("unpair"))
                .leading_icon(icon::from_name("list-remove-symbolic").size(16))
                .on_press(Message::Unpair(device_id)),
        ]
        .spacing(sp.space_xxs)
        .into();
    }

    // If reachable but not paired, show pair button
    if device.is_reachable {
        return column![
            text::heading(fl!("pairing")),
            text::caption(fl!("device-not-paired")),
            widget::button::suggested(fl!("pair"))
                .leading_icon(icon::from_name("list-add-symbolic").size(16))
                .on_press(Message::RequestPair(device_id)),
        ]
        .spacing(sp.space_xxs)
        .into();
    }

    // Offline and not paired
    column![
        text::heading(fl!("pairing")),
        text::caption(fl!("device-offline")),
    ]
    .spacing(sp.space_xxs)
    .into()
}

/// Build the notifications section.
fn build_notifications_section<'a>(device: &'a DeviceInfo) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();

    if device.notifications.is_empty() {
        return widget::Space::new(Length::Shrink, Length::Shrink).into();
    }

    let mut notif_column = column![text::heading(format!(
        "{} ({})",
        fl!("notifications"),
        device.notifications.len()
    )),]
    .spacing(sp.space_xxs);

    for notif in &device.notifications {
        let notif_widget = build_notification_row(device, notif);
        notif_column = notif_column.push(notif_widget);
    }

    notif_column.into()
}

/// Build a single notification row.
fn build_notification_row<'a>(
    device: &'a DeviceInfo,
    notif: &'a NotificationInfo,
) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();

    let notif_title = if notif.title.is_empty() {
        notif.app_name.clone()
    } else {
        format!("{}: {}", notif.app_name, notif.title)
    };

    let notif_content = column![text::body(notif_title), text::caption(&notif.text),].spacing(2);

    let mut notif_row = row![
        icon::from_name("notification-symbolic").size(20),
        notif_content,
        widget::horizontal_space(),
    ]
    .spacing(sp.space_xxs)
    .align_y(Alignment::Center);

    // Add dismiss button if notification is dismissable
    if notif.dismissable {
        let device_id = device.id.clone();
        let notif_id = notif.id.clone();
        notif_row = notif_row.push(
            widget::button::icon(icon::from_name("window-close-symbolic"))
                .on_press(Message::DismissNotification(device_id, notif_id)),
        );
    }

    widget::container(notif_row)
        .padding([sp.space_xxxs, sp.space_xxs])
        .width(Length::Fill)
        .into()
}
