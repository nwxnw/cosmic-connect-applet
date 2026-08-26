//! Subscription for incremental conversation list loading via D-Bus signals.
//!
//! This module provides a subscription that listens for conversationCreated and
//! conversationUpdated signals to provide real-time UI updates as conversations
//! are received from the phone.

use crate::app::Message;
use crate::constants::dbus::RETRY_DELAY_SECS;
use crate::constants::sms::{
    CONVERSATION_LIST_PHONE_WAIT_MS, CONVERSATION_LIST_QUIET_MS, CONVERSATION_LIST_RETRY_THRESHOLD,
    CONVERSATION_LIST_RETRY_WAIT_MS, CONVERSATION_TIMEOUT_CACHED_SECS,
};
use crate::subscriptions::with_dbus_retries;
use futures_util::StreamExt;
use kdeconnect_dbus::plugins::{
    parse_sms_message, ConversationSummary, ConversationsProxy, SmsProxy,
};
use std::collections::HashMap;
use zbus::{Connection, Proxy};

/// Heartbeat interval after sync indicator is dismissed (seconds).
/// Keeps the unfold alive so iced can cancel it when the view closes.
const HEARTBEAT_SLEEP_SECS: u64 = 30;

/// State for conversation list subscription.
#[allow(clippy::large_enum_variant)]
enum ConversationListState {
    Init {
        device_id: String,
    },
    /// Emitting cached conversations before listening for signals
    EmittingCached {
        conn: Connection,
        conversations_proxy: ConversationsProxy<'static>,
        stream: zbus::MessageStream,
        device_id: String,
        pending_conversations: Vec<ConversationSummary>,
        known_conversations: HashMap<i64, i64>,
    },
    Listening {
        #[allow(dead_code)]
        conn: Connection,
        conversations_proxy: ConversationsProxy<'static>,
        stream: zbus::MessageStream,
        device_id: String,
        /// Absolute deadline for the current bootstrap attempt.
        bootstrap_deadline: tokio::time::Instant,
        /// Whether `ConversationSyncComplete` has been emitted (sync spinner dismissed).
        sync_complete_emitted: bool,
        /// Whether this listening session started without cached conversations.
        cold_start: bool,
        /// Tracks the newest timestamp we have emitted per thread.
        known_conversations: HashMap<i64, i64>,
        /// Last time we observed meaningful bootstrap activity.
        last_activity: Option<tokio::time::Instant>,
        /// Number of bootstrap retries already issued.
        retry_count: u8,
    },
    /// Terminal state — stream is finished.
    Done,
}

/// Create a stream that listens for conversation list updates via D-Bus signals.
///
/// This subscription handles incremental conversation loading by:
/// 1. Setting up D-Bus match rules for signals
/// 2. Getting initial cached conversations via activeConversations()
/// 3. Firing requestAllConversationThreads() to trigger phone sync
/// 4. Listening for `conversationCreated`/`conversationUpdated` signals
/// 5. Emitting `Message::ConversationReceived` for each conversation (immediate UI update)
/// 6. Emitting `Message::ConversationSyncComplete` when phone deadline fires (dismisses spinner)
///
/// The subscription runs as long as the SMS view is open. It is cancelled by
/// iced dropping it when `conversation_list_subscription_active` becomes false.
pub fn conversation_list_subscription(
    device_id: String,
) -> impl futures_util::Stream<Item = Message> {
    futures_util::stream::unfold(
        ConversationListState::Init { device_id },
        |state| async move {
            match state {
                ConversationListState::Init { device_id } => {
                    // Connect to D-Bus
                    let conn =
                        match with_dbus_retries("list D-Bus session connect", Connection::session)
                            .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                return Some((
                                    Message::SmsError(format!("D-Bus connection failed: {}", e)),
                                    ConversationListState::Done, // NOT Init - see D.25
                                ));
                            }
                        };

                    // `conn_ref` is `Copy`, so the retry closure below can hand a fresh,
                    // independent future to each attempt without moving `conn` out of its
                    // environment (which would make it `FnOnce`, not `FnMut`).
                    let conn_ref = &conn;

                    // Add match rules for conversation signals
                    let dbus_proxy = match with_dbus_retries("list D-Bus proxy build", move || {
                        zbus::fdo::DBusProxy::new(conn_ref)
                    })
                    .await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            return Some((
                                Message::SmsError(format!("D-Bus proxy failed: {}", e)),
                                ConversationListState::Done, // NOT Init - see D.25
                            ));
                        }
                    };

                    // Subscribe to conversationCreated signals
                    let created_rule = zbus::MatchRule::builder()
                        .msg_type(zbus::message::Type::Signal)
                        .interface("org.kde.kdeconnect.device.conversations")
                        .and_then(|b| b.member("conversationCreated"))
                        .map(|b| b.build());

                    if let Ok(rule) = created_rule {
                        if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                            tracing::warn!("Failed to add conversationCreated match rule: {}", e);
                        } else {
                            tracing::debug!("Added match rule for conversationCreated signals");
                        }
                    }

                    // Subscribe to conversationUpdated signals
                    let updated_rule = zbus::MatchRule::builder()
                        .msg_type(zbus::message::Type::Signal)
                        .interface("org.kde.kdeconnect.device.conversations")
                        .and_then(|b| b.member("conversationUpdated"))
                        .map(|b| b.build());

                    if let Ok(rule) = updated_rule {
                        if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                            tracing::warn!("Failed to add conversationUpdated match rule: {}", e);
                        } else {
                            tracing::debug!("Added match rule for conversationUpdated signals");
                        }
                    }

                    // Subscribe to conversationLoaded signals
                    let loaded_rule = zbus::MatchRule::builder()
                        .msg_type(zbus::message::Type::Signal)
                        .interface("org.kde.kdeconnect.device.conversations")
                        .and_then(|b| b.member("conversationLoaded"))
                        .map(|b| b.build());

                    if let Ok(rule) = loaded_rule {
                        if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                            tracing::warn!("Failed to add conversationLoaded match rule: {}", e);
                        } else {
                            tracing::debug!("Added match rule for conversationLoaded signals");
                        }
                    }

                    // Create message stream BEFORE firing request
                    let stream = zbus::MessageStream::from(&conn);

                    // Build conversations proxy for the device
                    let device_path =
                        format!("{}/devices/{}", kdeconnect_dbus::BASE_PATH, device_id);

                    let conversations_proxy =
                        match with_dbus_retries("list conversations proxy build", || {
                            // `Proxy::new_owned` consumes its arguments, so clone per attempt
                            // rather than moving the captures out of the closure.
                            let conn = conn.clone();
                            let device_path = device_path.clone();
                            async move {
                                Proxy::new_owned(
                                    conn,
                                    kdeconnect_dbus::SERVICE_NAME,
                                    device_path,
                                    "org.kde.kdeconnect.device.conversations",
                                )
                                .await
                                .map(ConversationsProxy::from)
                            }
                        })
                        .await
                        {
                            Ok(p) => p,
                            Err(e) => {
                                return Some((
                                    Message::SmsError(format!(
                                        "Failed to create conversations proxy: {}",
                                        e
                                    )),
                                    ConversationListState::Done, // NOT Init - see D.25
                                ));
                            }
                        };

                    // Get cached conversations first (for immediate display)
                    let mut initial_conversations =
                        fetch_cached_conversations(&conversations_proxy, &device_id).await;
                    let mut known_conversations = HashMap::new();
                    for conversation in &initial_conversations {
                        known_conversations.insert(conversation.thread_id, conversation.timestamp);
                    }

                    // Fire TWO requests (mirrors the pattern from conversation message loading):
                    // 1. SMS plugin's requestAllConversations → sends network packet to phone →
                    //    response goes through addMessages() → populates m_conversations and
                    //    emits conversationCreated signals
                    // 2. Conversations interface's requestAllConversationThreads → may read
                    //    from local store → emits signals for cached conversations
                    //
                    // Without the SMS plugin request, the Conversations interface may only
                    // read from an empty local store and emit no signals.
                    request_conversation_bootstrap(&conn, &device_id, &conversations_proxy).await;

                    let now = tokio::time::Instant::now();

                    // If we have cached data, transition to EmittingCached state
                    if !initial_conversations.is_empty() {
                        tracing::info!(
                            "Emitting {} cached conversations for device {}",
                            initial_conversations.len(),
                            device_id
                        );

                        // Emit the whole cached set in one batch
                        return Some((
                            Message::ConversationsBatchReceived {
                                device_id: device_id.clone(),
                                conversations: std::mem::take(&mut initial_conversations),
                            },
                            ConversationListState::EmittingCached {
                                conn,
                                conversations_proxy,
                                stream,
                                device_id,
                                pending_conversations: initial_conversations,
                                known_conversations,
                            },
                        ));
                    }

                    // No cached data — use longer phone wait (cold start)
                    let phone_deadline =
                        now + tokio::time::Duration::from_millis(CONVERSATION_LIST_PHONE_WAIT_MS);
                    Some((
                        Message::ConversationSyncStarted {
                            device_id: device_id.clone(),
                        },
                        ConversationListState::Listening {
                            conn,
                            conversations_proxy,
                            stream,
                            device_id,
                            bootstrap_deadline: phone_deadline,
                            sync_complete_emitted: false,
                            cold_start: true,
                            known_conversations,
                            last_activity: None,
                            retry_count: 0,
                        },
                    ))
                }
                ConversationListState::EmittingCached {
                    conn,
                    conversations_proxy,
                    stream,
                    device_id,
                    mut pending_conversations,
                    known_conversations,
                } => {
                    // Emit cached batch conversations
                    if !pending_conversations.is_empty() {
                        tracing::debug!(
                            "Emitting cached batch: {} conversations",
                            pending_conversations.len()
                        );
                        return Some((
                            Message::ConversationsBatchReceived {
                                device_id: device_id.clone(),
                                conversations: std::mem::take(&mut pending_conversations),
                            },
                            ConversationListState::EmittingCached {
                                conn,
                                conversations_proxy,
                                stream,
                                device_id,
                                pending_conversations,
                                known_conversations,
                            },
                        ));
                    }

                    // All cached conversations emitted, transition to listening for signals.
                    // Use shorter phone wait since we have cache (warm start).
                    tracing::debug!(
                        "Finished emitting cached conversations, now listening for signals for device {}",
                        device_id
                    );
                    let now = tokio::time::Instant::now();
                    let phone_deadline =
                        now + tokio::time::Duration::from_secs(CONVERSATION_TIMEOUT_CACHED_SECS);
                    Some((
                        Message::ConversationSyncStarted {
                            device_id: device_id.clone(),
                        },
                        ConversationListState::Listening {
                            conn,
                            conversations_proxy,
                            stream,
                            device_id,
                            bootstrap_deadline: phone_deadline,
                            sync_complete_emitted: false,
                            cold_start: false,
                            known_conversations,
                            // Warm start already has cached rows on screen. Leave activity unset
                            // so the cached bootstrap deadline remains the settle condition unless
                            // real post-bootstrap updates arrive.
                            last_activity: None,
                            retry_count: 0,
                        },
                    ))
                }
                ConversationListState::Listening {
                    conn,
                    conversations_proxy,
                    mut stream,
                    device_id,
                    mut bootstrap_deadline,
                    mut sync_complete_emitted,
                    cold_start,
                    mut known_conversations,
                    mut last_activity,
                    mut retry_count,
                } => {
                    loop {
                        let now = tokio::time::Instant::now();

                        if !sync_complete_emitted {
                            if let Some(last) = last_activity {
                                if now.duration_since(last)
                                    >= tokio::time::Duration::from_millis(
                                        CONVERSATION_LIST_QUIET_MS,
                                    )
                                {
                                    tracing::info!(
                                        "Conversation list sync settled for device {} after activity, \
                                         dismissing spinner with {} known conversations",
                                        device_id,
                                        known_conversations.len()
                                    );
                                    sync_complete_emitted = true;
                                    return Some((
                                        Message::ConversationSyncComplete {
                                            device_id: device_id.clone(),
                                        },
                                        ConversationListState::Listening {
                                            conn,
                                            conversations_proxy,
                                            stream,
                                            device_id,
                                            bootstrap_deadline,
                                            sync_complete_emitted,
                                            cold_start,
                                            known_conversations,
                                            last_activity,
                                            retry_count,
                                        },
                                    ));
                                }
                            }

                            if now >= bootstrap_deadline {
                                if cold_start
                                    && retry_count == 0
                                    && known_conversations.len() < CONVERSATION_LIST_RETRY_THRESHOLD
                                {
                                    tracing::info!(
                                        "Conversation list bootstrap for device {} reached hard deadline \
                                         with only {} conversations; retrying once",
                                        device_id,
                                        known_conversations.len()
                                    );
                                    request_conversation_bootstrap(
                                        &conn,
                                        &device_id,
                                        &conversations_proxy,
                                    )
                                    .await;
                                    retry_count += 1;
                                    bootstrap_deadline = tokio::time::Instant::now()
                                        + tokio::time::Duration::from_millis(
                                            CONVERSATION_LIST_RETRY_WAIT_MS,
                                        );
                                    continue;
                                }

                                tracing::info!(
                                    "Conversation list sync: bootstrap deadline reached for device {}, \
                                     dismissing spinner with {} known conversations",
                                    device_id,
                                    known_conversations.len()
                                );
                                sync_complete_emitted = true;
                                return Some((
                                    Message::ConversationSyncComplete {
                                        device_id: device_id.clone(),
                                    },
                                    ConversationListState::Listening {
                                        conn,
                                        conversations_proxy,
                                        stream,
                                        device_id,
                                        bootstrap_deadline,
                                        sync_complete_emitted,
                                        cold_start,
                                        known_conversations,
                                        last_activity,
                                        retry_count,
                                    },
                                ));
                            }
                        }

                        // After bootstrap settles, remain connected and only wake
                        // periodically so iced can cancel the stream cleanly.
                        let sleep_duration = if sync_complete_emitted {
                            tokio::time::Duration::from_secs(HEARTBEAT_SLEEP_SECS)
                        } else {
                            let mut sleep_duration =
                                bootstrap_deadline.saturating_duration_since(now);
                            if let Some(last) = last_activity {
                                let quiet_deadline = last
                                    + tokio::time::Duration::from_millis(
                                        CONVERSATION_LIST_QUIET_MS,
                                    );
                                sleep_duration = sleep_duration
                                    .min(quiet_deadline.saturating_duration_since(now));
                            }
                            sleep_duration
                        };

                        tokio::select! {
                            biased;

                            // Wait for D-Bus signals
                            msg_option = stream.next() => {
                                match msg_option {
                                    Some(Ok(msg)) => {
                                        if msg.header().message_type() == zbus::message::Type::Signal {
                                            if let (Some(interface), Some(member)) =
                                                (msg.header().interface(), msg.header().member())
                                            {
                                                let iface_str = interface.as_str();
                                                let member_str = member.as_str();

                                                // Check if this signal is for our device
                                                let is_our_device = msg.header().path()
                                                    .map(|p| p.as_str().contains(&device_id))
                                                    .unwrap_or(false);

                                                if !is_our_device {
                                                    continue;
                                                }

                                                // Handle conversationCreated signals
                                                if iface_str == "org.kde.kdeconnect.device.conversations"
                                                    && member_str == "conversationCreated"
                                                {
                                                    let body = msg.body();
                                                    if let Ok(value) = body.deserialize::<zbus::zvariant::OwnedValue>() {
                                                        if let Some(sms_msg) = parse_sms_message(&value) {
                                                            if let Some(conversation) =
                                                                remember_signal_conversation(
                                                                    summarize_message(sms_msg),
                                                                    &mut known_conversations,
                                                                )
                                                            {
                                                                last_activity = Some(tokio::time::Instant::now());
                                                                tracing::debug!(
                                                                    "conversationCreated: thread {} for device {}",
                                                                    conversation.thread_id,
                                                                    device_id
                                                                );
                                                                return Some((
                                                                    Message::ConversationReceived {
                                                                        device_id: device_id.clone(),
                                                                        conversation,
                                                                    },
                                                                    ConversationListState::Listening {
                                                                        conn,
                                                                        conversations_proxy,
                                                                        stream,
                                                                        device_id,
                                                                        bootstrap_deadline,
                                                                        sync_complete_emitted,
                                                                        cold_start,
                                                                        known_conversations,
                                                                        last_activity,
                                                                        retry_count,
                                                                    },
                                                                ));
                                                            }
                                                        }
                                                    }
                                                }

                                                // Handle conversationUpdated signals
                                                if iface_str == "org.kde.kdeconnect.device.conversations"
                                                    && member_str == "conversationUpdated"
                                                {
                                                    let body = msg.body();
                                                    if let Ok(value) = body.deserialize::<zbus::zvariant::OwnedValue>() {
                                                        if let Some(sms_msg) = parse_sms_message(&value) {
                                                            if let Some(conversation) =
                                                                remember_signal_conversation(
                                                                    summarize_message(sms_msg),
                                                                    &mut known_conversations,
                                                                )
                                                            {
                                                                last_activity = Some(tokio::time::Instant::now());
                                                                tracing::debug!(
                                                                    "conversationUpdated: thread {} for device {}",
                                                                    conversation.thread_id,
                                                                    device_id
                                                                );
                                                                return Some((
                                                                    Message::ConversationReceived {
                                                                        device_id: device_id.clone(),
                                                                        conversation,
                                                                    },
                                                                    ConversationListState::Listening {
                                                                        conn,
                                                                        conversations_proxy,
                                                                        stream,
                                                                        device_id,
                                                                        bootstrap_deadline,
                                                                        sync_complete_emitted,
                                                                        cold_start,
                                                                        known_conversations,
                                                                        last_activity,
                                                                        retry_count,
                                                                    },
                                                                ));
                                                            }
                                                        }
                                                    }
                                                }

                                                // Handle conversationLoaded signals (progress marker)
                                                if iface_str == "org.kde.kdeconnect.device.conversations"
                                                    && member_str == "conversationLoaded"
                                                {
                                                    tracing::debug!(
                                                        "conversationLoaded signal for device {}",
                                                        device_id
                                                    );
                                                    last_activity = Some(tokio::time::Instant::now());
                                                }
                                            }
                                        }
                                    }
                                    Some(Err(e)) => {
                                        tracing::warn!("D-Bus stream error: {}", e);
                                    }
                                    None => {
                                        // Stream ended (D-Bus connection dropped)
                                        tracing::warn!(
                                            "D-Bus message stream ended for device {}",
                                            device_id
                                        );
                                        tokio::time::sleep(std::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                                        return Some ((
                                            Message::ConversationListStreamEnded { device_id },
                                            ConversationListState::Done,
                                        ));
                                    }
                                }
                            }

                            // Sleep until phone deadline or heartbeat
                            _ = tokio::time::sleep(sleep_duration) => {
                                // Loop back — deadline check at top will handle expiry
                            }
                        }
                    }
                }
                ConversationListState::Done => None,
            }
        },
    )
}

fn summarize_message(sms_msg: kdeconnect_dbus::plugins::SmsMessage) -> ConversationSummary {
    let has_attachments = !sms_msg.attachments.is_empty();
    ConversationSummary {
        thread_id: sms_msg.thread_id,
        addresses: sms_msg.addresses,
        last_message: sms_msg.body,
        timestamp: sms_msg.date,
        unread: !sms_msg.read,
        has_attachments,
        sub_id: sms_msg.sub_id,
    }
}

fn remember_signal_conversation(
    conversation: ConversationSummary,
    known_conversations: &mut HashMap<i64, i64>,
) -> Option<ConversationSummary> {
    let known = known_conversations.get(&conversation.thread_id).copied();
    if known.is_none() || known < Some(conversation.timestamp) {
        known_conversations.insert(conversation.thread_id, conversation.timestamp);
        Some(conversation)
    } else {
        None
    }
}

async fn fetch_cached_conversations(
    conversations_proxy: &ConversationsProxy<'_>,
    device_id: &str,
) -> Vec<ConversationSummary> {
    let cached = match conversations_proxy.active_conversations().await {
        Ok(cached) => cached,
        Err(e) => {
            tracing::warn!(
                "Failed to fetch cached conversations for {}: {}",
                device_id,
                e
            );
            return Vec::new();
        }
    };

    tracing::debug!(
        "Fetched {} cached conversation values for device {}",
        cached.len(),
        device_id
    );

    let mut conversations = Vec::new();
    for value in &cached {
        if let Some(sms_msg) = parse_sms_message(value) {
            conversations.push(summarize_message(sms_msg));
        }
    }

    conversations.sort_by_key(|c| std::cmp::Reverse(c.timestamp));
    let mut seen = std::collections::HashSet::new();
    conversations.retain(|conversation| seen.insert(conversation.thread_id));
    conversations
}

async fn request_conversation_bootstrap(
    conn: &Connection,
    device_id: &str,
    conversations_proxy: &ConversationsProxy<'_>,
) {
    let sms_path = format!("{}/devices/{}/sms", kdeconnect_dbus::BASE_PATH, device_id);
    let sms_builder = match SmsProxy::builder(conn).path(sms_path.as_str()) {
        Ok(builder) => builder,
        Err(_) => {
            tracing::warn!(
                "Failed to build SMS proxy path for {} (non-fatal)",
                device_id
            );
            return;
        }
    };
    match sms_builder.build().await {
        Ok(sms_proxy) => {
            if let Err(e) = sms_proxy.request_all_conversations().await {
                tracing::warn!(
                    "SMS plugin requestAllConversations failed for {} (non-fatal): {}",
                    device_id,
                    e
                );
            } else {
                tracing::debug!(
                    "SMS plugin requestAllConversations fired for device {} (cache priming)",
                    device_id
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create SMS proxy for {} (non-fatal): {}",
                device_id,
                e
            );
        }
    }

    tracing::debug!(
        "Firing requestAllConversationThreads for device {}",
        device_id
    );
    if let Err(e) = conversations_proxy.request_all_conversation_threads().await {
        tracing::warn!("Failed to request conversation threads: {}", e);
    }
}
