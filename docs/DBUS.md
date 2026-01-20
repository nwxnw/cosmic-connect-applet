# D-Bus Testing Commands

Reference commands for testing KDE Connect D-Bus interfaces from the command line.

## Basic Operations

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

## Device Operations

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

## Battery Plugin

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

## Monitoring Signals

```bash
# Watch all KDE Connect signals
dbus-monitor --session "sender='org.kde.kdeconnect.daemon'"

# Watch specific device signals
dbus-monitor --session "path='/modules/kdeconnect/devices/<device-id>'"
```

## Using busctl (alternative)

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
