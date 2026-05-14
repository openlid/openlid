<div align="center">

<img src="resources/app/AppIcon-readme.png" alt="Open-Lid" width="128" height="128" />

# Open-Lid

**Keep your Mac awake — even with the lid closed.**

[![CI](https://github.com/diyanbogdanov/open-lid/actions/workflows/ci.yml/badge.svg)](https://github.com/diyanbogdanov/open-lid/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/diyanbogdanov/open-lid/branch/main/graph/badge.svg)](https://codecov.io/gh/diyanbogdanov/open-lid)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/diyanbogdanov/open-lid)](https://github.com/diyanbogdanov/open-lid/releases/latest)
[![GitHub downloads](https://img.shields.io/github/downloads/diyanbogdanov/open-lid/total)](https://github.com/diyanbogdanov/open-lid/releases)
[![macOS 13+](https://img.shields.io/badge/macOS-13%2B-black.svg?logo=apple)](https://github.com/diyanbogdanov/open-lid)

</div>

Open-Lid is a tiny macOS menu bar utility that keeps your Mac awake —
with the lid open or closed. Carry your laptop around with a long build,
an agent, or a download running; or step away from your desk without
having macOS lock the screen and dim the display every time you check
your phone.

Inspired by [keep-awake-style](https://lightheadsw.com/keep-awake-style/) and ported from
[upstream](https://github.com/narcotic-sh) to Rust for a small
binary and a future-friendly architecture.

> [!NOTE]
> **Status:** pre-1.0. Apple Silicon, macOS 13+. **v0.2 ships signed and
> notarized** — no Gatekeeper warning. Helper installs automatically via
> `SMAppService` — no `sudo` required.

---

## Why?

You're at a meeting in a different room. You close your MacBook lid to
carry it. A coding agent is doing real work; your `cargo build` is 4
minutes from finishing; a long file is downloading. macOS sleeps the
system the moment the lid closes, killing everything.

Or: you're at your desk, you step away for five minutes, and you come
back to a locked screen for the third time today.

Open-Lid handles both:

- While the lid is **open**, the display stays awake — no idle dim, no
  screen lock. You can turn this off if you'd rather have the screen
  lock on idle.
- While the lid is **closed**, the system stays awake but the display
  turns off (so the battery doesn't suffer and the laptop doesn't heat
  up). If an external display is attached, that one stays awake.

Activate indefinitely or for a fixed duration. Auto-deactivate when the
battery gets low.

## Features

- **One-click toggle** in the menu bar (left-click). Right-click for the
  full menu.
- **Timed sessions** — Activate for 5 minutes, 30 minutes, 1 / 2 / 5
  hours, or indefinitely.
- **Display stays awake while preventing sleep** — no idle dim, no
  screen lock. Implemented via an `IOPMAssertion`, the same mechanism
  keep-awake-style uses. Opt out in Preferences if you want the screen to lock
  on idle.
- **Display off when lid closes** — your battery and your thermal envelope
  thank you. Skipped automatically when an external display is connected.
- **Native preferences window**:
  - Start at login
  - Activate at launch (or restore your last state)
  - Keep display awake while preventing sleep
  - Default duration for single-click toggles
  - Auto-deactivate below a configurable battery percent
- **First-class CLI** for scripting:
  ```
  open-lid on / off / status
  open-lid for 2h
  open-lid until 18:00
  ```
- **Single-instance** — running `open -a OpenLid` twice is a no-op.
- **No telemetry. No data leaves your machine. Ever.**

## Installation

### Download the signed DMG (recommended)

```bash
# Homebrew tap:
brew install --cask diyanbogdanov/tap/open-lid

# Or download the DMG directly:
# https://github.com/diyanbogdanov/open-lid/releases/latest
```

After installing, launch Open-Lid; macOS will prompt you to enable it in
**System Settings → General → Login Items → Allow in the Background**.
Flip the Open-Lid toggle on — no admin password required.

### From source

Prerequisites: macOS 13+ on Apple Silicon, Rust 1.88+, Xcode Command Line
Tools.

```bash
git clone https://github.com/diyanbogdanov/open-lid.git
cd open-lid

# Build (ad-hoc-signed, dev profile), install into /Applications, refresh caches:
./scripts/install.sh

# Optional: put `open-lid` on your PATH:
./scripts/install-cli-symlink.sh

# Launch:
open -a OpenLid
```

## Usage

### Menu bar

Click the laptop icon in the menu bar to toggle on/off. Right-click (or
option-click) to see the full menu:

```
Status: Active until 18:30 · lid closed · AC
─────────
Turn Off
─────────
Activate for ▸
   Indefinitely
   5 minutes
   10 minutes
   15 minutes
   30 minutes
   1 hour
   2 hours
   5 hours
─────────
Preferences…   ⌘,
─────────
Quit Open-Lid   ⌘Q
```

### CLI

| Command | What it does |
|---|---|
| `open-lid on` | Activate using your Default duration preference |
| `open-lid off` | Deactivate |
| `open-lid status` | Print current state. Use `--json` for machine-readable output. |
| `open-lid for <duration>` | Activate with a timer, e.g. `30m`, `2h`, `1h30m` |
| `open-lid until <time>` | Activate until `HH:MM` today (rolls over to tomorrow if past) |
| `open-lid config show / path / edit` | Inspect / edit config at `~/Library/Application Support/io.openlid.open-lid/config.toml` |

### Preferences

Open the menu and click **Preferences…** (or ⌘,) to configure:

- **Start Open-Lid at login** — auto-launches via `SMAppService.mainApp()`.
- **Activate Open-Lid at launch** — when on, every launch starts armed.
  When off (the default), restores your last state.
- **Keep display awake while preventing sleep** — on by default. Holds
  an `IOPMAssertion` whenever sleep prevention is active and the lid is
  open (or an external display is attached), so the display doesn't dim
  and the screen doesn't lock on idle. Turn it off if you'd rather the
  screen lock on idle even while Open-Lid is on; system sleep prevention
  still works.
- **Default duration** — what `open-lid on` and a single menu-bar click
  use. Defaults to *Indefinitely*.
- **Turn off below battery %** — auto-deactivate when battery falls below
  the threshold. Does not auto-reactivate when battery recovers; the user
  decides when to re-arm.

## How it works

Two cooperating mechanisms:

1. **System-sleep prevention** — a privileged launchd daemon
   (`open-lid-helper`) is the only component that can call `pmset -a
   disablesleep` (which requires root). The menu-bar app and CLI both
   talk to that daemon over NSXPC. The helper validates incoming
   connections by code-signature requirement string and auto-exits after
   15 seconds of idle.
2. **Display-sleep prevention** (idle dim / screen lock) — the menu-bar
   app holds an `IOPMAssertion` of type `PreventUserIdleDisplaySleep`
   while Open-Lid is on and the lid is open (or an external display is
   attached). No root needed. Released automatically when the lid closes
   with no external display so the battery-saving
   `pmset displaysleepnow` branch can land uncontested.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design.

## Configuration file

`~/Library/Application Support/io.openlid.open-lid/config.toml`:

```toml
enabled = false                       # last toggle state (persisted)
start_at_login = false
activate_at_launch = false
prevent_display_sleep = true          # keep display awake on idle; false to allow screen lock
default_duration_minutes = 30         # omit for Indefinite
battery_threshold_pct = 20            # omit to disable battery auto-off

[modifiers]                           # advanced / reserved for future UI
only_on_ac = false
min_battery = 20
```

## Privacy

Open-Lid does not collect, transmit, or store any user data. No telemetry.
No analytics. No automatic update checks. All state stays on your machine
in `~/Library/Application Support/io.openlid.open-lid/`.

The privileged helper writes a small marker file at
`/Library/Application Support/open-lid/sleep-prevention.enabled` while sleep
is overridden — this lets it recover gracefully if the helper restarts
after a crash.

## Uninstall

```bash
./scripts/dev-uninstall-helper.sh       # unloads helper + removes plist
rm -rf /Applications/OpenLid.app
rm -rf "~/Library/Application Support/io.openlid.open-lid"
sudo rm -f /usr/local/bin/open-lid
```

## Troubleshooting

**Menu bar icon doesn't appear** — Make sure the helper is installed:
v0.2+ registers the helper automatically via `SMAppService` — but the
user has to approve it in **System Settings → General → Login Items →
Allow in the Background**. If you've ignored that prompt, click the menu
bar icon and the menu will show an approval hint. Then check
`~/Library/Application Support/Logs/open-lid/app.log.<today>` for errors.

**"Apple cannot verify this app" on download** — Should not appear on
v0.2+ (the DMG is notarized). If you see it on a build from source, that's
expected: local `./scripts/install.sh` produces an ad-hoc-signed bundle.
Right-click → Open to bypass, or use the official notarized DMG.

**Two OpenLid entries in Spotlight** — You have a build artifact in your
project tree from an old `build-app-bundle.sh`. Re-run `./scripts/install.sh`;
the new install script cleans it up automatically.

**Sleep is still happening when I close the lid** — Check `pmset -g | grep
SleepDisabled`. If it shows `0`, the helper isn't being asked to override
sleep. Verify Open-Lid is *on* (`open-lid status` should say "ON
(preventing sleep now)" with the lid closed).

**Screen still locks on idle while Open-Lid is on** — by default,
Open-Lid holds an `IOPMAssertion` that prevents the display from
sleeping while the lid is open. If your screen is still locking, check
the "Keep display awake while preventing sleep" checkbox in Preferences
(or set `prevent_display_sleep = true` in `config.toml`). Note that the
assertion is intentionally released when you close the lid with no
external display attached — so the display can sleep and save battery.

**I want the screen to lock on idle even when Open-Lid is on** — turn
off "Keep display awake while preventing sleep" in Preferences, or set
`prevent_display_sleep = false` in `config.toml`. System sleep is still
prevented; only the display-idle assertion is dropped.

## Roadmap

- [x] **v0.1 — Local MVP.** Menu bar app + CLI + preferences + helper.
- [x] **v0.2 — Signed distribution.** Notarized DMG, Homebrew tap,
  `SMAppService` daemon registration replacing the manual `sudo` install.
- [ ] **v1.0 — Stable API.** Locked CLI, `config.toml` schema, and IPC
  surfaces under semver. Adds a `version` field to the config schema
  as a forward-compatibility hook for future v2.x. See
  [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).
- [ ] **v1.x — Cross-platform.** Linux (logind) and Windows
  (`SetThreadExecutionState`) implementations behind the existing
  `open-lid-core` traits.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). TL;DR: bug fixes welcome any time;
new features should go through an issue first.

## License

[Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for third-party
attributions (Tabler Icons, upstream, keep-awake-style).
