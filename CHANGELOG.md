# Changelog

All notable changes to Open-Lid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial public release infrastructure: CI, coverage, release automation.
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`.
- `version` field in `config.toml` (defaults to `1`; the schema's
  forward-compatibility hook for future v2.x). Pre-v1.0 configs load
  cleanly with no user action. Loading a config from a newer schema
  emits a warning and continues to load known fields rather than
  failing.
- `docs/COMPATIBILITY.md` declaring the v1.x semver promise: stable
  surfaces are the CLI subcommands/flags/exit codes, `status --json`
  output shape, `config.toml` field names, control-socket JSON wire
  shapes, and helper XPC method signatures.
- **Display-sleep prevention.** While Open-Lid is on and the lid is open
  (or an external display is attached), the menubar app holds an
  `IOPMAssertion` of type `PreventUserIdleDisplaySleep`, preventing the
  display from dimming and the screen from locking on idle — keep-awake-style
  equivalence. Released on lid-close without an external display so the
  existing `force_display_sleep` battery-saver still wins. New
  `prevent_display_sleep` field in `config.toml` (default `true`) and a
  matching "Keep display awake while preventing sleep" checkbox in
  Preferences. Opt out by either route to restore the v0.1 behavior.

### Changed

- Roadmap: removed v0.3 (schedule UI, state-change notifications, and
  configurable hotkey are not planned). v1.0 is the next milestone
  after v0.2.

### Fixed

- **Quit no longer silently disables sleep prevention.** The quit
  handler used to call `set_enabled(false, None)`, which not only
  released the helper but also persisted `enabled = false` to disk — so
  every relaunch came up as Off. Replaced with a new
  `StateRuntime::shutdown_cleanup` that releases runtime side-effects
  (helper sleep prevention + IOPMAssertion) without touching `AppState`
  or the on-disk config.
- **Documented config and control-socket paths corrected** from the
  friendlier `~/Library/Application Support/open-lid/...` to the
  actual `directories::ProjectDirs`-computed
  `~/Library/Application Support/io.openlid.open-lid/...` — affects
  README, CHANGELOG, COMPATIBILITY, and a couple of doc comments.

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
  `~/Library/Application Support/io.openlid.open-lid/config.toml`.
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
