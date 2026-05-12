# Changelog

All notable changes to Open-Lid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial public release infrastructure: CI, coverage, release automation.
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`.

## [0.1.0] - 2026-05-12

First tagged release. Local-use MVP.

### Added

- **Menu bar app** with a custom Tabler-derived laptop icon. Left-click
  toggles sleep prevention; right-click or option-click opens the menu.
- **Activate-for-duration submenu** with Indefinitely / 5 / 10 / 15 / 30
  minutes / 1 / 2 / 5 hours.
- **Preferences window** (native AppKit NSWindow) with four controls:
  - Start Open-Lid at login (via `SMAppService.mainApp()`)
  - Activate Open-Lid at launch
  - Default duration for single-click activation
  - Auto-deactivate when battery falls below configurable percent
- **CLI** (`open-lid on/off/status/for/until/config`) auto-launches the
  menu bar app if it isn't running.
- **Privileged helper daemon** (`open-lid-helper`) toggles
  `pmset -a disablesleep` over NSXPC, validating callers by code-signature
  requirement string.
- **Single-instance enforcement** via control-socket probe.
- **Display-off-on-lid-close** behavior when no external display is
  attached — preserves the "Open-Lid" name's promise.
- **Proactive timer expiry** scheduler with generation-counter race
  protection.
- **Configuration persistence** at
  `~/Library/Application Support/open-lid/config.toml`.
- **Logging** to `~/Library/Application Support/Logs/open-lid/` (rotated
  daily; will move to `~/Library/Logs/open-lid/` in a future release).

### Known limitations

- macOS Apple Silicon only; Intel and older macOS versions untested.
- The helper installs via `scripts/dev-install-helper.sh` (requires `sudo`).
  Production `SMAppService` install path requires Apple Developer ID
  signing — coming in v0.2.0 alongside notarized releases.
- DMG releases are unsigned at v0.1.0; users see Gatekeeper warning on
  first launch. Right-click → Open to bypass, or build from source.
- Schedule modifier (active hours / days) is in the config schema but
  not yet exposed in the preferences UI.

[Unreleased]: https://github.com/diyanbogdanov/open-lid/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/diyanbogdanov/open-lid/releases/tag/v0.1.0
