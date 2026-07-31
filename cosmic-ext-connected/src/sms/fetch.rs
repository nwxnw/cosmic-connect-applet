//! SMS conversation and message fetching from KDE Connect.

use crate::app::Message;
use kdeconnect_dbus::plugins::{parse_conversations, ConversationsProxy, SmsProxy};
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::Connection;

/// Ask the daemon to serve an older page of a conversation. The already-running
/// per-thread subscription receives the resulting `conversationUpdated` signals
/// through its normal phase-3 path (`subscriptions.rs`), so this fires the
/// request and returns - it does NOT collect the reply. Completion is detected
/// in the store from the streamed messages / `ConversationStoreLoaded`.
///
/// The request is always issued from offset **0**, widening the range rather than
/// advancing it: `[0, loaded_count + count)`. The daemon serves newest-first, so
/// this returns the same older page, and `crbegin() + 0` can never run past the
/// end of its cache. A non-zero offset against a cache holding fewer messages
/// segfaults kdeconnectd 23.08.5 and silently returns nothing on 26.04+. The
/// applet cannot know the daemon's cache size - every count it holds is stale in
/// exactly that scenario - so the offset is pinned rather than clamped. See D.29.
pub async fn request_older_messages_async(
    conn: Arc<Mutex<Connection>>,
    device_id: String,
    thread_id: i64,
    loaded_count: u32,
    count: u32,
) -> Message {
    let conn = conn.lock().await;
    let device_path = format!("{}/devices/{}", kdeconnect_dbus::BASE_PATH, device_id);

    let conversations_proxy = match ConversationsProxy::builder(&conn)
        .path(device_path.as_str())
        .ok()
        .map(|b| b.build())
    {
        Some(fut) => match fut.await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Failed to build conversations proxy for pagination: {}", e);
                return Message::OlderMessagesRequested {
                    thread_id,
                    ok: false,
                };
            }
        },
        None => {
            return Message::OlderMessagesRequested {
                thread_id,
                ok: false,
            }
        }
    };

    tracing::debug!(
        "requesting older messages for thread {} (requesting 0-{}, {} already loaded)",
        thread_id,
        loaded_count + count,
        loaded_count
    );
    match conversations_proxy
        .request_conversation(thread_id, 0, (loaded_count + count) as i32)
        .await
    {
        Ok(()) => Message::OlderMessagesRequested {
            thread_id,
            ok: true,
        },
        Err(e) => {
            tracing::warn!(
                "Failed to request older messages for thread {}: {}",
                thread_id,
                e
            );
            Message::OlderMessagesRequested {
                thread_id,
                ok: false,
            }
        }
    }
}

/// Timeout for waiting for attachment retrieval from phone (seconds).
const ATTACHMENT_TIMEOUT_SECS: u64 = 30;

/// Request a full-size attachment from the phone and wait for delivery.
///
/// 1. Calls `getAttachment(part_id, unique_identifier)` on the SMS plugin
/// 2. Watches for the file to appear in `~/.cache/kdeconnect.daemon/<device_name>/`
/// 3. Returns `AttachmentReady(file_path)` or `AttachmentError`
pub async fn request_attachment_async(
    conn: Arc<Mutex<Connection>>,
    device_id: String,
    device_name: String,
    part_id: i64,
    unique_identifier: String,
) -> Message {
    let conn = conn.lock().await;

    // Build SMS proxy for the attachment request
    let sms_path = format!("{}/devices/{}/sms", kdeconnect_dbus::BASE_PATH, device_id);
    let sms_proxy = match SmsProxy::builder(&conn)
        .path(sms_path.as_str())
        .ok()
        .map(|b| b.build())
    {
        Some(fut) => match fut.await {
            Ok(p) => p,
            Err(e) => {
                return Message::AttachmentError(format!("Failed to create SMS proxy: {}", e));
            }
        },
        None => {
            return Message::AttachmentError("Failed to build SMS proxy path".to_string());
        }
    };

    // Request the attachment from the phone
    if let Err(e) = sms_proxy.get_attachment(part_id, &unique_identifier).await {
        return Message::AttachmentError(format!("Failed to request attachment: {}", e));
    }

    tracing::info!(
        "Requested attachment part_id={} uid={} from device {}",
        part_id,
        unique_identifier,
        device_id
    );

    // Poll for the file to appear in the cache directory
    // KDE Connect daemon caches to ~/.cache/kdeconnect.daemon/<device-name>/
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let expected_path = std::path::PathBuf::from(&home)
        .join(".cache/kdeconnect.daemon")
        .join(&device_name)
        .join(&unique_identifier);

    let start = tokio::time::Instant::now();
    let timeout = tokio::time::Duration::from_secs(ATTACHMENT_TIMEOUT_SECS);

    loop {
        if expected_path.exists() {
            let path_str = expected_path.to_string_lossy().to_string();
            tracing::info!("Attachment ready at {}", path_str);
            return Message::AttachmentReady(path_str);
        }

        if start.elapsed() >= timeout {
            return Message::AttachmentError(format!(
                "Timed out waiting for attachment ({}s)",
                ATTACHMENT_TIMEOUT_SECS
            ));
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Prefetch SMS conversations for a device (best-effort, fire-and-forget).
///
/// Calls `activeConversations()` once and returns whatever is cached.
/// Does NOT start signal subscriptions or fire `requestAllConversationThreads()`.
/// Used by `SelectDevice` to have conversations ready before the user opens SMS.
pub async fn prefetch_conversations_async(
    conn: Arc<Mutex<Connection>>,
    device_id: String,
) -> Message {
    let conn = conn.lock().await;
    let device_path = format!("{}/devices/{}", kdeconnect_dbus::BASE_PATH, device_id);

    let conversations_proxy = match ConversationsProxy::builder(&conn)
        .path(device_path.as_str())
        .ok()
        .map(|b| b.build())
    {
        Some(fut) => match fut.await {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("SMS prefetch: failed to create proxy: {}", e);
                return Message::SmsPrefetchReady(device_id, Vec::new());
            }
        },
        None => {
            tracing::debug!("SMS prefetch: failed to build proxy path");
            return Message::SmsPrefetchReady(device_id, Vec::new());
        }
    };

    match conversations_proxy.active_conversations().await {
        Ok(values) => {
            let mut conversations = parse_conversations(values);
            conversations.sort_by_key(|c| std::cmp::Reverse(c.timestamp));
            tracing::debug!(
                "SMS prefetch: {} conversations for device {}",
                conversations.len(),
                device_id
            );
            Message::SmsPrefetchReady(device_id, conversations)
        }
        Err(e) => {
            tracing::debug!("SMS prefetch: activeConversations failed: {}", e);
            Message::SmsPrefetchReady(device_id, Vec::new())
        }
    }
}
