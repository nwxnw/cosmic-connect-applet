# D-Bus Interface Reference

KDE Connect D-Bus interfaces and testing commands.

## Interface Reference

| Interface | Path | Purpose |
|-----------|------|---------|
| `org.kde.kdeconnect.daemon` | `/modules/kdeconnect` | Device discovery, announcements |
| `org.kde.kdeconnect.device` | `/modules/kdeconnect/devices/<id>` | Per-device operations, pairing |
| `org.kde.kdeconnect.device.battery` | (same + /battery) | Battery status (charge, isCharging) |
| `org.kde.kdeconnect.device.clipboard` | (same + /clipboard) | Clipboard sync |
| `org.kde.kdeconnect.device.findmyphone` | (same + /findmyphone) | Trigger phone to ring |
| `org.kde.kdeconnect.device.mprisremote` | (same + /mprisremote) | Media player control |
| `org.kde.kdeconnect.device.ping` | (same + /ping) | Send ping to device |
| `org.kde.kdeconnect.device.notifications` | (same + /notifications) | List active notifications |
| `org.kde.kdeconnect.device.share` | (same + /share) | File/URL sharing |
| `org.kde.kdeconnect.device.sms` | (same + /sms) | Request SMS conversations |
| `org.kde.kdeconnect.device.conversations` | `/modules/kdeconnect/devices/<id>` | SMS data and signals |
| `org.kde.kdeconnect.device.telephony` | (same + /telephony) | Call notifications |

## Property Naming Convention

KDE Connect uses camelCase for D-Bus property names. In zbus, explicitly specify names:

```rust
#[zbus(property, name = "isCharging")]
fn is_charging(&self) -> zbus::Result<bool>;

#[zbus(property, name = "isPairRequested")]
fn is_pair_requested(&self) -> zbus::Result<bool>;
```

## Signal Subscription

To receive real-time updates, subscribe to D-Bus signals using match rules:

```rust
use zbus::fdo::DBusProxy;

let dbus_proxy = DBusProxy::new(&conn).await?;
let rule = zbus::MatchRule::builder()
    .msg_type(zbus::message::Type::Signal)
    .sender("org.kde.kdeconnect.daemon")
    .map(|b| b.build())?;
dbus_proxy.add_match_rule(rule).await?;

let stream = zbus::MessageStream::from(&conn);
```

Without explicit match rules, D-Bus signals may not be delivered.

## Testing Commands

### Basic Operations

```bash
# List paired devices
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect.daemon \
  /modules/kdeconnect \
  org.kde.kdeconnect.daemon.devices

# Introspect daemon interface
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect.daemon \
  /modules/kdeconnect \
  org.freedesktop.DBus.Introspectable.Introspect

# Ping a device
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect.daemon \
  /modules/kdeconnect/devices/<device-id>/ping \
  org.kde.kdeconnect.device.ping.sendPing
```

### Device Operations

```bash
# Get device name
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect.daemon \
  /modules/kdeconnect/devices/<device-id> \
  org.freedesktop.DBus.Properties.Get \
  string:org.kde.kdeconnect.device string:name

# Check if device is reachable
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect.daemon \
  /modules/kdeconnect/devices/<device-id> \
  org.freedesktop.DBus.Properties.Get \
  string:org.kde.kdeconnect.device string:isReachable

# Request pairing
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect.daemon \
  /modules/kdeconnect/devices/<device-id> \
  org.kde.kdeconnect.device.requestPairing
```

### Battery Plugin

```bash
# Get battery charge level
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect.daemon \
  /modules/kdeconnect/devices/<device-id>/battery \
  org.freedesktop.DBus.Properties.Get \
  string:org.kde.kdeconnect.device.battery string:charge

# Check if charging
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect.daemon \
  /modules/kdeconnect/devices/<device-id>/battery \
  org.freedesktop.DBus.Properties.Get \
  string:org.kde.kdeconnect.device.battery string:isCharging
```

### Monitoring Signals

```bash
# Watch all KDE Connect signals
dbus-monitor --session "sender='org.kde.kdeconnect.daemon'"

# Watch specific device signals
dbus-monitor --session "path='/modules/kdeconnect/devices/<device-id>'"
```

### Using busctl

```bash
# List all KDE Connect objects
busctl --user tree org.kde.kdeconnect.daemon

# Introspect a device
busctl --user introspect org.kde.kdeconnect.daemon \
  /modules/kdeconnect/devices/<device-id>

# Call a method
busctl --user call org.kde.kdeconnect.daemon \
  /modules/kdeconnect/devices/<device-id>/ping \
  org.kde.kdeconnect.device.ping sendPing
```

## Pitfalls

### Two `requestConversation` methods (different behavior)

The daemon exposes `requestConversation` on two interfaces with **different behavior**:

- **`org.kde.kdeconnect.device.sms`** (at `/devices/{id}/sms`): Sends a network packet to the phone. The response flows through `addMessages()` which populates the daemon's in-memory `m_conversations` cache. Use this to prime the cache.
- **`org.kde.kdeconnect.device.conversations`** (at `/devices/{id}`): Creates a `RequestConversationWorker` that reads from a local persistent store and emits `conversationUpdated` D-Bus signals, but does **NOT** populate `m_conversations`.

Thread loading fires both: SMS plugin first (cache priming for replies), then Conversations interface (per-message signals for UI). See `docs/SMS.md` → "Message Thread Loading".

### Pagination offsets: always request from 0

`requestConversation(conversationID, start, end)` on the Conversations interface has **no bounds check between the wire and the daemon's iterator arithmetic**. An out-of-range `start` dereferences past the end of the daemon's cached list: on kdeconnect 23.08.5 (Pop!_OS 24.04) that is a **SIGSEGV, and the daemon does not come back** - it is not D-Bus-activatable, so systemd leaves the unit failed and the user loses all KDE Connect functionality until they restart it manually. Newer daemons turn the crash into a silent "emit nothing, return Ok".

Always pass `start = 0` and vary only `end`. See `docs/SMS.md` → "Older Message Loading".

### Match rules are not uniformly scoped

- `subscriptions.rs` registers a rule filtered **only on the daemon's sender name**, with no interface or member filter, then accepts or drops each signal in an `is_relevant` match. A new daemon-emitted signal needs **no new match rule** here - only a new branch. 
- `sms/conversation_subscription.rs` registers **member-scoped** rules, one per signal (`conversationCreated`, `conversationUpdated`, `conversationLoaded`). A new signal there **does** need its own rule, or it never arrives.

Do not carry the conclusion from one file to the other.

### `isReachable` is not "the phone can be reached"

Upstream, `Device::isReachable()` is `!m_deviceLinks.isEmpty()` - a link *object* exists. A half-open TCP connection reports reachable for up to ~16 minutes. There is no D-Bus surface exposing link count, socket health, or last-seen, so this cannot be detected in-app. See `docs/KNOWN_ISSUES.md`.

### A void method's `Ok` is not an acknowledgement

`replyToConversation` and `sendSms` have **no out parameters** and cannot report delivery, and the daemon's send returns success as soon as the write buffers - which succeeds on a dead-but-ESTABLISHED socket. `Ok` means "the D-Bus call worked". The only evidence a message was delivered is the phone echoing it back.
