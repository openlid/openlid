<div align="center">

<img src="resources/app/AppIcon-readme.png?v=2" alt="Open-Lid" width="128" height="128" />

# OpenLid

**Keep your laptop awake — even with the lid closed.**

[![CI](https://github.com/openlid/openlid/actions/workflows/ci.yml/badge.svg)](https://github.com/openlid/openlid/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/openlid/openlid/branch/main/graph/badge.svg)](https://codecov.io/gh/openlid/openlid)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/openlid/openlid)](https://github.com/openlid/openlid/releases/latest)
[![GitHub downloads](https://img.shields.io/github/downloads/openlid/openlid/total?label=downloads)](https://github.com/openlid/openlid/releases)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%C2%B7%20Linux%20planned-black.svg?logo=apple)](https://github.com/openlid/openlid)

[Website](https://openlid.github.io/openlid/) · [Download](https://github.com/openlid/openlid/releases/latest) · [Install guide](https://openlid.github.io/openlid/#methods) · [Coding agents workflow](https://openlid.github.io/openlid/coding-agents)

</div>

Open-Lid is a tiny menu bar utility that keeps your laptop awake — with
the lid open or closed. Carry it around with a long build, an agent, or
a download running; or step away from your desk without having the OS
lock the screen and dim the display every time you check your phone.

Built in Rust with a platform-abstraction layer in `openlid-core` so the
state machine and CLI are OS-independent. The macOS implementation calls
IOKit + AppKit + ServiceManagement; the Linux implementation planned for
v3.0.0 will call logind via D-Bus.

> [!NOTE]
> **Status: v2.3.3 — stable API.** macOS 13+ on Apple Silicon today. **Linux
> support planned for v3.0.0; Windows depending on demand.** Signed and
> notarized — no Gatekeeper warning. Helper installs automatically via
> `SMAppService` — no `sudo` required. CLI subcommands, `config.toml`
> schema, and IPC wire shapes are locked under semver — see
> [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

---

## Why?

You're at a meeting in a different room. You close your laptop lid to
carry it. A coding agent is doing real work; your `cargo build` is 4
minutes from finishing; a long file is downloading. The OS sleeps the
system the moment the lid closes, killing everything.

Or: you're at your desk, you step away for five minutes, and you come
back to a locked screen for the third time today.

Open-Lid handles both:

- While Open-Lid is active, the display-idle path stays awake — no idle
  dim, no screen lock, including remote access with the lid closed. You
  can turn this off if you'd rather have the screen lock on idle.
- When the lid closes without an external display, Open-Lid still asks
  macOS to turn the physical display off so the battery and thermals do
  not suffer. If an external display is attached, that one stays awake.

Activate indefinitely, or set a recurring schedule for the hours of the
day you want sleep prevention active. Auto-deactivate when the battery
gets low.

## Features

- **One-click toggle** in the menu bar (left-click). Right-click for the
  full menu.
- **Recurring schedule** — keep sleep prevention active only during a
  given window (e.g. 08:00–18:00 on weekdays). CLI + UI.
- **Display stays awake while preventing sleep** — no idle dim, no
  screen lock, including closed-lid remote access. Implemented via
  Apple's documented `IOPMAssertion` API. Opt out in Preferences if you
  want the screen to lock on idle.
- **Display off when lid closes** — your battery and your thermal envelope
  thank you. Skipped automatically when an external display is connected.
- **Mock-matched sidebar preferences window**:
  - **General** — start at login, activate at launch, keep display awake
  - **Safeguards** — auto-deactivate below a configurable battery percent
    or when OpenLid detects the laptop is in transit
  - **Schedule** — recurring active-hours schedule with 24-hour or AM/PM
    time picking
- **First-class CLI** for scripting:
  ```
  openlid on / off / status
  openlid schedule set --from 08:00 --to 18:00 --days Mon,Tue,Wed,Thu,Fri
  ```
- **Single-instance** — running `open -a OpenLid` twice is a no-op.
- **No telemetry. No data leaves your machine. Ever.**

## Installation

> [!IMPORTANT]
> Installation today is macOS-only. Linux installation instructions
> will land here once the Linux backend ships in v3.0.0. If you'd find
> Open-Lid useful on Linux, please [open an issue](https://github.com/openlid/openlid/issues/new/choose)
> describing your distro and use case — it helps prioritize.

### Download the signed DMG (recommended) — macOS

```bash
# Homebrew tap:
brew install --cask openlid/tap/openlid

# Or download the DMG directly:
# https://github.com/openlid/openlid/releases/latest
```

After installing, launch Open-Lid; macOS will prompt you to enable it in
**System Settings → General → Login Items → Allow in the Background**.
Flip the Open-Lid toggle on — no admin password required.

The brew install also puts `openlid` on your `PATH`, so you can drive
everything from the terminal — see [CLI](#cli) below.

### From source — macOS

Prerequisites: macOS 13+ on Apple Silicon, Rust 1.88+, Xcode Command Line
Tools.

```bash
git clone https://github.com/openlid/openlid.git
cd openlid

# Build (ad-hoc-signed, dev profile), install into /Applications, refresh caches:
./scripts/install.sh

# Optional: put `openlid` on your PATH:
./scripts/install-cli-symlink.sh

# Launch:
open -a OpenLid
```

## Usage

### Menu bar

Click the laptop icon in the menu bar to toggle on/off. Right-click (or
option-click) to see the full menu:

```
Status: Active (indefinite) · lid closed · AC
─────────
Turn Off
─────────
Preferences…   ⌘,
─────────
Quit Open-Lid   ⌘Q
```

### CLI

| Command | What it does |
|---|---|
| `openlid on` | Activate indefinitely |
| `openlid off` | Deactivate |
| `openlid status` | Print current state. Use `--json` for machine-readable output. |
| `openlid schedule set --from HH:MM --to HH:MM [--days Mon,Tue,…]` | Set a recurring active-hours window. Implicitly turns the toggle on. |
| `openlid schedule clear` | Remove the recurring window. |
| `openlid schedule show [--json]` | Print the current schedule. |
| `openlid config show / path / edit` | Inspect / edit config at `~/Library/Application Support/io.openlid.app/config.toml` |

### Preferences

Open the menu and click **Preferences…** (or ⌘,) to configure the native
sidebar settings window:

| Section | Settings |
|---|---|
| **General** | Start Open-Lid at login; activate Open-Lid at launch; keep display awake while preventing sleep. |
| **Safeguards** | Turn off below a battery percent; auto-disable in transit after a configurable number of minutes. |
| **Schedule** | Enable recurring active hours; choose 24-hour or AM/PM time entry; set From/To hour and minute dropdowns; choose active days. |

Details:

- **Start Open-Lid at login** — auto-launches via `SMAppService.mainApp()`.
- **Activate Open-Lid at launch** — when on, every launch starts armed.
  When off (the default), restores your last state.
- **Keep display awake while preventing sleep** — on by default. Holds
  an `IOPMAssertion` whenever sleep prevention is active, so the display
  doesn't idle-dim and the screen doesn't lock on idle. This also covers
  VNC/headless use with the lid closed. Turn it off if you'd rather the
  screen lock on idle even while Open-Lid is on; system sleep prevention
  still works.
- **Turn off below battery %** — auto-deactivate when battery falls below
  the threshold. Does not auto-reactivate when battery recovers; the user
  decides when to re-arm.
- **Auto-disable in transit** — auto-deactivate after the laptop appears
  to be packed away: lid closed, on battery, no external display, and no
  network for the configured number of minutes.
- **Active only during scheduled hours** — when on, sleep prevention is
  gated to a recurring time window (e.g. 09:00–18:00 weekdays). Sleep is
  allowed outside the window even when the toggle is on. The UI supports
  both 24-hour and AM/PM time picking.

## How it works

Two cooperating mechanisms:

1. **System-sleep prevention** — a privileged launchd daemon
   (`openlid-helper`) is the only component that can call `pmset -a
   disablesleep` (which requires root). The menu-bar app and CLI both
   talk to that daemon over NSXPC. The helper validates incoming
   connections by code-signature requirement string and auto-exits after
   15 seconds of idle.
2. **Display-sleep prevention** (idle dim / screen lock) — the menu-bar
   app holds an `IOPMAssertion` of type `PreventUserIdleDisplaySleep`
   while Open-Lid is on. No root needed. The lid-close
   `pmset displaysleepnow` call is still made separately to save battery
   on the built-in panel.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design.

## Configuration file

`~/Library/Application Support/io.openlid.app/config.toml`:

```toml
enabled = false                       # last toggle state (persisted)
start_at_login = false
activate_at_launch = false
prevent_display_sleep = true          # keep display awake on idle; false to allow screen lock
battery_threshold_pct = 20            # omit to disable battery auto-off
in_transit_timeout_minutes = 2        # omit to disable in-transit auto-off

[modifiers]                           # advanced / reserved for future UI
only_on_ac = false
min_battery = 20

# Optional recurring active-hours window. Omit to disable.
# [modifiers.schedule]
# start = "08:00"
# end = "18:00"
# days = ["Mon", "Tue", "Wed", "Thu", "Fri"]
```

## Updating

Open-Lid keeps every preference outside the `.app` bundle, so any of the
update paths below preserves your toggle state, schedule, and all other
settings.

**Homebrew users** (recommended):

```bash
brew upgrade --cask openlid/tap/openlid
```

Use the full `openlid/tap/openlid` cask token. Recent Homebrew versions
can refuse the shorter `brew upgrade openlid` form for third-party taps
that have not been explicitly trusted.

**Manual installs** — from the terminal:

```bash
openlid update           # checks GitHub, prompts, downloads, swaps, relaunches
openlid update --check   # just check; exits 0 if up to date, 1 if available
openlid update --yes     # non-interactive
openlid update --json    # machine-readable status
```

**Manual installs** — from the menu bar:

Click the menu bar icon → **Check for updates…**. A dialog reports the
result and offers an Install button when a newer release is available.

Nothing contacts the network unless you trigger one of these paths.

## Privacy

Open-Lid does not collect, transmit, or store any user data. No telemetry.
No analytics. No automatic update checks — Open-Lid only contacts GitHub
when you run `openlid update` or click "Check for updates…" in the menu.
All state stays on your machine in
`~/Library/Application Support/io.openlid.app/`.

If you opt in to the **Auto-disable in transit** preference, Open-Lid
asks macOS's `SCNetworkReachability` framework whether the public
Internet appears reachable — passively, by observing interface and
routing state. No outbound traffic is generated on Open-Lid's behalf;
the framework only reports what macOS already knows about the
network. The feature is off by default and can be disabled at any
time in Preferences.

The privileged helper writes a small marker file at
`/Library/Application Support/openlid/sleep-prevention.enabled` while sleep
is overridden — this lets it recover gracefully if the helper restarts
after a crash.

## Uninstall

**Homebrew installs**:

```bash
brew uninstall --cask openlid/tap/openlid
```

To also remove Open-Lid preferences, logs, and helper state:

```bash
brew uninstall --zap --cask openlid/tap/openlid
```

**Manual DMG installs**:

1. Quit Open-Lid from the menu bar icon.
2. If you enabled **Start Open-Lid at login**, turn it off in
   Open-Lid preferences or in **System Settings → General → Login Items**.
3. Stop the privileged helper if it is still loaded:

   ```bash
   sudo launchctl bootout system/io.openlid.helper 2>/dev/null || true
   ```

4. Delete the app:

   ```bash
   rm -rf /Applications/OpenLid.app
   ```

5. If you created the optional CLI symlink, remove it:

   ```bash
   sudo rm -f /usr/local/bin/openlid
   ```

6. Optional: remove preferences, logs, and helper state:

   ```bash
   rm -rf "$HOME/Library/Application Support/io.openlid.app"
   rm -rf "$HOME/Library/Application Support/io.openlid.open-lid"
   rm -rf "$HOME/Library/Application Support/Logs/openlid"
   rm -rf "$HOME/Library/Application Support/Logs/open-lid"
   rm -rf "$HOME/Library/Logs/openlid"
   rm -rf "$HOME/Library/Logs/open-lid"
   sudo rm -rf "/Library/Application Support/openlid"
   sudo rm -rf "/Library/Application Support/open-lid"
   sudo rm -rf /Library/Logs/openlid
   sudo rm -rf /Library/Logs/open-lid
   ```

**Source/dev installs**:

```bash
./scripts/dev-uninstall-helper.sh       # unloads dev helper + removes plist
rm -rf /Applications/OpenLid.app
sudo rm -f /usr/local/bin/openlid
```

## Troubleshooting

**Menu bar icon doesn't appear** — Make sure the helper is installed:
v0.2+ registers the helper automatically via `SMAppService` — but the
user has to approve it in **System Settings → General → Login Items →
Allow in the Background**. If you've ignored that prompt, click the menu
bar icon and the menu will show an approval hint. Then check
`~/Library/Application Support/Logs/openlid/app.log.<today>` for errors.

**"Apple cannot verify this app" on download** — Should not appear on
v0.2+ (the DMG is notarized). If you see it on a build from source, that's
expected: local `./scripts/install.sh` produces an ad-hoc-signed bundle.
Right-click → Open to bypass, or use the official notarized DMG.

**Two OpenLid entries in Spotlight** — You have a build artifact in your
project tree from an old `build-app-bundle.sh`. Re-run `./scripts/install.sh`;
the new install script cleans it up automatically.

**Sleep is still happening when I close the lid** — Check `pmset -g | grep
SleepDisabled`. If it shows `0`, the helper isn't being asked to override
sleep. Verify Open-Lid is *on* (`openlid status` should say "ON
(preventing sleep now)" with the lid closed).

**Screen still locks on idle while Open-Lid is on** — by default,
Open-Lid holds an `IOPMAssertion` that prevents the display from
sleeping while sleep prevention is active, including closed-lid VNC.
If your screen is still locking, check the "Keep display awake while
preventing sleep" checkbox in Preferences (or set
`prevent_display_sleep = true` in `config.toml`).

**I want the screen to lock on idle even when Open-Lid is on** — turn
off "Keep display awake while preventing sleep" in Preferences, or set
`prevent_display_sleep = false` in `config.toml`. System sleep is still
prevented; only the display-idle assertion is dropped.

## Roadmap

- [x] **v0.1 — Local MVP.** Menu bar app + CLI + preferences + helper.
- [x] **v0.2 — Signed distribution.** Notarized DMG, Homebrew tap,
  `SMAppService` daemon registration replacing the manual `sudo` install.
- [x] **v1.0 — Stable API.** Locked CLI, `config.toml` schema, and IPC
  surfaces under semver. Adds a `version` field to the config schema
  as a forward-compatibility hook for future v2.x. See
  [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).
- [ ] **v3.0.0 — Linux support.** Linux backend talking to systemd-logind
  via D-Bus (`Inhibit("sleep:handle-lid-switch")`), wired into the
  existing `openlid-core` platform traits. UI shape TBD — either a
  GTK/Qt tray icon or a headless daemon driven by the CLI; tracked in
  the v3.0.0 design discussion.
- [ ] **Future — Windows on demand.** Windows backend
  (`SetThreadExecutionState` + `WM_POWERBROADCAST`) ships if there's
  user demand. The `openlid-core` platform traits are already
  cross-platform-shaped, so this is a backend addition, not a rewrite.
  [Open an issue](https://github.com/openlid/openlid/issues/new/choose)
  to register interest.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). TL;DR: bug fixes welcome any time;
new features should go through an issue first.

## License

[Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for third-party
attributions.
