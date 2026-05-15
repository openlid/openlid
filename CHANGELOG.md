# Changelog

All notable changes to Open-Lid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Homebrew cask now puts `open-lid` on `$PATH`.** Added a `binary` stanza
  that symlinks `/Applications/OpenLid.app/Contents/MacOS/open-lid` into
  `$HOMEBREW_PREFIX/bin/`, so `brew install --cask openlid/tap/open-lid`
  alone is enough to use the CLI from the terminal — no separate
  `install-cli-symlink.sh` step. The symlink is brew-managed and removed
  automatically on `brew uninstall --cask open-lid`. The from-source flow
  is unchanged.

## [1.0.0] - 2026-05-14

The "stable API" release. The CLI, `config.toml` schema, and IPC wire shapes
now ship under a binding semver promise — see
[docs/COMPATIBILITY.md](docs/COMPATIBILITY.md). No new user-visible features
versus v0.2.0; this is the moment those surfaces become a contract.

### Changed

- **Compatibility promise is binding.** Surfaces enumerated in
  `docs/COMPATIBILITY.md` (CLI subcommands and flags, exit codes, the
  `status --json` field set, `config.toml` field names, control-socket
  request/response/snapshot shapes, helper XPC method signatures) are
  locked under v1.x semver. Breaking changes require a v2.0 release.
  Additive changes (new subcommands, new optional config fields, new
  response variants) are explicitly allowed and do not constitute a break.

### Removed

- **Stub `uninstall` CLI subcommand** and its `ControlRequest::Uninstall`
  wire variant. The stub printed a "coming in a future release" message
  and never had a working implementation; locking it under semver would
  have committed the project to either delivering it or carrying the
  dead-letter wire variant forever. Removed entirely. Homebrew's
  `brew uninstall --cask open-lid` (plus the standard
  `~/Library/Application Support/io.openlid.open-lid` cleanup) remains
  the supported uninstall path for cask installs; manual uninstall steps
  are documented in the README.
- **Internal `unregister()` helper** in the SMAppService installer module.
  Its only documented caller was the stubbed uninstall command; with that
  gone, the function had zero call sites.

### Fixed

- **Stale dev-process language** in code comments, `scripts/dev-install-helper.sh`,
  and the manual test checklist (references to internal "Plan 1" / "Plan 2"
  development phases that completed during the v0.1 → v0.2 cycle).
  Rephrased to describe the actual user-visible behavior.
- **`docs/manual-test-checklist.md` rewritten.** The previous version
  referenced features that never shipped or were removed (the `Mode`
  submenu, ghost CLI command `open-lid mode <name>`, eye-slash icon).
  Replaced with a smoke checklist that matches what v1.0 actually ships.

## [0.2.0] - 2026-05-14

The "signed and notarized" release. Downloaders no longer see an
"Apple cannot verify this app is free of malware" warning. Helper
installation no longer requires `sudo`.

### Added

- **Signed and notarized DMG distribution.** Releases are produced by
  GitHub Actions: built on `macos-latest`, signed with the project's
  Developer ID Application certificate, notarized via Apple's notary
  service, stapled, and published as a draft GitHub Release. Each
  release also includes a `.sha256` checksum file.
- **SMAppService daemon registration.** The menubar app calls
  `SMAppService.daemon(plistName:).register()` at startup. On first
  launch the user sees a one-time "Allow in the Background" toggle in
  System Settings → Login Items. No more `sudo` install step.
- **Per-profile helper code-requirement.** Builds compile in either
  `dev` (permissive: bundle-id-only) or `prod` (Developer ID +
  Team-ID-pinned) profile, selected via `OPEN_LID_HELPER_PROFILE`. CI
  release builds always use `prod`; local `./scripts/build-app-bundle.sh`
  uses `dev` unless `PROFILE=release` is set. The default if
  misconfigured is `prod` — fail-safe to strict.
- **`version` field in `config.toml`** (defaults to `1`). Forward-
  compatibility hook for future v2.x. Pre-v1.0 configs load cleanly with
  no user action. Loading a newer-schema config emits a warning and
  continues with known fields rather than failing.
- **`docs/COMPATIBILITY.md`** declaring the v1.x semver promise: stable
  surfaces are CLI subcommands/flags/exit codes, `status --json` output
  shape, `config.toml` field names, control-socket JSON wire shapes, and
  helper XPC method signatures.
- **Display-sleep prevention.** While Open-Lid is on and the lid is open
  (or an external display is attached), the menubar app holds an
  `IOPMAssertion` of type `PreventUserIdleDisplaySleep`, preventing the
  display from dimming and the screen from locking on idle.
  Released on lid-close without an external display so the existing
  `force_display_sleep` battery-saver still wins. New
  `prevent_display_sleep` field in `config.toml` (default `true`) and a
  matching "Keep display awake while preventing sleep" checkbox in
  Preferences. Opt out by either route to restore the v0.1 behavior.
- **Public-launch repo infrastructure:** Apache 2.0 license, NOTICE
  with third-party attributions, CONTRIBUTING / CODE_OF_CONDUCT /
  SECURITY / CHANGELOG / ARCHITECTURE docs, GitHub issue + PR templates,
  CI workflow (fmt + clippy + test + build + audit + coverage), Dependabot
  config, Homebrew cask draft.

### Changed

- **MSRV** bumped to **Rust 1.88** (was 1.81) to pick up `time 0.3.47`,
  which resolves RUSTSEC-2026-0009 (stack-exhaustion DoS).
- **Roadmap pruned:** v0.3 dropped (schedule UI, state-change notifications,
  and configurable hotkey are not planned). v1.0 is the next milestone.

### Fixed

- **Quit no longer silently disables sleep prevention** on next launch.
  The quit handler used to call `set_enabled(false, None)`, which not
  only released the helper but persisted `enabled = false` to disk.
  Replaced with `StateRuntime::shutdown_cleanup` that releases runtime
  side-effects without touching `AppState` or the on-disk config.
- **Helper survives missing-helper case.** When the helper isn't yet
  registered, the NSXPC remote proxy is degenerate; objc2's debug-mode
  method-existence check used to panic. Now guarded by a proactive
  invalidation flag + `catch_unwind` race protection; the app degrades
  gracefully to "helper unavailable" errors.
- **NSXPC privileged-flag** (`kNSXPCConnectionPrivileged`, `1 << 12`)
  now passed when connecting to the system-domain helper. Required for
  the `/Library/LaunchDaemons` registration path.
- **Documented config and control-socket paths corrected** from
  `~/Library/Application Support/open-lid/...` to the actual
  `directories::ProjectDirs`-computed
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

- macOS Apple Silicon only (Intel and older macOS untested; Linux and
  Windows planned for future major releases).
- The helper installs via `scripts/dev-install-helper.sh` (requires `sudo`).
  Production `SMAppService` install path requires Apple Developer ID
  signing — coming in v0.2.0 alongside notarized releases.
- DMG releases are unsigned at v0.1.0; users see Gatekeeper warning on
  first launch. Right-click → Open to bypass, or build from source.
- Schedule modifier (active hours / days) is in the config schema but
  not yet exposed in the preferences UI.

[Unreleased]: https://github.com/openlid/open-lid/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/openlid/open-lid/releases/tag/v1.0.0
[0.2.0]: https://github.com/openlid/open-lid/releases/tag/v0.2.0
[0.1.0]: https://github.com/openlid/open-lid/releases/tag/v0.1.0
