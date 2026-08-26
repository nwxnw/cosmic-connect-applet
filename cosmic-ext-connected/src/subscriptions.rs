//! D-Bus signal subscriptions for real-time updates from KDE Connect.

use crate::app::Message;
use crate::constants::dbus::RETRY_DELAY_SECS;
use crate::constants::sms::{
    CONVERSATION_RETRY_WAIT_MS, MAX_MESSAGE_LOAD_RETRIES, MESSAGE_SUBSCRIPTION_TIMEOUT_SECS,
    PHONE_RESPONSE_TIMEOUT_MS,
};
use crate::notifications::{
    should_show_call_notification, should_show_file_notification, should_show_sms_notification,
};
use futures_util::StreamExt;
use kdeconnect_dbus::plugins::{parse_sms_message, MessageType};
use kdeconnect_dbus::DeviceProxy;
use zbus::Connection;

/// Retry a fallible D-Bus construction step `MAX_MESSAGE_LOAD_RETRIES` times with
/// `RETRY_DELAY_SECS` between attempts. Generalises D.17's inline loop
/// (`request_conversation`, below) to the construction steps it does not reach.
///
/// The caller terminates with `Done` on `Err` - it must NOT re-enter `Init`, or the
/// no-sleep failure paths become a hot loop (see D.25 ownership rule).
pub(crate) async fn with_dbus_retries<T, E, F, Fut>(label: &str, mut op: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt: u8 = 0;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                attempt += 1;
                if attempt >= MAX_MESSAGE_LOAD_RETRIES {
                    tracing::warn!("{} failed after {} attempts: {}", label, attempt, e);
                    return Err(e);
                }
                tracing::warn!(
                    "{} failed (attempt {}/{}), retrying in {}s: {}",
                    label,
                    attempt,
                    MAX_MESSAGE_LOAD_RETRIES,
                    RETRY_DELAY_SECS,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
            }
        }
    }
}

/// Re-issue `requestConversation` on the Conversations interface as part of the
/// first-open truncation recovery path. The daemon worker treats `[start, end)`
/// as indices into the local store sorted newest-first.
///
/// `start` is pinned to **0** and only `end` varies. Requesting `[numKnown, …)` -
/// the contract this used to follow - segfaults kdeconnectd 23.08.5 whenever the
/// daemon's cache holds fewer than `numKnown` messages, which is routine after a
/// daemon restart. Serving from 0 returns the same messages newest-first and can
/// never index past the end. See D.29.
async fn fire_retry_request(
    conn: &Connection,
    device_id: &str,
    thread_id: i64,
    end: i32,
) -> Result<(), String> {
    let device_path = format!("{}/devices/{}", kdeconnect_dbus::BASE_PATH, device_id);
    let proxy = kdeconnect_dbus::plugins::ConversationsProxy::builder(conn)
        .path(device_path.as_str())
        .map_err(|e| format!("path: {}", e))?
        .build()
        .await
        .map_err(|e| format!("build: {}", e))?;
    proxy
        .request_conversation(thread_id, 0, end)
        .await
        .map_err(|e| format!("call: {}", e))
}

/// State for D-Bus signal subscription.
#[allow(clippy::large_enum_variant)]
enum DbusSubscriptionState {
    Init,
    Listening {
        #[allow(dead_code)]
        conn: Connection,
        stream: zbus::MessageStream,
    },
}

/// Create a stream that listens for D-Bus signals from KDE Connect.
pub fn dbus_signal_subscription() -> impl futures_util::Stream<Item = Message> {
    futures_util::stream::unfold(DbusSubscriptionState::Init, |state| async move {
        match state {
            DbusSubscriptionState::Init => {
                // Connect to D-Bus
                let conn = match Connection::session().await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("Failed to connect to D-Bus for signals: {}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                        return Some((
                            Message::Error("D-Bus connection failed".to_string()),
                            DbusSubscriptionState::Init,
                        ));
                    }
                };

                // Add match rule to receive KDE Connect signals
                let dbus_proxy = match zbus::fdo::DBusProxy::new(&conn).await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("Failed to create DBus proxy: {}", e);
                        return Some((
                            Message::Error("D-Bus proxy failed".to_string()),
                            DbusSubscriptionState::Init,
                        ));
                    }
                };

                // Subscribe to all signals from KDE Connect daemon
                if let Ok(rule) = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .sender("org.kde.kdeconnect.daemon")
                    .map(|b| b.build())
                {
                    if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                        tracing::warn!("Failed to add match rule: {}", e);
                    } else {
                        tracing::debug!("Added match rule for kdeconnect daemon signals");
                    }
                }

                // Also subscribe to property changes (for battery, pairing state, etc.)
                if let Ok(rule) = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .interface("org.freedesktop.DBus.Properties")
                    .map(|b| b.build())
                {
                    if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                        tracing::warn!("Failed to add properties match rule: {}", e);
                    } else {
                        tracing::debug!("Added match rule for property change signals");
                    }
                }

                // Subscribe to share plugin signals for file notifications
                if let Ok(rule) = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .interface("org.kde.kdeconnect.device.share")
                    .map(|b| b.build())
                {
                    if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                        tracing::warn!("Failed to add share match rule: {}", e);
                    } else {
                        tracing::debug!("Added match rule for share signals");
                    }
                } else {
                    tracing::warn!("Failed to build share match rule");
                }

                tracing::debug!("D-Bus signal subscription started");

                // Create message stream
                let stream = zbus::MessageStream::from(&conn);

                Some((
                    Message::DbusSignalReceived,
                    DbusSubscriptionState::Listening { conn, stream },
                ))
            }
            DbusSubscriptionState::Listening { conn, mut stream } => {
                // Wait for relevant signals - be selective to avoid excessive refreshes
                loop {
                    match stream.next().await {
                        Some(Ok(msg)) => {
                            if msg.header().message_type() == zbus::message::Type::Signal {
                                if let (Some(interface), Some(member)) =
                                    (msg.header().interface(), msg.header().member())
                                {
                                    let iface_str = interface.as_str();
                                    let member_str = member.as_str();

                                    // Handle share signals for file notifications
                                    if iface_str == "org.kde.kdeconnect.device.share"
                                        && member_str == "shareReceived"
                                    {
                                        // Extract device ID from path
                                        if let Some(path) = msg.header().path() {
                                            let path_str = path.as_str();
                                            if let Some(rest) = path_str
                                                .strip_prefix("/modules/kdeconnect/devices/")
                                            {
                                                let device_id = rest
                                                    .split('/')
                                                    .next()
                                                    .unwrap_or(rest)
                                                    .to_string();

                                                // Parse the signal body
                                                let body = msg.body();
                                                if let Ok((file_url,)) =
                                                    body.deserialize::<(String,)>()
                                                {
                                                    // Cross-process deduplication via file lock
                                                    // KDE Connect sends 3 duplicate signals per file transfer
                                                    // and COSMIC spawns multiple applet processes
                                                    if !should_show_file_notification(&file_url) {
                                                        continue;
                                                    }

                                                    let file_name = file_url
                                                        .strip_prefix("file://")
                                                        .unwrap_or(&file_url)
                                                        .rsplit('/')
                                                        .next()
                                                        .unwrap_or("file")
                                                        .to_string();

                                                    return Some((
                                                        Message::FileReceived {
                                                            device_name: device_id,
                                                            file_url,
                                                            file_name,
                                                        },
                                                        DbusSubscriptionState::Listening {
                                                            conn,
                                                            stream,
                                                        },
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                    // Pairing failure carries its reason as a string.
                                    // The daemon emits pairStateChanged ~20µs later
                                    // (core/device.cpp:257-261), so pair state already
                                    // corrects itself — this is only the reason.
                                    if iface_str == "org.kde.kdeconnect.device"
                                        && member_str == "pairingFailed"
                                    {
                                        if let Some(path) = msg.header().path() {
                                            if let Some(rest) = path
                                                .as_str()
                                                .strip_prefix("/modules/kdeconnect/devices/")
                                            {
                                                let device_id = rest
                                                    .split('/')
                                                    .next()
                                                    .unwrap_or(rest)
                                                    .to_string();
                                                if let Ok((error,)) =
                                                    msg.body().deserialize::<(String,)>()
                                                {
                                                    return Some((
                                                        Message::PairingFailed { device_id, error },
                                                        DbusSubscriptionState::Listening {
                                                            conn,
                                                            stream,
                                                        },
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                    // Only trigger refresh on specific device-related signals.
                                    // Signal names match upstream KDE Connect
                                    // (see core/daemon.h and core/device.h). The previous
                                    // device-level names trustedChanged / pairingRequest /
                                    // hasPairingRequestsChanged do not exist upstream — pair
                                    // state is emitted as pairStateChanged(int).
                                    let is_relevant = match iface_str {
                                        // Daemon signals for device discovery and pair-state aggregate
                                        "org.kde.kdeconnect.daemon" => matches!(
                                            member_str,
                                            "deviceAdded"
                                                | "deviceRemoved"
                                                | "deviceVisibilityChanged"
                                                | "announcedNameChanged"
                                                | "pairingRequestsChanged"
                                        ),
                                        // Device signals for reachability and pair state
                                        "org.kde.kdeconnect.device" => matches!(
                                            member_str,
                                            "reachableChanged" | "pairStateChanged"
                                        ),
                                        // Battery and notification plugin signals
                                        "org.kde.kdeconnect.device.battery" => true,
                                        "org.kde.kdeconnect.device.notifications" => true,
                                        // Property changes for any kdeconnect interface
                                        "org.freedesktop.DBus.Properties" => {
                                            member_str == "PropertiesChanged"
                                        }
                                        _ => false,
                                    };

                                    if is_relevant {
                                        tracing::debug!("D-Bus signal: {}.{}", interface, member);
                                        return Some((
                                            Message::DbusSignalReceived,
                                            DbusSubscriptionState::Listening { conn, stream },
                                        ));
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!("D-Bus stream error: {}", e);
                        }
                        None => {
                            tracing::warn!("D-Bus stream ended, reconnecting...");
                            return Some((
                                Message::DbusSignalReceived,
                                DbusSubscriptionState::Init,
                            ));
                        }
                    }
                }
            }
        }
    })
}

/// State for SMS notification subscription.
#[allow(clippy::large_enum_variant)]
enum SmsSubscriptionState {
    Init,
    Listening {
        #[allow(dead_code)]
        conn: Connection,
        stream: zbus::MessageStream,
    },
}

/// Create a stream that listens for incoming SMS messages via D-Bus signals.
pub fn sms_notification_subscription() -> impl futures_util::Stream<Item = Message> {
    futures_util::stream::unfold(SmsSubscriptionState::Init, |state| async move {
        match state {
            SmsSubscriptionState::Init => {
                // Connect to D-Bus
                let conn = match Connection::session().await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("Failed to connect to D-Bus for SMS signals: {}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                        return Some((
                            Message::Error("D-Bus connection failed for SMS".to_string()),
                            SmsSubscriptionState::Init,
                        ));
                    }
                };

                // Add match rule for conversationUpdated signals
                let dbus_proxy = match zbus::fdo::DBusProxy::new(&conn).await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("Failed to create DBus proxy for SMS: {}", e);
                        return Some((
                            Message::Error("D-Bus proxy failed for SMS".to_string()),
                            SmsSubscriptionState::Init,
                        ));
                    }
                };

                // Subscribe to conversation signals from KDE Connect
                // Note: interface() returns Result, so we chain with and_then for member()
                let rule_result = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .interface("org.kde.kdeconnect.device.conversations")
                    .and_then(|b| b.member("conversationUpdated"))
                    .map(|b| b.build());

                if let Ok(rule) = rule_result {
                    if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                        tracing::warn!("Failed to add SMS match rule: {}", e);
                    } else {
                        tracing::debug!("Added match rule for SMS conversationUpdated signals");
                    }
                }

                tracing::debug!("SMS notification subscription started");

                // Create message stream
                let stream = zbus::MessageStream::from(&conn);

                // Don't emit a message on init, just move to listening state
                Some((
                    Message::RefreshDevices, // Trigger a refresh to pick up any pending state
                    SmsSubscriptionState::Listening { conn, stream },
                ))
            }
            SmsSubscriptionState::Listening { conn, mut stream } => {
                // Wait for conversationUpdated signals
                loop {
                    match stream.next().await {
                        Some(Ok(msg)) => {
                            if msg.header().message_type() == zbus::message::Type::Signal {
                                if let (Some(interface), Some(member)) =
                                    (msg.header().interface(), msg.header().member())
                                {
                                    let iface_str = interface.as_str();
                                    let member_str = member.as_str();

                                    // Only process conversationUpdated signals
                                    if iface_str == "org.kde.kdeconnect.device.conversations"
                                        && member_str == "conversationUpdated"
                                    {
                                        // Extract device ID from the path
                                        // Path format: /modules/kdeconnect/devices/{device_id}
                                        if let Some(path) = msg.header().path() {
                                            let path_str = path.as_str();
                                            if let Some(device_id) = path_str
                                                .strip_prefix("/modules/kdeconnect/devices/")
                                            {
                                                // Extract the device_id (may contain more path components)
                                                let device_id = device_id
                                                    .split('/')
                                                    .next()
                                                    .unwrap_or(device_id);

                                                // Parse the message body to get SMS data
                                                let body = msg.body();
                                                if let Ok(value) =
                                                    body.deserialize::<zbus::zvariant::OwnedValue>()
                                                {
                                                    if let Some(sms_msg) = parse_sms_message(&value)
                                                    {
                                                        // Only notify for received messages
                                                        // Standard Android SMS semantics: Inbox (1) = received from others
                                                        if sms_msg.message_type
                                                            == MessageType::Inbox
                                                        {
                                                            // Cross-process deduplication:
                                                            // COSMIC spawns multiple applet processes,
                                                            // so use file-based locking to ensure only one shows the notification
                                                            if !should_show_sms_notification(
                                                                sms_msg.thread_id,
                                                                sms_msg.date,
                                                            ) {
                                                                continue;
                                                            }

                                                            tracing::debug!(
                                                                "SMS received from {} on device {}",
                                                                sms_msg.primary_address(),
                                                                device_id
                                                            );
                                                            return Some((
                                                                Message::SmsNotificationReceived(
                                                                    device_id.to_string(),
                                                                    sms_msg,
                                                                ),
                                                                SmsSubscriptionState::Listening {
                                                                    conn,
                                                                    stream,
                                                                },
                                                            ));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!("D-Bus SMS stream error: {}", e);
                        }
                        None => {
                            tracing::warn!("D-Bus SMS stream ended, reconnecting...");
                            return Some((Message::RefreshDevices, SmsSubscriptionState::Init));
                        }
                    }
                }
            }
        }
    })
}

/// State for call notification subscription.
#[allow(clippy::large_enum_variant)]
enum CallSubscriptionState {
    Init,
    Listening {
        conn: Connection,
        stream: zbus::MessageStream,
    },
}

/// Create a stream that listens for incoming/missed calls via D-Bus signals.
pub fn call_notification_subscription() -> impl futures_util::Stream<Item = Message> {
    futures_util::stream::unfold(CallSubscriptionState::Init, |state| async move {
        match state {
            CallSubscriptionState::Init => {
                // Connect to D-Bus
                let conn = match Connection::session().await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("Failed to connect to D-Bus for call signals: {}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                        return Some((
                            Message::Error("D-Bus connection failed for calls".to_string()),
                            CallSubscriptionState::Init,
                        ));
                    }
                };

                // Create DBus proxy for adding match rules
                let dbus_proxy = match zbus::fdo::DBusProxy::new(&conn).await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("Failed to create DBus proxy for calls: {}", e);
                        return Some((
                            Message::Error("D-Bus proxy failed for calls".to_string()),
                            CallSubscriptionState::Init,
                        ));
                    }
                };

                // Subscribe to telephony callReceived signals
                let rule_result = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .interface("org.kde.kdeconnect.device.telephony")
                    .and_then(|b| b.member("callReceived"))
                    .map(|b| b.build());

                if let Ok(rule) = rule_result {
                    if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                        tracing::warn!("Failed to add call match rule: {}", e);
                    } else {
                        tracing::debug!("Added match rule for telephony callReceived signals");
                    }
                }

                tracing::debug!("Call notification subscription started");

                // Create message stream
                let stream = zbus::MessageStream::from(&conn);

                Some((
                    Message::RefreshDevices,
                    CallSubscriptionState::Listening { conn, stream },
                ))
            }
            CallSubscriptionState::Listening { conn, mut stream } => {
                // Wait for callReceived signals
                loop {
                    match stream.next().await {
                        Some(Ok(msg)) => {
                            if msg.header().message_type() == zbus::message::Type::Signal {
                                if let (Some(interface), Some(member)) =
                                    (msg.header().interface(), msg.header().member())
                                {
                                    let iface_str = interface.as_str();
                                    let member_str = member.as_str();

                                    // Only process callReceived signals from telephony
                                    if iface_str == "org.kde.kdeconnect.device.telephony"
                                        && member_str == "callReceived"
                                    {
                                        // Extract device ID from the path
                                        // Path format: /modules/kdeconnect/devices/{device_id}/telephony
                                        if let Some(path) = msg.header().path() {
                                            let path_str = path.as_str();
                                            if let Some(rest) = path_str
                                                .strip_prefix("/modules/kdeconnect/devices/")
                                            {
                                                let device_id =
                                                    rest.split('/').next().unwrap_or(rest);

                                                // Parse the signal arguments: (event, phone_number, contact_name)
                                                let body = msg.body();
                                                if let Ok((event, phone_number, contact_name)) =
                                                    body.deserialize::<(String, String, String)>()
                                                {
                                                    // Cross-process deduplication:
                                                    // COSMIC spawns multiple applet processes,
                                                    // so use file-based locking to ensure only one shows the notification
                                                    if !should_show_call_notification(
                                                        &event,
                                                        &phone_number,
                                                    ) {
                                                        continue;
                                                    }

                                                    tracing::debug!(
                                                        "Call signal: {} from {} ({}) on device {}",
                                                        event,
                                                        contact_name,
                                                        phone_number,
                                                        device_id
                                                    );

                                                    // Get device name from D-Bus
                                                    let device_name =
                                                        match DeviceProxy::builder(&conn)
                                                            .path(format!(
                                                                "{}/devices/{}",
                                                                kdeconnect_dbus::BASE_PATH,
                                                                device_id
                                                            ))
                                                            .ok()
                                                            .map(|b| b.build())
                                                        {
                                                            Some(fut) => match fut.await {
                                                                Ok(proxy) => proxy
                                                                    .name()
                                                                    .await
                                                                    .unwrap_or_else(|_| {
                                                                        device_id.to_string()
                                                                    }),
                                                                Err(_) => device_id.to_string(),
                                                            },
                                                            None => device_id.to_string(),
                                                        };

                                                    return Some((
                                                        Message::CallNotification {
                                                            device_name,
                                                            event,
                                                            phone_number,
                                                            contact_name,
                                                        },
                                                        CallSubscriptionState::Listening {
                                                            conn,
                                                            stream,
                                                        },
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!("D-Bus call stream error: {}", e);
                        }
                        None => {
                            tracing::warn!("D-Bus call stream ended, reconnecting...");
                            return Some((Message::RefreshDevices, CallSubscriptionState::Init));
                        }
                    }
                }
            }
        }
    })
}

/// State for conversation message subscription (incremental message loading).
#[allow(clippy::large_enum_variant)]
enum ConversationMessageState {
    Init {
        thread_id: i64,
        device_id: String,
        messages_per_page: u32,
    },
    Listening {
        conn: Connection,
        stream: zbus::MessageStream,
        thread_id: i64,
        device_id: String,
        messages_per_page: u32,
        /// Set when `conversationLoaded` arrives; enables phone_deadline.
        local_store_done: bool,
        /// Total message count from `conversationLoaded` signal.
        total_message_count: Option<u64>,
        /// Deadline for phone to START responding. Set when `conversationLoaded` arrives.
        /// When it fires, `ConversationLoadComplete` is emitted once and the subscription
        /// continues listening with a heartbeat sleep.
        phone_deadline: Option<tokio::time::Instant>,
        /// Whether `ConversationLoadComplete` has been emitted. Once true, the subscription
        /// continues silently with a heartbeat sleep until iced drops it.
        load_complete_emitted: bool,
        /// Count of `conversationUpdated` signals forwarded for this thread. Used to
        /// detect the first-open truncation case (daemon's local store had only a
        /// single message when the Conversations worker ran) at phone_deadline expiry.
        received_message_count: usize,
        /// Whether the one-shot truncation-recovery retry has fired. Bounds retries
        /// to one across both triggers (`docs/SMS.md`, "first-open truncation").
        retry_attempted: bool,
    },
    /// Terminal state - subscription is complete
    Done,
}

/// Heartbeat interval after initial load completes (seconds).
const MESSAGE_HEARTBEAT_SLEEP_SECS: u64 = 30;

/// Create a stream that listens for conversation messages during loading.
///
/// This subscription handles incremental message loading by:
/// 1. Setting up D-Bus match rules for signals
/// 2. Firing the request_conversation D-Bus call (AFTER rules are set up)
/// 3. Listening for `conversationUpdated` signals (individual messages)
/// 4. Emitting `ConversationLoadComplete` when phone deadline fires (initial load done)
/// 5. Continuing to listen for new messages until the conversation is closed
///
/// The subscription runs as long as the conversation is open. It is canceled by
/// iced dropping it when `conversation_load_active` becomes false (CloseConversation).
///
/// The request is fired from within the subscription to avoid race conditions
/// where signals arrive before we're ready to receive them.
pub fn conversation_message_subscription(
    thread_id: i64,
    device_id: String,
    messages_per_page: u32,
) -> impl futures_util::Stream<Item = Message> {
    futures_util::stream::unfold(
        ConversationMessageState::Init {
            thread_id,
            device_id,
            messages_per_page,
        },
        |state| async move {
            match state {
                ConversationMessageState::Init {
                    thread_id,
                    device_id,
                    messages_per_page,
                } => {
                    // Connect to D-Bus
                    let conn = match with_dbus_retries(
                        "thread D-Bus session connect",
                        Connection::session,
                    )
                    .await
                    {
                        Ok(c) => c,
                        Err(_) => {
                            return Some((
                                Message::SmsError(
                                    "D-Bus connection failed for conversation".to_string(),
                                ),
                                ConversationMessageState::Done, // NOT Init - see D.25
                            ));
                        }
                    };

                    // `conn_ref` is `Copy`, so the retry closures below can hand a fresh,
                    // independent future to each attempt without moving `conn` out of their
                    // environment (which would make them `FnOnce`, not `FnMut`).
                    let conn_ref = &conn;

                    // Add match rule for conversationUpdated signals
                    let dbus_proxy =
                        match with_dbus_retries("thread D-Bus proxy build", move || {
                            zbus::fdo::DBusProxy::new(conn_ref)
                        })
                        .await
                        {
                            Ok(p) => p,
                            Err(_) => {
                                return Some((
                                    Message::SmsError(
                                        "D-Bus proxy failed for conversation".to_string(),
                                    ),
                                    ConversationMessageState::Done, // NOT Init - see D.25
                                ));
                            }
                        };

                    // Subscribe to conversationUpdated signals (individual messages)
                    let updated_rule = zbus::MatchRule::builder()
                        .msg_type(zbus::message::Type::Signal)
                        .interface("org.kde.kdeconnect.device.conversations")
                        .and_then(|b| b.member("conversationUpdated"))
                        .map(|b| b.build());

                    if let Ok(rule) = updated_rule {
                        if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                            tracing::warn!("Failed to add conversationUpdated match rule: {}", e);
                        } else {
                            tracing::debug!(
                                "Added match rule for conversation {} message signals",
                                thread_id
                            );
                        }
                    }

                    // Subscribe to conversationLoaded signals (completion marker)
                    let loaded_rule = zbus::MatchRule::builder()
                        .msg_type(zbus::message::Type::Signal)
                        .interface("org.kde.kdeconnect.device.conversations")
                        .and_then(|b| b.member("conversationLoaded"))
                        .map(|b| b.build());

                    if let Ok(rule) = loaded_rule {
                        if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                            tracing::warn!("Failed to add conversationLoaded match rule: {}", e);
                        } else {
                            tracing::debug!(
                                "Added match rule for conversation {} loaded signal",
                                thread_id
                            );
                        }
                    }

                    // Create message stream BEFORE firing request
                    let stream = zbus::MessageStream::from(&conn);

                    // NOW fire D-Bus requests - after match rules are set up
                    // This ensures we don't miss any signals
                    let device_path =
                        format!("{}/devices/{}", kdeconnect_dbus::BASE_PATH, device_id);

                    // Fire TWO requests:
                    // 1. SMS plugin's requestConversation → sends network packet to phone →
                    //    response goes through addMessages() → populates m_conversations
                    //    (required for replyToConversation to look up addresses)
                    // 2. Conversations interface's requestConversation → reads from local
                    //    store via RequestConversationWorker → emits per-message signals
                    //    (required for our subscription to receive all messages)
                    //
                    // The SMS plugin request primes the daemon cache; the Conversations
                    // request provides the per-message signals for UI display.
                    let sms_path =
                        format!("{}/devices/{}/sms", kdeconnect_dbus::BASE_PATH, device_id);

                    // Fire SMS plugin request first (cache priming, async - phone responds later)
                    match kdeconnect_dbus::plugins::SmsProxy::builder(&conn)
                        .path(sms_path.as_str())
                        .ok()
                        .map(|b| b.build())
                    {
                        Some(fut) => match fut.await {
                            Ok(sms_proxy) => {
                                if let Err(e) = sms_proxy
                                    .request_conversation(thread_id, 0, messages_per_page as i64)
                                    .await
                                {
                                    tracing::warn!(
                                        "SMS plugin request_conversation failed (non-fatal): {}",
                                        e
                                    );
                                } else {
                                    tracing::debug!(
                                        "SMS plugin request_conversation fired for thread {} (cache priming)",
                                        thread_id
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to create SMS proxy (non-fatal): {}", e);
                            }
                        },
                        None => {
                            tracing::warn!("Failed to build SMS proxy path (non-fatal)");
                        }
                    }

                    // Fire Conversations interface request (provides per-message signals)
                    let device_path_str = device_path.as_str();
                    let conversations_proxy = match with_dbus_retries(
                        "thread conversations proxy build",
                        move || async move {
                            kdeconnect_dbus::plugins::ConversationsProxy::builder(conn_ref)
                                .path(device_path_str)?
                                .build()
                                .await
                        },
                    )
                    .await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            return Some((
                                Message::SmsError(format!(
                                    "Failed to create conversations proxy: {}",
                                    e
                                )),
                                ConversationMessageState::Done, //NOT init - See D.25
                            ));
                        }
                    };
                    tracing::debug!(
                        "Firing request_conversation for thread {} (messages 0-{})",
                        thread_id,
                        messages_per_page
                    );
                    let mut attempt: u8 = 0;
                    loop {
                        match conversations_proxy
                            .request_conversation(thread_id, 0, messages_per_page as i32)
                            .await
                        {
                            Ok(()) => break,
                            Err(e) => {
                                attempt += 1;
                                if attempt >= MAX_MESSAGE_LOAD_RETRIES {
                                    tracing::warn!(
                                                    "request_conversation for thread {} failed after {} attempts: {}",
                                                    thread_id, attempt, e
                                                );
                                    return Some((
                                        Message::SmsError(format!(
                                            "Failed to request conversation after {} attempts:{}",
                                            attempt, e
                                        )),
                                        ConversationMessageState::Done, //give up gracefully
                                    ));
                                }
                                tracing::warn!(
                                                "request_conversation for thread {} failed (attempt {}/{}), retrying in  {}s: {}",
                                                thread_id, attempt, MAX_MESSAGE_LOAD_RETRIES, RETRY_DELAY_SECS, e
                                            );
                                tokio::time::sleep(std::time::Duration::from_secs(
                                    RETRY_DELAY_SECS,
                                ))
                                .await;
                            }
                        }
                    }
                    tracing::info!(
                        "Conversation {} request sent, listening for signals",
                        thread_id
                    );

                    // Move to listening state, emit started message
                    Some((
                        Message::ConversationLoadStarted { thread_id },
                        ConversationMessageState::Listening {
                            conn,
                            stream,
                            thread_id,
                            device_id,
                            messages_per_page,
                            local_store_done: false,
                            total_message_count: None,
                            phone_deadline: None,
                            load_complete_emitted: false,
                            received_message_count: 0,
                            retry_attempted: false,
                        },
                    ))
                }
                ConversationMessageState::Listening {
                    conn,
                    mut stream,
                    thread_id,
                    device_id,
                    messages_per_page,
                    mut local_store_done,
                    mut total_message_count,
                    mut phone_deadline,
                    mut load_complete_emitted,
                    mut received_message_count,
                    mut retry_attempted,
                } => {
                    // Two-phase loading, then long-lived listening:
                    //
                    // Phase 1 (before conversationLoaded): Local store signals arrive.
                    //   Hard timeout (MESSAGE_SUBSCRIPTION_TIMEOUT_SECS) as safety net
                    //   in case conversationLoaded never fires.
                    //
                    // Phase 2 (after conversationLoaded): phone_deadline (8s) waits for
                    //   the phone to respond. When it fires, ConversationLoadComplete is
                    //   emitted to signal initial load is done.
                    //
                    // Phase 3 (after load complete): Subscription continues silently with
                    //   a heartbeat sleep, catching new messages (including sent echoes)
                    //   until iced drops it on CloseConversation.
                    let local_store_timeout =
                        std::time::Duration::from_secs(MESSAGE_SUBSCRIPTION_TIMEOUT_SECS);
                    let phone_wait = std::time::Duration::from_millis(PHONE_RESPONSE_TIMEOUT_MS);
                    // Hard deadline only for local store phase (before conversationLoaded)
                    let local_store_deadline = if !local_store_done && !load_complete_emitted {
                        Some(tokio::time::Instant::now() + local_store_timeout)
                    } else {
                        None
                    };

                    loop {
                        let now = tokio::time::Instant::now();

                        // Phase-1 completion: full cached page already delivered.
                        //
                        // The daemon's RequestConversationWorker serves messages from its
                        // local store and, when it satisfies the whole request from cache
                        // (`numHandled >= howMany`), returns WITHOUT emitting
                        // `conversationLoaded` (docs/SMS.md). Phase 1 then has no signal-driven
                        // completion and would sit on the MESSAGE_SUBSCRIPTION_TIMEOUT_SECS hard
                        // timeout. Detect it locally: a full page of per-message signals means
                        // the initial load is done. The subscription keeps running (phase 3) to
                        // catch later phone data, so completing now loses nothing.
                        if !load_complete_emitted
                            && !local_store_done
                            && received_message_count >= messages_per_page as usize
                        {
                            tracing::info!(
                                "Subscription: thread {} received full page ({} messages, \
                                conversationLoaded withheld), signaling load complete",
                                thread_id,
                                received_message_count
                            );
                            local_store_done = true;
                            load_complete_emitted = true;
                            return Some((
                                Message::ConversationLoadComplete {
                                    thread_id,
                                    total_count: total_message_count.unwrap_or(0),
                                },
                                ConversationMessageState::Listening {
                                    conn,
                                    stream,
                                    thread_id,
                                    device_id,
                                    messages_per_page,
                                    local_store_done,
                                    total_message_count,
                                    phone_deadline,
                                    load_complete_emitted,
                                    received_message_count,
                                    retry_attempted,
                                },
                            ));
                        }

                        // Check local store hard timeout (Phase 1 safety net)
                        if let Some(lsd) = local_store_deadline {
                            if !local_store_done && !load_complete_emitted && now >= lsd {
                                tracing::info!(
                                    "Subscription: local store timeout for thread {} \
                                     (conversationLoaded never received), signaling load complete",
                                    thread_id
                                );
                                load_complete_emitted = true;
                                return Some((
                                    Message::ConversationLoadComplete {
                                        thread_id,
                                        total_count: total_message_count.unwrap_or(0),
                                    },
                                    ConversationMessageState::Listening {
                                        conn,
                                        stream,
                                        thread_id,
                                        device_id,
                                        messages_per_page,
                                        local_store_done,
                                        total_message_count,
                                        phone_deadline,
                                        load_complete_emitted,
                                        received_message_count,
                                        retry_attempted,
                                    },
                                ));
                            }
                        }

                        // Check phone deadline (Phase 2 → Phase 3 transition).
                        //
                        // Fallback trigger (`docs/SMS.md`, "first-open truncation"):
                        // if the phone deadline expires after the local store
                        // delivered at most one message, the daemon's first-open
                        // Conversations worker likely finished before the
                        // phone-supplied messages were written to the local store.
                        // Re-issue requestConversation on the Conversations
                        // interface — by now the local store contains the phone
                        // data — and let those signals stream through normally.
                        // Bound to one attempt.
                        if !load_complete_emitted {
                            if let Some(pd) = phone_deadline {
                                if now >= pd {
                                    if !retry_attempted && received_message_count <= 1 {
                                        tracing::info!(
                                            "Subscription: thread {} truncation suspected \
                                             ({} message(s) after phone deadline), retrying \
                                             Conversations interface read (fallback path)",
                                            thread_id,
                                            received_message_count
                                        );
                                        let end = (received_message_count
                                            + messages_per_page as usize)
                                            as i32;
                                        match fire_retry_request(&conn, &device_id, thread_id, end)
                                            .await
                                        {
                                            Ok(()) => {
                                                tracing::info!(
                                                    "Retry request_conversation fired for thread {}",
                                                    thread_id
                                                );
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    "Retry request_conversation failed for thread {}: {}",
                                                    thread_id,
                                                    e
                                                );
                                            }
                                        }
                                        retry_attempted = true;
                                        phone_deadline = Some(
                                            tokio::time::Instant::now()
                                                + std::time::Duration::from_millis(
                                                    CONVERSATION_RETRY_WAIT_MS,
                                                ),
                                        );
                                        // Fall through to the select! loop so retry
                                        // signals are received as they arrive.
                                    } else {
                                        if retry_attempted {
                                            tracing::info!(
                                                "Subscription: thread {} retry settled with {} \
                                                 message(s); signaling load complete",
                                                thread_id,
                                                received_message_count
                                            );
                                        } else {
                                            tracing::info!(
                                                "Subscription: phone deadline reached for thread {}, \
                                                 signaling load complete (subscription continues)",
                                                thread_id
                                            );
                                        }
                                        load_complete_emitted = true;
                                        return Some((
                                            Message::ConversationLoadComplete {
                                                thread_id,
                                                total_count: total_message_count.unwrap_or(0),
                                            },
                                            ConversationMessageState::Listening {
                                                conn,
                                                stream,
                                                thread_id,
                                                device_id,
                                                messages_per_page,
                                                local_store_done,
                                                total_message_count,
                                                phone_deadline,
                                                load_complete_emitted,
                                                received_message_count,
                                                retry_attempted,
                                            },
                                        ));
                                    }
                                }
                            }
                        }

                        // Compute sleep duration based on phase
                        let wait_duration = if load_complete_emitted {
                            // Phase 3: long heartbeat, just keeping unfold alive
                            tokio::time::Duration::from_secs(MESSAGE_HEARTBEAT_SLEEP_SECS)
                        } else if let Some(pd) = phone_deadline {
                            // Phase 2: sleep until phone deadline
                            pd.saturating_duration_since(now)
                        } else if let Some(lsd) = local_store_deadline {
                            // Phase 1: sleep until local store hard timeout
                            lsd.saturating_duration_since(now)
                        } else {
                            // Fallback heartbeat
                            tokio::time::Duration::from_secs(MESSAGE_HEARTBEAT_SLEEP_SECS)
                        };

                        tokio::select! {
                            biased;

                            // Priority: D-Bus signals
                            msg_option = stream.next() => {
                                match msg_option {
                                    Some(Ok(msg)) => {
                                        if msg.header().message_type() == zbus::message::Type::Signal {
                                            if let (Some(interface), Some(member)) =
                                                (msg.header().interface(), msg.header().member())
                                            {
                                                let iface_str = interface.as_str();
                                                let member_str = member.as_str();

                                                // Handle conversationUpdated signals (individual messages)
                                                if iface_str == "org.kde.kdeconnect.device.conversations"
                                                    && member_str == "conversationUpdated"
                                                {
                                                    let body = msg.body();
                                                    if let Ok(value) =
                                                        body.deserialize::<zbus::zvariant::OwnedValue>()
                                                    {
                                                        if let Some(sms_msg) = parse_sms_message(&value) {
                                                            if sms_msg.thread_id == thread_id {
                                                                tracing::debug!(
                                                                    "Subscription: received message uid={} for thread {}",
                                                                    sms_msg.uid,
                                                                    thread_id
                                                                );
                                                                received_message_count =
                                                                    received_message_count
                                                                        .saturating_add(1);
                                                                return Some((
                                                                    Message::ConversationMessageReceived {
                                                                        thread_id,
                                                                        message: sms_msg,
                                                                    },
                                                                    ConversationMessageState::Listening {
                                                                        conn,
                                                                        stream,
                                                                        thread_id,
                                                                        device_id,
                                                                        messages_per_page,
                                                                        local_store_done,
                                                                        total_message_count,
                                                                        phone_deadline,
                                                                        load_complete_emitted,
                                                                        received_message_count,
                                                                        retry_attempted,
                                                                    },
                                                                ));
                                                            }
                                                        }
                                                    }
                                                }

                                                // Handle conversationLoaded signals (local store done)
                                                if iface_str == "org.kde.kdeconnect.device.conversations"
                                                    && member_str == "conversationLoaded"
                                                {
                                                    let body = msg.body();
                                                    if let Ok((conv_id, message_count)) =
                                                        body.deserialize::<(i64, u64)>()
                                                    {
                                                        if conv_id == thread_id {
                                                            let was_already_done =
                                                                local_store_done;
                                                            local_store_done = true;
                                                            total_message_count =
                                                                Some(message_count);
                                                            if retry_attempted {
                                                                // Retry's own conversationLoaded.
                                                                // Don't extend phone_deadline — its
                                                                // CONVERSATION_RETRY_WAIT_MS deadline
                                                                // is already running.
                                                                tracing::info!(
                                                                    "Subscription: retry conversationLoaded for thread {}, \
                                                                     {} messages in store",
                                                                    thread_id,
                                                                    message_count
                                                                );
                                                            } else if was_already_done
                                                                && (message_count as usize)
                                                                    > received_message_count
                                                                && received_message_count
                                                                    < messages_per_page as usize
                                                            {
                                                                // Primary trigger: the daemon
                                                                // re-emitted conversationLoaded
                                                                // (its `addMessages()` told us the
                                                                // local store grew) but we got
                                                                // fewer per-message signals than
                                                                // the store now reports, and we
                                                                // haven't filled a page yet — i.e.
                                                                // a deficit in our requested
                                                                // initial range. Covers both the
                                                                // original "received only 1 of N"
                                                                // truncation and the off-by-one
                                                                // case (e.g. store=4, received=3)
                                                                // observed for some short threads.
                                                                // The page-size guard avoids
                                                                // triggering on natural scroll
                                                                // pagination boundaries.
                                                                tracing::info!(
                                                                    "Subscription: thread {} duplicate conversationLoaded \
                                                                     (store={}, received={}), retrying Conversations \
                                                                     interface read (Option 1 path)",
                                                                    thread_id,
                                                                    message_count,
                                                                    received_message_count
                                                                );
                                                                let end = (received_message_count
                                                                    + messages_per_page as usize)
                                                                    as i32;
                                                                if let Err(e) = fire_retry_request(
                                                                    &conn, &device_id, thread_id,
                                                                    end,
                                                                )
                                                                .await
                                                                {
                                                                    tracing::warn!(
                                                                        "Retry request_conversation failed for thread {}: {}",
                                                                        thread_id,
                                                                        e
                                                                    );
                                                                }
                                                                retry_attempted = true;
                                                                phone_deadline = Some(
                                                                    tokio::time::Instant::now()
                                                                        + std::time::Duration::from_millis(
                                                                            CONVERSATION_RETRY_WAIT_MS,
                                                                        ),
                                                                );
                                                            } else {
                                                                tracing::debug!(
                                                                    "Subscription: conversationLoaded for thread {}, \
                                                                     {} messages in store. Waiting up to {:?} for phone...",
                                                                    thread_id,
                                                                    message_count,
                                                                    phone_wait
                                                                );
                                                                phone_deadline = Some(
                                                                    tokio::time::Instant::now()
                                                                        + phone_wait,
                                                                );
                                                            }
                                                            return Some((
                                                                Message::ConversationStoreLoaded {
                                                                    thread_id,
                                                                    total_count: message_count,
                                                                },
                                                                ConversationMessageState::Listening {
                                                                    conn,
                                                                    stream,
                                                                    thread_id,
                                                                    device_id,
                                                                    messages_per_page,
                                                                    local_store_done,
                                                                    total_message_count,
                                                                    phone_deadline,
                                                                    load_complete_emitted,
                                                                    received_message_count,
                                                                    retry_attempted,
                                                                },
                                                            ));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Some(Err(e)) => {
                                        tracing::warn!("D-Bus conversation stream error: {}", e);
                                    }
                                    None => {
                                        // Stream ended (D-Bus connection dropped)
                                        tracing::warn!(
                                            "D-Bus message stream ended for thread {}",
                                            thread_id
                                        );
                                        if !load_complete_emitted {
                                            return Some((
                                                Message::ConversationLoadComplete {
                                                    thread_id,
                                                    total_count: total_message_count.unwrap_or(0),
                                                },
                                                ConversationMessageState::Done,
                                            ));
                                        }
                                        return None;
                                    }
                                }
                            }

                            // Sleep until next deadline or heartbeat
                            _ = tokio::time::sleep(wait_duration) => {
                                // Loop back — deadline checks at top will handle expiry
                            }
                        }
                    }
                }
                ConversationMessageState::Done => {
                    // Terminal state - subscription is complete
                    None
                }
            }
        },
    )
}
