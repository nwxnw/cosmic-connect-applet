//! SMS view components for conversation list and message threads.

use crate::app::{LoadingPhase, Message, SmsLoadingState};
use crate::fl;
use crate::views::helpers::{format_timestamp, WIDE_POPUP_WIDTH};
use cosmic::applet;
use cosmic::iced::widget::{column, row};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, text};
use cosmic::Element;
use kdeconnect_dbus::contacts::ContactLookup;
use kdeconnect_dbus::plugins::{is_address_valid, ConversationSummary, MessageType, SmsMessage};

// --- Helper functions for loading state ---

/// Get display text for conversation loading state.
fn conversation_loading_text(state: &SmsLoadingState) -> String {
    match state {
        SmsLoadingState::LoadingConversations(phase) => match phase {
            LoadingPhase::Connecting => fl!("loading-connecting"),
            LoadingPhase::Requesting => fl!("loading-requesting"),
        },
        _ => fl!("loading-conversations"),
    }
}

/// Get display text for message loading state.
fn message_loading_text(state: &SmsLoadingState) -> String {
    match state {
        SmsLoadingState::LoadingMessages(phase) => match phase {
            LoadingPhase::Connecting => fl!("loading-connecting"),
            LoadingPhase::Requesting => fl!("loading-requesting"),
        },
        _ => fl!("loading-messages"),
    }
}

/// Check if conversations are in a loading state.
fn is_loading_conversations(state: &SmsLoadingState) -> bool {
    matches!(state, SmsLoadingState::LoadingConversations(_))
}

/// Check if messages are in a loading state (not pagination).
fn is_loading_messages(state: &SmsLoadingState) -> bool {
    matches!(state, SmsLoadingState::LoadingMessages(_))
}

/// Check if loading more messages (pagination).
fn is_loading_more(state: &SmsLoadingState) -> bool {
    matches!(state, SmsLoadingState::LoadingMoreMessages)
}

// --- View params and functions ---

/// Parameters for the conversation list view.
pub struct ConversationListParams<'a> {
    pub device_name: Option<&'a str>,
    pub conversations: &'a [ConversationSummary],
    pub conversations_displayed: usize,
    pub contacts: &'a ContactLookup,
    pub loading_state: &'a SmsLoadingState,
    /// Whether background sync is active (syncing conversations from phone)
    pub sync_active: bool,
}

/// Render the SMS conversation list view.
pub fn view_conversation_list(params: ConversationListParams<'_>) -> Element<'_, Message> {
    let sp = cosmic::theme::spacing();
    let default_device = fl!("device");
    let device_name = params.device_name.unwrap_or(&default_device);

    // Build header with optional sync indicator
    let mut header_row = row![
        widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
            .on_press(Message::CloseSmsView),
        text::heading(fl!("messages-title", device = device_name)),
    ]
    .spacing(sp.space_xxs)
    .align_y(Alignment::Center);

    // Show sync indicator when background sync is active
    if params.sync_active {
        header_row = header_row.push(
            widget::tooltip(
                widget::icon::from_name("emblem-synchronizing-symbolic").size(16),
                text::caption(fl!("syncing")),
                widget::tooltip::Position::Bottom,
            )
            .padding(sp.space_xxxs),
        );
    }

    let new_msg_btn = widget::tooltip(
        widget::button::icon(widget::icon::from_name("list-add-symbolic"))
            .on_press(Message::OpenNewMessage),
        text::caption(fl!("new-message")),
        widget::tooltip::Position::Bottom,
    )
    .gap(sp.space_xxxs)
    .padding(sp.space_xxs);

    let header = applet::padded_control(
        header_row
            .push(widget::horizontal_space())
            .push(new_msg_btn),
    );

    let content: Element<Message> = if is_loading_conversations(params.loading_state)
        && params.conversations.is_empty()
    {
        widget::container(
            column![text::body(conversation_loading_text(params.loading_state)),]
                .align_x(Alignment::Center),
        )
        .center(Length::Fill)
        .into()
    } else if params.conversations.is_empty() {
        widget::container(
            column![
                widget::icon::from_name("mail-message-new-symbolic").size(48),
                text::heading(fl!("no-conversations")),
                text::caption(fl!("start-new-message")),
            ]
            .spacing(sp.space_xs)
            .align_x(Alignment::Center),
        )
        .center(Length::Fill)
        .into()
    } else {
        // Build conversation list (limited to conversations_displayed)
        let mut conv_column = column![].spacing(sp.space_xxxs);
        for conv in params
            .conversations
            .iter()
            .take(params.conversations_displayed)
        {
            let display_name = params.contacts.get_group_display_name(&conv.addresses, 3);

            let snippet = conv.last_message.chars().take(50).collect::<String>();
            let date_str = format_timestamp(conv.timestamp);

            let conv_row = applet::menu_button(
                row![
                    column![text::body(display_name), text::caption(snippet),].spacing(2),
                    widget::horizontal_space(),
                    text::caption(date_str),
                    widget::icon::from_name("go-next-symbolic").size(16),
                ]
                .spacing(sp.space_xxs)
                .align_y(Alignment::Center),
            )
            .on_press(Message::OpenConversation(conv.thread_id));

            conv_column = conv_column.push(conv_row);
        }

        // Add "Load More" button if there are more conversations
        if params.conversations_displayed < params.conversations.len() {
            let load_more_row = row![
                widget::icon::from_name("go-down-symbolic").size(16),
                text::body(fl!("load-more-conversations")),
            ]
            .spacing(sp.space_xxs)
            .align_y(Alignment::Center);

            let load_more_button = applet::menu_button(
                widget::container(load_more_row)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            )
            .on_press(Message::LoadMoreConversations);

            conv_column = conv_column.push(load_more_button);
        }

        // Show sync progress indicator at bottom when still syncing
        if params.sync_active {
            conv_column = conv_column.push(
                applet::padded_control(
                    row![
                        widget::icon::from_name("emblem-synchronizing-symbolic").size(16),
                        text::caption(fl!("syncing-conversations")),
                    ]
                    .spacing(sp.space_xxs)
                    .align_y(Alignment::Center),
                )
                .align_x(Alignment::Center),
            );
        }

        widget::scrollable(conv_column.padding([0, sp.space_xxs as u16]))
            .width(Length::Fill)
            .into()
    };

    column![header, content,]
        .spacing(sp.space_xxs)
        .width(Length::Fill)
        .into()
}

/// Parameters for the message thread view.
pub struct MessageThreadParams<'a> {
    pub thread_addresses: Option<&'a [String]>,
    pub messages: &'a [SmsMessage],
    pub contacts: &'a ContactLookup,
    pub loading_state: &'a SmsLoadingState,
    pub sms_compose_text: &'a str,
    pub sms_sending: bool,
    /// Whether background sync is active (syncing messages from phone)
    pub sync_active: bool,
    /// UID of message bubble currently being pressed (for visual feedback)
    pub pressed_bubble_uid: Option<i32>,
    /// Whether to show the "Hold to copy" hint (500ms elapsed)
    pub show_copy_hint: bool,
    /// Status message to display (e.g. send confirmation or error)
    pub status_message: Option<&'a str>,
}

/// Render the SMS message thread view.
pub fn view_message_thread(params: MessageThreadParams<'_>) -> Element<'_, Message> {
    let sp = cosmic::theme::spacing();
    let display_name = match params.thread_addresses {
        Some(addrs) => params.contacts.get_group_display_name(addrs, 3),
        None => fl!("unknown"),
    };

    // Build header with optional sync indicator
    let mut header_row = row![
        widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
            .on_press(Message::CloseConversation),
        text::heading(display_name),
    ]
    .spacing(sp.space_xxs)
    .align_y(Alignment::Center);

    // Show sync indicator when background sync is active
    if params.sync_active {
        header_row = header_row.push(
            widget::tooltip(
                widget::icon::from_name("emblem-synchronizing-symbolic").size(16),
                text::caption(fl!("syncing")),
                widget::tooltip::Position::Bottom,
            )
            .padding(sp.space_xxxs),
        );
    }

    let header = applet::padded_control(
        header_row.push(widget::horizontal_space()),
    );

    // Show loading indicator only when loading AND no messages yet
    // Once messages start arriving, show them (scrolled to bottom)
    let content: Element<Message> = if is_loading_messages(params.loading_state)
        && params.messages.is_empty()
    {
        widget::container(
            column![text::body(message_loading_text(params.loading_state)),]
                .align_x(Alignment::Center),
        )
        .center(Length::Fill)
        .into()
    } else if params.messages.is_empty() {
        widget::container(column![text::body(fl!("no-messages")),].align_x(Alignment::Center))
            .center(Length::Fill)
            .into()
    } else {
        // Build message list with improved styling
        // Max width for bubbles is ~75% of popup width for better readability
        let bubble_max_width = (WIDE_POPUP_WIDTH * 0.75) as u16;
        let loading_more = is_loading_more(params.loading_state);

        let mut msg_column = column![].spacing(sp.space_xs).padding([sp.space_xxs, sp.space_xs]);

        // Show loading indicator at top when fetching older messages
        if loading_more {
            let loading_indicator: Element<Message> = widget::container(
                row![
                    widget::icon::from_name("process-working-symbolic").size(16),
                    text::body(fl!("loading-older")),
                ]
                .spacing(sp.space_xxs)
                .align_y(Alignment::Center),
            )
            .padding(sp.space_xxs)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .into();

            msg_column = msg_column.push(loading_indicator);
        }

        for msg in params.messages {
            // MessageType::Inbox (1) = incoming/received, MessageType::Sent (2) = outgoing/sent
            let is_received = msg.message_type == MessageType::Inbox;
            let time_str = format_timestamp(msg.date);
            let is_pressed = params.pressed_bubble_uid == Some(msg.uid);
            let show_hint = is_pressed && params.show_copy_hint;

            // Message bubble content (long-press to copy)
            let bubble_content =
                column![
                    text::body(&msg.body).wrapping(cosmic::iced::widget::text::Wrapping::Word),
                    text::caption(time_str),
                ]
                .spacing(sp.space_xxxs);

            // Use highlighted style when pressed for high contrast visual feedback
            let bubble: Element<Message> = if is_pressed {
                // Wrap in two containers for a "selected" border effect
                let inner = widget::container(bubble_content)
                    .padding([sp.space_xxs, sp.space_xs])
                    .max_width(bubble_max_width - 8)
                    .class(cosmic::theme::Container::Primary);
                widget::container(inner)
                    .padding(sp.space_xxxs)
                    .class(cosmic::theme::Container::Dropdown)
                    .into()
            } else {
                widget::container(bubble_content)
                    .padding([sp.space_xxs, sp.space_xs])
                    .max_width(bubble_max_width)
                    .class(if is_received {
                        cosmic::theme::Container::Card
                    } else {
                        cosmic::theme::Container::Primary
                    })
                    .into()
            };

            // Wrap bubble in mouse_area for long-press detection
            let bubble_with_press = widget::mouse_area(bubble)
                .on_press(Message::BubblePressStarted {
                    uid: msg.uid,
                    body: msg.body.clone(),
                })
                .on_release(Message::BubblePressReleased);

            // Bubble with optional "Hold to copy" hint (only after 500ms)
            let bubble_element: Element<Message> = if show_hint {
                column![
                    bubble_with_press,
                    text::caption(fl!("hold-to-copy")),
                ]
                .spacing(2)
                .into()
            } else {
                bubble_with_press.into()
            };

            // Received messages: align left, show sender name only in group chats
            // Sent messages: align right
            // Note: thread_addresses may contain duplicates or both user + recipient,
            // so we deduplicate and use a threshold of >1 unique addresses for "group"
            let is_group = params.thread_addresses.is_some_and(|addrs| {
                let unique: std::collections::HashSet<_> = addrs.iter().collect();
                unique.len() > 1
            });
            let msg_row: Element<Message> = if is_received {
                if is_group {
                    let sender_name =
                        params.contacts.get_name_or_number(msg.primary_address());
                    column![
                        text::caption(sender_name),
                        row![bubble_element, widget::horizontal_space(),].width(Length::Fill),
                    ]
                    .spacing(sp.space_xxxs)
                    .width(Length::Fill)
                    .into()
                } else {
                    row![bubble_element, widget::horizontal_space(),]
                        .width(Length::Fill)
                        .into()
                }
            } else {
                row![widget::horizontal_space(), bubble_element,]
                    .width(Length::Fill)
                    .into()
            };

            msg_column = msg_column.push(msg_row);
        }

        widget::scrollable(msg_column)
            .id(widget::Id::new("message-thread"))
            .width(Length::Fill)
            .height(Length::Fill)
            .on_scroll(Message::MessageThreadScrolled)
            .into()
    };

    // Compose row
    let compose_input = widget::text_input(fl!("type-message"), params.sms_compose_text)
        .on_input(Message::SmsComposeInput)
        .on_submit(|_| Message::SendSms)
        .width(Length::Fill);

    let send_btn: Element<Message> = if params.sms_sending {
        widget::button::standard(fl!("sending"))
            .leading_icon(widget::icon::from_name("process-working-symbolic").size(16))
            .into()
    } else {
        let can_send = !params.sms_compose_text.is_empty() && !params.sms_sending;
        widget::button::suggested(fl!("send"))
            .leading_icon(widget::icon::from_name("mail-send-symbolic").size(16))
            .on_press_maybe(if can_send {
                Some(Message::SendSms)
            } else {
                None
            })
            .into()
    };

    let compose_row = applet::padded_control(
        row![compose_input, send_btn,]
            .spacing(sp.space_xxs)
            .align_y(Alignment::Center),
    );

    let mut thread_column = column![
        header,
        content,
        compose_row,
    ]
    .spacing(sp.space_xxxs)
    .width(Length::Fill);

    if let Some(msg) = params.status_message {
        thread_column = thread_column.push(
            widget::container(text::caption(msg).wrapping(cosmic::iced::widget::text::Wrapping::Word))
                .padding([sp.space_xxxs, sp.space_xs])
                .width(Length::Fill)
                .class(cosmic::theme::Container::Card),
        );
    }

    thread_column.into()
}

/// Parameters for the new message view.
pub struct NewMessageParams<'a> {
    pub recipient: &'a str,
    pub body: &'a str,
    pub recipient_valid: bool,
    pub sending: bool,
    /// Contact suggestions as (contact_name, phone_number) tuples
    pub contact_suggestions: &'a [(String, String)],
}

/// Render the new message compose view.
pub fn view_new_message(params: NewMessageParams<'_>) -> Element<'_, Message> {
    let sp = cosmic::theme::spacing();

    let header = applet::padded_control(
        row![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .on_press(Message::CloseNewMessage),
            text::heading(fl!("new-message")),
            widget::horizontal_space(),
        ]
        .spacing(sp.space_xxs)
        .align_y(Alignment::Center),
    );

    // Recipient input with validation indicator
    let recipient_input = widget::text_input(fl!("recipient-placeholder"), params.recipient)
        .on_input(Message::NewMessageRecipientInput)
        .width(Length::Fill)
        .id(widget::Id::new("new-message-recipient"));

    let validation_icon: Element<Message> = if params.recipient.is_empty() {
        widget::Space::new(Length::Fixed(20.0), Length::Fixed(20.0)).into()
    } else if params.recipient_valid {
        widget::icon::from_name("emblem-ok-symbolic")
            .size(20)
            .into()
    } else {
        widget::icon::from_name("dialog-error-symbolic")
            .size(20)
            .into()
    };

    let recipient_row = applet::padded_control(
        row![text::body(fl!("to")), recipient_input, validation_icon,]
            .spacing(sp.space_xxs)
            .align_y(Alignment::Center),
    );

    // Contact suggestions (show if recipient is being typed and we have matches)
    // Each suggestion is a (contact_name, phone_number) tuple, sorted by conversation recency
    let suggestions_section: Element<Message> = if !params.recipient.is_empty()
        && !is_address_valid(params.recipient)
        && !params.contact_suggestions.is_empty()
    {
        let mut suggestions_col = column![].spacing(sp.space_xxxs);
        for (name, phone) in params.contact_suggestions.iter() {
            let contact_row = applet::menu_button(
                row![
                    widget::icon::from_name("contact-new-symbolic").size(20),
                    column![text::body(name.clone()), text::caption(phone.clone()),]
                        .spacing(2),
                ]
                .spacing(sp.space_xxs)
                .align_y(Alignment::Center),
            )
            .on_press(Message::SelectContact(name.clone(), phone.clone()));
            suggestions_col = suggestions_col.push(contact_row);
        }
        widget::container(suggestions_col)
            .padding([0, sp.space_xs as u16])
            .width(Length::Fill)
            .into()
    } else {
        widget::Space::new(Length::Shrink, Length::Shrink).into()
    };

    // Message input
    let message_input = widget::text_input(fl!("type-message"), params.body)
        .on_input(Message::NewMessageBodyInput)
        .width(Length::Fill);

    // Send button
    let send_enabled = params.recipient_valid && !params.body.is_empty() && !params.sending;

    let send_btn = if params.sending {
        widget::button::standard(fl!("sending"))
    } else {
        widget::button::suggested(fl!("send"))
            .leading_icon(widget::icon::from_name("mail-send-symbolic").size(16))
            .on_press_maybe(if send_enabled {
                Some(Message::SendNewMessage)
            } else {
                None
            })
    };

    let send_row = applet::padded_control(
        row![widget::horizontal_space(), send_btn,]
            .spacing(sp.space_xxs)
            .align_y(Alignment::Center),
    );

    column![
        header,
        recipient_row,
        suggestions_section,
        applet::padded_control(message_input),
        send_row,
        widget::vertical_space(),
    ]
    .spacing(sp.space_xxxs)
    .width(Length::Fill)
    .into()
}
