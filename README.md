# Connected

A phone connectivity applet for the [COSMIC](https://github.com/pop-os/cosmic-epoch) desktop panel, powered by [KDE Connect](https://kdeconnect.kde.org/).

<img src="screenshots/connected-applet.png" alt="Connected applet showing device page" width="380">

## Features

- **Device Management** - Pair, unpair, and monitor connected devices (phones, tablets, laptops, desktops)
- **SMS Messaging** - View conversations, reply, and compose new messages with contact lookup; open MMS photo and video attachments
- **Smart SMS Threading** - Automatically merges conversations that iOS reaction-over-SMS splits into multiple threads on Android, with a one-click toggle to view them split
- **File Sharing** - Send and receive files and URLs, with desktop notifications
- **Clipboard Sync** - Send clipboard content to your device
- **Notifications** - View and dismiss phone notifications; desktop alerts for SMS and calls (with privacy controls)
- **Battery Status** - Monitor battery level and charging state
- **Media Controls** - Control music playback (play/pause, next/previous, volume)
- **Find My Phone** - Ring or ping your phone to locate it

### SMS Reaction-Thread Merging

When someone reacts to an SMS from iOS, Android often files the reaction into a separate thread from the original conversation. Connected detects these split threads and merges them on the desktop side, with a one-click toggle to switch between merged and split views. Merging is on by default and recommended: besides reuniting the conversation, it routes your replies so the recipient receives a single copy. With merging off, replying in a split thread can deliver duplicate copies. Only use the split view if the merge heuristic ever combines conversations it shouldn't.

  <table>
    <tr>
      <td align="center"><b>Split view</b></td>
      <td align="center"><b>Merged view</b></td>
    </tr>
    <tr>
      <td><img src="screenshots/sms-threads-split.png" alt="Split conversation view" width="380"></td>
      <td><img src="screenshots/sms-threads-merged.png" alt="Merged conversation view" width="380"></td>
    </tr>
  </table>

## Requirements

Connected requires KDE Connect on both your desktop and Android phone.

**Desktop:**
```sh
# Debian/Ubuntu/Pop!_OS
sudo apt install kdeconnect

# Fedora
sudo dnf install kde-connect

# Arch
sudo pacman -S kdeconnect
```

**Phone:** Install the KDE Connect app from [Google Play](https://play.google.com/store/apps/details?id=org.kde.kdeconnect_tp) or [F-Droid](https://f-droid.org/packages/org.kde.kdeconnect_tp/).

## Installation

Connected is available in the COSMIC Store in the Applets category. Installing from the Store provides automatic updates.

To remove it, use the Store, or run `flatpak --user uninstall io.github.nwxnw.cosmic-ext-connected`

### Manual Download

Download the latest `.deb` or `.flatpak` release from the [Releases](https://github.com/nwxnw/cosmic-ext-connected/releases) page.

**Flatpak bundle**

- Install: `flatpak install --user ./cosmic-ext-connected_*.flatpak`
- Uninstall: `flatpak uninstall --user io.github.nwxnw.cosmic-ext-connected`

**Debian/Ubuntu (.deb)**

- Install: `sudo apt install ./cosmic-ext-connected_*_amd64.deb`
- Uninstall: `sudo apt remove cosmic-ext-connected`

### From source

Requires Rust, [just](https://github.com/casey/just), and the `pkg-config`, `libxkbcommon-dev`, `wayland-protocols`, and `libwayland-dev` system dependencies.

- Install: `just build-release && just install`
- Uninstall: `just uninstall`

If you previously installed via `sudo just install`, run `sudo just uninstall-system` once to clear the old system copy.

## Usage

1. Ensure both devices are on the same network
2. Click the Connected applet in your panel - your phone should appear
3. Click your phone and select "Pair", then accept on your phone
4. **Important:** After pairing, enable the requested permissions in the KDE Connect app on your phone (SMS, Contacts, etc.)

## Configuration

Notification settings live on the **Notifications** page, opened from the notifications icon at the top of the applet. You can toggle each desktop alert and tune what it reveals:

- **SMS notifications** - Desktop alerts for incoming SMS, with options to show or hide the sender and the message content
- **Call notifications** - Desktop alerts for incoming and missed calls, with options to show or hide the caller's name and number
- **File notifications** - Desktop alerts for received files

App and version information is on the **About** page, reached from the identity line at the bottom of the device list.

## Troubleshooting

### No devices appear

Work down this list:

1. **KDE Connect is running on your phone**, and phone and desktop are on the same network.
2. **The firewall allows ports 1714-1764**, TCP and UDP. Fedora's `FedoraWorkstation` zone opens them, but **the COSMIC spin defaults to the `public` zone, which does not** - installing KDE Connect does not change the firewall, so the daemon runs and no phone is ever found.
   ```sh
   firewall-cmd --get-active-zones                         # which zone am I in?
   sudo firewall-cmd --permanent --add-service=kdeconnect  # note: no hyphen
   sudo firewall-cmd --reload
   ```
   firewalld ships the `kdeconnect` service definition itself, covering 1714-1764 on TCP and UDP.

### "KDE Connect was not found"

The D-Bus service that starts the daemon is not installed. Install KDE Connect and reopen the applet; if the card is still showing, press **Retry**.

### "KDE Connect could not be started"

The service file exists but the daemon failed to launch. Check `journalctl --user -b | grep -i kdeconnect`, and confirm the path in `/usr/share/dbus-1/services/org.kde.kdeconnect.service` matches the installed binary - `/usr/bin/kdeconnectd` on Fedora and Arch, `/usr/lib/x86_64-linux-gnu/libexec/kdeconnectd` on Debian and Ubuntu.

<!-- Anchor #duplicate-notifications-with-kde-connect is linked from the in-app Notifications page ("Learn more"). Do not rename this heading. -->
### Duplicate notifications with KDE Connect

Connected raises its own desktop notifications for incoming SMS and calls. KDE Connect can announce the same events independently, so depending on which KDE Connect plugins you have enabled, you may see each SMS or call notified twice. (File transfers are only notified by Connected, so they never duplicate.)

- **SMS** - The duplicate comes from KDE Connect's **"Receive notifications"** plugin, which mirrors your phone's notifications to the desktop. Disabling that plugin stops the duplicate SMS toast, but it also empties Connected's per-device notification list and the notification count badge - both are populated from that plugin's data. Connected's own SMS toast is separate and keeps working either way.
- **Calls** - The duplicate comes from KDE Connect's **telephony** plugin. Connected reads call events from that same plugin, so disabling it would remove Connected's call notifications too; there is no clean per-app toggle today.

The cleanest fix would be to mute KDE Connect's toasts at the COSMIC notification service. As COSMIC's notification settings evolve, per-application controls may offer a built-in way to do this.

## Contributing

Contributions welcome! Please submit issues and pull requests.

See `docs/` and `CLAUDE.md` for development documentation.

## License

GNU General Public License v3.0 - see [LICENSE](LICENSE).

## Acknowledgments

- [KDE Connect](https://kdeconnect.kde.org/) - The daemon that makes this possible
- [System76](https://system76.com/) / [libcosmic](https://github.com/pop-os/libcosmic) - The COSMIC desktop and UI toolkit
