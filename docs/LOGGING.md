# Logging and diagnostics

Connected emits structured tracing events directly to the user's systemd journal via the `tracing_journald` layer in `src/main.rs`.

**Live tail:**

```bash
journalctl --user SYSLOG_IDENTIFIER=cosmic-ext-connected -f
```

**Filter by level or message:**

```bash
journalctl --user SYSLOG_IDENTIFIER=cosmic-ext-connected -p warning      # WARN+
journalctl --user SYSLOG_IDENTIFIER=cosmic-ext-connected --grep "<text>"
journalctl --user SYSLOG_IDENTIFIER=cosmic-ext-connected _PID=<pid>      # one process at a time
```

The default filter directive depends on build profile: `cosmic_ext_connected=debug` for debug builds (`cargo run`, `cargo build`) and `cosmic_ext_connected=warn` for release builds (`cargo build --release`, installed `.deb`/`.flatpak`). Other crates default to ERROR-level (so libcosmic warnings and errors still surface). Setting `RUST_LOG` overrides the default entirely:

```bash
RUST_LOG=cosmic_ext_connected=info cargo run -p cosmic-ext-connected
RUST_LOG=cosmic_ext_connected=trace,zbus=debug cargo run -p cosmic-ext-connected
```

The release default of `warn` keeps installed-build journald output to actionable problems only; raise it ad-hoc via `RUST_LOG` when debugging a deployed installation — but see the next section for how, because exporting it in a shell does **not** work for a panel-launched applet.

## Raising the level for an installed applet

**A panel-hosted applet does not inherit your shell environment.** cosmic-panel is spawned by `cosmic-session` at login, and it spawns the applet; your interactive shell is nowhere in that chain. Exporting `RUST_LOG` before running `journalctl` sets it for `journalctl`, which has no use for it — the applet keeps the compiled-in `warn` default and any `info`/`debug` events you were hoping for never exist. Verify with `tr '\0' '\n' < /proc/$(pgrep -f 'bin/cosmic-ext-connected' | head -1)/environ | grep RUST_LOG`.

Three ways out, cheapest first:

**Run it outside the panel — usually the right answer.** `cargo run` builds a normal window (every `COSMIC_PANEL_*` variable has a default), events print straight to your terminal *and* still reach journald, and only one instance runs instead of one per panel/dock surface:

```bash
RUST_LOG=cosmic_ext_connected=debug cargo run --release -p cosmic-ext-connected
```

Use `--release` so timing-sensitive behaviour stays representative; `RUST_LOG` overrides the profile default either way.

**Wrap the installed `Exec`** if you need it hosted by the panel. Edit `~/.local/share/applications/io.github.nwxnw.cosmic-ext-connected.desktop`:

```
Exec=env RUST_LOG=cosmic_ext_connected=debug /home/<user>/.local/bin/cosmic-ext-connected
```

then `killall cosmic-panel`. Note `just install` rewrites that line, so re-apply after each install.

**Set it session-wide** for something persistent: put `RUST_LOG=cosmic_ext_connected=debug` in `~/.config/environment.d/50-connected-debug.conf` and log out and back in. Survives reinstalls; easy to forget to remove.

**Pick the level deliberately.** Some diagnostics that matter live at `debug`, not `info` — the older-messages pagination request (`sms/fetch.rs`) and the per-signal `D-Bus signal: <iface>.<member>` line (`subscriptions.rs`) among them. Running at `info` and seeing nothing is not evidence that nothing happened.

**Why direct routing:** cosmic-panel pipes applet stdout/stderr and re-emits each line through its own tracing tree, then drops INFO under its default `warn` filter. The journald layer bypasses this, preserving each event's original level under our own `SYSLOG_IDENTIFIER`. Inside Flatpak the layer may fail to construct (sandboxed journal socket) and silently falls back to fmt-only — see `CLAUDE.md` "Flatpak Debug Logging" for the file-based alternative.

**Adding diagnostics:** use `tracing::info!`/`warn!`/`error!` macros — they route through both layers automatically. For structured fields, prefer `tracing::info!(thread_id = %tid, "loaded")` over format-string interpolation; structured fields render as separate journald entries when the layer supports them.
