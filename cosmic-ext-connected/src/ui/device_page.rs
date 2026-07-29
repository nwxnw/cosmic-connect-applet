//! Individual device page view.
//!
//! Shows detailed information and actions for a specific device.

use crate::app::{DeviceInfo, Message};
use crate::device::DeviceClass;
use crate::fl;
use cosmic::applet;
use cosmic::iced::widget::{column, row};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, icon, text};
use cosmic::Element;
use kdeconnect_dbus::plugins::NotificationInfo;

/// Localized caption for a device's type, shown under the device name.
fn device_type_label(device_type: &str) -> String {
    match device_type {
        "smartphone" | "phone" => fl!("device-type-phone"),
        "tablet" => fl!("device-type-tablet"),
        "desktop" => fl!("device-type-desktop"),
        "laptop" => fl!("device-type-laptop"),
        "tv" => fl!("device-type-tv"),
        _ => fl!("device-type-unknown"),
    }
}

/// Render the device detail page.
pub fn view<'a>(
    device: &'a DeviceInfo,
    status_message: Option<&'a str>,
    pairing_error: Option<&'a str>,
) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();
    let class = DeviceClass::from_device_type(&device.device_type);

    // Device info row with back button, icon, name, and type caption
    let device_info: Element<Message> = {
        let back_btn = widget::button::icon(icon::from_name("go-previous-symbolic"))
            .class(cosmic::theme::Button::Link)
            .on_press(Message::BackToList);

        let device_icon = icon::from_name(class.icon_name()).size(48);

        let info_row = row![
            back_btn,
            device_icon,
            column![
                text::title4(device.name.clone()),
                text::caption(device_type_label(&device.device_type)),
            ]
            .spacing(sp.space_xxxs),
            widget::space::horizontal(),
        ]
        .spacing(sp.space_s)
        .align_y(Alignment::Center);

        applet::padded_control(info_row).into()
    };

    // Build the combined status row with connected, paired, and battery
    let status_row = build_status_row(device);

    // Actions section - suppressed entirely while a pair request is in flight
    // (pairing section below owns the UI for those transient states).
    let pair_in_flight = device.is_pair_requested || device.is_pair_requested_by_peer;
    let actions: Option<Element<Message>> = if pair_in_flight {
        None
    } else if device.is_paired {
        let device_id_for_unpair = device.id.clone();
        let mut items: Vec<Element<Message>> = Vec::new();

        if !device.is_reachable {
            // Offline but paired: online-only actions are unavailable, but Unpair
            // still works.
            items.push(text::caption(fl!("device-offline-actions-unavailable")).into());
        } else {
            let device_id_for_media = device.id.clone();
            if class.is_mobile() {
                // Mobile: SMS → Send-to submenu → Media → Find Phone.
                let device_id_for_sms = device.id.clone();
                let device_id_for_sendto = device.id.clone();
                let device_type_for_sendto = device.device_type.clone();
                let device_id_for_find = device.id.clone();
                let device_label = device_type_label(&device.device_type);

                let sms_row = row![
                    icon::from_name("mail-message-new-symbolic").size(24),
                    text::body(fl!("sms-messages")),
                    widget::space::horizontal(),
                    icon::from_name("go-next-symbolic").size(16),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(sms_row)
                        .on_press(Message::OpenSmsView(device_id_for_sms))
                        .into(),
                );

                let sendto_row = row![
                    icon::from_name("document-send-symbolic").size(24),
                    text::body(fl!("send-to", device = device_label.as_str())),
                    widget::space::horizontal(),
                    icon::from_name("go-next-symbolic").size(16),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(sendto_row)
                        .on_press(Message::OpenSendToView(
                            device_id_for_sendto,
                            device_type_for_sendto,
                        ))
                        .into(),
                );

                let media_row = row![
                    icon::from_name("multimedia-player-symbolic").size(24),
                    text::body(fl!("media-controls")),
                    widget::space::horizontal(),
                    icon::from_name("go-next-symbolic").size(16),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(media_row)
                        .on_press(Message::OpenMediaView(device_id_for_media))
                        .into(),
                );

                let find_row = row![
                    icon::from_name("audio-volume-high-symbolic").size(24),
                    text::body(fl!("find-phone")),
                    widget::space::horizontal(),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(find_row)
                        .on_press(Message::FindMyPhone(device_id_for_find))
                        .into(),
                );
            } else {
                // Non-mobile: inline share primitives as direct actions; Share Text
                // navigates to a focused compose view. Media stays a submenu nav.
                let device_id_for_file = device.id.clone();
                let device_id_for_clipboard = device.id.clone();
                let device_id_for_ping = device.id.clone();
                let device_id_for_text = device.id.clone();
                let device_type_for_text = device.device_type.clone();

                let share_file_row = row![
                    icon::from_name("document-send-symbolic").size(24),
                    text::body(fl!("share-file")),
                    widget::space::horizontal(),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(share_file_row)
                        .on_press(Message::ShareFile(device_id_for_file))
                        .into(),
                );

                let clipboard_row = row![
                    icon::from_name("edit-copy-symbolic").size(24),
                    text::body(fl!("share-clipboard")),
                    widget::space::horizontal(),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(clipboard_row)
                        .on_press(Message::SendClipboard(device_id_for_clipboard))
                        .into(),
                );

                let ping_row = row![
                    icon::from_name("network-transmit-symbolic").size(24),
                    text::body(fl!("send-ping")),
                    widget::space::horizontal(),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(ping_row)
                        .on_press(Message::SendPing(device_id_for_ping))
                        .into(),
                );

                let share_text_row = row![
                    icon::from_name("edit-paste-symbolic").size(24),
                    text::body(fl!("share-text")),
                    widget::space::horizontal(),
                    icon::from_name("go-next-symbolic").size(16),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(share_text_row)
                        .on_press(Message::OpenShareTextView(
                            device_id_for_text,
                            device_type_for_text,
                        ))
                        .into(),
                );

                let media_row = row![
                    icon::from_name("multimedia-player-symbolic").size(24),
                    text::body(fl!("media-controls")),
                    widget::space::horizontal(),
                    icon::from_name("go-next-symbolic").size(16),
                ]
                .spacing(sp.space_xs)
                .align_y(Alignment::Center);
                items.push(
                    applet::menu_button(media_row)
                        .on_press(Message::OpenMediaView(device_id_for_media))
                        .into(),
                );
            }
        }

        // Divider + Unpair — shared across classes and reachability state.
        items.push(applet::padded_control(widget::divider::horizontal::default()).into());
        let unpair_row = row![
            icon::from_name("list-remove-symbolic").size(24),
            text::body(fl!("unpair")),
            widget::space::horizontal(),
        ]
        .spacing(sp.space_xs)
        .align_y(Alignment::Center);
        items.push(
            applet::menu_button(unpair_row)
                .on_press(Message::Unpair(device_id_for_unpair))
                .into(),
        );

        //Unpairing an offline device is one-sided until it reconnects
        if !device.is_reachable {
            items.push(text::caption(fl!("unpair-offline-note")).into());
        }

        Some(column(items).spacing(sp.space_xxxs).into())
    } else if device.is_reachable {
        let device_id_for_pair = device.id.clone();
        let pair_row = row![
            icon::from_name("list-add-symbolic")
                .size(24)
                .icon()
                .class(cosmic::theme::Svg::custom(|theme| {
                    cosmic::iced::widget::svg::Style {
                        color: Some(theme.cosmic().accent_text_color().into()),
                    }
                })),
            text::body(fl!("pair")),
            widget::space::horizontal(),
        ]
        .spacing(sp.space_xs)
        .align_y(Alignment::Center);
        Some(
            applet::menu_button(pair_row)
                .on_press(Message::RequestPair(device_id_for_pair))
                .into(),
        )
    } else {
        //Unpaired and offline: nothing actionable and filtered from the list
        Some(text::caption(fl!("device-must-be-connected")).into())
    };

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
        widget::Space::new().into()
    };

    let divider = || applet::padded_control(widget::divider::horizontal::default());

    let mut content = column![status_bar, device_info, status_row]
        .spacing(sp.space_xxs)
        .padding([0, sp.space_s as u16, sp.space_s as u16, sp.space_s as u16]);

    if let Some(actions_elem) = actions {
        content = content.push(divider());
        content = content.push(actions_elem);
    }

    // Pairing failure reason, shown under the Pair action it explains.
    if let Some(err) = pairing_error {
        content = content.push(applet::padded_control(text::caption(fl!(
            "pairing-failed",
            error = err
        ))));
    }

    if needs_pairing_section(device) {
        content = content.push(divider());
        content = content.push(build_pairing_section(device));
    }

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
                widget::Space::new().into()
            }
        } else {
            // No battery info available - empty space
            widget::Space::new().into()
        };

    row![
        connected_element,
        widget::space::horizontal(),
        paired_element,
        widget::space::horizontal(),
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

/// Whether the device state requires a pairing section separate from the actions list.
/// Only pair-request flows (incoming or outgoing) need a dedicated section; steady
/// paired/unpaired states are handled by the actions list (Unpair / Pair).
fn needs_pairing_section(device: &DeviceInfo) -> bool {
    device.is_pair_requested_by_peer || device.is_pair_requested
}

/// Build the pairing section for in-flight pair request flows.
fn build_pairing_section<'a>(device: &'a DeviceInfo) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();
    let device_id = device.id.clone();

    // Peer requested pairing — show accept/reject buttons
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

    // We requested pairing — show waiting caption and cancel button
    column![
        text::heading(fl!("pairing")),
        text::caption(fl!("waiting-for-device")),
        widget::button::standard(fl!("cancel")).on_press(Message::RejectPairing(device_id)),
    ]
    .spacing(sp.space_xxs)
    .into()
}

/// Build the notifications section.
fn build_notifications_section<'a>(device: &'a DeviceInfo) -> Element<'a, Message> {
    let sp = cosmic::theme::spacing();

    if device.notifications.is_empty() {
        return widget::Space::new().into();
    }

    let header = row![
        text::heading(fl!("notifications")),
        widget::container(text::caption(format!("{}", device.notifications.len())))
            .padding([2, sp.space_xxxs as u16 + 2])
            .class(cosmic::theme::Container::Card),
    ]
    .spacing(sp.space_xxs)
    .align_y(Alignment::Center);

    let mut notif_column = column![header].spacing(sp.space_xxs);

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

    let notif_content = column![
        text::body(notif_title).wrapping(cosmic::iced::widget::text::Wrapping::WordOrGlyph),
        text::caption(&notif.text).wrapping(cosmic::iced::widget::text::Wrapping::WordOrGlyph),
    ]
    .spacing(2)
    .width(Length::Fill);

    let mut notif_row = row![
        icon::from_name("notification-symbolic").size(20),
        notif_content,
    ]
    .spacing(sp.space_xxs)
    .align_y(Alignment::Center);

    // Add dismiss button if notification is dismissable
    if notif.dismissable {
        let device_id = device.id.clone();
        let notif_id = notif.id.clone();
        notif_row = notif_row.push(
            widget::tooltip(
                widget::button::icon(icon::from_name("window-close-symbolic"))
                    .on_press(Message::DismissNotification(device_id, notif_id)),
                text::caption(fl!("dismiss")),
                widget::tooltip::Position::Bottom,
            )
            .gap(sp.space_xxxs)
            .padding(sp.space_xxs),
        );
    }

    widget::container(notif_row)
        .padding([sp.space_xxxs, sp.space_xxs])
        .width(Length::Fill)
        .into()
}
