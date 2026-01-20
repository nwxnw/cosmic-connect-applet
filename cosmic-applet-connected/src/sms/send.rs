//! SMS sending functionality.

use crate::app::Message;
use kdeconnect_dbus::plugins::ConversationsProxy;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::zvariant::{Structure, Value};
use zbus::Connection;

/// Send an SMS message using the SMS plugin's sendSms method directly.
///
/// This bypasses the daemon's conversation cache (which replyToConversation depends on)
/// and sends directly to the phone. The sub_id (SIM subscription ID) is required for
/// MMS group messages to work correctly.
///
/// Sends to ALL addresses in the recipients list, supporting group conversations.
pub async fn send_sms_async(
    conn: Arc<Mutex<Connection>>,
    device_id: String,
    _thread_id: i64,
    recipients: Vec<String>,
    message: String,
    sub_id: i64,
) -> Message {
    let conn = conn.lock().await;
    // SMS plugin is on /sms subpath
    let sms_path = format!("{}/devices/{}/sms", kdeconnect_dbus::BASE_PATH, device_id);

    // Format addresses as D-Bus structs: array of variants containing structs with single string
    // This matches what KDE Connect's ConversationAddress serializes to
    // NOTE: Do NOT deduplicate - MMS groups need exact address list to match the thread
    let addresses: Vec<Value<'_>> = recipients
        .iter()
        .map(|addr| Value::Structure(Structure::from((addr.clone(),))))
        .collect();
    let empty_attachments: Vec<Value<'_>> = vec![];

    tracing::info!(
        "Sending SMS via sendSms to {} recipient(s), sub_id={}",
        addresses.len(),
        sub_id
    );

    // Use the SMS plugin's sendSms method directly with sub_id
    // This is what replyToConversation does internally after looking up addresses from cache
    let result = conn
        .call_method(
            Some("org.kde.kdeconnect.daemon"),
            sms_path.as_str(),
            Some("org.kde.kdeconnect.device.sms"),
            "sendSms",
            &(addresses, message.as_str(), empty_attachments, sub_id),
        )
        .await;

    match result {
        Ok(_) => {
            tracing::info!("SMS sent successfully via sendSms");
            Message::SmsSendResult(Ok(message))
        }
        Err(e) => {
            tracing::error!("SMS send failed: {}", e);
            Message::SmsSendResult(Err(format!("Send failed: {}", e)))
        }
    }
}

/// Send an SMS to a new recipient (creates or adds to existing conversation).
pub async fn send_new_sms_async(
    conn: Arc<Mutex<Connection>>,
    device_id: String,
    recipient: String,
    message: String,
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
                return Message::NewMessageSendResult(Err(format!(
                    "Failed to create proxy: {}",
                    e
                )));
            }
        },
        None => {
            return Message::NewMessageSendResult(Err("Failed to build proxy path".to_string()));
        }
    };

    // Format address as D-Bus struct for KDE Connect
    // KDE Connect's ConversationAddress is a struct containing a single string: (s)
    let addresses: Vec<Value<'_>> = vec![Value::Structure(Structure::from((recipient.clone(),)))];
    let empty_attachments: Vec<Value<'_>> = vec![];

    match conversations_proxy
        .send_without_conversation(addresses, &message, empty_attachments)
        .await
    {
        Ok(()) => Message::NewMessageSendResult(Ok("Message sent".to_string())),
        Err(e) => Message::NewMessageSendResult(Err(format!("Send failed: {}", e))),
    }
}
