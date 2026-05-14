# Open-Lid Design Document

**Status:** Draft for implementation
**Date:** 2026-05-10
**Author:** Diyan Bogdanov (with Claude)

## Summary

Open-Lid is a Rust rewrite of [narcotic-sh](https://github.com/narcotic-sh) — a macOS menu-bar utility that prevents the Mac from sleeping when the lid is closed while still letting the display turn off. The rewrite adds composable modes/profiles and a first-class CLI, retains upstream's clean UX (no admin password, single approval toggle in System Settings), and lays the architectural groundwork for future Windows and Linux ports without speculatively building them.

v1 ships macOS-only as a signed, notarized `OpenLid.app` distributed via GitHub Releases and a Homebrew tap.

---

## Goals

1. **Feature parity with upstream's core:** prevent sleep on lid-close, force display sleep when no external display is attached, restore normal behavior on quit/uninstall, idle-exit the helper after 15 s of inactivity.
2. **Three modes:** `lid-closed` (default, upstream behavior), `always-awake` (override regardless of lid state), `timed` (override until a specified instant, then auto-revert).
3. **Three modifiers** that constrain any active mode: `only-on-ac` (auto-revert on battery), `min-battery <N%>` (auto-revert below threshold), `schedule <days, start, end>` (only active inside window).
4. **CLI is a first-class control surface,** not an afterthought. `open-lid on/off/status/mode/for/until/modifier/config` works against the running menu-bar process and is scriptable from coding-agent lifecycle hooks.
5. **Native macOS polish:** AppKit menu bar (NSStatusItem) and a native preferences NSWindow using `objc2-app-kit`. No web-view, no cross-platform UI toolkit approximation.
6. **Single signed `.app` bundle, two binaries inside,** ~7–8 MB total. Statically linked except for system frameworks.
7. **Architecturally extensible** to Windows (`SetThreadExecutionState` + WM_POWERBROADCAST) and Linux (logind D-Bus `Inhibit("sleep:handle-lid-switch")`) without rewriting model, state, IPC, or CLI code.
8. **Clean install UX:** SMAppService-based helper install (one toggle in System Settings → Login Items → Allow in the Background). No admin password prompt anywhere.

## Non-Goals (v1)

- Auto-update mechanism (defer until Sparkle integration or distribution via the App Store).
- Multiple named profiles ("Work" / "Travel" presets). One active config; modes/modifiers cover real needs.
- User-visible notifications on state change.
- Lock-screen behavior customization.
- Localization beyond English.
- Windows or Linux implementations (only the trait shape is built; no platform impls).
- macOS Intel testing (Apple Silicon only for v1, matching upstream).

## Background: How upstream Works

For readers who haven't read the upstream source, here is the mechanism in brief — the rewrite preserves it.

- **Lid-state monitoring:** upstream subscribes to `IOPMrootDomain` via `IOServiceAddInterestNotification(..., kIOGeneralInterest, ...)` and listens for the clamshell-state-change message (subsystem `errSub(13)`, message `0x100`). It also reads the `AppleClamshellState` IOKit property for the current state.
- **External-display detection:** uses `CGGetActiveDisplayList` + `CGDisplayIsBuiltin` — when the lid closes with an external display attached, do nothing (the external display continues working normally).
- **Force display sleep on lid close (no external):** shells out to `/usr/bin/pmset displaysleepnow` (no privilege required).
- **Override system sleep on lid close:** the privileged helper runs `/usr/bin/pmset -a disablesleep 1` (and `0` to restore). This writes to `/Library/Preferences/SystemConfiguration/com.apple.PowerManagement.plist` and *requires root*. There is no IOKit assertion API or `caffeinate` flag that overrides clamshell-driven sleep — `pmset disablesleep` is the only mechanism.
- **Helper install:** upstream uses `SMAppService.daemon(plistName:).register()`. The helper plist is embedded inside the app bundle. macOS validates the signature/Team ID and puts the daemon into "requires approval"; user enables via System Settings.
- **XPC + Team-ID validation:** the helper accepts NSXPC connections only from clients whose signing identity matches a code-requirement string (same Team ID, expected bundle identifier).
- **Stale-state recovery:** the helper writes an "ownership marker" file at `/Library/Application Support/upstream/sleep-prevention.enabled` while sleep is overridden. On helper startup it checks for this marker; if present (meaning the app crashed without cleanup), it restores normal sleep behavior.
- **Idle exit:** when no clients are connected and no leases are active, the helper exits after a 15 s grace period. launchd will relaunch it on the next XPC connection.

---

## Architecture

### Process model

Three runtime roles, packaged as two binaries:

```
OpenLid.app/Contents/MacOS/open-lid             (signed, runs as user)
  ├─ role: menubar      (default — long-running UI process)
  └─ role: cli          (short-lived — connects to menubar process)

OpenLid.app/Contents/MacOS/open-lid-helper      (signed, runs as root via launchd)
  └─ role: daemon       (auto-loaded, auto-exits when idle)
```

Both `open-lid` roles dispatch from `main()` based on argv. The `cli` role connects to the menubar process over a Unix domain socket; the `menubar` role connects to the helper over NSXPC.

### Why two IPC channels

- **CLI ↔ menubar (Unix domain socket):** same-user, no privilege boundary. The menubar process owns *all state* — current mode, modifier values, persistent config, helper connection. CLI is a thin client that sends a command and renders a response. Single source of truth for state.
- **menubar ↔ helper (NSXPC):** crosses a privilege boundary (user → root). NSXPC provides Mach-port-level isolation, code-signature-based client authentication, and is the macOS-native way to talk to a launchd daemon.

```
   ┌──────────────┐    UDS    ┌───────────────┐    XPC    ┌────────────┐
   │   open-lid   │ ◄──────►  │   open-lid    │ ◄──────►  │ open-lid-  │
   │   (cli role) │           │  (menubar)    │           │  helper    │
   │   per-cmd    │           │  long-running │           │  root      │
   └──────────────┘           └───────────────┘           └────────────┘
```

### CLI auto-launch behavior

When `open-lid on` is invoked and the menubar process isn't running, the CLI:
1. Checks for the socket at `~/Library/Application Support/open-lid/control.sock`.
2. If missing, launches the `.app` (via `NSWorkspace.openApplication` equivalent or `open -a OpenLid`) and waits up to 3 s for the socket to appear.
3. Sends the command.

This means scripting `open-lid on` from a shell or agent lifecycle hook "just works" without requiring the user to remember to open the app first.

### Helper authentication

The helper validates incoming XPC connections by:
1. Reading the connecting client's audit token (`xpc_connection_get_audit_token` equivalent through NSXPC).
2. Resolving it to a `SecCode` via `SecCodeCopyGuestWithAttributes`.
3. Checking that code against the requirement string:
   ```
   identifier "io.openlid.app" and anchor apple generic
     and certificate leaf[subject.OU] = "<TeamID>"
     and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */
     and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */
   ```
4. Rejecting connections from anything that doesn't match.

### Cross-platform trait shape

The shared `open-lid-core` crate defines platform-agnostic traits. Only the shared crate is touched when adding a platform — adding Windows means adding `crates/app/src/platform/windows/` with impls, not touching `core`.

```rust
// crates/core/src/platform.rs

pub trait PowerController: Send + Sync {
    fn prevent_sleep(&self) -> Result<()>;
    fn allow_sleep(&self) -> Result<()>;
}

pub trait LidObserver: Send + Sync {
    fn current(&self) -> LidState;
    fn subscribe(&self, callback: Box<dyn Fn(LidState) + Send + Sync>);
}

pub trait PowerSourceMonitor: Send + Sync {
    fn current(&self) -> PowerSource;
    fn subscribe(&self, callback: Box<dyn Fn(PowerSource) + Send + Sync>);
}

pub trait DisplayController: Send + Sync {
    fn has_external_display(&self) -> bool;
    fn force_display_sleep(&self) -> Result<()>;
}

pub trait UiHost {
    fn run(self, state: Arc<Mutex<AppState>>, events: EventBus);
}
```

The `core` crate compiles on any OS — pure types, traits, serde, chrono, state machine, IPC message definitions. macOS-specific code (IOKit, AppKit, XPC, SMAppService) lives under `crates/app/src/platform/macos/` and `crates/helper/src/...`, never imported by `core`.

---

## Workspace & File Layout

```
open-lid/
├── Cargo.toml                          # workspace manifest
├── Cargo.lock
├── README.md
├── LICENSE                             # MIT (matching upstream)
├── rust-toolchain.toml                 # pin to stable channel
│
├── crates/
│   ├── core/                           # open-lid-core (lib): zero platform deps
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs               # TOML schema, load/save
│   │       ├── mode.rs                 # Mode, Modifiers, Schedule
│   │       ├── state.rs                # AppState, decision function
│   │       ├── ipc/
│   │       │   ├── mod.rs
│   │       │   ├── control.rs          # CLI ↔ menubar messages (serde)
│   │       │   └── helper.rs           # menubar ↔ helper protocol types
│   │       └── platform.rs             # PowerController, LidObserver, etc.
│   │
│   ├── app/                            # open-lid (bin)
│   │   ├── Cargo.toml
│   │   ├── build.rs                    # cargo:rerun-if-changed for plist/icons
│   │   └── src/
│   │       ├── main.rs                 # argv dispatch (menubar / cli)
│   │       ├── menubar/
│   │       │   ├── mod.rs              # MenuBarApp entry point
│   │       │   ├── status_item.rs      # NSStatusItem + icon refresh
│   │       │   ├── menu.rs             # NSMenu construction
│   │       │   ├── preferences.rs      # NSWindow prefs panel
│   │       │   └── icon.rs             # SF Symbols selection
│   │       ├── cli/
│   │       │   ├── mod.rs              # clap-based dispatch
│   │       │   └── commands.rs         # one fn per subcommand
│   │       ├── control_server.rs       # UDS server in menubar process
│   │       ├── control_client.rs       # UDS client in CLI role
│   │       ├── helper_client.rs        # NSXPC client wrapper
│   │       ├── helper_installer.rs     # SMAppService wrapper
│   │       ├── state_runtime.rs        # ties traits → AppState → helper
│   │       └── platform/
│   │           └── macos/
│   │               ├── mod.rs
│   │               ├── lid_monitor.rs       # impl LidObserver via IOKit
│   │               ├── power_source.rs      # impl PowerSourceMonitor (IOPowerSources)
│   │               ├── display.rs           # impl DisplayController (CGDisplay + pmset)
│   │               └── smappservice.rs      # SMAppService objc2 binding
│   │
│   └── helper/                         # open-lid-helper (bin)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── xpc_listener.rs         # NSXPCListener + delegate
│           ├── client_validator.rs     # code-requirement-string validation
│           ├── pmset.rs                # Command::new("/usr/bin/pmset")
│           ├── ownership_marker.rs     # /Library/Application Support/open-lid/...
│           └── idle_exit.rs            # 15 s timer
│
├── resources/
│   ├── app/
│   │   ├── Info.plist                  # for OpenLid.app
│   │   ├── OpenLid.entitlements
│   │   └── icons/                      # AppIcon.icns + menu bar PNGs (if not SF Symbols)
│   ├── helper/
│   │   ├── Info.plist
│   │   ├── Helper.entitlements
│   │   └── io.openlid.helper.plist     # LaunchDaemon plist embedded in bundle
│   └── dmg/
│       ├── background.png
│       └── dmg.json                    # create-dmg config
│
├── scripts/
│   ├── dev-build.sh                    # debug build + ad-hoc sign for local testing
│   ├── release.sh                      # full pipeline (calls xtask)
│   └── reset-helper.sh                 # dev convenience: unload + reload helper
│
└── xtask/                              # cargo xtask — release pipeline
    ├── Cargo.toml
    └── src/
        ├── main.rs
        ├── bundle.rs                   # cargo build → OpenLid.app structure
        ├── sign.rs                     # codesign both binaries + bundle
        ├── notarize.rs                 # notarytool submit + stapler staple
        └── dmg.rs                      # create-dmg wrapper
```

### `OpenLid.app` bundle layout (produced by `cargo xtask bundle`)

```
OpenLid.app/
├── Contents/
│   ├── Info.plist
│   ├── MacOS/
│   │   ├── open-lid                    # main binary, signed
│   │   └── open-lid-helper             # helper binary, signed
│   ├── Resources/
│   │   ├── AppIcon.icns
│   │   └── ...
│   ├── Library/
│   │   └── LaunchDaemons/
│   │       └── io.openlid.helper.plist
│   └── _CodeSignature/
```

---

## Key Dependencies

| Crate | Purpose | Risk / Notes |
|---|---|---|
| `objc2` (0.6+) | Obj-C runtime | Stable, active standard |
| `objc2-foundation` | NSString, NSData, NSXPCConnection | Stable |
| `objc2-app-kit` | NSStatusItem, NSMenu, NSWindow, NSButton, NSPopUpButton | Stable |
| `objc2-service-management` | `SMAppService` wrapper | **Verify availability at impl time** — if missing, write manual objc2 binding (~30 lines) |
| `objc2-io-kit` *(or raw FFI)* | IOPMrootDomain, IOPowerSources, CGDisplay | **Verify coverage** — likely need some raw FFI via `core-foundation` + manual `extern "C"` decls for IOPM message constants |
| `block2` | Obj-C blocks for XPC callbacks | Required by NSXPC reply handlers |
| `core-foundation` | CFTypes (CFString, CFNumber, CFRunLoop) | Foundational for IOKit interop |
| `clap` (4.x) | CLI parsing | Subcommands + derive macros |
| `serde`, `serde_derive`, `toml` | Config persistence | TOML for human-editable config |
| `serde_json` | UDS message framing | JSON-line protocol for CLI ↔ menubar |
| `tracing`, `tracing-subscriber` | Structured logs | File appender → `~/Library/Logs/open-lid/` |
| `anyhow` | Error wrapping at binary boundaries | Pair with `thiserror` in library code |
| `thiserror` | Typed error enums in `core` | Library convention |
| `directories` | Application Support / Logs paths | Cross-platform-friendly |
| `interprocess` | Unix domain socket (CLI ↔ app) | Cross-platform (named pipes on Windows later) |
| `chrono` | Timed mode, schedule windows | Local timezone handling |
| `humantime` | Parse `2h`, `30m`, `1h30m` for `open-lid for` | Tiny dep |
| `dialoguer` *(maybe)* | CLI prompts during `uninstall` confirmation | Optional — could use plain stdin |

### Workspace dependency direction

```
xtask
  ↓
app  ──→ core ←── helper
            ↑
       (no deps the other way)
```

`core` depends on `serde`, `chrono`, `thiserror`, `directories`. No macOS-specific deps.
`app` and `helper` depend on `core` + their macOS-specific deps.

---

## Mode & Modifier Model

### Types (in `open-lid-core::mode`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Mode {
    LidClosed,
    AlwaysAwake,
    Timed { until: DateTime<Local> },
}

impl Default for Mode {
    fn default() -> Self { Mode::LidClosed }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Modifiers {
    #[serde(default)]
    pub only_on_ac: bool,
    #[serde(default)]
    pub min_battery: Option<u8>,            // 0..=100, None = disabled
    #[serde(default)]
    pub schedule: Option<Schedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schedule {
    pub days: DaysOfWeek,                   // bitflags
    pub start: NaiveTime,
    pub end: NaiveTime,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LidState { Open, Closed }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerSource { Ac, Battery { percent: u8 } }
```

### State (in `open-lid-core::state`)

```rust
pub struct AppState {
    pub enabled: bool,        // user's master on/off
    pub mode: Mode,
    pub modifiers: Modifiers,
    pub lid: LidState,        // observed
    pub power: PowerSource,   // observed
}

/// Pure function. The single source of truth for "should we be preventing sleep right now."
pub fn should_prevent_sleep(state: &AppState, now: DateTime<Local>) -> bool {
    if !state.enabled { return false; }
    if !modifiers_allow(&state.modifiers, now, &state.power) { return false; }
    match &state.mode {
        Mode::LidClosed   => state.lid == LidState::Closed,
        Mode::AlwaysAwake => true,
        Mode::Timed { until } => now < *until,
    }
}

fn modifiers_allow(m: &Modifiers, now: DateTime<Local>, power: &PowerSource) -> bool {
    if m.only_on_ac && !matches!(power, PowerSource::Ac) { return false; }
    if let Some(min) = m.min_battery {
        if let PowerSource::Battery { percent } = power {
            if *percent < min { return false; }
        }
    }
    if let Some(sched) = &m.schedule {
        if !sched.contains(now) { return false; }
    }
    true
}
```

### Runtime reconciliation

The menubar process re-evaluates `should_prevent_sleep` and calls `helper.set_sleep_prevention(...)` whenever any input changes:

- User toggles via menu / CLI / preferences → `enabled` flips.
- IOKit clamshell notification → `lid` updates.
- IOPowerSources notification → `power` updates.
- Schedule timer tick at next start/end boundary → re-evaluate.
- Timed-mode `until` reached → auto-flip `enabled = false`.

The reconcile loop is idempotent: it diffs the current desired state vs last-sent state and only calls the helper on changes.

### Persistence

Config file: `~/Library/Application Support/open-lid/config.toml`

```toml
enabled = false
mode = "lid-closed"

[modifiers]
only_on_ac = false
# min_battery omitted when disabled

# [modifiers.schedule]              # entire table omitted when disabled
# days = ["Mon", "Tue", "Wed", "Thu", "Fri"]
# start = "09:00"
# end   = "18:00"
```

`enabled`, `mode`, `modifiers` are persisted. `lid` and `power` are observed at runtime and never persisted. Config is rewritten atomically (write-temp + rename) on any change.

---

## CLI Surface

All commands are subcommands of `open-lid`. Where the menubar process is required and isn't running, the CLI auto-launches the `.app`.

| Command | Behavior |
|---|---|
| `open-lid` *(no args)* | Runs the menubar role (foreground, blocks the calling shell or `.app` launcher). This is what `OpenLid.app/Contents/MacOS/open-lid` executes. |
| `open-lid menubar` | Explicit menubar role — equivalent to no args; kept for clarity in scripts. |
| `open-lid helper` | Helper role (used by launchd). Refuses to run if not invoked by launchd (checks parent PID and that stdin is not a TTY). |
| `open-lid on` | Set `enabled = true`. Auto-launches `.app` if menubar process isn't running. |
| `open-lid off` | Set `enabled = false`. |
| `open-lid status` | Print current state. `--json` for machine-readable output. Does not auto-launch; if no menubar process, prints "not running" and exits 1. |
| `open-lid mode lid-closed` | Switch to mode. |
| `open-lid mode always-awake` | Switch to mode. |
| `open-lid for <duration>` | Switch to `Timed` mode and enable. `<duration>` parsed by `humantime` (`2h`, `30m`, `1h30m`). |
| `open-lid until <time>` | Switch to `Timed` mode and enable. Strict formats only for v1: `HH:MM` (today, or tomorrow if the time has already passed) and ISO 8601 `YYYY-MM-DDTHH:MM`. No natural-language parsing. |
| `open-lid modifier only-on-ac <on\|off>` | Toggle modifier. |
| `open-lid modifier min-battery <N\|off>` | Set or disable threshold. |
| `open-lid modifier schedule <on\|off>` | Toggle schedule (only meaningful if schedule is configured). |
| `open-lid config show` | Print resolved config as TOML. |
| `open-lid config path` | Print path to config file. |
| `open-lid config edit` | Open in `$EDITOR` (fallback `open`). |
| `open-lid uninstall` | Disable helper, unregister via SMAppService, remove config, terminate menubar. Confirms first. |
| `open-lid --version` | Print version + commit SHA. |

### `status` output (default)

```
$ open-lid status
Sleep prevention: ACTIVE (lid is closed, no external display)
Mode:             lid-closed
Modifiers:        none
Helper:           registered, running (pid 4521)
Config:           /Users/diyan/Library/Application Support/open-lid/config.toml
```

### `status --json`

```json
{
  "preventing_sleep_now": true,
  "enabled": true,
  "mode": { "type": "lid-closed" },
  "modifiers": { "only_on_ac": false, "min_battery": null, "schedule": null },
  "lid": "closed",
  "power": { "type": "battery", "percent": 73 },
  "helper": { "status": "running", "pid": 4521 }
}
```

### CLI ↔ menubar protocol

Line-delimited JSON over Unix domain socket. One request per connection, server replies and closes.

```rust
// open-lid-core::ipc::control
#[derive(Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum ControlRequest {
    GetStatus,
    SetEnabled { enabled: bool },
    SetMode { mode: Mode },
    SetModifier { modifier: ModifierKey, value: ModifierValue },
    Uninstall,
    Ping,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum ControlResponse {
    Ok { state: Snapshot },
    Error { message: String },
}
```

---

## Menu Bar & Preferences UI

### NSStatusItem (menu bar icon)

- Icon: SF Symbol `eye.fill` (active) / `eye.slash.fill` (inactive), `isTemplate = true`.
- Tooltip: "Mac is on Open-Lid" / "Mac is not on Open-Lid".
- Left-click: toggle on/off (current mode).
- Right-click / option-click: open menu.

### Menu structure

```
Status: Preventing sleep · Mode: Lid-closed
─────────────────────────────────────────
Turn Off                              ⌘L     (or "Turn On")
─────────────────────────────────────────
Mode                                       ▸
  ✓ Lid-closed
    Always awake
    Timed for…                              (opens sheet: 30m / 1h / 2h / Custom)
    Timed until…                            (opens sheet: time picker)

Modifiers                                  ▸
  ☐ Only on AC power
    Min battery threshold…                  (opens sheet: stepper for %)
    Schedule…                               (opens sheet: days + start/end)
─────────────────────────────────────────
Preferences…                          ⌘,
Open Login Items Settings                   (only when helper needs approval)
─────────────────────────────────────────
About Open-Lid
Uninstall…
Quit Open-Lid                          ⌘Q
```

Status row and "Error: …" row are non-selectable (disabled NSMenuItems).

### Preferences window

A single NSWindow, non-resizable, with three tabs (NSTabView) — small and Mac-like:

**General**
- ☐ Launch Open-Lid at login (uses `SMAppService.loginItem`)
- ☐ Show icon in menu bar (if off, app becomes headless — CLI-only)
- Default mode: [Lid-closed ▾] (NSPopUpButton)

**Modifiers**
- ☐ Only when plugged in (AC power)
- ☐ Disable below battery: [20 ▾]% (stepper)
- ☐ Active only during: [Mon][Tue][Wed][Thu][Fri][Sat][Sun] toggle pills
   - From: [09:00] To: [18:00] (NSDatePicker time-only)

**About**
- App icon + "Open-Lid v0.1.0"
- "Inspired by upstream by narcotic-sh" — link
- "View source on GitHub" — link
- License (MIT) — link

---

## Install / Uninstall Flow

### First-run install (signed Developer ID + SMAppService)

1. User downloads `OpenLid.dmg`, drags to `/Applications`, double-clicks.
2. App launches, registers as menu bar item.
3. User left-clicks the icon (turn on). App calls `SMAppService.daemon(plistName: "io.openlid.helper.plist").register()`.
4. macOS validates the bundle signature/Team ID, status returns `.requiresApproval`.
5. App shows a sheet: "Open-Lid needs background activity permission to keep your Mac awake when the lid is closed. Open System Settings to allow."
6. User clicks → `SMAppService.openSystemSettingsLoginItems()` opens the right pane.
7. User flips the "Open-Lid" toggle in **Allow in the Background**.
8. App detects `.enabled` status (via polling or NSDistributedNotification), calls `helper.set_sleep_prevention(true)`.
9. Helper toggles `pmset -a disablesleep 1`, writes ownership marker, replies success.
10. App updates icon to active state.

### Uninstall flow

Triggered by menu → "Uninstall…" or `open-lid uninstall`:

1. Confirm with user (NSAlert or stdin prompt).
2. Send `set_sleep_prevention(false)` to helper. Helper restores `pmset -a disablesleep 0`, removes ownership marker.
3. Call `SMAppService.daemon(...).unregister()`. Helper plist is removed from launchd's database.
4. Remove `~/Library/Application Support/open-lid/` (config + control socket).
5. Remove `~/Library/Logs/open-lid/` (optional — keep behind a `--keep-logs` flag).
6. Self-delete: spawn a detached shell script that waits 1 s then `rm -rf /Applications/OpenLid.app`.
7. Terminate menubar process.

### Stale-state recovery

On helper startup (every time launchd loads it):
1. Check for ownership marker at `/Library/Application Support/open-lid/sleep-prevention.enabled`.
2. If present and no client has reconnected within 5 s → app crashed without cleanup. Run `pmset -a disablesleep 0`, remove marker.
3. Begin idle-exit timer (15 s with no client).

---

## Signing, Entitlements, Distribution

### Code signing

Both binaries are signed with **Developer ID Application** certificate, hardened runtime enabled.

**`resources/app/OpenLid.entitlements`:**
```xml
<plist version="1.0">
<dict>
    <!-- Hardened runtime is implicit when codesign --options=runtime is used -->
    <!-- No sandbox: Developer ID distribution, needs UDS + IOKit access -->
</dict>
</plist>
```

**`resources/helper/Helper.entitlements`:**
```xml
<plist version="1.0">
<dict>
    <!-- Empty: helper runs as root via launchd, doesn't need anything special -->
</dict>
</plist>
```

### Helper LaunchDaemon plist

`resources/helper/io.openlid.helper.plist`:
```xml
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.openlid.helper</string>

    <key>BundleProgram</key>
    <string>Contents/MacOS/open-lid-helper</string>

    <key>MachServices</key>
    <dict>
        <key>io.openlid.helper</key>
        <true/>
    </dict>

    <key>AssociatedBundleIdentifiers</key>
    <array>
        <string>io.openlid.app</string>
    </array>
</dict>
</plist>
```

### Build pipeline (cargo xtask)

```
cargo xtask bundle        → cargo build --release for app + helper
                            assemble OpenLid.app structure
                            copy Info.plist + entitlements + plist + icons

cargo xtask sign          → codesign --options=runtime --entitlements ...
                              --sign "Developer ID Application: <name> (<TeamID>)"
                              both binaries + the .app bundle (deep sign)

cargo xtask notarize      → ditto OpenLid.app → OpenLid.zip
                            xcrun notarytool submit ... --wait
                            xcrun stapler staple OpenLid.app

cargo xtask dmg           → create-dmg with background.png + dmg.json
                            codesign + notarize the DMG

cargo xtask release       → bundle → sign → notarize → dmg in one shot
```

### Distribution channels

1. **GitHub Releases:** notarized DMG attached to each `vX.Y.Z` tag.
2. **Homebrew tap:** `brew install --cask openlid/open-lid/open-lid` (cask formula maintained in a sibling repo).
3. **README install instructions:** drag-to-Applications + first-launch approval steps.

---

## Logging & Diagnostics

- **App logs:** `~/Library/Logs/open-lid/app.log` (`tracing` file appender, rolled daily, 7-day retention).
- **Helper logs:** `/Library/Logs/open-lid/helper.log` (root-writable).
- Log level configurable via env var `OPEN_LID_LOG=debug` (defaults to `info`).
- `open-lid status --debug` prints recent log tail.
- Crash reports: macOS handles these natively into `~/Library/Logs/DiagnosticReports/`.

---

## Testing Strategy

| Layer | Test Type | Where |
|---|---|---|
| `core::state::should_prevent_sleep` | Pure unit tests, full combinatoric coverage | `crates/core/src/state.rs#[cfg(test)]` |
| `core::config` load/save round-trip | Unit tests | `crates/core/src/config.rs#[cfg(test)]` |
| `core::ipc` serde round-trip | Unit tests | `crates/core/src/ipc/` |
| `core::mode::Schedule::contains` | Unit tests covering edge cases (midnight wrap, day boundaries) | `crates/core/src/mode.rs` |
| Platform traits (mocks) | Integration tests with `MockPowerController` etc. | `crates/app/tests/state_runtime.rs` |
| Lid monitor (IOKit) | Manual / interactive — closes lid in test rig | Documented in `docs/manual-test-checklist.md` |
| Helper XPC | Spawn helper, send commands, assert state | `crates/helper/tests/xpc_integration.rs` (run only on macOS) |
| End-to-end install/uninstall | Manual on a fresh user account or VM | `docs/manual-test-checklist.md` |

CI: GitHub Actions on `macos-latest` runs unit + integration tests on every PR. Signing/notarization happens only on release-tag push (with Apple credentials in repo secrets).

---

## Implementation Risks & Open Questions

1. **`objc2-service-management` coverage.** `SMAppService` is the central API for v1. If the `objc2-service-management` crate doesn't expose `daemon(plistName:)` or status enums, we write a manual binding via `objc2::extern_class!` and `extern_methods!` — well-trodden territory, ~30–50 lines.

2. **`objc2-io-kit` coverage of IOPM constants.** `IOPMrootDomain`, `kIOGeneralInterest`, and the clamshell-state-change message code (`errSystem(0x38) | errSub(13) | 0x100`) may not all be exposed. Likely need manual constants. The actual IOServiceAddInterestNotification call is straightforward FFI.

3. **NSXPC in Rust.** Defining an `@objc protocol` from Rust and implementing it as an `NSObject` subclass is non-trivial but documented in `objc2`. Worst case: write a 1-file Objective-C++ shim that exposes a C ABI to Rust. Acceptable per the original goals — the *application logic* stays in Rust.

4. **`SecCode` client validation.** `SecCodeCopyGuestWithAttributes` + `SecRequirementCreateWithString` + `SecCodeCheckValidity` are stable C APIs in the Security framework. Bindings exist in the `security-framework` crate; verify it covers these calls or supplement.

5. **SMAppService approval-state polling.** No published notification fires when the user flips the toggle. upstream polls `service.status` after opening Settings. We do the same: poll every 1 s while preferences UI is visible, debounce to 5 s otherwise.

6. **Code-signing local dev loop.** Ad-hoc signing (`codesign -s -`) is enough for compilation, but `SMAppService` won't load an ad-hoc-signed helper. Dev requires Developer ID. Workaround during early development: use `launchctl bootstrap` with a manually-installed plist that points at the dev-built binary, bypassing SMAppService until the build pipeline is wired up.

7. **Apple Silicon assumption.** No Intel testing for v1. Universal binary support is a one-line Cargo target change later if needed.

---

## Out of Scope (v1 — Punt List)

- Sparkle / auto-update.
- Multiple named profiles.
- State-change notifications.
- Lock-screen behavior.
- Localization.
- Windows / Linux platform impls (trait shape ready; no code).
- Settings sync across machines.
- Per-app rules ("only prevent sleep when Xcode is running").
- IOKit assertion-based "keep display on" mode (the inverse of what we do).

---

## Success Criteria

v1 ships when:

1. Drag-to-Applications + double-click → menu bar icon appears.
2. Click icon → System Settings approval prompt → flip toggle → click icon again → sleep prevention active.
3. Close lid with no external display → display turns off, system stays awake.
4. Close lid with external display → display continues, system stays awake.
5. CLI `open-lid status` reflects reality. `open-lid on/off` works. `open-lid for 2h` enables Timed mode.
6. Each mode and each modifier behaves per spec under manual test.
7. Uninstall fully reverses install (no plist remnants, normal sleep restored).
8. Notarized DMG installs cleanly on a fresh macOS account with no Gatekeeper warning.
9. README documents install, uninstall, troubleshooting, and CLI usage.

---

*End of design document.*
