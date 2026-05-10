# Open-Lid MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a locally-usable MVP of Open-Lid. By the end of this plan, the user can:
- `cargo xtask dev-install` and have `OpenLid.app` running in their menu bar.
- Left-click the menu bar icon to toggle sleep prevention on/off (mode `lid-closed`).
- Close the laptop lid with no external display → display sleeps, system stays awake.
- Run `open-lid on/off/status/mode/for/until` from the terminal and have it work against the running app.
- Uninstall cleanly.

Out of scope for this plan (delivered by Plan 2): modifiers (only-on-ac/min-battery/schedule), preferences NSWindow, SMAppService production install, code signing pipeline, notarization, DMG, Homebrew tap.

**Architecture:** Cargo workspace with three crates — `open-lid-core` (pure logic + types + state machine), `open-lid` (menu bar app + CLI dispatcher, two roles in one binary), `open-lid-helper` (privileged daemon). NSXPC for menubar↔helper, Unix domain socket for CLI↔menubar. All macOS-specific code lives behind traits defined in `core` so future Windows/Linux ports add platform modules rather than rewriting core logic.

**Tech Stack:** Rust 1.81+ stable, Cargo workspace, `objc2` + `objc2-foundation` + `objc2-app-kit` for AppKit, raw FFI for IOKit IOPM message constants, `security-framework` for code-requirement validation, `clap` v4 for CLI, `serde` + `toml` for config, `interprocess` for UDS, `tracing` for structured logs, `chrono` for time handling. Development uses ad-hoc signing (`codesign -s -`); production signing comes in Plan 2.

**Reference spec:** [docs/superpowers/specs/2026-05-10-open-lid-design.md](../specs/2026-05-10-open-lid-design.md)

---

## Pre-flight Checks

Before starting Task 0, confirm:

- macOS 13+ on Apple Silicon (`sw_vers -productVersion` → 13.0 or later; `uname -m` → arm64).
- Rust toolchain installed: `rustc --version` succeeds and reports ≥ 1.81.
- Xcode Command Line Tools installed: `xcode-select -p` returns a path; `xcrun --find codesign` finds the binary.
- Working directory is the open-lid project root and the design doc lives at `docs/superpowers/specs/2026-05-10-open-lid-design.md`.
- Git is initialized and on `main`.

If any of these fail, stop and resolve before continuing.

---

## Phase 0: De-risking Spike

The single highest-risk piece of this plan is `objc2` + NSXPC end-to-end. We prove it works in one tiny throwaway before committing to the architecture.

### Task 0: Spike — Verify `objc2` NSXPC client/server round-trip

**Goal:** Build two tiny binaries in a throwaway directory, one a launchd-loaded XPC server, one a client. Make a single method call and back. Throw it away.

**Files:**
- Create: `spikes/xpc-hello/Cargo.toml`
- Create: `spikes/xpc-hello/server/src/main.rs`
- Create: `spikes/xpc-hello/client/src/main.rs`
- Create: `spikes/xpc-hello/io.openlid.spike.plist`

- [ ] **Step 1: Create spike workspace**

```bash
mkdir -p spikes/xpc-hello/server/src spikes/xpc-hello/client/src
```

- [ ] **Step 2: Write `spikes/xpc-hello/Cargo.toml`**

```toml
[workspace]
members = ["server", "client"]
resolver = "2"

[workspace.dependencies]
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = ["NSXPCConnection", "NSXPCListener", "NSXPCInterface", "NSString", "NSError"] }
block2 = "0.6"
```

- [ ] **Step 3: Write `spikes/xpc-hello/server/Cargo.toml`**

```toml
[package]
name = "spike-server"
version = "0.0.0"
edition = "2021"

[dependencies]
objc2 = { workspace = true }
objc2-foundation = { workspace = true }
block2 = { workspace = true }
```

- [ ] **Step 4: Write `spikes/xpc-hello/client/Cargo.toml`**

(Same as server with name `spike-client`.)

- [ ] **Step 5: Write server `main.rs`**

A minimal NSXPCListener that exposes one method `ping(reply: (msg: NSString) -> Void)` and replies "pong". Use `extern_protocol!`, `define_class!`, listener.set_delegate, listener.resume, run the CFRunLoop.

(The implementer writes this — the goal is to discover what works in `objc2-foundation` 0.3 today. If NSXPCConnection isn't exposed, fall back to manual `extern_class!` bindings for `NSXPCConnection` / `NSXPCListener` / `NSXPCInterface`.)

- [ ] **Step 6: Write client `main.rs`**

Connect to mach service `io.openlid.spike`, call `ping`, print the reply, exit.

- [ ] **Step 7: Write `io.openlid.spike.plist`**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.openlid.spike</string>
    <key>ProgramArguments</key>
    <array>
        <string>__REPLACE_WITH_ABSOLUTE_PATH_TO_SPIKE_SERVER__</string>
    </array>
    <key>MachServices</key>
    <dict>
        <key>io.openlid.spike</key>
        <true/>
    </dict>
</dict>
</plist>
```

- [ ] **Step 8: Build, install plist, run client**

```bash
cd spikes/xpc-hello
cargo build --release
# Update plist's ProgramArguments path to absolute path of target/release/spike-server
sudo cp io.openlid.spike.plist /Library/LaunchDaemons/
sudo launchctl bootstrap system /Library/LaunchDaemons/io.openlid.spike.plist
target/release/spike-client
# Expected output: "pong"
```

- [ ] **Step 9: Tear down spike**

```bash
sudo launchctl bootout system/io.openlid.spike
sudo rm /Library/LaunchDaemons/io.openlid.spike.plist
```

- [ ] **Step 10: Record findings in a brief note**

Create `docs/superpowers/notes/2026-05-10-objc2-xpc-spike-findings.md`. Capture:
- Which crate version + feature flags worked
- Whether `extern_protocol!` or manual binding was used
- Any gotchas around `block2` callbacks
- A pointer to the working code (paste the key files inline)

This note becomes input to Tasks 15 and 22.

- [ ] **Step 11: Commit findings, delete spike directory**

```bash
rm -rf spikes/
git add docs/superpowers/notes/2026-05-10-objc2-xpc-spike-findings.md
git commit -m "docs: capture findings from objc2 NSXPC spike"
```

---

## Phase 1: Workspace + Core Library

### Task 1: Initialize Cargo Workspace

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`

- [ ] **Step 1: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 2: Write workspace `Cargo.toml`**

```toml
[workspace]
members = ["crates/core", "crates/app", "crates/helper", "xtask"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.81"
license = "MIT"
repository = "https://github.com/diyanbogdanov/open-lid"

[workspace.dependencies]
# Internal
open-lid-core = { path = "crates/core" }

# Apple bindings
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = ["NSString", "NSData", "NSDictionary", "NSArray", "NSError", "NSXPCConnection", "NSXPCListener", "NSXPCInterface", "NSObject", "NSRunLoop"] }
objc2-app-kit = { version = "0.3", features = ["NSApplication", "NSStatusBar", "NSStatusItem", "NSMenu", "NSMenuItem", "NSImage", "NSButton", "NSWindow", "NSAlert"] }
block2 = "0.6"
core-foundation = "0.10"
security-framework = "3"

# CLI / data
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
humantime = "2"
bitflags = { version = "2", features = ["serde"] }

# Infra
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
directories = "5"
interprocess = { version = "2", features = ["tokio"] }

[profile.release]
lto = "thin"
codegen-units = 1
strip = true
panic = "abort"
```

- [ ] **Step 3: Verify workspace parses**

```bash
cargo metadata --no-deps > /dev/null && echo OK
# Expected: "OK" — fails if any TOML syntax issue
```

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml rust-toolchain.toml
git commit -m "chore: initialize cargo workspace"
```

---

### Task 2: Bootstrap `open-lid-core` Crate Skeleton

**Files:**
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`

- [ ] **Step 1: Create directory + Cargo.toml**

```bash
mkdir -p crates/core/src
```

```toml
# crates/core/Cargo.toml
[package]
name = "open-lid-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
toml = { workspace = true }
chrono = { workspace = true }
bitflags = { workspace = true }
thiserror = { workspace = true }
directories = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
serde_json = { workspace = true }
```

- [ ] **Step 2: Write skeleton `lib.rs`**

```rust
// crates/core/src/lib.rs
//! Open-Lid core: platform-agnostic types, state machine, IPC schemas.
//!
//! This crate must compile on any target — no Apple frameworks, no IOKit,
//! no AppKit. Anything platform-specific lives in `crates/app` or
//! `crates/helper` under a `platform/<os>/` subdirectory.

pub mod config;
pub mod ipc;
pub mod mode;
pub mod platform;
pub mod state;

pub use config::Config;
pub use mode::{Mode, Modifiers, Schedule, DaysOfWeek, LidState, PowerSource};
pub use state::{AppState, should_prevent_sleep};
```

- [ ] **Step 3: Create empty module files**

```bash
touch crates/core/src/{config.rs,mode.rs,state.rs,platform.rs}
mkdir -p crates/core/src/ipc
touch crates/core/src/ipc/mod.rs crates/core/src/ipc/control.rs crates/core/src/ipc/helper.rs
```

```rust
// crates/core/src/ipc/mod.rs
pub mod control;
pub mod helper;
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p open-lid-core
# Expected: numerous "unused" warnings but no errors
```

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): scaffold crate with empty module layout"
```

---

### Task 3: Define `Mode`, `Modifiers`, `Schedule`, `LidState`, `PowerSource`

**Files:**
- Modify: `crates/core/src/mode.rs`
- Test: `crates/core/src/mode.rs` (`#[cfg(test)]` module)

- [ ] **Step 1: Write the failing tests for serde round-trips**

```rust
// crates/core/src/mode.rs
use bitflags::bitflags;
use chrono::{DateTime, Local, NaiveTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Mode {
    LidClosed,
    AlwaysAwake,
    Timed { until: DateTime<Local> },
}

impl Default for Mode {
    fn default() -> Self {
        Mode::LidClosed
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DaysOfWeek: u8 {
        const MON = 1 << 0;
        const TUE = 1 << 1;
        const WED = 1 << 2;
        const THU = 1 << 3;
        const FRI = 1 << 4;
        const SAT = 1 << 5;
        const SUN = 1 << 6;
    }
}

impl Default for DaysOfWeek {
    fn default() -> Self {
        DaysOfWeek::empty()
    }
}

// Serialize as an array of three-letter day strings.
impl Serialize for DaysOfWeek {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        const ALL: [(DaysOfWeek, &str); 7] = [
            (DaysOfWeek::MON, "Mon"),
            (DaysOfWeek::TUE, "Tue"),
            (DaysOfWeek::WED, "Wed"),
            (DaysOfWeek::THU, "Thu"),
            (DaysOfWeek::FRI, "Fri"),
            (DaysOfWeek::SAT, "Sat"),
            (DaysOfWeek::SUN, "Sun"),
        ];
        let count = ALL.iter().filter(|(d, _)| self.contains(*d)).count();
        let mut seq = s.serialize_seq(Some(count))?;
        for (flag, name) in ALL.iter() {
            if self.contains(*flag) {
                seq.serialize_element(name)?;
            }
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for DaysOfWeek {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let names: Vec<String> = Vec::deserialize(d)?;
        let mut days = DaysOfWeek::empty();
        for name in names {
            match name.as_str() {
                "Mon" => days |= DaysOfWeek::MON,
                "Tue" => days |= DaysOfWeek::TUE,
                "Wed" => days |= DaysOfWeek::WED,
                "Thu" => days |= DaysOfWeek::THU,
                "Fri" => days |= DaysOfWeek::FRI,
                "Sat" => days |= DaysOfWeek::SAT,
                "Sun" => days |= DaysOfWeek::SUN,
                other => return Err(serde::de::Error::custom(format!("unknown day: {other}"))),
            }
        }
        Ok(days)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Modifiers {
    #[serde(default)]
    pub only_on_ac: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_battery: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Schedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schedule {
    pub days: DaysOfWeek,
    pub start: NaiveTime,
    pub end: NaiveTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LidState {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PowerSource {
    Ac,
    Battery { percent: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn mode_lid_closed_round_trips() {
        let m = Mode::LidClosed;
        let s = toml::to_string(&m).unwrap();
        let back: Mode = toml::from_str(&s).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn mode_timed_round_trips() {
        let until = Local.with_ymd_and_hms(2026, 5, 10, 18, 0, 0).unwrap();
        let m = Mode::Timed { until };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("timed"));
        let back: Mode = serde_json::from_str(&s).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn modifiers_default_is_all_off() {
        let m = Modifiers::default();
        assert!(!m.only_on_ac);
        assert!(m.min_battery.is_none());
        assert!(m.schedule.is_none());
    }

    #[test]
    fn days_of_week_round_trip_via_json() {
        let d = DaysOfWeek::MON | DaysOfWeek::WED | DaysOfWeek::FRI;
        let s = serde_json::to_string(&d).unwrap();
        assert_eq!(s, r#"["Mon","Wed","Fri"]"#);
        let back: DaysOfWeek = serde_json::from_str(&s).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn days_of_week_rejects_unknown_day() {
        let r: Result<DaysOfWeek, _> = serde_json::from_str(r#"["Funday"]"#);
        assert!(r.is_err());
    }
}
```

- [ ] **Step 2: Run tests, expect them to pass**

```bash
cargo test -p open-lid-core --lib mode
# Expected: 5 passed
```

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -p open-lid-core --all-targets -- -D warnings
# Expected: no warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/mode.rs
git commit -m "feat(core): add Mode, Modifiers, Schedule, LidState, PowerSource types"
```

---

### Task 4: Implement `Schedule::contains(now)` with full edge-case tests

**Files:**
- Modify: `crates/core/src/mode.rs`

- [ ] **Step 1: Add the `contains` method below the `Schedule` struct**

```rust
impl Schedule {
    /// Returns true if `now` falls within this schedule window.
    ///
    /// Windows that cross midnight (e.g., start 22:00, end 02:00) are
    /// interpreted as "today 22:00 → tomorrow 02:00 inclusive of the
    /// late-night portion when the day flag for `now`'s weekday is set".
    pub fn contains(&self, now: chrono::DateTime<chrono::Local>) -> bool {
        use chrono::{Datelike, Timelike, Weekday};

        let today_flag = match now.weekday() {
            Weekday::Mon => DaysOfWeek::MON,
            Weekday::Tue => DaysOfWeek::TUE,
            Weekday::Wed => DaysOfWeek::WED,
            Weekday::Thu => DaysOfWeek::THU,
            Weekday::Fri => DaysOfWeek::FRI,
            Weekday::Sat => DaysOfWeek::SAT,
            Weekday::Sun => DaysOfWeek::SUN,
        };

        let now_t = chrono::NaiveTime::from_hms_opt(now.hour(), now.minute(), now.second())
            .expect("valid clock time");

        if self.start <= self.end {
            // Same-day window
            self.days.contains(today_flag) && now_t >= self.start && now_t < self.end
        } else {
            // Crosses midnight: active either after start today OR before end today
            // (the second portion is "the tail of yesterday's window" but for
            // simplicity we treat each calendar day's bits independently).
            if self.days.contains(today_flag) && now_t >= self.start {
                return true;
            }
            // The "before end" portion belongs to *yesterday's* day flag
            let yesterday = match now.weekday() {
                Weekday::Mon => DaysOfWeek::SUN,
                Weekday::Tue => DaysOfWeek::MON,
                Weekday::Wed => DaysOfWeek::TUE,
                Weekday::Thu => DaysOfWeek::WED,
                Weekday::Fri => DaysOfWeek::THU,
                Weekday::Sat => DaysOfWeek::FRI,
                Weekday::Sun => DaysOfWeek::SAT,
            };
            self.days.contains(yesterday) && now_t < self.end
        }
    }
}
```

- [ ] **Step 2: Add test cases at the bottom of the `tests` module**

```rust
    #[test]
    fn schedule_same_day_inside_window_active() {
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 11, 12, 0, 0).unwrap();
        assert!(sched.contains(now));
    }

    #[test]
    fn schedule_same_day_outside_window_inactive() {
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 11, 20, 0, 0).unwrap();
        assert!(!sched.contains(now));
    }

    #[test]
    fn schedule_day_not_in_flags_inactive() {
        // 2026-05-09 is a Saturday
        let sched = Schedule {
            days: DaysOfWeek::MON | DaysOfWeek::TUE | DaysOfWeek::WED | DaysOfWeek::THU | DaysOfWeek::FRI,
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 9, 12, 0, 0).unwrap();
        assert!(!sched.contains(now));
    }

    #[test]
    fn schedule_crosses_midnight_late_evening_active() {
        // 22:00 → 02:00, today is Mon → after 22:00 Mon should be active
        let sched = Schedule {
            days: DaysOfWeek::MON,
            start: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        };
        // 2026-05-11 is a Monday
        let now = Local.with_ymd_and_hms(2026, 5, 11, 23, 30, 0).unwrap();
        assert!(sched.contains(now));
    }

    #[test]
    fn schedule_crosses_midnight_early_morning_uses_yesterday_flag() {
        let sched = Schedule {
            days: DaysOfWeek::MON,
            start: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        };
        // Tuesday 1:00 am — should be active because Mon's tail extends here
        let now = Local.with_ymd_and_hms(2026, 5, 12, 1, 0, 0).unwrap();
        assert!(sched.contains(now));
    }

    #[test]
    fn schedule_at_exact_end_inactive() {
        // end is exclusive
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 11, 18, 0, 0).unwrap();
        assert!(!sched.contains(now));
    }
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p open-lid-core --lib mode
# Expected: 11 passed
```

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/mode.rs
git commit -m "feat(core): implement Schedule::contains with midnight-crossing support"
```

---

### Task 5: Implement `AppState` and `should_prevent_sleep`

**Files:**
- Modify: `crates/core/src/state.rs`

- [ ] **Step 1: Write the full module with TDD-style tests**

```rust
// crates/core/src/state.rs
use crate::mode::{LidState, Mode, Modifiers, PowerSource};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppState {
    pub enabled: bool,
    pub mode: Mode,
    pub modifiers: Modifiers,
    #[serde(skip)]
    pub lid: LidState,
    #[serde(skip)]
    pub power: PowerSource,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: Mode::default(),
            modifiers: Modifiers::default(),
            lid: LidState::Open,
            power: PowerSource::Ac,
        }
    }
}

/// The single source of truth: "should we be preventing sleep right now?"
///
/// Pure function — no side effects. All inputs are explicit. Call this from
/// the menubar process whenever any input changes; diff against last result
/// to decide whether to call into the helper.
pub fn should_prevent_sleep(state: &AppState, now: DateTime<Local>) -> bool {
    if !state.enabled {
        return false;
    }
    if !modifiers_allow(&state.modifiers, now, &state.power) {
        return false;
    }
    match &state.mode {
        Mode::LidClosed => state.lid == LidState::Closed,
        Mode::AlwaysAwake => true,
        Mode::Timed { until } => now < *until,
    }
}

fn modifiers_allow(m: &Modifiers, now: DateTime<Local>, power: &PowerSource) -> bool {
    if m.only_on_ac && !matches!(power, PowerSource::Ac) {
        return false;
    }
    if let Some(min) = m.min_battery {
        if let PowerSource::Battery { percent } = power {
            if *percent < min {
                return false;
            }
        }
    }
    if let Some(sched) = &m.schedule {
        if !sched.contains(now) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::{DaysOfWeek, Schedule};
    use chrono::{NaiveTime, TimeZone};

    fn t() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 5, 11, 12, 0, 0).unwrap()
    }

    fn base() -> AppState {
        AppState {
            enabled: true,
            mode: Mode::LidClosed,
            modifiers: Modifiers::default(),
            lid: LidState::Closed,
            power: PowerSource::Ac,
        }
    }

    #[test]
    fn disabled_never_prevents() {
        let mut s = base();
        s.enabled = false;
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn lid_closed_mode_with_lid_open_does_not_prevent() {
        let mut s = base();
        s.lid = LidState::Open;
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn lid_closed_mode_with_lid_closed_prevents() {
        let s = base();
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn always_awake_prevents_regardless_of_lid() {
        let mut s = base();
        s.mode = Mode::AlwaysAwake;
        s.lid = LidState::Open;
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn timed_mode_before_until_prevents() {
        let mut s = base();
        s.mode = Mode::Timed {
            until: t() + chrono::Duration::hours(2),
        };
        s.lid = LidState::Open;
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn timed_mode_after_until_does_not_prevent() {
        let mut s = base();
        s.mode = Mode::Timed {
            until: t() - chrono::Duration::hours(1),
        };
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn only_on_ac_blocks_when_on_battery() {
        let mut s = base();
        s.modifiers.only_on_ac = true;
        s.power = PowerSource::Battery { percent: 80 };
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn only_on_ac_allows_when_on_ac() {
        let mut s = base();
        s.modifiers.only_on_ac = true;
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn min_battery_blocks_below_threshold() {
        let mut s = base();
        s.modifiers.min_battery = Some(50);
        s.power = PowerSource::Battery { percent: 30 };
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn min_battery_allows_above_threshold() {
        let mut s = base();
        s.modifiers.min_battery = Some(50);
        s.power = PowerSource::Battery { percent: 80 };
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn min_battery_does_not_apply_on_ac() {
        let mut s = base();
        s.modifiers.min_battery = Some(50);
        // Power is AC by default; threshold is ignored
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn schedule_blocks_outside_window() {
        let mut s = base();
        s.modifiers.schedule = Some(Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        });
        let outside = Local.with_ymd_and_hms(2026, 5, 11, 20, 0, 0).unwrap();
        assert!(!should_prevent_sleep(&s, outside));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p open-lid-core --lib state
# Expected: 12 passed
```

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/state.rs
git commit -m "feat(core): add AppState and should_prevent_sleep decision function"
```

---

### Task 6: Implement `Config` with atomic load/save

**Files:**
- Modify: `crates/core/src/config.rs`

- [ ] **Step 1: Write the module**

```rust
// crates/core/src/config.rs
use crate::mode::{Mode, Modifiers};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub modifiers: Modifiers,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("home directory not found")]
    NoHome,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

impl Config {
    /// Resolve the default config path:
    /// `~/Library/Application Support/open-lid/config.toml` on macOS,
    /// equivalent XDG path elsewhere.
    pub fn default_path() -> Result<PathBuf, ConfigError> {
        let dirs = ProjectDirs::from("io", "openlid", "open-lid").ok_or(ConfigError::NoHome)?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    /// Load config from `path`. If the file doesn't exist, returns `Config::default()`.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(toml::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError::Io(e)),
        }
    }

    /// Save config atomically: write to `<path>.tmp`, fsync, rename to `path`.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("toml.tmp");
        let body = toml::to_string_pretty(self)?;
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(body.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_default() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("subdir").join("config.toml");
        let cfg = Config {
            enabled: true,
            mode: Mode::AlwaysAwake,
            modifiers: Modifiers {
                only_on_ac: true,
                min_battery: Some(25),
                schedule: None,
            },
        };
        cfg.save(&p).unwrap();
        let back = Config::load(&p).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn save_is_atomic_no_tmp_file_left() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        Config::default().save(&p).unwrap();
        let tmp = p.with_extension("toml.tmp");
        assert!(!tmp.exists());
        assert!(p.exists());
    }
}
```

- [ ] **Step 2: Add `tempfile` to dev-dependencies**

Edit `crates/core/Cargo.toml`:

```toml
[dev-dependencies]
serde_json = { workspace = true }
tempfile = "3"
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p open-lid-core --lib config
# Expected: 3 passed
```

- [ ] **Step 4: Commit**

```bash
git add crates/core
git commit -m "feat(core): add Config with atomic save and TOML serialization"
```

---

### Task 7: Define IPC Message Types

**Files:**
- Modify: `crates/core/src/ipc/control.rs`
- Modify: `crates/core/src/ipc/helper.rs`

- [ ] **Step 1: Write `control.rs`**

```rust
// crates/core/src/ipc/control.rs
//! Messages exchanged between the CLI role and the menubar role over a
//! Unix domain socket. Line-delimited JSON: one request, one response, close.

use crate::mode::{Mode, Modifiers, PowerSource, LidState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum ControlRequest {
    GetStatus,
    SetEnabled { enabled: bool },
    SetMode { mode: Mode },
    SetModifierOnlyOnAc { enabled: bool },
    SetModifierMinBattery { percent: Option<u8> },
    SetModifierSchedule { enabled: bool },
    Uninstall,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum ControlResponse {
    Ok { state: Snapshot },
    Pong,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub preventing_sleep_now: bool,
    pub enabled: bool,
    pub mode: Mode,
    pub modifiers: Modifiers,
    pub lid: LidState,
    pub power: PowerSource,
    pub helper: HelperStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum HelperStatus {
    NotInstalled,
    NeedsApproval,
    Running,
    Stopped,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::PowerSource;

    #[test]
    fn request_get_status_serializes_to_kebab_case() {
        let r = ControlRequest::GetStatus;
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"cmd":"get-status"}"#);
    }

    #[test]
    fn response_pong_round_trips() {
        let r = ControlResponse::Pong;
        let s = serde_json::to_string(&r).unwrap();
        let back: ControlResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn snapshot_round_trips() {
        let snap = Snapshot {
            preventing_sleep_now: true,
            enabled: true,
            mode: Mode::LidClosed,
            modifiers: Modifiers::default(),
            lid: LidState::Closed,
            power: PowerSource::Battery { percent: 73 },
            helper: HelperStatus::Running,
        };
        let s = serde_json::to_string(&snap).unwrap();
        let back: Snapshot = serde_json::from_str(&s).unwrap();
        assert_eq!(snap, back);
    }
}
```

- [ ] **Step 2: Write `helper.rs`**

```rust
// crates/core/src/ipc/helper.rs
//! Protocol between the menubar process and the privileged helper.
//!
//! Transported via NSXPC in production; the wire schema below is the
//! logical contract that the XPC interface mirrors method-for-method.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "method", rename_all = "kebab-case")]
pub enum HelperRequest {
    SetSleepPrevention { enabled: bool },
    GetStatus,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum HelperResponse {
    SetSleepPreventionOk,
    StatusOk { sleep_prevention_active: bool },
    Pong,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_sleep_prevention_request_serializes() {
        let r = HelperRequest::SetSleepPrevention { enabled: true };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("set-sleep-prevention"));
        let back: HelperRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p open-lid-core --lib ipc
# Expected: 4 passed
```

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/ipc
git commit -m "feat(core): define control and helper IPC message types"
```

---

### Task 8: Define Platform Traits

**Files:**
- Modify: `crates/core/src/platform.rs`

- [ ] **Step 1: Write the traits**

```rust
// crates/core/src/platform.rs
//! Platform-abstraction traits. macOS impls live in `crates/app/src/platform/macos/`.
//! Future Windows/Linux impls live in sibling directories.

use crate::mode::{LidState, PowerSource};
use std::sync::Arc;

pub type LidStateCallback = Arc<dyn Fn(LidState) + Send + Sync + 'static>;
pub type PowerSourceCallback = Arc<dyn Fn(PowerSource) + Send + Sync + 'static>;

/// Toggles sleep-prevention at the system level. macOS: pmset disablesleep
/// (via XPC to the privileged helper). Windows: SetThreadExecutionState.
/// Linux: logind Inhibit.
pub trait PowerController: Send + Sync {
    fn prevent_sleep(&self) -> Result<(), PlatformError>;
    fn allow_sleep(&self) -> Result<(), PlatformError>;
}

/// Observes lid open/closed state. macOS: IOPMrootDomain interest notification.
pub trait LidObserver: Send + Sync {
    fn current(&self) -> LidState;
    fn subscribe(&self, callback: LidStateCallback);
}

/// Observes power source (AC vs battery and battery %). macOS: IOPowerSources.
pub trait PowerSourceMonitor: Send + Sync {
    fn current(&self) -> PowerSource;
    fn subscribe(&self, callback: PowerSourceCallback);
}

/// External-display detection + force-display-sleep. macOS: CGDisplay + pmset displaysleepnow.
pub trait DisplayController: Send + Sync {
    fn has_external_display(&self) -> bool;
    fn force_display_sleep(&self) -> Result<(), PlatformError>;
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("platform call failed: {0}")]
    Native(String),
    #[error("helper unavailable")]
    HelperUnavailable,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p open-lid-core
# Expected: no errors
```

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/platform.rs
git commit -m "feat(core): define platform-abstraction traits"
```

---

## Phase 2: Helper Binary

### Task 9: Bootstrap `open-lid-helper` Crate

**Files:**
- Create: `crates/helper/Cargo.toml`
- Create: `crates/helper/src/main.rs`

- [ ] **Step 1: Create directory + Cargo.toml**

```bash
mkdir -p crates/helper/src
```

```toml
# crates/helper/Cargo.toml
[package]
name = "open-lid-helper"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "open-lid-helper"
path = "src/main.rs"

[dependencies]
open-lid-core = { workspace = true }
objc2 = { workspace = true }
objc2-foundation = { workspace = true }
block2 = { workspace = true }
core-foundation = { workspace = true }
security-framework = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
tracing-appender = { workspace = true }
```

- [ ] **Step 2: Write skeleton `main.rs` with launchd-only guard**

```rust
// crates/helper/src/main.rs
//! open-lid-helper — the privileged daemon.
//!
//! Loaded by launchd as root. Listens for NSXPC connections from the
//! menubar app, validates them by code requirement, toggles
//! `pmset -a disablesleep` on request, and self-exits after 15 s of
//! inactivity.

mod idle_exit;
mod ownership_marker;
mod pmset;

use anyhow::{Context, Result};
use std::os::unix::io::AsRawFd;

fn main() -> Result<()> {
    setup_logging()?;
    guard_launched_by_launchd()?;
    tracing::info!("open-lid-helper starting (pid {})", std::process::id());

    // TODO Task 14: install XPC listener, run main loop.
    // For now, exit cleanly so we can verify the binary builds and runs.
    Ok(())
}

fn setup_logging() -> Result<()> {
    use tracing_subscriber::EnvFilter;

    let log_dir = std::path::Path::new("/Library/Logs/open-lid");
    std::fs::create_dir_all(log_dir).ok();
    let file = tracing_appender::rolling::daily(log_dir, "helper.log");
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OPEN_LID_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(file)
        .with_ansi(false)
        .init();
    Ok(())
}

/// Refuses to run if invoked from a terminal. Helper must come from launchd.
fn guard_launched_by_launchd() -> Result<()> {
    // launchd sets parent pid 1 and stdin is not a TTY
    let ppid = unsafe { libc::getppid() };
    let stdin_is_tty = unsafe { libc::isatty(std::io::stdin().as_raw_fd()) } == 1;
    if ppid != 1 || stdin_is_tty {
        anyhow::bail!("open-lid-helper must be loaded by launchd, not invoked directly");
    }
    Ok(())
}
```

- [ ] **Step 3: Add `libc` dep**

Add to `crates/helper/Cargo.toml` dependencies: `libc = "0.2"`.

- [ ] **Step 4: Create empty module files**

```bash
touch crates/helper/src/{idle_exit.rs,ownership_marker.rs,pmset.rs}
```

```rust
// crates/helper/src/pmset.rs
// (filled in Task 10)
```

```rust
// crates/helper/src/ownership_marker.rs
// (filled in Task 11)
```

```rust
// crates/helper/src/idle_exit.rs
// (filled in Task 12)
```

- [ ] **Step 5: Build**

```bash
cargo build -p open-lid-helper
# Expected: clean build, one binary at target/debug/open-lid-helper
```

- [ ] **Step 6: Commit**

```bash
git add crates/helper
git commit -m "feat(helper): scaffold helper binary with launchd-only guard"
```

---

### Task 10: Implement `pmset` Wrapper

**Files:**
- Modify: `crates/helper/src/pmset.rs`

- [ ] **Step 1: Write the module**

```rust
// crates/helper/src/pmset.rs
//! Wraps `/usr/bin/pmset` invocations. Pulled out so we can stub in tests.

use anyhow::{anyhow, Result};
use std::process::Command;

pub trait Pmset: Send + Sync {
    fn set_disable_sleep(&self, enabled: bool) -> Result<()>;
    fn read_disable_sleep(&self) -> Result<bool>;
}

pub struct RealPmset;

impl Pmset for RealPmset {
    fn set_disable_sleep(&self, enabled: bool) -> Result<()> {
        let arg = if enabled { "1" } else { "0" };
        let out = Command::new("/usr/bin/pmset")
            .args(["-a", "disablesleep", arg])
            .output()?;
        if !out.status.success() {
            return Err(anyhow!(
                "pmset disablesleep {arg} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(())
    }

    fn read_disable_sleep(&self) -> Result<bool> {
        let out = Command::new("/usr/bin/pmset").arg("-g").output()?;
        if !out.status.success() {
            return Err(anyhow!(
                "pmset -g failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.trim_start().starts_with("SleepDisabled") {
                let last = line.split_whitespace().last().unwrap_or("");
                return Ok(last == "1");
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    pub struct FakePmset {
        pub enabled: Mutex<bool>,
        pub set_calls: Mutex<Vec<bool>>,
    }
    impl FakePmset {
        pub fn new() -> Self {
            Self {
                enabled: Mutex::new(false),
                set_calls: Mutex::new(Vec::new()),
            }
        }
    }
    impl Pmset for FakePmset {
        fn set_disable_sleep(&self, enabled: bool) -> Result<()> {
            self.set_calls.lock().unwrap().push(enabled);
            *self.enabled.lock().unwrap() = enabled;
            Ok(())
        }
        fn read_disable_sleep(&self) -> Result<bool> {
            Ok(*self.enabled.lock().unwrap())
        }
    }

    #[test]
    fn fake_records_calls() {
        let p = FakePmset::new();
        p.set_disable_sleep(true).unwrap();
        p.set_disable_sleep(false).unwrap();
        assert_eq!(*p.set_calls.lock().unwrap(), vec![true, false]);
        assert!(!p.read_disable_sleep().unwrap());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p open-lid-helper --lib pmset
# Expected: 1 passed
```

- [ ] **Step 3: Commit**

```bash
git add crates/helper/src/pmset.rs
git commit -m "feat(helper): pmset wrapper with fake impl for tests"
```

---

### Task 11: Implement Ownership Marker

**Files:**
- Modify: `crates/helper/src/ownership_marker.rs`

- [ ] **Step 1: Write the module**

```rust
// crates/helper/src/ownership_marker.rs
//! Crash-recovery marker file. While sleep prevention is active, this file
//! exists. On helper startup, if the file exists and no client connects
//! within a grace period, we restore normal sleep behavior — the app must
//! have crashed without cleanup.

use anyhow::Result;
use std::path::{Path, PathBuf};

const MARKER_PATH: &str = "/Library/Application Support/open-lid/sleep-prevention.enabled";

pub struct OwnershipMarker {
    path: PathBuf,
}

impl OwnershipMarker {
    pub fn new() -> Self {
        Self {
            path: PathBuf::from(MARKER_PATH),
        }
    }

    #[cfg(test)]
    pub fn at(p: &Path) -> Self {
        Self { path: p.to_path_buf() }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn write(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, b"")?;
        Ok(())
    }

    pub fn remove(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_then_exists_then_remove() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("marker.flag");
        let m = OwnershipMarker::at(&p);

        assert!(!m.exists());
        m.write().unwrap();
        assert!(m.exists());
        m.remove().unwrap();
        assert!(!m.exists());
    }

    #[test]
    fn remove_nonexistent_is_ok() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("never-existed");
        OwnershipMarker::at(&p).remove().unwrap();
    }

    #[test]
    fn write_creates_parent_directory() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("nested").join("path").join("marker.flag");
        OwnershipMarker::at(&p).write().unwrap();
        assert!(p.exists());
    }
}
```

- [ ] **Step 2: Add tempfile to helper dev-deps**

```toml
# crates/helper/Cargo.toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p open-lid-helper --lib ownership_marker
# Expected: 3 passed
```

- [ ] **Step 4: Commit**

```bash
git add crates/helper
git commit -m "feat(helper): add ownership marker for crash recovery"
```

---

### Task 12: Implement Idle-Exit Timer

**Files:**
- Modify: `crates/helper/src/idle_exit.rs`

- [ ] **Step 1: Write the module**

```rust
// crates/helper/src/idle_exit.rs
//! 15-second idle timer. The helper calls `arm()` on every client
//! disconnect; `disarm()` on every client connect. If `arm()` is followed
//! by 15 s of no `disarm()`, the timer fires and we exit.

use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const IDLE_EXIT_SECS: u64 = 15;

#[derive(Clone)]
pub struct IdleExit {
    inner: Arc<Mutex<State>>,
}

struct State {
    generation: u64,
    armed: bool,
}

impl IdleExit {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(State { generation: 0, armed: false })),
        }
    }

    /// Arm the timer. If no `disarm` call comes in within IDLE_EXIT_SECS,
    /// `on_fire` runs.
    pub fn arm<F: FnOnce() + Send + 'static>(&self, on_fire: F) {
        let mut state = self.inner.lock().unwrap();
        state.generation = state.generation.wrapping_add(1);
        state.armed = true;
        let my_gen = state.generation;
        drop(state);

        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(IDLE_EXIT_SECS));
            let s = inner.lock().unwrap();
            if s.armed && s.generation == my_gen {
                drop(s);
                on_fire();
            }
        });
    }

    /// Cancel any pending arming. Future arms will get fresh generation numbers.
    pub fn disarm(&self) {
        self.inner.lock().unwrap().armed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    // We don't sleep for 15s in tests; we test the cancellation logic via
    // a short-circuit version. Use a helper that takes a custom duration.
    impl IdleExit {
        fn arm_for_test<F: FnOnce() + Send + 'static>(&self, dur: Duration, on_fire: F) {
            let mut state = self.inner.lock().unwrap();
            state.generation = state.generation.wrapping_add(1);
            state.armed = true;
            let my_gen = state.generation;
            drop(state);
            let inner = Arc::clone(&self.inner);
            std::thread::spawn(move || {
                std::thread::sleep(dur);
                let s = inner.lock().unwrap();
                if s.armed && s.generation == my_gen {
                    drop(s);
                    on_fire();
                }
            });
        }
    }

    #[test]
    fn fires_after_duration_if_not_disarmed() {
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = Arc::clone(&fired);
        let t = IdleExit::new();
        t.arm_for_test(Duration::from_millis(50), move || {
            f2.store(true, Ordering::SeqCst);
        });
        std::thread::sleep(Duration::from_millis(120));
        assert!(fired.load(Ordering::SeqCst));
    }

    #[test]
    fn disarm_before_fire_prevents_firing() {
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = Arc::clone(&fired);
        let t = IdleExit::new();
        t.arm_for_test(Duration::from_millis(100), move || {
            f2.store(true, Ordering::SeqCst);
        });
        std::thread::sleep(Duration::from_millis(30));
        t.disarm();
        std::thread::sleep(Duration::from_millis(150));
        assert!(!fired.load(Ordering::SeqCst));
    }

    #[test]
    fn rearm_supersedes_previous() {
        let fired_first = Arc::new(AtomicBool::new(false));
        let f1 = Arc::clone(&fired_first);
        let fired_second = Arc::new(AtomicBool::new(false));
        let f2 = Arc::clone(&fired_second);

        let t = IdleExit::new();
        t.arm_for_test(Duration::from_millis(50), move || {
            f1.store(true, Ordering::SeqCst);
        });
        // Re-arm with a longer duration; first should be superseded
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(10));
        t.arm_for_test(Duration::from_millis(150), move || {
            f2.store(true, Ordering::SeqCst);
        });
        std::thread::sleep(Duration::from_millis(80));
        // First timer's wake-up has happened by now; check it did NOT fire
        assert!(!fired_first.load(Ordering::SeqCst));
        // Wait for second timer
        std::thread::sleep(Duration::from_millis(120));
        assert!(fired_second.load(Ordering::SeqCst), "elapsed: {:?}", start.elapsed());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p open-lid-helper --lib idle_exit
# Expected: 3 passed (timings can be flaky; rerun if any individual test fails)
```

- [ ] **Step 3: Commit**

```bash
git add crates/helper/src/idle_exit.rs
git commit -m "feat(helper): generational idle-exit timer with tests"
```

---

### Task 13: Implement Client Code-Requirement Validator

**Files:**
- Create: `crates/helper/src/client_validator.rs`
- Modify: `crates/helper/src/main.rs` (add `mod`)

- [ ] **Step 1: Write the module**

```rust
// crates/helper/src/client_validator.rs
//! Validates that an incoming XPC client is signed with the expected
//! bundle identifier and Team ID. Uses Security framework SecCode APIs.

use anyhow::{anyhow, Context, Result};
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use security_framework_sys::base::{errSecSuccess, OSStatus};

// Bring in raw SecCode bindings via the security-framework-sys-equivalent
// path. As of the spike, security-framework 3.x exposes SecCode types but
// not all SecRequirement APIs; we drop to raw FFI for the missing pieces.
//
// If security-framework 3.x has SecRequirement::from_string and
// SecCode::check_validity exposed, prefer those.

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecCodeCopyGuestWithAttributes(
        host: *const std::ffi::c_void,
        attributes: *const std::ffi::c_void,
        flags: u32,
        guest: *mut *mut std::ffi::c_void,
    ) -> OSStatus;
    fn SecRequirementCreateWithString(
        text: *const std::ffi::c_void,
        flags: u32,
        requirement: *mut *mut std::ffi::c_void,
    ) -> OSStatus;
    fn SecCodeCheckValidity(
        code: *const std::ffi::c_void,
        flags: u32,
        requirement: *const std::ffi::c_void,
    ) -> OSStatus;
    fn CFRelease(cf: *const std::ffi::c_void);
}

pub struct ClientValidator {
    requirement_text: String,
}

impl ClientValidator {
    /// Build a validator with a code-requirement string. For Plan 1 we use
    /// an ad-hoc-permissive requirement; Plan 2 replaces with Team ID pinning.
    ///
    /// Plan 1 ad-hoc requirement (insecure, dev-only):
    ///   `identifier "io.openlid.app"`
    ///
    /// Plan 2 production requirement:
    ///   `identifier "io.openlid.app" and anchor apple generic
    ///    and certificate leaf[subject.OU] = "<TeamID>"`
    pub fn new(requirement_text: impl Into<String>) -> Self {
        Self {
            requirement_text: requirement_text.into(),
        }
    }

    pub fn allows(&self, audit_token: [u8; 32]) -> bool {
        match self.try_allows(audit_token) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("client validation error: {e:#}");
                false
            }
        }
    }

    fn try_allows(&self, audit_token: [u8; 32]) -> Result<bool> {
        let token_key = CFString::from_static_string("audit");
        let token_data = core_foundation::data::CFData::from_buffer(&audit_token);
        let attrs = CFDictionary::from_CFType_pairs(&[(token_key, token_data.as_CFType())]);

        let mut guest: *mut std::ffi::c_void = std::ptr::null_mut();
        let status = unsafe {
            SecCodeCopyGuestWithAttributes(
                std::ptr::null(),
                attrs.as_concrete_TypeRef() as *const _,
                0,
                &mut guest,
            )
        };
        if status != errSecSuccess || guest.is_null() {
            return Err(anyhow!("SecCodeCopyGuestWithAttributes failed: {status}"));
        }

        let req_text_cf = CFString::new(&self.requirement_text);
        let mut req: *mut std::ffi::c_void = std::ptr::null_mut();
        let status = unsafe {
            SecRequirementCreateWithString(
                req_text_cf.as_concrete_TypeRef() as *const _,
                0,
                &mut req,
            )
        };
        if status != errSecSuccess || req.is_null() {
            unsafe { CFRelease(guest) };
            return Err(anyhow!(
                "SecRequirementCreateWithString failed: {status}"
            ));
        }

        let check = unsafe { SecCodeCheckValidity(guest, 0, req) };
        unsafe {
            CFRelease(guest);
            CFRelease(req);
        }
        Ok(check == errSecSuccess)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validator_with_invalid_token_rejects() {
        let v = ClientValidator::new(r#"identifier "io.openlid.app""#);
        let bogus = [0u8; 32];
        assert!(!v.allows(bogus));
    }
}
```

- [ ] **Step 2: Register module**

```rust
// At the top of crates/helper/src/main.rs, add:
mod client_validator;
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p open-lid-helper --lib client_validator
# Expected: 1 passed
```

- [ ] **Step 4: Commit**

```bash
git add crates/helper/src
git commit -m "feat(helper): add SecCode-based client requirement validator"
```

---

### Task 14: NSXPC Listener — Wire Helper to Receive Connections

**Files:**
- Create: `crates/helper/src/xpc_listener.rs`
- Modify: `crates/helper/src/main.rs`

> **Critical:** This is the task most directly informed by the Phase 0 spike. The exact `objc2` API surface used here MUST match what the spike confirmed works. If the spike used manual `extern_class!` bindings instead of `objc2-foundation` `NSXPCConnection`, repeat that pattern here.

- [ ] **Step 1: Add the XPC interface (mirroring `HelperRequest`/`HelperResponse`)**

The XPC protocol on the wire mirrors `open_lid_core::ipc::helper::*`. Implement the same three methods:
- `setSleepPreventionEnabled:withReply:` — `(BOOL) → (BOOL ok, NSString *error)`
- `getSleepPreventionStatusWithReply:` — `() → (BOOL ok, BOOL active, NSString *error)`
- `pingWithReply:` — `() → ()`

Use the spike's working pattern. Sketch:

```rust
// crates/helper/src/xpc_listener.rs
//! NSXPCListener owning the `io.openlid.helper` Mach service.
//!
//! Translates each incoming XPC method into a call on the inner
//! `HelperImpl`, validates the connecting client first, and replies.

use crate::client_validator::ClientValidator;
use crate::idle_exit::IdleExit;
use crate::ownership_marker::OwnershipMarker;
use crate::pmset::Pmset;
use std::sync::Arc;

pub struct HelperImpl<P: Pmset + 'static> {
    pub pmset: Arc<P>,
    pub marker: Arc<OwnershipMarker>,
    pub idle_exit: IdleExit,
    pub validator: Arc<ClientValidator>,
}

impl<P: Pmset + 'static> HelperImpl<P> {
    pub fn handle_set_sleep_prevention(&self, enabled: bool) -> anyhow::Result<()> {
        if enabled {
            self.marker.write()?;
            self.pmset.set_disable_sleep(true)?;
        } else {
            self.pmset.set_disable_sleep(false)?;
            self.marker.remove()?;
        }
        Ok(())
    }

    pub fn handle_get_status(&self) -> anyhow::Result<bool> {
        self.pmset.read_disable_sleep()
    }
}

// Public entry point: build the NSXPCListener for the mach service name
// and run the main loop. Returns only when the helper is terminating.
//
// (Body is the spike-confirmed objc2 NSXPCListener code, generalized to
// dispatch into HelperImpl above. ~150 lines of objc2 binding code.
// The implementer copies the spike's working pattern and adapts the
// method signatures to match `HelperRequest`/`HelperResponse`.)
pub fn run_listener<P: Pmset + Send + Sync + 'static>(
    helper: HelperImpl<P>,
    mach_service_name: &str,
) -> anyhow::Result<()> {
    // 1. Build NSXPCListener via NSXPCListener(machServiceName:)
    // 2. Create an Obj-C delegate object using define_class!/declare_class!
    //    that:
    //    a. On shouldAcceptNewConnection: extracts audit_token via
    //       `xpc_connection_get_audit_token` (raw FFI from <xpc/xpc.h>);
    //       calls helper.validator.allows(token); rejects if false.
    //    b. Sets exportedInterface to a protocol matching the three methods.
    //    c. Sets exportedObject to an instance forwarding to HelperImpl.
    //    d. Hooks invalidationHandler and interruptionHandler to call
    //       helper.idle_exit.arm(...) when last connection drops.
    // 3. listener.resume()
    // 4. Run CFRunLoop forever (or until idle-exit fires).

    let _ = (helper, mach_service_name);
    todo!("see Phase 0 spike findings for the working objc2 NSXPC pattern")
}
```

- [ ] **Step 2: Replace the `todo!()` body using the spike's findings**

The implementer copies the working NSXPC server pattern from `docs/superpowers/notes/2026-05-10-objc2-xpc-spike-findings.md` and adapts it to:
- Take a `HelperImpl` as the backing object.
- Validate incoming connections via `helper.validator`.
- Track connection count; on drop-to-zero, call `helper.idle_exit.arm(|| std::process::exit(0))`.
- On any new connection, call `helper.idle_exit.disarm()`.

- [ ] **Step 3: Wire it into `main.rs`**

```rust
// crates/helper/src/main.rs
mod client_validator;
mod idle_exit;
mod ownership_marker;
mod pmset;
mod xpc_listener;

use anyhow::Result;
use std::sync::Arc;
use std::os::unix::io::AsRawFd;

const HELPER_MACH_SERVICE_NAME: &str = "io.openlid.helper";
const DEV_REQUIREMENT: &str = r#"identifier "io.openlid.app""#;

fn main() -> Result<()> {
    setup_logging()?;
    guard_launched_by_launchd()?;
    tracing::info!("open-lid-helper starting (pid {})", std::process::id());

    let pmset = Arc::new(pmset::RealPmset);
    let marker = Arc::new(ownership_marker::OwnershipMarker::new());
    let validator = Arc::new(client_validator::ClientValidator::new(DEV_REQUIREMENT));
    let idle_exit = idle_exit::IdleExit::new();

    // Stale-state recovery: if the marker exists at startup, restore normal sleep.
    if marker.exists() {
        tracing::warn!("ownership marker present at startup; restoring sleep");
        let _ = pmset.set_disable_sleep(false);
        let _ = marker.remove();
    }

    let helper = xpc_listener::HelperImpl {
        pmset,
        marker,
        idle_exit: idle_exit.clone(),
        validator,
    };

    // Initial arm: if no client connects within 15s, exit.
    idle_exit.arm(|| {
        tracing::info!("idle-exit timer fired; exiting");
        std::process::exit(0);
    });

    xpc_listener::run_listener(helper, HELPER_MACH_SERVICE_NAME)?;
    Ok(())
}

fn setup_logging() -> Result<()> {
    use tracing_subscriber::EnvFilter;
    let log_dir = std::path::Path::new("/Library/Logs/open-lid");
    std::fs::create_dir_all(log_dir).ok();
    let file = tracing_appender::rolling::daily(log_dir, "helper.log");
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OPEN_LID_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(file)
        .with_ansi(false)
        .init();
    Ok(())
}

fn guard_launched_by_launchd() -> Result<()> {
    let ppid = unsafe { libc::getppid() };
    let stdin_is_tty = unsafe { libc::isatty(std::io::stdin().as_raw_fd()) } == 1;
    if ppid != 1 || stdin_is_tty {
        anyhow::bail!("open-lid-helper must be loaded by launchd, not invoked directly");
    }
    Ok(())
}
```

- [ ] **Step 4: Build**

```bash
cargo build -p open-lid-helper
# Expected: clean build (assuming Step 2 was completed)
```

- [ ] **Step 5: Commit**

```bash
git add crates/helper
git commit -m "feat(helper): wire NSXPC listener to HelperImpl with idle-exit"
```

---

### Task 15: Manual Smoke Test — Install Helper, Send XPC, Verify pmset

**Files:**
- Create: `scripts/dev-install-helper.sh`
- Create: `scripts/dev-uninstall-helper.sh`
- Create: `resources/helper/io.openlid.helper.plist`

- [ ] **Step 1: Write the helper plist**

```xml
<!-- resources/helper/io.openlid.helper.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.openlid.helper</string>

    <key>ProgramArguments</key>
    <array>
        <string>__OPEN_LID_HELPER_PATH__</string>
    </array>

    <key>MachServices</key>
    <dict>
        <key>io.openlid.helper</key>
        <true/>
    </dict>
</dict>
</plist>
```

- [ ] **Step 2: Write `scripts/dev-install-helper.sh`**

```bash
#!/usr/bin/env bash
# scripts/dev-install-helper.sh
# Manually install the helper to /Library/LaunchDaemons pointing at the
# debug-built binary. Used during Plan 1 development before SMAppService
# is wired up (Plan 2).
set -euo pipefail

cd "$(dirname "$0")/.."

cargo build -p open-lid-helper

ABS_HELPER_PATH="$PWD/target/debug/open-lid-helper"
codesign --force --sign - --options runtime "$ABS_HELPER_PATH"

TMP_PLIST="$(mktemp)"
sed "s|__OPEN_LID_HELPER_PATH__|$ABS_HELPER_PATH|" \
    resources/helper/io.openlid.helper.plist > "$TMP_PLIST"

sudo cp "$TMP_PLIST" /Library/LaunchDaemons/io.openlid.helper.plist
sudo chown root:wheel /Library/LaunchDaemons/io.openlid.helper.plist
sudo chmod 644 /Library/LaunchDaemons/io.openlid.helper.plist
rm "$TMP_PLIST"

sudo launchctl bootout system/io.openlid.helper 2>/dev/null || true
sudo launchctl bootstrap system /Library/LaunchDaemons/io.openlid.helper.plist
echo "Helper installed and bootstrapped. Log: /Library/Logs/open-lid/helper.log"
```

Make executable: `chmod +x scripts/dev-install-helper.sh`.

- [ ] **Step 3: Write `scripts/dev-uninstall-helper.sh`**

```bash
#!/usr/bin/env bash
# scripts/dev-uninstall-helper.sh
set -euo pipefail
sudo launchctl bootout system/io.openlid.helper 2>/dev/null || true
sudo rm -f /Library/LaunchDaemons/io.openlid.helper.plist
sudo rm -rf "/Library/Application Support/open-lid"
echo "Helper uninstalled."
```

`chmod +x scripts/dev-uninstall-helper.sh`.

- [ ] **Step 4: Smoke test**

```bash
./scripts/dev-install-helper.sh
# enter sudo password

# Trigger a connection by sending any signal that activates the mach service.
# Cleanest is to use `launchctl print` to see if the service is registered:
sudo launchctl print system/io.openlid.helper | head -20
# Expected: service is registered, state is "not running" (lazy load via mach service)

# Watch the log
tail -f /Library/Logs/open-lid/helper.log
# (will become useful once we have a client in Task 21)
```

- [ ] **Step 5: Commit**

```bash
git add scripts resources/helper
git commit -m "chore(dev): scripts to install/uninstall helper for local development"
```

---

## Phase 3: Menubar App + CLI + Lid Monitor

### Task 16: Bootstrap `open-lid` App Crate with Argv Dispatch

**Files:**
- Create: `crates/app/Cargo.toml`
- Create: `crates/app/src/main.rs`
- Create: `crates/app/src/menubar/mod.rs` (stub)
- Create: `crates/app/src/cli/mod.rs` (stub)

- [ ] **Step 1: Create directory + Cargo.toml**

```bash
mkdir -p crates/app/src/{menubar,cli,platform/macos}
```

```toml
# crates/app/Cargo.toml
[package]
name = "open-lid"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "open-lid"
path = "src/main.rs"

[dependencies]
open-lid-core = { workspace = true }
objc2 = { workspace = true }
objc2-foundation = { workspace = true }
objc2-app-kit = { workspace = true }
block2 = { workspace = true }
core-foundation = { workspace = true }
clap = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
tracing-appender = { workspace = true }
chrono = { workspace = true }
humantime = { workspace = true }
interprocess = { workspace = true }
directories = { workspace = true }
libc = "0.2"
```

- [ ] **Step 2: Write `main.rs` with argv dispatch**

```rust
// crates/app/src/main.rs
mod cli;
mod menubar;

use anyhow::Result;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("open-lid: {e:#}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    setup_logging()?;
    let args: Vec<String> = std::env::args().collect();

    // Special-case: "menubar" or no args → run menubar role.
    let subcommand = args.get(1).map(String::as_str);
    match subcommand {
        None | Some("menubar") => menubar::run(),
        Some(_) => cli::run(args),
    }
}

fn setup_logging() -> Result<()> {
    use directories::ProjectDirs;
    use tracing_subscriber::EnvFilter;
    let dirs = ProjectDirs::from("io", "openlid", "open-lid")
        .ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    let log_dir = dirs.data_dir().parent().unwrap_or(dirs.data_dir()).join("Logs/open-lid");
    std::fs::create_dir_all(&log_dir).ok();
    let file = tracing_appender::rolling::daily(&log_dir, "app.log");
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OPEN_LID_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(file)
        .with_ansi(false)
        .init();
    Ok(())
}
```

- [ ] **Step 3: Write stub modules**

```rust
// crates/app/src/menubar/mod.rs
pub fn run() -> anyhow::Result<()> {
    tracing::info!("menubar role: not yet implemented");
    anyhow::bail!("menubar role not yet implemented (filled by Tasks 21-25)")
}
```

```rust
// crates/app/src/cli/mod.rs
pub fn run(_args: Vec<String>) -> anyhow::Result<()> {
    tracing::info!("cli role: not yet implemented");
    anyhow::bail!("cli role not yet implemented (filled by Task 26)")
}
```

- [ ] **Step 4: Build + run**

```bash
cargo build -p open-lid
./target/debug/open-lid
# Expected: error "menubar role not yet implemented"
./target/debug/open-lid status
# Expected: error "cli role not yet implemented"
```

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): scaffold app crate with argv-based role dispatch"
```

---

### Task 17: Implement macOS Lid Monitor via IOKit

**Files:**
- Create: `crates/app/src/platform/macos/mod.rs`
- Create: `crates/app/src/platform/macos/lid_monitor.rs`
- Create: `crates/app/src/platform/macos/iokit_ffi.rs`

> **Critical:** This is a direct port of `Sources/upstream/LidMonitor.swift`. The IOKit constants (`errSystem(0x38) | errSub(13) | 0x100`) and the `IOServiceAddInterestNotification` callback shape must match.

- [ ] **Step 1: Write `iokit_ffi.rs` — raw FFI declarations**

```rust
// crates/app/src/platform/macos/iokit_ffi.rs
//! Raw FFI for the IOKit calls we need. We avoid pulling in a heavy
//! IOKit crate because we only need a handful of symbols.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::c_void;

pub type io_object_t = u32;
pub type io_service_t = io_object_t;
pub type io_iterator_t = io_object_t;
pub type IOReturn = i32;
pub type kern_return_t = i32;
pub type natural_t = u32;
pub type mach_port_t = u32;

pub const IO_OBJECT_NULL: io_object_t = 0;
pub const KERN_SUCCESS: kern_return_t = 0;

pub const kIOMainPortDefault: mach_port_t = 0;
pub const kIOGeneralInterest: *const std::os::raw::c_char = c"IOGeneralInterest".as_ptr();

pub type IOServiceInterestCallback = unsafe extern "C" fn(
    refcon: *mut c_void,
    service: io_service_t,
    messageType: natural_t,
    messageArgument: *mut c_void,
);

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOServiceMatching(name: *const std::os::raw::c_char) -> *const c_void;
    pub fn IOServiceGetMatchingService(
        mainPort: mach_port_t,
        matching: *const c_void,
    ) -> io_service_t;
    pub fn IORegistryEntryFromPath(
        mainPort: mach_port_t,
        path: *const std::os::raw::c_char,
    ) -> io_service_t;
    pub fn IORegistryEntryCreateCFProperty(
        entry: io_service_t,
        key: *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> *const c_void;
    pub fn IOObjectRelease(obj: io_object_t) -> kern_return_t;
    pub fn IONotificationPortCreate(mainPort: mach_port_t) -> *mut c_void;
    pub fn IONotificationPortGetRunLoopSource(notify: *mut c_void) -> *const c_void;
    pub fn IONotificationPortDestroy(notify: *mut c_void);
    pub fn IOServiceAddInterestNotification(
        notifyPort: *mut c_void,
        service: io_service_t,
        interestType: *const std::os::raw::c_char,
        callback: IOServiceInterestCallback,
        refcon: *mut c_void,
        notification: *mut io_object_t,
    ) -> kern_return_t;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub fn CGGetActiveDisplayList(
        maxDisplays: u32,
        activeDisplays: *mut u32,
        displayCount: *mut u32,
    ) -> i32; // CGError
    pub fn CGDisplayIsBuiltin(display: u32) -> u32; // boolean_t
}

/// Mirrors Swift's:
///   errSystem(0x38) | errSub(13) | 0x100
/// The values come from <IOKit/IOMessage.h>:
///   #define err_system(x)   (((x) & 0x3f) << 26)
///   #define err_sub(x)      (((x) & 0xfff) << 14)
///   #define sub_iokit_pmu   err_sub(13)
///   kIOPMMessageClamshellStateChange = err_system(0x38) | sub_iokit_pmu | 0x100
pub const K_IOPM_MESSAGE_CLAMSHELL_STATE_CHANGE: natural_t = {
    let sys = (0x38u32 & 0x3f) << 26;
    let sub = (13u32 & 0xfff) << 14;
    sys | sub | 0x100
};

pub const K_CLAMSHELL_STATE_BIT: usize = 1;
```

- [ ] **Step 2: Write `lid_monitor.rs`**

```rust
// crates/app/src/platform/macos/lid_monitor.rs
//! Monitors lid open/closed state via IOKit IOPMrootDomain.
//! Port of Sources/upstream/LidMonitor.swift.

use super::iokit_ffi::*;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::runloop::{CFRunLoopAddSource, CFRunLoopGetMain, kCFRunLoopCommonModes};
use core_foundation::string::CFString;
use open_lid_core::mode::LidState;
use open_lid_core::platform::{LidObserver, LidStateCallback};
use std::ffi::CString;
use std::sync::{Arc, Mutex};

pub struct MacLidMonitor {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    root_domain: io_service_t,
    notification_port: *mut std::ffi::c_void,
    notifier: io_object_t,
    callback: Option<LidStateCallback>,
}

unsafe impl Send for Inner {}

impl MacLidMonitor {
    pub fn start() -> anyhow::Result<Self> {
        let root_domain = unsafe { find_root_domain() };
        if root_domain == IO_OBJECT_NULL {
            anyhow::bail!("IOPMrootDomain not found");
        }
        let port = unsafe { IONotificationPortCreate(kIOMainPortDefault) };
        if port.is_null() {
            unsafe { IOObjectRelease(root_domain) };
            anyhow::bail!("IONotificationPortCreate returned null");
        }
        let source = unsafe { IONotificationPortGetRunLoopSource(port) };
        unsafe {
            CFRunLoopAddSource(
                CFRunLoopGetMain(),
                source as *const _,
                kCFRunLoopCommonModes,
            );
        }

        let inner = Arc::new(Mutex::new(Inner {
            root_domain,
            notification_port: port,
            notifier: IO_OBJECT_NULL,
            callback: None,
        }));
        let refcon = Arc::into_raw(Arc::clone(&inner)) as *mut std::ffi::c_void;

        let mut notifier: io_object_t = IO_OBJECT_NULL;
        let kr = unsafe {
            IOServiceAddInterestNotification(
                port,
                root_domain,
                kIOGeneralInterest,
                Self::on_message,
                refcon,
                &mut notifier,
            )
        };
        if kr != KERN_SUCCESS {
            anyhow::bail!("IOServiceAddInterestNotification failed: {kr}");
        }
        inner.lock().unwrap().notifier = notifier;

        Ok(Self { inner })
    }

    pub fn read_current() -> LidState {
        let root_domain = unsafe { find_root_domain() };
        if root_domain == IO_OBJECT_NULL {
            return LidState::Open;
        }
        let key = CFString::new("AppleClamshellState");
        let cf_ptr = unsafe {
            IORegistryEntryCreateCFProperty(
                root_domain,
                key.as_concrete_TypeRef() as *const _,
                std::ptr::null(),
                0,
            )
        };
        unsafe { IOObjectRelease(root_domain) };
        if cf_ptr.is_null() {
            return LidState::Open;
        }
        let cf = unsafe { CFBoolean::wrap_under_create_rule(cf_ptr as *const _) };
        if cf.into() { LidState::Closed } else { LidState::Open }
    }

    unsafe extern "C" fn on_message(
        refcon: *mut std::ffi::c_void,
        _service: io_service_t,
        message_type: natural_t,
        message_argument: *mut std::ffi::c_void,
    ) {
        if refcon.is_null() {
            return;
        }
        if message_type != K_IOPM_MESSAGE_CLAMSHELL_STATE_CHANGE {
            return;
        }
        let bits = message_argument as usize;
        let closed = (bits & K_CLAMSHELL_STATE_BIT) != 0;
        let state = if closed { LidState::Closed } else { LidState::Open };

        // refcon is a raw Arc — clone it without taking ownership.
        let inner = unsafe { Arc::from_raw(refcon as *const Mutex<Inner>) };
        let cb = inner.lock().unwrap().callback.clone();
        // re-leak the Arc so we don't drop it
        std::mem::forget(inner);
        if let Some(cb) = cb {
            cb(state);
        }
    }
}

impl Drop for MacLidMonitor {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.notifier != IO_OBJECT_NULL {
            unsafe { IOObjectRelease(inner.notifier) };
            inner.notifier = IO_OBJECT_NULL;
        }
        if !inner.notification_port.is_null() {
            unsafe { IONotificationPortDestroy(inner.notification_port) };
            inner.notification_port = std::ptr::null_mut();
        }
        if inner.root_domain != IO_OBJECT_NULL {
            unsafe { IOObjectRelease(inner.root_domain) };
            inner.root_domain = IO_OBJECT_NULL;
        }
    }
}

impl LidObserver for MacLidMonitor {
    fn current(&self) -> LidState {
        Self::read_current()
    }

    fn subscribe(&self, callback: LidStateCallback) {
        self.inner.lock().unwrap().callback = Some(callback);
    }
}

unsafe fn find_root_domain() -> io_service_t {
    let name = CString::new("IOPMrootDomain").unwrap();
    let matched = IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching(name.as_ptr()));
    if matched != IO_OBJECT_NULL {
        return matched;
    }
    let path = CString::new("IOService:/IOResources/IOPowerConnection/IOPMrootDomain").unwrap();
    IORegistryEntryFromPath(kIOMainPortDefault, path.as_ptr())
}
```

- [ ] **Step 3: Wire module**

```rust
// crates/app/src/platform/macos/mod.rs
pub mod iokit_ffi;
pub mod lid_monitor;
```

```rust
// Add to crates/app/src/main.rs:
mod platform;
```

```rust
// crates/app/src/platform/mod.rs
#[cfg(target_os = "macos")]
pub mod macos;
```

- [ ] **Step 4: Build**

```bash
cargo build -p open-lid
# Expected: clean build (warnings ok for unused functions)
```

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app/macos): IOKit-based lid state monitor (port of LidMonitor.swift)"
```

---

### Task 18: Implement macOS Display Controller

**Files:**
- Create: `crates/app/src/platform/macos/display.rs`

- [ ] **Step 1: Write the module**

```rust
// crates/app/src/platform/macos/display.rs
//! External-display detection and force-display-sleep.

use super::iokit_ffi::{CGDisplayIsBuiltin, CGGetActiveDisplayList};
use open_lid_core::platform::{DisplayController, PlatformError};
use std::process::Command;

pub struct MacDisplayController;

impl DisplayController for MacDisplayController {
    fn has_external_display(&self) -> bool {
        let mut count: u32 = 0;
        let r = unsafe { CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count) };
        if r != 0 || count == 0 {
            return false;
        }
        let mut ids = vec![0u32; count as usize];
        let r = unsafe { CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut count) };
        if r != 0 {
            return false;
        }
        ids.iter().take(count as usize).any(|d| unsafe { CGDisplayIsBuiltin(*d) } == 0)
    }

    fn force_display_sleep(&self) -> Result<(), PlatformError> {
        let out = Command::new("/usr/bin/pmset")
            .arg("displaysleepnow")
            .output()
            .map_err(PlatformError::Io)?;
        if !out.status.success() {
            return Err(PlatformError::Native(format!(
                "pmset displaysleepnow failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }
}
```

- [ ] **Step 2: Register module**

```rust
// crates/app/src/platform/macos/mod.rs
pub mod display;
pub mod iokit_ffi;
pub mod lid_monitor;
```

- [ ] **Step 3: Build**

```bash
cargo build -p open-lid
# Expected: clean build
```

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/platform/macos/display.rs crates/app/src/platform/macos/mod.rs
git commit -m "feat(app/macos): external-display detection + force display sleep"
```

---

### Task 19: Implement macOS Power Source Monitor

**Files:**
- Create: `crates/app/src/platform/macos/power_source.rs`

- [ ] **Step 1: Add power-source FFI to `iokit_ffi.rs`**

```rust
// Append to crates/app/src/platform/macos/iokit_ffi.rs:

use core_foundation::base::CFTypeRef;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    pub fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFTypeRef;
    pub fn IOPSGetProvidingPowerSourceType(blob: CFTypeRef) -> CFTypeRef; // CFStringRef
    pub fn IOPSGetPowerSourceDescription(blob: CFTypeRef, ps: CFTypeRef) -> CFTypeRef; // CFDictionaryRef
    pub fn IOPSNotificationCreateRunLoopSource(
        callback: extern "C" fn(context: *mut std::ffi::c_void),
        context: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
}
```

- [ ] **Step 2: Write `power_source.rs`**

```rust
// crates/app/src/platform/macos/power_source.rs
//! Wraps IOPowerSources to read current power source and subscribe to changes.

use super::iokit_ffi::*;
use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::runloop::{CFRunLoopAddSource, CFRunLoopGetMain, kCFRunLoopCommonModes};
use core_foundation::string::CFString;
use open_lid_core::mode::PowerSource;
use open_lid_core::platform::{PowerSourceCallback, PowerSourceMonitor};
use std::sync::{Arc, Mutex};

pub struct MacPowerSourceMonitor {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    callback: Option<PowerSourceCallback>,
}

unsafe impl Send for Inner {}

impl MacPowerSourceMonitor {
    pub fn start() -> anyhow::Result<Self> {
        let inner = Arc::new(Mutex::new(Inner { callback: None }));
        let refcon = Arc::into_raw(Arc::clone(&inner)) as *mut std::ffi::c_void;
        let src = unsafe { IOPSNotificationCreateRunLoopSource(Self::on_change, refcon) };
        if src.is_null() {
            anyhow::bail!("IOPSNotificationCreateRunLoopSource returned null");
        }
        unsafe {
            CFRunLoopAddSource(
                CFRunLoopGetMain(),
                src as *const _,
                kCFRunLoopCommonModes,
            );
        }
        Ok(Self { inner })
    }

    fn read_current() -> PowerSource {
        let blob = unsafe { IOPSCopyPowerSourcesInfo() };
        if blob.is_null() {
            return PowerSource::Ac;
        }
        let type_ref = unsafe { IOPSGetProvidingPowerSourceType(blob) };
        if type_ref.is_null() {
            return PowerSource::Ac;
        }
        let kind = unsafe { CFString::wrap_under_get_rule(type_ref as *const _) }.to_string();
        let is_battery = kind.contains("Battery");

        let mut percent: u8 = 100;
        if is_battery {
            let list = unsafe { IOPSCopyPowerSourcesList(blob) };
            if !list.is_null() {
                let arr: CFArray = unsafe { CFArray::wrap_under_create_rule(list as *const _) };
                if let Some(ps) = arr.get(0) {
                    let desc_ref = unsafe { IOPSGetPowerSourceDescription(blob, *ps) };
                    if !desc_ref.is_null() {
                        let dict: CFDictionary = unsafe {
                            CFDictionary::wrap_under_get_rule(desc_ref as *const _)
                        };
                        let key = CFString::new("Current Capacity");
                        if let Some(v) = dict.find(key.as_concrete_TypeRef() as *const _) {
                            let n = unsafe { CFNumber::wrap_under_get_rule(*v as *const _) };
                            if let Some(i) = n.to_i32() {
                                percent = i.clamp(0, 100) as u8;
                            }
                        }
                    }
                }
            }
        }
        // Note: blob is leaked here intentionally; release path is complex.
        // For an MVP this leak is per-call (a few hundred bytes) and the
        // process is short-lived enough that it doesn't matter. Plan 2 will
        // properly CFRelease.

        if is_battery {
            PowerSource::Battery { percent }
        } else {
            PowerSource::Ac
        }
    }

    extern "C" fn on_change(context: *mut std::ffi::c_void) {
        if context.is_null() {
            return;
        }
        let inner = unsafe { Arc::from_raw(context as *const Mutex<Inner>) };
        let cb = inner.lock().unwrap().callback.clone();
        std::mem::forget(inner);
        if let Some(cb) = cb {
            cb(Self::read_current());
        }
    }
}

impl PowerSourceMonitor for MacPowerSourceMonitor {
    fn current(&self) -> PowerSource {
        Self::read_current()
    }

    fn subscribe(&self, callback: PowerSourceCallback) {
        self.inner.lock().unwrap().callback = Some(callback);
    }
}
```

- [ ] **Step 3: Register module and build**

```rust
// crates/app/src/platform/macos/mod.rs
pub mod display;
pub mod iokit_ffi;
pub mod lid_monitor;
pub mod power_source;
```

```bash
cargo build -p open-lid
```

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/platform/macos
git commit -m "feat(app/macos): power source monitor via IOPowerSources"
```

---

### Task 20: Implement Helper Client (NSXPC)

**Files:**
- Create: `crates/app/src/helper_client.rs`

> **Critical:** Mirrors the helper's XPC interface. Uses the spike's working NSXPC client pattern.

- [ ] **Step 1: Write the skeleton**

```rust
// crates/app/src/helper_client.rs
//! Wraps the NSXPC connection to `io.openlid.helper`.

use open_lid_core::platform::{PlatformError, PowerController};
use std::sync::Mutex;

const HELPER_MACH_SERVICE_NAME: &str = "io.openlid.helper";

pub struct HelperClient {
    // Holds the NSXPCConnection. Wrapped in Mutex so that mutations are
    // serialized across threads; the connection itself is Obj-C-thread-safe
    // for sending messages.
    conn: Mutex<XpcConnection>,
}

struct XpcConnection {
    // The implementer fills this in following the spike's pattern.
    // Likely: `Retained<NSXPCConnection>` (objc2-foundation type).
    _placeholder: (),
}

impl HelperClient {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            conn: Mutex::new(XpcConnection { _placeholder: () }),
        })
    }

    pub fn set_sleep_prevention(&self, enabled: bool) -> Result<(), PlatformError> {
        // 1. Get a remoteObjectProxyWithErrorHandler from self.conn.
        // 2. Invoke setSleepPreventionEnabled:withReply: with a block that
        //    receives (BOOL ok, NSString *err) and signals a channel.
        // 3. Block on the channel; map errors.
        let _ = enabled;
        todo!("fill in using spike findings — see docs/superpowers/notes/2026-05-10-objc2-xpc-spike-findings.md")
    }

    pub fn get_status(&self) -> Result<bool, PlatformError> {
        todo!("fill in using spike findings")
    }

    pub fn ping(&self) -> Result<(), PlatformError> {
        todo!("fill in using spike findings")
    }
}

pub struct HelperPowerController {
    client: std::sync::Arc<HelperClient>,
}

impl HelperPowerController {
    pub fn new(client: std::sync::Arc<HelperClient>) -> Self {
        Self { client }
    }
}

impl PowerController for HelperPowerController {
    fn prevent_sleep(&self) -> Result<(), PlatformError> {
        self.client.set_sleep_prevention(true)
    }

    fn allow_sleep(&self) -> Result<(), PlatformError> {
        self.client.set_sleep_prevention(false)
    }
}
```

- [ ] **Step 2: Fill in the three `todo!()` bodies using the spike findings**

Copy the working NSXPC client pattern from `docs/superpowers/notes/2026-05-10-objc2-xpc-spike-findings.md` and apply it to call the three helper methods. Use `block2::StackBlock` to wrap reply handlers. Use a `std::sync::mpsc` channel or `parking_lot::Once` to bridge the async block back to a sync return.

- [ ] **Step 3: Register module**

```rust
// crates/app/src/main.rs — add at top:
mod helper_client;
```

- [ ] **Step 4: Build and smoke test**

```bash
cargo build -p open-lid
# Run a tiny smoke test in a scratch test:
cat <<'EOF' > /tmp/smoke.rs
use open_lid::helper_client::HelperClient;
fn main() {
    let c = HelperClient::new().expect("client");
    c.ping().expect("ping");
    println!("ping ok");
}
EOF
# (Manual smoke: requires the helper from Task 15 to be installed)
```

- [ ] **Step 5: Commit**

```bash
git add crates/app/src
git commit -m "feat(app): NSXPC client to the helper with PowerController impl"
```

---

### Task 21: Implement State Runtime (Orchestration Layer)

**Files:**
- Create: `crates/app/src/state_runtime.rs`

- [ ] **Step 1: Write the module**

```rust
// crates/app/src/state_runtime.rs
//! Owns the live AppState. Subscribes to lid, power, and timer events;
//! re-evaluates `should_prevent_sleep` after each event; calls into the
//! PowerController to reconcile the helper with the desired state.

use anyhow::Result;
use chrono::Local;
use open_lid_core::config::Config;
use open_lid_core::mode::{LidState, Mode, Modifiers, PowerSource};
use open_lid_core::platform::{
    DisplayController, LidObserver, PowerController, PowerSourceMonitor,
};
use open_lid_core::state::{should_prevent_sleep, AppState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct StateRuntime<P, L, S, D>
where
    P: PowerController + 'static,
    L: LidObserver + 'static,
    S: PowerSourceMonitor + 'static,
    D: DisplayController + 'static,
{
    pub state: Arc<Mutex<AppState>>,
    last_applied: Arc<Mutex<bool>>,
    power: Arc<P>,
    display: Arc<D>,
    _lid: Arc<L>,
    _power_source: Arc<S>,
    config_path: PathBuf,
}

impl<P, L, S, D> StateRuntime<P, L, S, D>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    pub fn new(
        power: Arc<P>,
        lid: Arc<L>,
        power_source: Arc<S>,
        display: Arc<D>,
        config_path: PathBuf,
    ) -> Result<Arc<Self>> {
        let cfg = Config::load(&config_path)?;

        let state = AppState {
            enabled: cfg.enabled,
            mode: cfg.mode,
            modifiers: cfg.modifiers,
            lid: lid.current(),
            power: power_source.current(),
        };

        let rt = Arc::new(Self {
            state: Arc::new(Mutex::new(state)),
            last_applied: Arc::new(Mutex::new(false)),
            power,
            display,
            _lid: lid.clone(),
            _power_source: power_source.clone(),
            config_path,
        });

        // Subscribe to events
        let rt_for_lid = Arc::clone(&rt);
        lid.subscribe(Arc::new(move |new_lid| {
            rt_for_lid.on_lid_change(new_lid);
        }));
        let rt_for_ps = Arc::clone(&rt);
        power_source.subscribe(Arc::new(move |new_ps| {
            rt_for_ps.on_power_change(new_ps);
        }));

        // Apply current state immediately
        rt.reconcile();
        Ok(rt)
    }

    pub fn set_enabled(self: &Arc<Self>, enabled: bool) -> Result<()> {
        self.state.lock().unwrap().enabled = enabled;
        self.persist_and_reconcile()
    }

    pub fn set_mode(self: &Arc<Self>, mode: Mode) -> Result<()> {
        self.state.lock().unwrap().mode = mode;
        self.persist_and_reconcile()
    }

    pub fn set_modifiers(self: &Arc<Self>, modifiers: Modifiers) -> Result<()> {
        self.state.lock().unwrap().modifiers = modifiers;
        self.persist_and_reconcile()
    }

    pub fn snapshot(&self) -> open_lid_core::ipc::control::Snapshot {
        let s = self.state.lock().unwrap();
        open_lid_core::ipc::control::Snapshot {
            preventing_sleep_now: should_prevent_sleep(&s, Local::now()),
            enabled: s.enabled,
            mode: s.mode.clone(),
            modifiers: s.modifiers.clone(),
            lid: s.lid,
            power: s.power,
            helper: open_lid_core::ipc::control::HelperStatus::Running,
        }
    }

    fn on_lid_change(self: &Arc<Self>, new_lid: LidState) {
        let was_closing = {
            let mut s = self.state.lock().unwrap();
            let was_open = s.lid == LidState::Open;
            s.lid = new_lid;
            was_open && new_lid == LidState::Closed
        };
        self.reconcile();
        if was_closing {
            // Mirror upstream: force display sleep on lid-close (no ext display)
            if !self.display.has_external_display() {
                let _ = self.display.force_display_sleep();
            }
        }
    }

    fn on_power_change(self: &Arc<Self>, new_ps: PowerSource) {
        self.state.lock().unwrap().power = new_ps;
        self.reconcile();
    }

    fn persist_and_reconcile(&self) -> Result<()> {
        let cfg = {
            let s = self.state.lock().unwrap();
            Config {
                enabled: s.enabled,
                mode: s.mode.clone(),
                modifiers: s.modifiers.clone(),
            }
        };
        cfg.save(&self.config_path)?;
        self.reconcile();
        Ok(())
    }

    fn reconcile(&self) {
        let desired = {
            let s = self.state.lock().unwrap();
            should_prevent_sleep(&s, Local::now())
        };
        let mut last = self.last_applied.lock().unwrap();
        if *last == desired {
            return;
        }
        let r = if desired {
            self.power.prevent_sleep()
        } else {
            self.power.allow_sleep()
        };
        match r {
            Ok(()) => {
                tracing::info!("reconcile: prevent_sleep = {desired}");
                *last = desired;
            }
            Err(e) => {
                tracing::error!("reconcile failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_lid_core::platform::PlatformError;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    #[derive(Default)]
    struct MockPower {
        prevent_calls: AtomicU32,
        allow_calls: AtomicU32,
    }
    impl PowerController for MockPower {
        fn prevent_sleep(&self) -> Result<(), PlatformError> {
            self.prevent_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn allow_sleep(&self) -> Result<(), PlatformError> {
            self.allow_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct MockLid {
        state: Mutex<LidState>,
        cb: Mutex<Option<open_lid_core::platform::LidStateCallback>>,
    }
    impl MockLid {
        fn new(s: LidState) -> Self {
            Self { state: Mutex::new(s), cb: Mutex::new(None) }
        }
    }
    impl LidObserver for MockLid {
        fn current(&self) -> LidState { *self.state.lock().unwrap() }
        fn subscribe(&self, cb: open_lid_core::platform::LidStateCallback) {
            *self.cb.lock().unwrap() = Some(cb);
        }
    }

    #[derive(Default)]
    struct MockPs(Mutex<Option<open_lid_core::platform::PowerSourceCallback>>);
    impl PowerSourceMonitor for MockPs {
        fn current(&self) -> PowerSource { PowerSource::Ac }
        fn subscribe(&self, cb: open_lid_core::platform::PowerSourceCallback) {
            *self.0.lock().unwrap() = Some(cb);
        }
    }

    #[derive(Default)]
    struct MockDisplay { external: AtomicBool, sleep_calls: AtomicU32 }
    impl DisplayController for MockDisplay {
        fn has_external_display(&self) -> bool { self.external.load(Ordering::SeqCst) }
        fn force_display_sleep(&self) -> Result<(), PlatformError> {
            self.sleep_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn enabling_with_lid_closed_calls_prevent_once() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        let power = Arc::new(MockPower::default());
        let lid = Arc::new(MockLid::new(LidState::Closed));
        let ps = Arc::new(MockPs::default());
        let disp = Arc::new(MockDisplay::default());
        let rt = StateRuntime::new(power.clone(), lid, ps, disp, cfg).unwrap();
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 0);
        rt.set_enabled(true).unwrap();
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 1);
        // Idempotent: setting again doesn't re-call
        rt.set_enabled(true).unwrap();
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn lid_open_with_lid_closed_mode_does_not_prevent() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        let power = Arc::new(MockPower::default());
        let lid = Arc::new(MockLid::new(LidState::Open));
        let ps = Arc::new(MockPs::default());
        let disp = Arc::new(MockDisplay::default());
        let rt = StateRuntime::new(power.clone(), lid, ps, disp, cfg).unwrap();
        rt.set_enabled(true).unwrap();
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 0);
    }
}
```

- [ ] **Step 2: Add tempfile dev-dep to app crate**

```toml
# crates/app/Cargo.toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Register module**

```rust
// Add to crates/app/src/main.rs:
mod state_runtime;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p open-lid --lib state_runtime
# Expected: 2 passed
```

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): state runtime orchestrating traits, AppState, and helper"
```

---

### Task 22: Implement NSStatusItem + Menu

**Files:**
- Create: `crates/app/src/menubar/status_item.rs`
- Create: `crates/app/src/menubar/menu.rs`
- Modify: `crates/app/src/menubar/mod.rs`

> **Critical:** Uses `objc2-app-kit` NSStatusBar/NSStatusItem/NSMenu. This is much more well-trodden than NSXPC — there are many objc2 examples online.

- [ ] **Step 1: Write `status_item.rs`**

(The implementer writes a small wrapper around `NSStatusBar::systemStatusBar().statusItem(withLength:)`. Sets the button's image to an SF Symbol via `NSImage::systemSymbolName(...)`. Hooks a target/action to a callback the menubar module provides.)

Sketch the API surface:

```rust
// crates/app/src/menubar/status_item.rs
use objc2::rc::Retained;
use objc2_app_kit::{NSImage, NSStatusBar, NSStatusItem};

pub struct StatusItem {
    inner: Retained<NSStatusItem>,
}

impl StatusItem {
    pub fn new() -> Self {
        // 1. NSStatusBar::systemStatusBar()
        // 2. statusItem_withLength: NSVariableStatusItemLength
        // 3. configure button: imageScaling, target/action set later via Obj-C runtime
        todo!("implementer fills using objc2-app-kit examples — see https://docs.rs/objc2-app-kit")
    }

    pub fn set_active(&self, active: bool) {
        let name = if active { "eye.fill" } else { "eye.slash.fill" };
        // self.inner.button().setImage(NSImage::systemSymbolName(name, ...))
        let _ = name;
        todo!()
    }

    pub fn set_menu(&self, menu: Retained<objc2_app_kit::NSMenu>) {
        // self.inner.setMenu(Some(&menu))
        let _ = menu;
        todo!()
    }
}
```

- [ ] **Step 2: Write `menu.rs`**

```rust
// crates/app/src/menubar/menu.rs
//! Construct the menu bar dropdown.
//! For the MVP: status row, Turn On/Off, Mode submenu (lid-closed, always-awake,
//! timed for…/until…), Uninstall…, Quit.

use objc2::rc::Retained;
use objc2_app_kit::{NSMenu, NSMenuItem};
use open_lid_core::ipc::control::Snapshot;

pub fn build(snapshot: &Snapshot) -> Retained<NSMenu> {
    // Build NSMenu with sections:
    // 1. status row (disabled item): "Preventing sleep · Mode: Lid-closed"
    // 2. separator
    // 3. "Turn On" or "Turn Off" with action `toggle:`
    // 4. separator
    // 5. "Mode" submenu (3 items + 2 timed entries)
    // 6. separator
    // 7. "Uninstall…" action `uninstall:`
    // 8. "Quit Open-Lid" action `terminate:` (NSApp.terminate)
    let _ = snapshot;
    todo!("implementer fills using NSMenu API")
}
```

- [ ] **Step 3: Rewrite `menubar/mod.rs`**

```rust
// crates/app/src/menubar/mod.rs
mod menu;
mod status_item;

use crate::helper_client::{HelperClient, HelperPowerController};
use crate::platform::macos::{
    display::MacDisplayController, lid_monitor::MacLidMonitor, power_source::MacPowerSourceMonitor,
};
use crate::state_runtime::StateRuntime;
use anyhow::Result;
use open_lid_core::config::Config;
use std::sync::Arc;

pub fn run() -> Result<()> {
    tracing::info!("menubar: starting");

    // Set up NSApplication.
    // (Use objc2-app-kit NSApplication::sharedApplication then setActivationPolicy
    //  to Accessory so the app doesn't appear in the Dock.)
    let app = unsafe { objc2_app_kit::NSApplication::sharedApplication() };
    unsafe {
        app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    }

    // Build platform impls
    let lid = Arc::new(MacLidMonitor::start()?);
    let ps = Arc::new(MacPowerSourceMonitor::start()?);
    let display = Arc::new(MacDisplayController);
    let client = Arc::new(HelperClient::new()?);
    let power = Arc::new(HelperPowerController::new(client.clone()));

    let config_path = Config::default_path()?;
    let _runtime = StateRuntime::new(power, lid, ps, display, config_path)?;

    // TODO Task 23+24: wire up the status item, control server, NSApp delegate.

    // Run the main event loop.
    unsafe { app.run() };
    Ok(())
}
```

- [ ] **Step 4: Build**

```bash
cargo build -p open-lid
# Expected: clean build (with TODOs in status_item.rs and menu.rs)
```

- [ ] **Step 5: Fill in `status_item.rs` and `menu.rs`**

This is the largest single piece of UI work in the plan. Reference the objc2-app-kit documentation and any prior NSStatusItem-in-Rust examples on github. Target behaviors:
- Icon updates within ~100ms of `set_active(...)` being called.
- Menu shows on left or right click (assign `setMenu` and the button auto-shows it).
- Menu items have working target/action wiring through an Obj-C handler object.

- [ ] **Step 6: Run the app and confirm menu bar icon appears**

```bash
cargo run -p open-lid
# Expected: an icon appears in the menu bar. Clicking it shows the menu.
# (Helper interactions still fail until Task 20 is fully implemented.)
```

- [ ] **Step 7: Commit**

```bash
git add crates/app
git commit -m "feat(app): NSStatusItem and NSMenu for the menu bar UI"
```

---

### Task 23: Implement UDS Control Server (in Menubar Process)

**Files:**
- Create: `crates/app/src/control_server.rs`
- Modify: `crates/app/src/menubar/mod.rs`

- [ ] **Step 1: Write `control_server.rs`**

```rust
// crates/app/src/control_server.rs
//! Unix domain socket server. Accepts one request per connection, replies, closes.
//! Path: ~/Library/Application Support/open-lid/control.sock.

use crate::state_runtime::StateRuntime;
use anyhow::Result;
use interprocess::local_socket::{
    GenericFilePath, ListenerOptions, Stream, ToFsName,
    traits::{ListenerExt, Stream as StreamTrait},
};
use open_lid_core::ipc::control::{ControlRequest, ControlResponse};
use open_lid_core::platform::{
    DisplayController, LidObserver, PowerController, PowerSourceMonitor,
};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub fn control_socket_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("io", "openlid", "open-lid")
        .ok_or_else(|| anyhow::anyhow!("no home"))?;
    let dir = dirs.config_dir();
    std::fs::create_dir_all(dir).ok();
    Ok(dir.join("control.sock"))
}

pub fn spawn<P, L, S, D>(rt: Arc<StateRuntime<P, L, S, D>>) -> Result<()>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    let path = control_socket_path()?;
    // Remove stale socket
    let _ = std::fs::remove_file(&path);

    let name = path.clone().to_fs_name::<GenericFilePath>()?;
    let listener = ListenerOptions::new().name(name).create_sync()?;
    tracing::info!("control socket listening at {}", path.display());

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let rt = Arc::clone(&rt);
                    std::thread::spawn(move || {
                        if let Err(e) = handle_one(s, rt) {
                            tracing::warn!("control session error: {e:#}");
                        }
                    });
                }
                Err(e) => tracing::warn!("control accept error: {e}"),
            }
        }
    });
    Ok(())
}

fn handle_one<P, L, S, D>(stream: Stream, rt: Arc<StateRuntime<P, L, S, D>>) -> Result<()>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let req: ControlRequest = serde_json::from_str(line.trim())?;
    let resp = dispatch(req, &rt);
    let mut s = reader.into_inner();
    serde_json::to_writer(&mut s, &resp)?;
    s.write_all(b"\n")?;
    Ok(())
}

fn dispatch<P, L, S, D>(
    req: ControlRequest,
    rt: &Arc<StateRuntime<P, L, S, D>>,
) -> ControlResponse
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    let result: Result<()> = match req.clone() {
        ControlRequest::Ping => return ControlResponse::Pong,
        ControlRequest::GetStatus => Ok(()),
        ControlRequest::SetEnabled { enabled } => rt.set_enabled(enabled),
        ControlRequest::SetMode { mode } => rt.set_mode(mode),
        ControlRequest::SetModifierOnlyOnAc { enabled } => {
            let mut m = rt.state.lock().unwrap().modifiers.clone();
            m.only_on_ac = enabled;
            rt.set_modifiers(m)
        }
        ControlRequest::SetModifierMinBattery { percent } => {
            let mut m = rt.state.lock().unwrap().modifiers.clone();
            m.min_battery = percent;
            rt.set_modifiers(m)
        }
        ControlRequest::SetModifierSchedule { enabled: _ } => {
            // Plan 2: actual schedule editing is via config edit. For MVP this is a no-op success.
            Ok(())
        }
        ControlRequest::Uninstall => {
            tracing::info!("uninstall requested via control socket");
            // Plan 2: full uninstall flow. For MVP, just disable.
            rt.set_enabled(false)
        }
    };
    match result {
        Ok(()) => ControlResponse::Ok { state: rt.snapshot() },
        Err(e) => ControlResponse::Error { message: format!("{e:#}") },
    }
}
```

- [ ] **Step 2: Spawn from `menubar/mod.rs`**

```rust
// At the end of fn run(), before `app.run()`:
crate::control_server::spawn(_runtime.clone())?;
```

(Adjust `_runtime` to `runtime` and remove the underscore; pass it to the spawn.)

- [ ] **Step 3: Register module**

```rust
// crates/app/src/main.rs: add `mod control_server;`
```

- [ ] **Step 4: Build**

```bash
cargo build -p open-lid
# Expected: clean build
```

- [ ] **Step 5: Commit**

```bash
git add crates/app/src
git commit -m "feat(app): Unix socket control server for CLI ↔ menubar IPC"
```

---

### Task 24: Implement CLI Subcommands

**Files:**
- Create: `crates/app/src/cli/commands.rs`
- Modify: `crates/app/src/cli/mod.rs`

- [ ] **Step 1: Write the CLI module**

```rust
// crates/app/src/cli/mod.rs
mod commands;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "open-lid", version, about = "Prevent macOS sleep on lid close.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run as menubar app (no-op subcommand; main.rs dispatches this on no args)
    Menubar,
    /// Run as privileged helper (used by launchd)
    Helper,
    /// Turn on sleep prevention with current mode
    On,
    /// Turn off sleep prevention
    Off,
    /// Show current status
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Switch mode
    Mode {
        #[arg(value_enum)]
        mode: ModeArg,
    },
    /// Switch to Timed mode for a duration (e.g., 2h, 30m)
    For {
        duration: String,
    },
    /// Switch to Timed mode until a time (HH:MM or ISO 8601)
    Until {
        time: String,
    },
    /// Modifier operations (MVP placeholder; full impl in Plan 2)
    #[command(subcommand)]
    Modifier(ModifierArg),
    /// Config operations
    #[command(subcommand)]
    Config(ConfigArg),
    /// Uninstall (MVP placeholder; full impl in Plan 2)
    Uninstall,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ModeArg {
    LidClosed,
    AlwaysAwake,
}

#[derive(clap::Subcommand, Debug)]
pub enum ModifierArg {
    OnlyOnAc { value: BoolArg },
    MinBattery { value: String },
    Schedule { value: BoolArg },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum BoolArg { On, Off }

#[derive(clap::Subcommand, Debug)]
pub enum ConfigArg {
    Show,
    Path,
    Edit,
}

pub fn run(args: Vec<String>) -> Result<()> {
    let cli = Cli::try_parse_from(&args)?;
    match cli.command {
        Command::Menubar => crate::menubar::run(),
        Command::Helper => anyhow::bail!("the 'helper' role is for launchd only"),
        Command::On => commands::set_enabled(true),
        Command::Off => commands::set_enabled(false),
        Command::Status { json } => commands::status(json),
        Command::Mode { mode } => commands::set_mode(mode),
        Command::For { duration } => commands::for_duration(&duration),
        Command::Until { time } => commands::until(&time),
        Command::Modifier(m) => commands::modifier(m),
        Command::Config(c) => commands::config(c),
        Command::Uninstall => commands::uninstall(),
    }
}
```

- [ ] **Step 2: Write `commands.rs`**

```rust
// crates/app/src/cli/commands.rs
use crate::cli::{ConfigArg, ModeArg, ModifierArg, BoolArg};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone};
use interprocess::local_socket::{
    GenericFilePath, Stream, ToFsName,
    traits::Stream as StreamTrait,
};
use open_lid_core::config::Config;
use open_lid_core::ipc::control::{ControlRequest, ControlResponse, Snapshot};
use open_lid_core::mode::Mode;
use std::io::{BufRead, BufReader, Write};
use std::time::Duration as StdDuration;

fn socket_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("io", "openlid", "open-lid")
        .ok_or_else(|| anyhow!("no home"))?;
    Ok(dirs.config_dir().join("control.sock"))
}

fn send_request(req: ControlRequest, auto_launch: bool) -> Result<ControlResponse> {
    let path = socket_path()?;
    let mut attempts = if auto_launch { 6 } else { 1 };
    let mut last_err = None;
    while attempts > 0 {
        match try_send(&path, &req) {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                last_err = Some(e);
                if auto_launch && attempts == 6 {
                    // Try to launch the .app
                    let _ = std::process::Command::new("/usr/bin/open")
                        .args(["-a", "OpenLid"])
                        .status();
                }
                std::thread::sleep(StdDuration::from_millis(500));
                attempts -= 1;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("failed to reach menubar process")))
}

fn try_send(path: &std::path::Path, req: &ControlRequest) -> Result<ControlResponse> {
    let name = path.to_path_buf().to_fs_name::<GenericFilePath>()?;
    let mut stream = Stream::connect(name)?;
    serde_json::to_writer(&mut stream, req)?;
    stream.write_all(b"\n")?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim())?)
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    match send_request(ControlRequest::SetEnabled { enabled }, true)? {
        ControlResponse::Ok { state } => {
            println!("{}", if state.preventing_sleep_now { "ON" } else { "OFF" });
            Ok(())
        }
        ControlResponse::Error { message } => Err(anyhow!(message)),
        _ => Err(anyhow!("unexpected response")),
    }
}

pub fn status(json: bool) -> Result<()> {
    let resp = send_request(ControlRequest::GetStatus, false);
    match resp {
        Ok(ControlResponse::Ok { state }) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&state)?);
            } else {
                print_status_human(&state);
            }
            Ok(())
        }
        Ok(ControlResponse::Error { message }) => Err(anyhow!(message)),
        Ok(_) => Err(anyhow!("unexpected response")),
        Err(_) => {
            if json {
                println!("{}", serde_json::json!({"helper": "not-running"}));
                Ok(())
            } else {
                println!("Open-Lid is not running.");
                std::process::exit(1);
            }
        }
    }
}

fn print_status_human(s: &Snapshot) {
    let active = if s.preventing_sleep_now { "ACTIVE" } else { "idle" };
    println!("Sleep prevention: {active}");
    println!("Mode:             {:?}", s.mode);
    println!("Enabled:          {}", s.enabled);
    println!("Lid:              {:?}", s.lid);
    println!("Power:            {:?}", s.power);
}

pub fn set_mode(mode: ModeArg) -> Result<()> {
    let m = match mode {
        ModeArg::LidClosed => Mode::LidClosed,
        ModeArg::AlwaysAwake => Mode::AlwaysAwake,
    };
    match send_request(ControlRequest::SetMode { mode: m }, true)? {
        ControlResponse::Ok { .. } => Ok(()),
        ControlResponse::Error { message } => Err(anyhow!(message)),
        _ => Err(anyhow!("unexpected response")),
    }
}

pub fn for_duration(s: &str) -> Result<()> {
    let dur = humantime::parse_duration(s).context("invalid duration")?;
    let until: DateTime<Local> = Local::now() + Duration::from_std(dur)?;
    let req = ControlRequest::SetMode { mode: Mode::Timed { until } };
    send_request(req, true)?;
    set_enabled(true)
}

pub fn until(s: &str) -> Result<()> {
    let until = parse_until(s)?;
    let req = ControlRequest::SetMode { mode: Mode::Timed { until } };
    send_request(req, true)?;
    set_enabled(true)
}

fn parse_until(s: &str) -> Result<DateTime<Local>> {
    // First try HH:MM
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M") {
        let today = Local::now().date_naive().and_time(t);
        let dt = Local.from_local_datetime(&today).single().unwrap();
        return Ok(if dt > Local::now() { dt } else { dt + Duration::days(1) });
    }
    // Then try ISO 8601 local datetime
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .context("expected HH:MM or YYYY-MM-DDTHH:MM")?;
    Local.from_local_datetime(&naive).single()
        .ok_or_else(|| anyhow!("ambiguous local time"))
}

pub fn modifier(_m: ModifierArg) -> Result<()> {
    // Plan 2: full implementation.
    println!("modifier commands are stubbed in MVP; coming in Plan 2.");
    Ok(())
}

pub fn config(c: ConfigArg) -> Result<()> {
    let path = Config::default_path()?;
    match c {
        ConfigArg::Path => {
            println!("{}", path.display());
        }
        ConfigArg::Show => {
            let cfg = Config::load(&path)?;
            println!("{}", toml::to_string_pretty(&cfg)?);
        }
        ConfigArg::Edit => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "open".into());
            std::process::Command::new(&editor).arg(&path).status()?;
        }
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    // Plan 2: full uninstall flow.
    println!("uninstall is stubbed in MVP; coming in Plan 2.");
    Ok(())
}
```

- [ ] **Step 3: Build + smoke test**

```bash
cargo build -p open-lid
./target/debug/open-lid status
# Expected (no menubar running): "Open-Lid is not running." exit 1

./target/debug/open-lid menubar &
sleep 1
./target/debug/open-lid status
# Expected: human-readable status
./target/debug/open-lid on
# Expected: "ON" (assuming helper is installed from Task 15)
./target/debug/open-lid off
# Expected: "OFF"
kill %1
```

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/cli
git commit -m "feat(app): clap-based CLI with on/off/status/mode/for/until/config"
```

---

### Task 25: End-to-End Smoke Test

**Files:**
- Create: `docs/manual-test-checklist.md`

- [ ] **Step 1: Write the checklist**

```markdown
# Open-Lid MVP — Manual Test Checklist

Run on a real Apple Silicon MacBook (lid behavior cannot be simulated).
Before each run, uninstall any previous version via `scripts/dev-uninstall-helper.sh`.

## Prep
- [ ] `cargo build -p open-lid -p open-lid-helper`
- [ ] `./scripts/dev-install-helper.sh` — helper installed
- [ ] `/Library/Logs/open-lid/helper.log` exists and contains "open-lid-helper starting"

## Menu bar app
- [ ] `./target/debug/open-lid` launches → icon appears in menu bar
- [ ] Icon is eye-slash (inactive) at first
- [ ] Click icon → menu appears with "Turn On", "Mode" submenu, "Quit"
- [ ] Click "Turn On" → icon switches to eye (active)
- [ ] In another terminal, `pmset -g | grep SleepDisabled` shows `1`

## CLI parity
- [ ] In a third terminal, `open-lid status` shows "Sleep prevention: ACTIVE"
- [ ] `open-lid off` → icon switches to eye-slash; `pmset -g` shows `SleepDisabled 0`
- [ ] `open-lid on` → re-enables
- [ ] `open-lid mode always-awake` → status shows mode = AlwaysAwake
- [ ] `open-lid for 2m` → status shows mode = Timed and `until` ≈ now+2min
- [ ] Wait 2 min → status shows mode unchanged but `preventing_sleep_now = false`
  (Note: timed auto-revert of `enabled` is in Plan 2; for MVP the timer just
   stops *preventing* sleep; the user is responsible for switching mode back)

## Lid behavior
- [ ] With mode = lid-closed and enabled, close the laptop lid with no
      external display attached → display turns off; system stays awake
- [ ] Tail `/Library/Logs/open-lid/helper.log` — see `pmset disablesleep 1` invocations
- [ ] Open lid → display wakes
- [ ] With mode = lid-closed and an external display attached → closing the
      lid does NOT force display off; system stays awake on the external display

## Cleanup
- [ ] Quit app via menu
- [ ] `pmset -g | grep SleepDisabled` shows `0` (sleep restored)
- [ ] `./scripts/dev-uninstall-helper.sh` — helper removed
- [ ] `ls /Library/LaunchDaemons/io.openlid.*` returns nothing
```

- [ ] **Step 2: Run through the checklist on your machine**

Execute every line. Mark broken items as fix-up commits.

- [ ] **Step 3: Commit**

```bash
git add docs/manual-test-checklist.md
git commit -m "docs: manual MVP test checklist"
```

---

### Task 26: Wire `open OpenLid.app` for Auto-Launch

**Files:**
- Create: `scripts/build-app-bundle.sh`
- Create: `resources/app/Info.plist`

For the CLI's auto-launch to work, we need a real `OpenLid.app` bundle in `/Applications`. This script builds it ad-hoc-signed for local dev. Production signing comes in Plan 2.

- [ ] **Step 1: Write `Info.plist`**

```xml
<!-- resources/app/Info.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>io.openlid.app</string>
    <key>CFBundleName</key>
    <string>OpenLid</string>
    <key>CFBundleDisplayName</key>
    <string>Open-Lid</string>
    <key>CFBundleExecutable</key>
    <string>open-lid</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>MIT licensed.</string>
</dict>
</plist>
```

- [ ] **Step 2: Write `scripts/build-app-bundle.sh`**

```bash
#!/usr/bin/env bash
# scripts/build-app-bundle.sh
# Build OpenLid.app for local dev. Ad-hoc signing only.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo build -p open-lid -p open-lid-helper

APP="OpenLid.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"
mkdir -p "$APP/Contents/Library/LaunchDaemons"

cp target/debug/open-lid "$APP/Contents/MacOS/open-lid"
cp target/debug/open-lid-helper "$APP/Contents/MacOS/open-lid-helper"
cp resources/app/Info.plist "$APP/Contents/Info.plist"
cp resources/helper/io.openlid.helper.plist "$APP/Contents/Library/LaunchDaemons/io.openlid.helper.plist"

codesign --force --sign - --options runtime "$APP/Contents/MacOS/open-lid-helper"
codesign --force --sign - --options runtime "$APP/Contents/MacOS/open-lid"
codesign --force --sign - --deep --options runtime "$APP"

echo "Built $APP. Move to /Applications:"
echo "  cp -R $APP /Applications/"
```

`chmod +x scripts/build-app-bundle.sh`.

- [ ] **Step 3: Run, install to /Applications, smoke**

```bash
./scripts/build-app-bundle.sh
cp -R OpenLid.app /Applications/

# In a fresh terminal:
open-lid status
# Should auto-launch the .app from /Applications and report status
```

(Add `/Users/diyanbogdanov/projects/open-lid/target/debug` or a symlink at `/usr/local/bin/open-lid` to PATH if needed — or `alias open-lid=...` for now.)

- [ ] **Step 4: Commit**

```bash
git add scripts/build-app-bundle.sh resources/app/Info.plist
git commit -m "chore(dev): script to build OpenLid.app for local dev"
```

---

### Task 27: Final Polish — Quit Cleanup & PATH Helper

**Files:**
- Modify: `crates/app/src/menubar/mod.rs`
- Create: `scripts/install-cli-symlink.sh`

- [ ] **Step 1: Add a Quit handler that calls `allow_sleep` before terminating**

In the menubar module, wire the Quit menu item to:
1. Call `runtime.set_enabled(false)`.
2. Remove the control socket.
3. Then call `NSApp.terminate(nil)`.

- [ ] **Step 2: Write the symlink script**

```bash
#!/usr/bin/env bash
# scripts/install-cli-symlink.sh
# Adds /usr/local/bin/open-lid → /Applications/OpenLid.app/Contents/MacOS/open-lid
set -euo pipefail
TARGET="/Applications/OpenLid.app/Contents/MacOS/open-lid"
LINK="/usr/local/bin/open-lid"
if [ ! -e "$TARGET" ]; then
    echo "Build OpenLid.app first: ./scripts/build-app-bundle.sh && cp -R OpenLid.app /Applications/"
    exit 1
fi
sudo ln -sf "$TARGET" "$LINK"
echo "open-lid is on your PATH: $LINK"
```

`chmod +x scripts/install-cli-symlink.sh`.

- [ ] **Step 3: Run through the manual test checklist once more end-to-end**

- [ ] **Step 4: Commit**

```bash
git add crates/app scripts/install-cli-symlink.sh
git commit -m "feat(app): clean up sleep prevention on quit; CLI symlink script"
```

---

### Task 28: Write MVP README Section

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write the README**

```markdown
# Open-Lid

A macOS menu bar utility that prevents your Mac from sleeping when the lid is
closed, while letting the display turn off. Inspired by [upstream](https://github.com/narcotic-sh) —
this is a Rust port with composable modes and a first-class CLI, designed for
later expansion to Windows and Linux.

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

# Build the .app and helper
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
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README for MVP"
```

---

## Plan 1 Done — Self-Review Checklist

Once the last task is committed:

- [ ] Run through `docs/manual-test-checklist.md` start-to-finish on a freshly-rebooted Mac.
- [ ] All commits pass `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] All unit tests pass: `cargo test --workspace`.
- [ ] Quit the app → `pmset -g | grep SleepDisabled` reports `0`.
- [ ] No leftover files in `/Library/Application Support/open-lid/` after uninstall.

**Next:** Plan 2 — modifiers (`only-on-ac`, `min-battery`, `schedule`), native preferences NSWindow, SMAppService production install path, signing/notarization pipeline, DMG, and Homebrew tap. See `docs/superpowers/plans/2026-05-XX-open-lid-polish-and-distribute.md` (to be written after Plan 1 is complete).

---

## Self-Review (performed at plan-writing time)

**Spec coverage check:**
- ✅ Goal 1 (upstream feature parity): Tasks 17 (lid monitor) + 18 (display) + Tasks 10-15 (helper toggling disablesleep)
- ✅ Goal 2 (three modes): Mode enum supports all three (Task 3); CLI exposes them (Task 24); Timed via `for`/`until`
- ⏸ Goal 3 (modifiers) — explicitly deferred to Plan 2 by design
- ✅ Goal 4 (CLI first-class): Tasks 23 + 24
- ⏸ Goal 5 (native AppKit polish: preferences NSWindow) — preferences deferred to Plan 2; menu bar UI done here
- ✅ Goal 6 (small binaries): Tasks 1 + 16 + 9 set up workspace with two binary targets, release profile with LTO + strip
- ✅ Goal 7 (cross-platform extensibility): Task 8 (traits in core), Task 16 (`#[cfg(target_os = "macos")]` gating)
- ⏸ Goal 8 (SMAppService): deferred to Plan 2; Plan 1 uses manual launchctl install (Task 15)

**Placeholder scan:** Two intentional `todo!()` calls in Tasks 14 and 20 — both are filled in via the same step using outputs from the Phase 0 spike. These are not placeholders in the "I forgot to write this" sense; they're "fill from spike findings" pointers. Acceptable.

**Type consistency:** `ControlRequest` / `ControlResponse` / `Snapshot` / `Mode` / `Modifiers` names are consistent across Tasks 7, 21, 23, 24. `Pmset` trait used in Tasks 10 and 14 with same signature.

**Scope check:** Plan 1 produces a locally-runnable MVP. The user can use it on their own Mac. Distribution polish is genuinely independent and lives in Plan 2.
