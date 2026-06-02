# Changelog

All notable changes to Open-Lid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.3.0] - 2026-06-02

### Changed

- **Preferences now render from the approved mock UI.** The macOS
  preferences window keeps the native window shell but renders the settings
  surface in a bundled WebView so the sidebar, safeguard cards, schedule
  controls, spacing, and typography match the mockups.
- **Preferences changes use a typed Rust/WebView bridge.** The WebView sends
  typed JSON messages into Rust, and Rust continues to own persistence through
  the existing `PrefsActions` pipeline.

## [2.2.0] - 2026-06-02

### Changed

- **Preferences window redesigned around a sidebar.** Settings are now
  grouped into General, Safeguards, and Schedule sections instead of one
  long form, making it clearer which controls affect launch behavior,
  automatic turn-off rules, and recurring active hours.
- **Schedule time picking uses native dropdowns.** The Schedule section
  now supports a 24-hour / AM-PM selector and separate hour/minute
  dropdowns, removing the fragile free-text time fields.
- **Battery and in-transit values are easier to change.** Numeric
  safeguards now include steppers alongside their visible values, so
  edits dispatch immediately and persist through the existing
  preferences pipeline.

## [2.1.0] - 2026-05-31

Open-Lid remains macOS-only in v2.1.0. Linux support is planned for
v3.0.0, where the platform backend can ship without stretching the v2.x
compatibility promise.

### Added

- **Recurring schedule.** `[modifiers.schedule]` in `config.toml` and the
  matching `openlid schedule set/clear/show` CLI subcommands gate sleep
  prevention to a recurring time window (e.g. `09:00-18:00 Mon-Fri`).
  The Preferences window grows a master "Active only during scheduled
  hours" checkbox plus From/To fields and seven day-of-week checkboxes.
  Outside the window, sleep is allowed even when the toggle is on.
  Setting a schedule from CLI or UI implicitly turns the toggle on.

### Removed

- **`openlid for <duration>` and `openlid until <time>` removed.** Use
  `openlid schedule set --from HH:MM --to HH:MM` for a recurring window
  instead. One-off timed sessions are no longer supported.
- **"Activate for" menubar submenu removed.** Same rationale — the
  recurring schedule replaces ad-hoc timers.
- **"Default duration" preference removed.** `openlid on` and a single
  menubar click now always start an indefinite session.
- **`default_duration_minutes` config field removed.** v2.0 configs that
  carry this field load cleanly under v2.1 (serde ignores the unknown
  field); the value is dropped on the next save.

These surfaces were listed as stable in `docs/COMPATIBILITY.md` under
v2.0. The project trimmed them in v2.1 rather than carrying them to
v3.0; see [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) for the
updated stable-surface list.

## [2.0.0] - 2026-05-15

The rebrand release. The hyphenated `open-lid` name has been retired
everywhere the user can see it — terminal command, cask, Cargo crates,
configuration directory, **and the GitHub repo itself**
(`openlid/open-lid` → `openlid/openlid`; old URL redirects). The macOS
bundle IDs (`io.openlid.app`, `io.openlid.helper`) are unchanged.
v1.x users get their config auto-migrated on first launch of v2.0.

### Changed (breaking)

- **CLI binary renamed `open-lid` → `openlid`.** Every invocation in scripts
  needs to be updated; `open-lid status` is now `openlid status`. Per
  [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md), renaming the CLI surface
  requires a major-version bump; this release is that bump.
- **Cask renamed `open-lid` → `openlid`.** Install is now
  `brew install --cask openlid/tap/openlid`. Existing v1 cask installs
  must be removed first: `brew uninstall --cask open-lid && brew install --cask openlid/tap/openlid`.
  The new cask's `binary` stanza puts `openlid` directly on `$PATH` — no
  separate `install-cli-symlink.sh` step.
- **Cargo crates renamed.** `open-lid` → `openlid`, `open-lid-core` →
  `openlid-core`, `open-lid-helper` → `openlid-helper`,
  `open-lid-helper-protocol` → `openlid-helper-protocol`. Downstream
  consumers that depend on these crates need to update their `Cargo.toml`.
- **Configuration directory changed.** Was
  `~/Library/Application Support/io.openlid.open-lid/`; now
  `~/Library/Application Support/io.openlid.app/` (matching the app's
  bundle ID, ending the redundant hyphenated suffix). v1 users do not
  need to copy files — v2 reads the v1 directory on first launch if the
  v2 directory doesn't exist yet, then writes to the v2 path going
  forward. The v1 directory is left in place for safety; users can
  `rm -rf ~/Library/Application\ Support/io.openlid.open-lid` once v2 is
  confirmed working.
- **Log directories changed.** `~/Library/Logs/open-lid/` →
  `~/Library/Logs/openlid/`. System-side
  `/Library/Application Support/open-lid/sleep-prevention.enabled` →
  `/Library/Application Support/openlid/sleep-prevention.enabled`.
- **Privileged helper binary renamed `open-lid-helper` → `openlid-helper`.**
  The launchd plist (`io.openlid.helper.plist`) is unchanged in name, but
  its `ProgramArguments` now points to the renamed binary. v1 → v2
  upgrade reinstalls the helper via `SMAppService` on first launch.

### Added

- **New app icon.** Flat teal (`#2688a8`) squircle with a white Tabler
  laptop glyph. The four corners outside the squircle are genuinely
  transparent (alpha = 0), so the icon composites cleanly against any
  wallpaper, Dock theme, or README background. Regeneratable from
  [scripts/generate-icon.sh](scripts/generate-icon.sh); the script now
  renders the SVG via a Swift one-liner (`NSImage` + `NSBitmapImageRep`
  with `hasAlpha: true`) instead of `qlmanage -t`, because `qlmanage`
  bakes in an opaque white background regardless of the SVG's alpha.
  Still no Homebrew dependencies — Swift ships with Xcode CLT, which
  the project already requires.
- **Auto-migration of v1 config on first launch.** On startup, v2 checks
  whether `~/Library/Application Support/io.openlid.app/config.toml`
  exists; if not but `io.openlid.open-lid/config.toml` does, v2 reads
  from the v1 path. The next write lands at the v2 path.

### Migration guide

For brew installs:

```bash
brew uninstall --cask open-lid
brew install --cask openlid/tap/openlid
```

For from-source installs:

```bash
git pull
cargo install --path crates/app  # rebuilds as `openlid`
```

For scripts/automations that call the CLI: replace `open-lid` with `openlid`.

For dependents of the Rust crates: rename in your `Cargo.toml`:

```toml
# v1.x
open-lid-core = "1"
# v2.0
openlid-core = "2"
```

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

[Unreleased]: https://github.com/openlid/openlid/compare/v2.3.0...HEAD
[2.3.0]: https://github.com/openlid/openlid/compare/v2.2.0...v2.3.0
[2.2.0]: https://github.com/openlid/openlid/compare/v2.1.0...v2.2.0
[2.1.0]: https://github.com/openlid/openlid/releases/tag/v2.1.0
[2.0.0]: https://github.com/openlid/openlid/releases/tag/v2.0.0
[1.0.0]: https://github.com/openlid/openlid/releases/tag/v1.0.0
[0.2.0]: https://github.com/openlid/openlid/releases/tag/v0.2.0
[0.1.0]: https://github.com/openlid/openlid/releases/tag/v0.1.0
