# `openlid update` and Check-for-Updates — design

## Summary

Adds two user-initiated update paths:

1. A CLI subcommand: `openlid update [--check] [--yes] [--json]`
2. A menubar menu item: "Check for updates…"

Both fetch the latest release from GitHub, compare versions, and for
*manual* installs do the full download-verify-swap-relaunch flow. For
*Homebrew* installs, both surface a clear instruction to run
`brew upgrade --cask openlid/tap/openlid` and stop — we never fight `brew`.

User settings persist across the update for free: the config lives at
`~/Library/Application Support/io.openlid.app/config.toml`, outside the
`.app` bundle. Swapping the bundle does not touch any user state.

## Goals

- A user on a manual DMG install can run `openlid update` and end up
  on the latest version, with the toggle in the same on/off state
  they had before, schedule and other preferences preserved, no
  manual drag-to-Applications step.
- A user on Homebrew gets a clear, single-command instruction
  (`brew upgrade --cask openlid/tap/openlid`) rather than a competing install path.
- A user can check for updates from the menubar without touching
  the terminal.
- The README's "no automatic update checks" privacy stance is
  preserved verbatim — every path requires user action.

## Non-goals

- Background update checks of any kind. No daily timer, no launch-time
  ping.
- Auto-installing security patches.
- Update channels (stable/beta). One channel: latest GitHub release.
- Rollback / version pinning.
- Updating via SMAppService (the helper updates implicitly when the
  bundle does, because the launchd plist points inside the bundle).
- Touching Homebrew metadata directly.

## CLI surface

```
openlid update            # interactive: check, prompt, install
openlid update --check    # check-only; exit 0 = up to date, exit 1 = available
openlid update --yes      # non-interactive install (for scripts/automation)
openlid update --json     # machine-readable status, no install
```

Flag combinations:

- `--check --json` -> JSON status, never installs.
- `--yes --json` -> JSON status plus performs install (status printed
  before the parent exits and the installer takes over).
- `--check --yes` -> rejected with an error; the flags contradict.

Exit codes for `--check` (matching standard "check tool" conventions):

- `0` — up to date
- `1` — update available (the only "non-error" non-zero — lets shells
  do `openlid update --check || openlid update --yes`)
- `2+` — actual errors (no network, bad response, version parse failed)

## Install-method detection

Resolve `/Applications/OpenLid.app` to its canonical path via
`std::fs::canonicalize`. If the resolved path is under either:

- `/usr/local/Caskroom/openlid` (Intel-prefix Homebrew)
- `/opt/homebrew/Caskroom/openlid` (Apple-silicon Homebrew)

then the install is **Homebrew**. Otherwise treat as **manual**.

Edge case: a user with Homebrew installed but who manually downloaded
the DMG has no `Caskroom/openlid` directory — correctly classified as
manual.

For `openlid update` called on a Homebrew install:

```
You installed openlid via Homebrew. To update, run:

  brew upgrade --cask openlid/tap/openlid

This will pull the latest cask and replace the .app for you.
```

Exit code 0 (advisory, not an error). The user is on the canonical
update path for their install method.

## Release fetching

```
GET https://api.github.com/repos/openlid/openlid/releases/latest
Accept: application/vnd.github+json
User-Agent: openlid/<CARGO_PKG_VERSION>
```

Parse the relevant subset:

```rust
struct ReleaseInfo {
    tag_name: String,           // e.g. "v2.1.0"
    body: String,               // release notes (used for confirm prompt)
    assets: Vec<AssetInfo>,
}
struct AssetInfo {
    name: String,               // "OpenLid-v2.1.0.dmg"
    browser_download_url: String,
    size: u64,
    digest: Option<String>,     // "sha256:<hex>" from GitHub API
}
```

Strip leading `v` from `tag_name` and compare against
`env!("CARGO_PKG_VERSION")` via the `semver` crate. Any parse error
on either side is a hard error (exit code 2+).

Pick the DMG asset: filename ending in `.dmg`. There is currently only
one DMG per release. If multiple are added later (Intel + ARM), pick
the one matching the host arch — but for now the cask only ships ARM,
so we error explicitly on a non-ARM host.

## Download + verify

Destination: `~/Library/Caches/io.openlid.app/updates/OpenLid-v<ver>.dmg`
(via `ProjectDirs::cache_dir`). The updates subdirectory is wiped at the
start of each `download_dmg` call so a failed mid-download from a previous
run doesn't accumulate, and so we always download into a known-empty dir.
The installer script removes the DMG file itself at the end of a
successful install.

```rust
fn download_dmg(url: &str, dest: &Path) -> Result<()> { ... }
fn verify_sha256(file: &Path, expected_hex: &str) -> Result<()> { ... }
```

If `asset.digest` is present, verify SHA-256 against it. Format on the
wire is `sha256:<hex>`; strip the prefix.

If `asset.digest` is absent (older releases, GitHub backfill miss), log
a warning and proceed to download. The bytes are still TLS-protected in
transit from GitHub, and — crucially — the detached installer verifies the
new bundle's Developer ID signature with `codesign --verify -R=<requirement>`
(pinned to our Team ID, the same anchor the helper requires of XPC clients)
*before* the destructive swap. A bundle not signed by us is rejected and the
existing app is left untouched.

Note we do NOT rely on Gatekeeper here: the DMG is downloaded
programmatically, so it never receives the `com.apple.quarantine` attribute
that triggers a Gatekeeper launch-time assessment. `open`-ing an
unquarantined app performs no signature check, which is exactly why the
installer must verify the signature itself.

## Installer (manual installs)

Two-phase: a detached shell script that survives the parent's exit
performs the destructive operations.

### Phase 1 — staged in-process

Inside `openlid update --yes` (or after user confirms `openlid update`):

1. Download DMG to cache dir.
2. Verify SHA-256 (if available).
3. Write installer script to `/tmp/openlid-installer-<pid>.sh`
   (template-substituted with the DMG path, the parent PID, and the
   target install path).
4. Make it executable.
5. Spawn it detached: `setsid nohup sh /tmp/openlid-installer-*.sh
   >/tmp/openlid-installer-*.log 2>&1 </dev/null &`
6. Print "Installing in the background; OpenLid will relaunch in a few
   seconds." Exit 0.

### Phase 2 — detached script

```sh
#!/bin/sh
set -eu

PARENT_PID="<substituted>"
DMG_PATH="<substituted>"
APP_PATH="/Applications/OpenLid.app"

# (1) Wait for the parent to exit so the running CLI's binary file
#     handle is released. kill -0 returns 0 if the process exists.
while kill -0 "$PARENT_PID" 2>/dev/null; do sleep 0.2; done

# (2) Kill any remaining menubar instance.
pkill -f "$APP_PATH/Contents/MacOS/openlid" 2>/dev/null || true
sleep 0.5

# (3) Mount the DMG and capture the volume path.
MOUNT_OUTPUT="$(hdiutil attach -nobrowse -readonly -plist "$DMG_PATH")"
VOLUME_PATH="$(echo "$MOUNT_OUTPUT" | plutil -extract \
    'system-entities.0.mount-point' raw - || true)"
# Fall back to scanning if the first entity isn't the volume:
if [ -z "$VOLUME_PATH" ] || [ ! -d "$VOLUME_PATH" ]; then
    VOLUME_PATH="$(echo "$MOUNT_OUTPUT" \
        | grep -Eo '/Volumes/[^<]+' | head -1)"
fi

# (4) Stage new .app next to the old, then atomically swap.
cp -R "$VOLUME_PATH/OpenLid.app" "$APP_PATH.new"
rm -rf "$APP_PATH"
mv "$APP_PATH.new" "$APP_PATH"

# (5) Detach the DMG; ignore errors (it may auto-detach).
hdiutil detach "$VOLUME_PATH" 2>/dev/null || true

# (6) Refresh LaunchServices/Spotlight metadata, same as
#     scripts/dev-install-app.sh.
touch "$APP_PATH"
mdimport "$APP_PATH" 2>/dev/null || true
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/\
LaunchServices.framework/Versions/A/Support/lsregister -f "$APP_PATH" \
    2>/dev/null || true

# (7) Relaunch the app. The bundle identifier is preserved across
#     versions; `open -b` resolves through LaunchServices.
open -b io.openlid.app

# (8) Clean up the staged DMG file to keep the cache dir tidy.
rm -f "$DMG_PATH"
```

### Atomicity and failure modes

- `cp -R` to `.new` THEN `rm -rf` of the old THEN `mv .new` -> the only
  destructive step is the `rm -rf`, and at that point the new bundle
  is fully staged. A small window exists where neither `OpenLid.app`
  nor `OpenLid.app.new` lives at the canonical path -- this is fine
  because the menubar was killed in step 2; LaunchServices will
  re-read on next `open`.
- If `cp -R` fails (disk full, permission error), the old .app is
  intact. The script aborts; the user sees the log file path.
- If `hdiutil attach` fails, ditto.
- If the parent crashes between step 1 and step 5 in the in-process
  phase, the cache dir holds a verified DMG but no installer ran.
  The user can re-run `openlid update`.

### Helper daemon

The launchd plist at `/Library/LaunchDaemons/io.openlid.helper.plist`
points at `/Applications/OpenLid.app/Contents/MacOS/openlid-helper`.
After the swap, the path is unchanged but the binary is new. The
currently-running helper (if any) idle-exits 15 s after the last XPC
connection; launchd spawns the new binary on the next connection.
No manual restart needed; no `launchctl bootout` from the installer.

### State preservation

`~/Library/Application Support/io.openlid.app/config.toml` and the
helper's marker file at
`/Library/Application Support/openlid/sleep-prevention.enabled` are
outside the .app bundle. The swap does not touch them. The relaunched
menubar reads the same config and applies the same `enabled`,
`modifiers.schedule`, `start_at_login`, `activate_at_launch`,
`default_duration_minutes`, `battery_threshold_pct`, and
`prevent_display_sleep` values. No migration needed; this is just
the normal launch path.

The transient `until` (timer-based auto-deactivate) is intentionally
not persisted today and is not persisted by this change either. A
user with a timer running who updates will land in the persisted
`enabled` state (no timer). This is consistent with how a normal
restart works today.

## Menubar UI

Add a new `NSMenuItem` between "Preferences…" and "Quit" in the
existing menu structure (`crates/app/src/menubar/menu.rs`):

```
Title: "Check for updates…"
Action: triggers MenuActions::check_for_updates()
```

When clicked:

1. Spawn a worker thread (HTTP is blocking; AppKit needs main free).
2. Worker calls `release::fetch_latest()`.
3. Result is sent back via `main_thread::run_on_main` for UI display.
4. Display an `NSAlert`:
   - Up to date -> "OpenLid is up to date (vX.Y.Z)" + [OK]
   - Update available -> "Update available: vX.Y.Z" with release
     notes in the informative text + [Install Now] / [Later]
   - Error -> "Couldn't reach update server: <reason>" + [OK]
5. "Install Now" path: same flow as the CLI's `update --yes`. The
   alert closes, a brief notification ("Installing in the
   background…"), and the detached installer takes over.

For Homebrew installs, the alert instead reads "Update available;
run `brew upgrade --cask openlid/tap/openlid` in your terminal" + [OK] -- no install
button.

## README update

The README at line 226 reads:

> No analytics. No automatic update checks. All state stays on your
> machine in `~/Library/Application Support/io.openlid.app/`.

Replace with:

> No analytics. No automatic update checks. Open-Lid never contacts
> any server unless you run `openlid update` or click "Check for
> updates…" in the menu. All state stays on your machine in
> `~/Library/Application Support/io.openlid.app/`.

Add a short "Updating" section after "Install":

```
## Updating

Homebrew users: `brew upgrade --cask openlid/tap/openlid` (the recommended path).

Other installs:

- From the terminal: `openlid update` checks GitHub, prompts you,
  and replaces the .app for you. Your toggle state, schedule, and
  every other preference is preserved.
- From the menu bar: click the icon, then "Check for updates…".

No automatic checks: nothing contacts the network unless you trigger
an update.
```

## Dependencies

New workspace dependencies (added to `Cargo.toml [workspace.dependencies]`):

- `ureq` (~50 KB minified) with the `rustls` feature so the binary
  has no OpenSSL dependency. Sync API matches our CLI structure;
  the menubar wraps it in a worker thread.
- `semver` ("1" pinned; minimal API surface).
- `sha2` ("0.10"; standard for sync SHA-256 in Rust).

Only `crates/app` depends on these (not `core`, since `core` must stay
platform-agnostic and HTTP-free).

## Tests

### Pure / unit-testable

- `release::parse_release_json` — fixture JSON -> `ReleaseInfo`.
- `release::pick_dmg_asset` — assets list -> the `.dmg` entry; error
  if none.
- `release::strip_v_prefix` — `v2.1.0` -> `2.1.0`; `2.1.0` -> `2.1.0`;
  `latest` -> error.
- `release::is_newer_than_current` — semver comparison wrapper.
- `release::parse_digest` — `sha256:abc...` -> `abc...`; other
  algorithms -> error; missing -> None.
- `install_detect::is_homebrew_install` — fed a canonical path,
  returns boolean. Test the Intel-prefix, ARM-prefix, and
  non-Caskroom cases.
- `installer::verify_sha256_matches` — write a known-content tempfile,
  feed correct/incorrect expected hex.
- `installer::render_installer_script` — pure string templating:
  parent_pid, dmg_path, app_path substituted into the script body.
  Snapshot-test the output.
- `cli::tests::parses_update_with_check_flag` etc.
- `cli::tests::rejects_update_with_check_and_yes` — flag conflict.

### Manual / integration (added to manual-test-checklist.md)

- `openlid update --check` on the current version -> up-to-date message.
- Bump `Cargo.toml` version DOWN to 1.0.0 locally, build, run
  `openlid update --check` -> "update available" message.
- `openlid update --yes` on a manual install -> downloads, swaps,
  relaunches; toggle state preserved.
- Same flow on a Homebrew install -> tells user to run brew upgrade.
- Menubar -> Check for updates… -> NSAlert appears with the right text.
- Network down -> `openlid update --check` exits with code 2+, message.
- Tampered DMG (modify a byte after download but before script runs;
  hard to script -- inspect the SHA-mismatch error path manually).

### Coverage target

The pure modules above should land at >90% line coverage per file.
The installer script generator is tested via snapshot. The actual
shell execution and the AppKit click handlers fall under manual
checklist coverage, same convention as the rest of the menubar code.

## Files touched

NEW:
- `crates/app/src/updater/mod.rs`
- `crates/app/src/updater/release.rs`
- `crates/app/src/updater/install_detect.rs`
- `crates/app/src/updater/installer.rs`
- `docs/superpowers/specs/2026-05-22-openlid-update-design.md`

MODIFIED:
- `Cargo.toml` (workspace) — `ureq`, `semver`, `sha2`
- `crates/app/Cargo.toml`
- `crates/app/src/cli/mod.rs` — `Update(UpdateArg)` subcommand
- `crates/app/src/cli/commands.rs` — `update` dispatcher + helpers
- `crates/app/src/menubar/menu.rs` — "Check for updates…" item
- `crates/app/src/menubar/mod.rs` — `RuntimeActions::check_for_updates`
- `README.md` — privacy clarification + Updating section
- `docs/manual-test-checklist.md` — new manual cases

## Risks

- **macOS code signing**: the downloaded DMG must be signed by our
  Developer ID, same as today. The CI's `release.yml` already produces
  notarized DMGs. If a future build accidentally ships unsigned (or a
  bundle were tampered with), the installer's `codesign --verify` step
  rejects it before the swap and aborts with the existing app intact —
  we do NOT depend on a Gatekeeper launch-time check, which never fires
  for the unquarantined, programmatically-downloaded bundle.
- **Detached script lifetime**: a `setsid nohup ... &` should be
  enough on macOS, but the script must close stdin/stdout/stderr to
  avoid TTY hang. We redirect to a log file.
- **Disk-full / permission errors**: the `cp -R` step fails loudly,
  the old .app stays intact. Surfaced in the log file at
  `/tmp/openlid-installer-<pid>.log`.
- **GitHub rate limit**: 60 requests/hr unauthenticated. The CLI is
  manual; the menubar is click-driven. Reaching 60 requires deliberate
  hammering. Documented as a non-issue.
- **Self-update from a dev build** (running from `target/`): detect
  this case and refuse with a message ("you're running a dev build;
  rebuild from source"). Treat any install path not under
  `/Applications/OpenLid.app` as dev.
- **Helper signing requirement post-swap**: the new helper binary
  inside the swapped .app must match the Developer ID anchor in the
  helper's code-requirement string. Same Apple Developer ID issues
  signed releases -> matches. A mid-version cert rotation would
  break helper validation; out of scope for this PR.

## Open questions resolved

- **Should the update touch Homebrew?** No. Homebrew has its own
  cask metadata and `brew upgrade` is its canonical path. We detect
  and route the user there.
- **Should we auto-check on launch?** No. The README's privacy stance
  is explicit and the answer to the brainstorming question was "user
  action only".
- **Multiple DMG assets per release?** Not today (ARM-only). When that
  changes, the asset picker filters by host arch; out of scope until
  the release pipeline ships multiple DMGs.
- **Update channels (beta/stable)?** Out of scope. One channel: GitHub
  "latest" release.
