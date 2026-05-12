# Open-Lid

A macOS menu bar utility that prevents your Mac from sleeping when the lid is
closed, while letting the display turn off. Inspired by
[upstream](https://github.com/narcotic-sh) — this is a Rust port
with composable modes and a first-class CLI, designed for later expansion to
Windows and Linux.

**Status:** MVP for local use (Apple Silicon, macOS 13+). Production-signed
distribution coming in Plan 2.

## Why?

If you carry your MacBook between meetings and rooms while a coding agent or
long-running task runs on it, normal macOS sleeps the system when you close
the lid. Open-Lid lets you keep the system running while the screen turns off,
preserving battery and reducing heat.

## Quick Start (local dev)

```bash
# Build everything
cargo build --release -p open-lid -p open-lid-helper

# Build the .app and helper bundle
./scripts/build-app-bundle.sh
cp -R OpenLid.app /Applications/

# Install the privileged helper (one-time sudo)
./scripts/dev-install-helper.sh

# Optional: put `open-lid` on your PATH
./scripts/install-cli-symlink.sh

# Launch the menu bar app
open -a OpenLid

# Or use the CLI
open-lid on
open-lid status
open-lid for 2h
```

## CLI

| Command | What it does |
|---|---|
| `open-lid on` / `off` | Enable/disable sleep prevention with current mode |
| `open-lid status [--json]` | Show current state |
| `open-lid mode lid-closed` | Mode: prevent sleep only when lid is closed (default) |
| `open-lid mode always-awake` | Mode: prevent sleep regardless of lid |
| `open-lid for 2h` | Switch to Timed mode for the duration |
| `open-lid until 18:00` | Switch to Timed mode until the time |
| `open-lid config show / path / edit` | Inspect/edit `~/Library/Application Support/open-lid/config.toml` |

## How it works

A privileged launchd daemon (`open-lid-helper`) toggles `pmset -a disablesleep`
when asked. The menu bar app and CLI both talk to that daemon — the daemon
talks to no one else. Lid state is observed via IOKit `IOPMrootDomain`. On
lid close (no external display attached), the display is told to sleep with
`pmset displaysleepnow`. All state and reconciliation logic lives in a single
pure function in the `open-lid-core` crate.

See [docs/superpowers/specs/2026-05-10-open-lid-design.md](docs/superpowers/specs/2026-05-10-open-lid-design.md)
for the full design.

## Uninstall (local dev)

```bash
./scripts/dev-uninstall-helper.sh
rm -rf /Applications/OpenLid.app
sudo rm -f /usr/local/bin/open-lid
```

## License

MIT.
