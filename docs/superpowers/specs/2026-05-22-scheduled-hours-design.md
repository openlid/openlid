# Scheduled hours — design

## Summary

Let users define a recurring time window (e.g. 08:00-18:00 Mon-Fri) during
which openlid prevents sleep. Configurable from the CLI and the native
macOS Preferences window. Outside the window, sleep is allowed even
if the main toggle is on.

The pure schedule-evaluation logic — `Schedule { days, start, end }`,
`Schedule::contains(now)`, the `Modifiers::schedule` slot, and the
`modifiers_allow` gate inside `should_prevent_sleep` — is already
implemented and unit-tested in `crates/core/src/mode.rs` and
`crates/core/src/state.rs`. This spec is about **wiring that existing
primitive to user-visible surfaces**: CLI, preferences UI, IPC, and
status output. There are no new state-machine concepts.

## Goals

- A user can set, clear, and inspect a schedule from the CLI.
- A user can do the same from the Preferences window.
- Status output (text and JSON) reflects an active schedule.
- Setting a schedule for the first time "just works" — the toggle
  becomes active without a separate `openlid on`.
- Persistence to `config.toml` works for free (the field is already
  serde-wired); no schema bump.

## Non-goals

- No new "Scheduled" toggle state. The schedule is a *gate* over the
  existing `enabled` flag, exactly as the current `should_prevent_sleep`
  function already treats it.
- No multiple, named schedules (single window per modifier set).
- No timezone or DST handling beyond what `chrono::Local` already
  provides.
- No menubar dropdown shortcut for the schedule — preferences only.
- No helper/IPC-protocol changes (NSXPC layer).
- No `SCHEMA_VERSION` bump.

## Semantics

### When the schedule is set

`Modifiers::schedule = Some(Schedule { days, start, end })`.

`should_prevent_sleep(state, now)` already returns `false` if `now` falls
outside the schedule, regardless of `enabled`. No change to this function.

### Implicit "turn on" when setting a schedule

When the CLI `schedule set` or the UI sets a schedule and the user's
`enabled` flag is currently `false`, the CLI/UI also issues a
`SetEnabled { enabled: true, until: None }` so the new schedule has
something to gate. This avoids the "I set my hours but nothing happens"
trap.

Rationale: the natural-language phrasing "allow openlid to run from
08:00 to 18:00" implies activation, not just gating. Without this, the
user must perform two steps for the common case.

This implicit-enable lives in the CLI command layer (a second IPC call
following the first), and in the menubar preferences action handler.
The IPC `PrefsPatch` itself stays preferences-only — it does not touch
`enabled`. This keeps the protocol abstraction clean and the implicit
behavior visible in the user-facing code paths where it matters.

### When the schedule is cleared

`Modifiers::schedule = None`. The `enabled` flag is **not** touched.
Symmetry: setting opts you in (implicit enable), clearing is a no-op
on `enabled` (the user must explicitly `openlid off` to turn off).

### When `openlid off` is invoked

Existing semantics: `enabled = false`, `until = None`. The schedule is
preserved in `Modifiers`. A subsequent `openlid on` resumes the same
schedule.

### When the user is "ON but outside the window"

`enabled = true`, `Schedule::contains(now) = false`. Status string:
`ON (idle, scheduled HH:MM-HH:MM <day-summary>)`. The menubar icon
treats this as the existing "ON (armed, idle)" state.

## Surfaces

### CLI

New top-level subcommand mirroring the existing `config` style:

```
openlid schedule set --from <HH:MM> --to <HH:MM> [--days <CSV>]
openlid schedule clear
openlid schedule show [--json]
```

- `--from` / `--to`: required. Parsed by `NaiveTime::parse_from_str(s,
  "%H:%M")`. Invalid format → error with hint `expected HH:MM`.
- `--days`: optional CSV of three-letter day names
  (`Mon,Tue,Wed,Thu,Fri,Sat,Sun`), **case-insensitive**.
  Default when omitted: all 7 days (`DaysOfWeek::all()`).
  Unknown token → error naming the bad token.
  Empty `--days ""` → error ("at least one day required").
- `--from == --to` → error ("schedule window must be non-empty").
- `--from > --to` is **allowed** and means a window that crosses
  midnight (e.g. `--from 22:00 --to 02:00`). Already supported by
  `Schedule::contains`.

#### `schedule set` workflow

1. CLI parses args into a `Schedule`.
2. Sends `SetPreferences { schedule: Some(Some(s)) }`. Receives a
   `Snapshot`.
3. If `snapshot.enabled == false`, sends a second
   `SetEnabled { enabled: true, until: None }`.
4. Prints a one-line confirmation:
   `Schedule: 08:00-18:00 (Mon, Tue, Wed, Thu, Fri); openlid is now ON`.

#### `schedule clear` workflow

1. Sends `SetPreferences { schedule: Some(None) }`.
2. Prints `Schedule cleared.`

#### `schedule show` workflow

1. Sends `GetStatus`, reads `snapshot.modifiers.schedule`.
2. Plain text: prints `No schedule set.` or
   `Schedule: HH:MM-HH:MM (<day-summary>)`.
3. `--json`: prints `snapshot.modifiers.schedule` as JSON (`null` or
   the object).

#### `status` text update

`format_status_human` adds one line when `modifiers.schedule.is_some()`:
```
Schedule:         08:00-18:00 (Mon, Tue, Wed, Thu, Fri)
```

Day-summary rules:
- All 7 days → `daily`
- Mon-Fri exactly → `Mon-Fri`
- Sat,Sun exactly → `weekends`
- Otherwise → comma-separated three-letter names in Mon→Sun order

### IPC

`crates/core/src/ipc/control.rs`:

Add one field to `ControlRequest::SetPreferences`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub schedule: Option<Option<Schedule>>,
```

Three-state semantics, matching the other `Option<Option<_>>` fields:
- `None` — don't touch the schedule.
- `Some(None)` — clear it.
- `Some(Some(s))` — set to `s`.

Older clients that don't send the field continue to work (`#[serde(default)]`).
`Snapshot` already carries `modifiers`, so the `schedule show` and
preferences UI can read the value without any new field.

### Runtime

`crates/app/src/state_runtime.rs`:

Extend `PrefsPatch`:
```rust
pub schedule: Option<Option<Schedule>>,
```

In `set_preferences`:
- If `Some(v) = patch.schedule`, write to `state.modifiers.schedule`
  (under the `state` mutex, mirroring how `battery_threshold_pct`
  propagates to `state.modifiers.min_battery` today).
- The existing `persist_and_reconcile` path picks up `state.modifiers`
  and persists `cfg.modifiers = s.modifiers.clone()`.

### Control server

`crates/app/src/control_server.rs`:

In `dispatch`, the `SetPreferences` arm receives the new field and
forwards it through `PrefsPatch`. One-line additive change.

### Preferences UI

`crates/app/src/menubar/preferences.rs`:

Add a new section after the existing battery-threshold row:

```
[ ] Active only during scheduled hours
    From: [   09:00 ▼]    To: [   18:00 ▼]
    Days: [Mo] [Tu] [We] [Th] [Fr] [Sa] [Su]
```

- Master checkbox `NSButton` (style: switch or checkbox; match existing
  prefs window style).
- `From` and `To`: `NSDatePicker` with `datePickerMode = .timePicker`
  and `datePickerStyle = .textFieldAndStepper`, no calendar.
- Seven small `NSButton` checkboxes for days.
- When the master checkbox is off: sub-controls call `setEnabled:NO`
  and visually grey out.
- When the master checkbox is turned on with no prior schedule: the
  handler synthesizes a default `Schedule { days: all 7,
  start: 09:00, end: 17:00 }` and persists it via
  `set_schedule(Some(s))` immediately. The user can then adjust any
  control to refine.
- Apply model: matches the existing prefs window. Each control change
  immediately fires a `PrefsActions::set_schedule(Some(s))` call with
  the current values of all controls. The master checkbox unchecking
  fires `set_schedule(None)`.

Add to `PrefsActions`:
```rust
fn set_schedule(&self, schedule: Option<Schedule>);
```

`RuntimeActions::set_schedule` (in `crates/app/src/menubar/mod.rs`)
builds a `PrefsPatch { schedule: Some(schedule), ..Default::default() }`
and calls `StateRuntime::set_preferences`. If the new schedule is
`Some(_)` and `state.enabled` is `false`, it also calls
`set_enabled(true, None)`. (Mirrors the CLI's implicit-enable.)

#### Cocoa dependencies

`objc2-app-kit` currently lists features but not `NSDatePicker`.
Add `"NSDatePicker"` to the feature list in workspace `Cargo.toml`.

### CLI module

`crates/app/src/cli/mod.rs`:

Add subcommand:
```rust
#[command(subcommand)]
Schedule(ScheduleArg),
```

```rust
#[derive(clap::Subcommand, Debug)]
pub enum ScheduleArg {
    Set {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        days: Option<String>,
    },
    Clear,
    Show {
        #[arg(long)]
        json: bool,
    },
}
```

Dispatcher branch in `run` forwards to new functions in
`commands.rs`.

## Tests

### New unit tests

- `cli::tests::parses_schedule_set_with_days` — clap parse round-trip.
- `cli::tests::parses_schedule_set_default_days_is_none` — no `--days`
  leaves the field as `None` at the clap level (defaulting to all-days
  happens in `commands.rs`).
- `cli::tests::parses_schedule_clear` and `parses_schedule_show`.
- `commands::tests::parse_days_csv_case_insensitive` — `"mon,tue"` →
  `MON | TUE`.
- `commands::tests::parse_days_csv_rejects_unknown` — `"funday"` → Err
  with the bad token in the message.
- `commands::tests::parse_days_csv_empty_string_is_error`.
- `commands::tests::parse_schedule_rejects_equal_from_to`.
- `commands::tests::format_status_includes_schedule_when_set` — uses an
  all-days noon-to-evening schedule; assert "Schedule:" line present.
- `commands::tests::format_status_omits_schedule_when_unset` — guard
  against an accidental always-print regression.
- `commands::tests::format_day_summary_*` — `all_seven_is_daily`,
  `mon_fri_is_weekdays`, `sat_sun_is_weekends`, `arbitrary_subset_csv`.
- `ipc::control::tests::set_preferences_with_schedule_round_trips` —
  serde round-trip with `Some(Some(s))`.
- `ipc::control::tests::set_preferences_omitting_schedule_round_trips` —
  back-compat: a JSON missing `schedule` deserializes to `None`.
- `state_runtime::tests::set_preferences_schedule_some_some_applies` —
  mock runtime, assert `state.modifiers.schedule == Some(s)` after
  call, and config on disk reflects it.
- `state_runtime::tests::set_preferences_schedule_some_none_clears`.
- `state_runtime::tests::set_preferences_schedule_none_leaves_alone`.

### Existing tests reused

- `core::mode::tests` — all schedule-evaluation behavior tests already
  cover same-day, cross-midnight, day-flag dispatch, exact-end boundary.
  No new core tests needed.
- `core::state::tests::schedule_blocks_outside_window` and
  `schedule_allows_inside_window` — unchanged.

### Manual / integration

Add to `docs/manual-test-checklist.md`:
- "Set a schedule via CLI, verify status output, verify config.toml
  contents."
- "Set a schedule via Preferences, verify ON-but-idle behavior outside
  window, verify prevention activates inside window."
- "Cross-midnight: set 22:00-02:00, verify prevention at 23:00 and 01:00."

## Compatibility

- `config.toml` schema: adding `modifiers.schedule` is additive and
  already supported by serde via `#[serde(default,
  skip_serializing_if = "Option::is_none")]` on the field. **No
  `SCHEMA_VERSION` bump.**
- Existing v1.x configs (no `modifiers.schedule` key): load with
  `schedule = None`. Identical behavior to today.
- New configs written by this build: include `[modifiers.schedule]`
  when set, omit when not. Older builds will warn about the unknown
  schema version *only if* `SCHEMA_VERSION` had been bumped — since it
  isn't, older builds simply ignore `modifiers.schedule` and run as
  if no schedule were set. This is a graceful downgrade: the user
  loses the schedule constraint but their `enabled` toggle continues
  to work.

## Files touched (estimate)

1. `crates/core/src/ipc/control.rs` — one new field on `SetPreferences`
   + one new test.
2. `crates/app/src/state_runtime.rs` — extend `PrefsPatch`, apply in
   `set_preferences`, three new tests.
3. `crates/app/src/control_server.rs` — forward the new field
   (one-line edit).
4. `crates/app/src/cli/mod.rs` — new `Schedule(ScheduleArg)` subcommand,
   four new parse tests.
5. `crates/app/src/cli/commands.rs` — new `schedule_set/clear/show`
   functions, day-CSV parser, day-summary formatter, status-string
   extension, ~10 new tests.
6. `crates/app/src/menubar/preferences.rs` — new "Schedule" section
   (checkbox, two date pickers, seven day buttons), trait method,
   handler selector wiring.
7. `crates/app/src/menubar/mod.rs` — `RuntimeActions::set_schedule`
   with the implicit-enable bridge.
8. `Cargo.toml` (workspace) — add `"NSDatePicker"` to the
   `objc2-app-kit` features list.
9. `docs/manual-test-checklist.md` — three new manual test cases.

Net estimate: ~300-400 lines including tests, roughly half in
`preferences.rs` for the new AppKit controls.

## Open risks

- **`NSDatePicker` feature**: the workspace `Cargo.toml` enables a
  specific subset of `objc2-app-kit` features. Adding `NSDatePicker`
  may pull in adjacent types; trivial to add but worth verifying it
  builds clean before going deep into the UI work.
- **UI defaulting**: when the user first ticks "Active only during
  scheduled hours" with no prior schedule, we have to materialize a
  `Schedule` to send through `set_schedule`. Choice locked in: default
  to `09:00-17:00, all days`. The user immediately sees a sensible
  state and can adjust.
- **Race between the CLI's two IPC calls**: `SetPreferences` then
  `SetEnabled` is not atomic. In a single-user, single-binary tool with
  one user actively at the terminal this is not a real concern;
  documented as a known non-issue.

## Out-of-scope follow-ups (intentionally deferred)

- A status-line summary in the menubar dropdown (e.g. a disabled
  menu item showing "Schedule: 08:00-18:00").
- Per-day time windows (e.g. weekdays 09:00-17:00, weekends none).
- Multiple named schedules.
- Calendar / DST visualization.
