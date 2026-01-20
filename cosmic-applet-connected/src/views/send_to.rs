//! SendTo view component for sharing content with a device.

use crate::app::Message;
use crate::fl;
use cosmic::iced::widget::{column, row, text};
use cosmic::iced::{Alignment, Length};
use cosmic::widget;
use cosmic::widget::icon;
use cosmic::Element;

/// View parameters for the SendTo submenu.
pub struct SendToParams<'a> {
    /// Device type (e.g., "phone", "tablet").
    pub device_type: &'a str,
    /// Device ID.
    pub device_id: &'a str,
    /// Current text input for sharing.
    pub share_text_input: &'a str,
    /// Status message to display, if any.
    pub status_message: Option<&'a str>,
}

/// View for the "Send to device" submenu.
pub fn view_send_to(params: SendToParams<'_>) -> Element<'_, Message> {
    let device_type = params.device_type;
    let device_id = params.device_id.to_string();

    // Back button
    let back_btn = widget::button::text(fl!("back"))
        .leading_icon(icon::from_name("go-previous-symbolic").size(16))
        .on_press(Message::BackFromSendTo);

    // Header
    let header = text(fl!("send-to-title", device = device_type)).size(16);

    // Action list items (consistent with device page style)
    let device_id_for_file = device_id.clone();
    let device_id_for_clipboard = device_id.clone();
    let device_id_for_ping = device_id.clone();
    let device_id_for_text = device_id.clone();
    let text_to_share = params.share_text_input.to_string();

    // Share file list item
    let share_file_row = row![
        icon::from_name("document-send-symbolic").size(24),
        text(fl!("share-file")).size(14),
        widget::horizontal_space(),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let share_file_item = widget::button::custom(
        widget::container(share_file_row)
            .padding(8)
            .width(Length::Fill),
    )
    .class(cosmic::theme::Button::Text)
    .on_press(Message::ShareFile(device_id_for_file))
    .width(Length::Fill);

    // Send clipboard list item
    let send_clipboard_row = row![
        icon::from_name("edit-copy-symbolic").size(24),
        text(fl!("share-clipboard")).size(14),
        widget::horizontal_space(),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let send_clipboard_item = widget::button::custom(
        widget::container(send_clipboard_row)
            .padding(8)
            .width(Length::Fill),
    )
    .class(cosmic::theme::Button::Text)
    .on_press(Message::SendClipboard(device_id_for_clipboard))
    .width(Length::Fill);

    // Send ping list item
    let send_ping_row = row![
        icon::from_name("emblem-synchronizing-symbolic").size(24),
        text(fl!("send-ping")).size(14),
        widget::horizontal_space(),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let send_ping_item = widget::button::custom(
        widget::container(send_ping_row)
            .padding(8)
            .width(Length::Fill),
    )
    .class(cosmic::theme::Button::Text)
    .on_press(Message::SendPing(device_id_for_ping))
    .width(Length::Fill);

    // Share text section
    let share_text_heading = text(fl!("share-text")).size(14);

    let share_text_input =
        widget::text_input(fl!("share-text-placeholder"), params.share_text_input)
            .on_input(Message::ShareTextInput)
            .width(Length::Fill);

    let send_text_btn = widget::button::standard(fl!("send-text"))
        .leading_icon(icon::from_name("edit-paste-symbolic").size(16))
        .on_press_maybe(if params.share_text_input.is_empty() {
            None
        } else {
            Some(Message::ShareText(device_id_for_text, text_to_share))
        });

    // Status message if present
    let status_bar: Element<Message> = if let Some(msg) = params.status_message {
        widget::container(text(msg).size(11))
            .padding([4, 8])
            .width(Length::Fill)
            .class(cosmic::theme::Container::Card)
            .into()
    } else {
        widget::Space::new(Length::Shrink, Length::Shrink).into()
    };

    widget::container(
        column![
            back_btn,
            status_bar,
            widget::divider::horizontal::default(),
            header,
            share_file_item,
            send_clipboard_item,
            send_ping_item,
            widget::divider::horizontal::default(),
            share_text_heading,
            share_text_input,
            send_text_btn,
        ]
        .spacing(12)
        .padding(16),
    )
    .into()
}
