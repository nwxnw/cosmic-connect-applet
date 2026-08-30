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
    .sender("org.kde.kdeconnect")
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
  --dest=org.kde.kdeconnect \
  /modules/kdeconnect \
  org.kde.kdeconnect.daemon.devices

# Introspect daemon interface
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect \
  /modules/kdeconnect \
  org.freedesktop.DBus.Introspectable.Introspect

# Ping a device
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect \
  /modules/kdeconnect/devices/<device-id>/ping \
  org.kde.kdeconnect.device.ping.sendPing
```

### Device Operations

```bash
# Get device name
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect \
  /modules/kdeconnect/devices/<device-id> \
  org.freedesktop.DBus.Properties.Get \
  string:org.kde.kdeconnect.device string:name

# Check if device is reachable
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect \
  /modules/kdeconnect/devices/<device-id> \
  org.freedesktop.DBus.Properties.Get \
  string:org.kde.kdeconnect.device string:isReachable

# Request pairing
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect \
  /modules/kdeconnect/devices/<device-id> \
  org.kde.kdeconnect.device.requestPairing
```

### Battery Plugin

```bash
# Get battery charge level
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect \
  /modules/kdeconnect/devices/<device-id>/battery \
  org.freedesktop.DBus.Properties.Get \
  string:org.kde.kdeconnect.device.battery string:charge

# Check if charging
dbus-send --session --print-reply \
  --dest=org.kde.kdeconnect \
  /modules/kdeconnect/devices/<device-id>/battery \
  org.freedesktop.DBus.Properties.Get \
  string:org.kde.kdeconnect.device.battery string:isCharging
```

### Monitoring Signals

```bash
# Watch all KDE Connect signals
dbus-monitor --session "sender='org.kde.kdeconnect'"

# Watch specific device signals
dbus-monitor --session "path='/modules/kdeconnect/devices/<device-id>'"
```

### Using busctl

```bash
# List all KDE Connect objects
busctl --user tree org.kde.kdeconnect

# Introspect a device
busctl --user introspect org.kde.kdeconnect \
  /modules/kdeconnect/devices/<device-id>

# Call a method
busctl --user call org.kde.kdeconnect \
  /modules/kdeconnect/devices/<device-id>/ping \
  org.kde.kdeconnect.device.ping sendPing
```

## Pitfalls

### Two `requestConversation` methods (different behavior)

The daemon exposes `requestConversation` on two interfaces with **different behavior**:

- **`org.kde.kdeconnect.device.sms`** (at `/devices/{id}/sms`): Sends a network packet to the phone. The response flows through `addMessages()` which populates the daemon's in-memory `m_conversations` cache. Use this to prime the cache.
- **`org.kde.kdeconnect.device.conversations`** (at `/devices/{id}`): Creates a `RequestConversationWorker` that reads `m_conversations` and emits one `conversationUpdated` D-Bus signal per message it can already serve. The worker itself never writes to the cache; it only reads and emits.

Thread loading fires both: SMS plugin first (cache priming for replies), then Conversations interface (per-message signals for UI). See `docs/SMS.md` → "Message Thread Loading".

**There is no persistent store.** `m_conversations` is an in-memory `QMap` on the daemon, populated
only by `addMessages()` from phone responses, and it dies with the daemon process. Nothing in
`plugins/sms/` touches sqlite, `QSettings` or `KConfig`. A restarted daemon starts empty however
long the phone has been paired, which is why a cold open cannot be served from cache and why
`activeConversations()` returns nothing until the phone has answered at least once.

The worker is not purely local either. If the cache cannot satisfy the request
(`numHandled < howMany`), it calls `updateConversation()`, which asks the phone and **blocks**
until `addMessages()` sees new messages, then re-reads and emits the rest. It also tops up
pre-emptively when the remaining cache falls below `CACHE_LOW_WATER_MARK_PERCENT`. So a
Conversations-interface read can turn into a phone round trip, and the cache does get populated -
just by `addMessages()` on the response path, never by the worker.

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

### The daemon's interface name is not its destination

`kdeconnectd` owns two well-known names, and only one of them can be auto-started:

| Name | Where it comes from | Activatable |
|---|---|---|
| `org.kde.kdeconnect` | `/usr/share/dbus-1/services/org.kde.kdeconnect.service` | **Yes** |
| `org.kde.kdeconnect.daemon` | runtime alias from KDBusService, derived from the daemon's `KAboutData` component name | No |

`org.kde.kdeconnect.daemon` is **also the interface name** on `/modules/kdeconnect`, which is what makes the two easy to conflate. While the
daemon is running both names reach the same object, so addressing the alias looks correct indefinitely. It diverges only when the daemon is
**not** running: the bus starts it on demand for `org.kde.kdeconnect` and returns `org.freedesktop.DBus.Error.ServiceUnknown: The name is
not activatable` for the alias. Connected addressed the alias everywhere before v0.8.0 and so could never start the daemon on any distro -
invisible on Pop!_OS only because `/etc/xdg/autostart/` starts it at session login.

**Destinations** - `default_service`, `SERVICE_NAME`, the `sender=` match rule - use `org.kde.kdeconnect`. **Interfaces** keep
`org.kde.kdeconnect.*` unchanged. In `kdeconnect-dbus/src/daemon.rs` the two sit on adjacent lines and once held the identical string, so
never do this rename with a bare find-and-replace; `kdeconnect-dbus/src/lib.rs` carries a test asserting every proxy's
`Defaults::DESTINATION` equals `SERVICE_NAME`.

Unrelated despite the spelling: `~/.cache/kdeconnect.daemon/` is the daemon's Qt application name, not a bus name (`docs/SMS.md`).

```bash
busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
  org.freedesktop.DBus ListActivatableNames | tr ' ' '\n' | grep kdeconnect
# → "org.kde.kdeconnect" only, even with the daemon running
```


### Owning the name is not exporting the objects

Addressing the activatable name makes the bus start `kdeconnectd` on demand, which introduces a
race the alias never could. The bus considers activation complete as soon as the process **owns
`org.kde.kdeconnect`**, which happens before the daemon has exported `/modules/kdeconnect`. The
first call after a cold start therefore comes back `UnknownObject` or `UnknownInterface` **on a
healthy install** - it is a timing artifact, not a fault.

`fetch_device_ids` (`device/fetch.rs`) absorbs this with the backoff schedule
`ACTIVATION_RETRY_DELAYS_MS` (`constants.rs`), currently `[0, 150, 400, 1000]` - attempt once
immediately, then three retries, ~1.55 s worst case before a genuine error reaches the UI.
Recovery is normally sub-second, so this is far tighter than `RETRY_DELAY_SECS`, which is tuned
for SMS page loads.

**Only `Starting` is retried.** `classify()` maps the D-Bus error *name* to a `DaemonUnavailable`,
and every other variant is permanent until something changes outside the applet:

| Error name | Variant | Retried | Means |
|---|---|---|---|
| `UnknownObject`, `UnknownInterface` | `Starting` | Yes | Name owned, objects not exported yet |
| `ServiceUnknown` | `NotInstalled` | No | No activation service file: KDE Connect is not installed |
| `Spawn.*` | `FailedToStart` | No | Service file present, the bus could not exec it |
| `NoReply`, `Timeout`, `TimedOut` | `NotResponding` | No | Name owned, no reply |
| anything else, or a non-`MethodError` | `Other(String)` | No | Raw error shown to the user |

Retrying a `NoReply` is actively harmful: it has already cost zbus's call timeout, so a second
attempt doubles the wait before the user sees anything. The classification reads
`zbus::Error::MethodError`; `zbus::fdo::Error` is the server-side type and never appears here.

The connection mutex is acquired *inside* each attempt so the guard drops before the backoff
sleep. Nineteen call sites share that `Arc<Mutex<Connection>>`, and holding it across a backoff
would stall all of them during exactly the cold open where the user is most likely to click
something.
