# In-transit auto-disable — design

## Summary

Auto-disable openlid when the laptop appears to be in transit (lid
closed, on battery, no external display, no network for N minutes).
The intent is to prevent in-backpack overheating without
sacrificing the existing use cases (clamshell mode against an
external display, desktop-style "lid closed at home server").

Opt-in, default off. Matches the existing `battery_threshold_pct`
auto-disable pattern: once tripped, the toggle is OFF until the user
manually re-enables it -- no auto-reactivate on network return.

## Goals

- A user who closes the lid, puts the laptop into a backpack, and
  walks out of network range sees openlid disable itself within ~2
  minutes, allowing the system to sleep normally.
- A user in clamshell mode against an external display is NEVER
  caught by this rule, even if their network briefly drops.
- A user with a wired-Ethernet desktop setup (Wi-Fi off) is NEVER
  caught -- "no network" means "no reachable Internet on any
  interface," not "Wi-Fi disassociated."
- A user on AC power is NEVER caught -- this is the in-transit
  detector, and "on AC" is the strongest "at a desk" signal we have.

## Non-goals

- Active probing of the network. SCNetworkReachability observes
  interface state; we do not generate traffic.
- Geofencing, SSID allowlists, or any location-aware behavior.
- Auto-reactivate when the network returns -- the user must
  re-enable manually, same as the battery threshold path.
- Bluetooth peripheral disconnect detection.

## Predicate

```
state.enabled
  AND state.lid == Closed
  AND matches!(state.power, Battery {..})
  AND !display.has_external_display()
  AND duration_since_network_unreachable >= config.in_transit_timeout
  -> set enabled = false, clear timer, persist, log
```

Each guard exists for a specific failure mode:

| Guard | Eliminates |
|-------|------------|
| `enabled` | Don't fire when openlid is already off. |
| `lid == Closed` | Don't fire when the laptop is open in front of the user. |
| `Battery` | "On AC" is the strongest "at a desk" signal we have. |
| `!has_external_display` | Clamshell mode (laptop closed, monitor attached). |
| `duration >= N` | Wi-Fi blips of < N minutes do not trip. |

All five must hold simultaneously for the auto-disable to fire.

## Components

### 1. NetworkMonitor trait (openlid-core)

Modeled after the existing `LidObserver` and `PowerSourceMonitor`
traits in `crates/core/src/platform.rs`:

```rust
pub trait NetworkMonitor: Send + Sync {
    fn is_reachable(&self) -> bool;
    fn subscribe(&self, callback: NetworkStateCallback);
}

pub type NetworkStateCallback = Arc<dyn Fn(bool) + Send + Sync>;
```

`is_reachable() == true` means "at least one interface can reach the
Internet"; `false` means "no path out." Implementation choice (Wi-Fi
only vs all interfaces vs ping target) is hidden behind the trait.

### 2. macOS implementation: `MacNetworkMonitor`

Uses `SCNetworkReachability` from `core-foundation` and
`system-configuration` (need to add the `system-configuration` crate
or call FFI directly). Target: `apple.com` -- a stable host that
isn't ours, so users don't see openlid making outbound DNS lookups
for our own infrastructure.

`SCNetworkReachabilityScheduleWithRunLoop` registers the monitor on
the main thread's run loop and delivers callbacks when reachability
flags change.

Implementation file: `crates/app/src/platform/macos/network_monitor.rs`,
parallel to the existing `lid_monitor.rs` and `power_source.rs`.

Manual-checklist coverage only -- the FFI layer can't be unit-tested
in CI.

### 3. Config field

Additive: `in_transit_timeout_minutes: Option<u32>` on `Config`.
* `None` = feature disabled (default; preserves current behavior).
* `Some(n)` = enabled, threshold of `n` minutes.

Sensible UI defaults: 2 minutes (your colleague's suggestion).
Allowed range in the UI: 1-30. The config schema is permissive
(any u32) so config-editing power users can pick whatever they want.

No SCHEMA_VERSION bump -- additive optional fields don't require it
per the policy comment in `config.rs`.

### 4. AppState fields (runtime-only)

```rust
pub network_reachable: bool,                   // default: true (optimistic)
pub network_unreachable_since: Option<Instant>, // None when reachable or never observed
```

Both `#[serde(skip)]` -- transient, not persisted. Matches the
pattern for `lid`, `power`, `until`.

### 5. Pure decision function

In `crates/core/src/state.rs`:

```rust
pub fn should_auto_disable_in_transit(
    state: &AppState,
    has_external_display: bool,
    timeout: std::time::Duration,
    now: std::time::Instant,
) -> bool {
    if !state.enabled { return false; }
    if state.lid != LidState::Closed { return false; }
    if !matches!(state.power, PowerSource::Battery { .. }) { return false; }
    if has_external_display { return false; }
    let Some(since) = state.network_unreachable_since else { return false; }
    now.duration_since(since) >= timeout
}
```

Exhaustively tested: all five guards individually (negative cases),
combination true case, the exact-boundary duration check.

### 6. StateRuntime integration

Extend `StateRuntime` with the network monitor and a generation-
counted timer (same pattern as the existing expiry timer in
`arm_timer`).

* New private field: `network_monitor: Arc<N>` where `N: NetworkMonitor`.
* Generic parameter: `StateRuntime<P, L, S, D, N>`.
* On construction, subscribe to network callbacks; on each callback,
  call `on_network_change(reachable)`.
* `on_network_change(reachable: bool)`:
  - Update `state.network_reachable`.
  - If `reachable == false` and the timer isn't armed yet:
    set `network_unreachable_since = Some(now)`, arm a one-shot
    thread that fires after `timeout`.
  - If `reachable == true`: clear `network_unreachable_since`,
    bump the generation counter so any in-flight thread becomes a
    no-op.
  - Call `reconcile()` (which doesn't change anything by itself,
    but matches the existing event-handler pattern).
* When the timer fires (and its generation is still current):
  call the pure decision function; if it returns true, set
  `enabled = false`, persist, reconcile, notify listeners.

### 7. IPC + Snapshot

`SetPreferences` gains:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
in_transit_timeout_minutes: Option<Option<u32>>,
```

Same `Option<Option<u32>>` patch-state pattern as
`default_duration_minutes` (none = leave alone; Some(None) = clear;
Some(Some(n)) = set).

`PrefsPatch` gains the same field.

`Snapshot` gains:
```rust
in_transit_timeout_minutes: Option<u32>,
```

So `openlid status --json` and the Preferences UI can read it.

### 8. Preferences UI

New row, placed below the battery threshold (same visual style):

```
[x] Auto-disable in transit (lid closed, on battery, no external
    display, no network for [2 ▼] minutes)
```

* Master checkbox: toggles `in_transit_timeout_minutes` between
  `None` (unchecked) and `Some(2)` (default when first checked).
* Numeric field: minutes; clamped to 1-30; sets `Some(n)` directly.

Implementation parallels the existing battery-threshold controls.

### 9. Status output

`openlid status` adds one line when enabled:

```
In-transit auto-off: 2 min (lid closed, on battery, no display, no network)
```

JSON output already exposes the field via `Snapshot`.

### 10. README + manual checklist

README "Privacy" section gets an addendum:

> SCNetworkReachability is passive -- it observes interface state
> without generating outbound traffic.

Privacy section's "Open-Lid only contacts GitHub when..." line stays
literally true: SCNetworkReachability does not phone home.

Manual checklist gets new test cases:
- Toggle the preference on, verify default is 2 min.
- Close lid on battery without external display, disable Wi-Fi /
  pull Ethernet -> verify auto-disable fires after ~N minutes.
- Same setup but on AC -> verify auto-disable does NOT fire.
- Same setup but with external display -> verify auto-disable does
  NOT fire.
- Network blip < N min -> verify auto-disable does NOT fire.
- After auto-disable, restore network -> verify toggle stays OFF
  (no auto-reactivate).

## Files touched (estimate)

NEW:
- `crates/app/src/platform/macos/network_monitor.rs` -- FFI wrapper
- `docs/superpowers/specs/2026-05-22-in-transit-auto-disable-design.md`

MODIFIED:
- `Cargo.toml` (workspace) -- add `system-configuration` if needed
- `crates/core/src/platform.rs` -- new trait
- `crates/core/src/config.rs` -- new field
- `crates/core/src/state.rs` -- new pure decision function + state fields
- `crates/core/src/ipc/control.rs` -- SetPreferences + Snapshot extensions
- `crates/app/src/state_runtime.rs` -- new generic param, monitor subscription,
  timer arming, auto-disable path
- `crates/app/src/control_server.rs` -- forward new patch field
- `crates/app/src/menubar/mod.rs` -- inject the monitor into the runtime;
  new RuntimeActions setter
- `crates/app/src/menubar/preferences.rs` -- new UI controls
- `crates/app/src/cli/commands.rs` -- status-line addition
- `crates/app/src/platform/macos/mod.rs` -- declare the new module
- `README.md`
- `docs/manual-test-checklist.md`

Net estimate: ~600-700 lines including tests, mostly in
`state_runtime.rs` (timer arming + new tests) and `preferences.rs`
(new UI row + handler).

## Risks

- **`SCNetworkReachability` callback threading.** Callbacks fire on
  the runloop the API was scheduled with. We'll schedule on the main
  runloop and hop to whichever thread the existing event handlers
  use. Same model as `LidObserver`.
- **Generic-parameter creep.** `StateRuntime<P, L, S, D>` gains an
  `N`. All call sites get a fifth generic. The existing pattern uses
  monomorphization with concrete types in `menubar::run`, so the
  cost is type-signature noise, not runtime.
- **DNS for `apple.com`.** First lookup may emit a DNS query if the
  resolver hasn't seen the name in a while. This is sub-1 KB once
  per system boot at most. Documented in the README addendum.
- **Asymmetric reactivate semantics.** The auto-disable doesn't
  re-enable when the network returns -- the user has to do that
  manually. Matches the battery threshold's design (the user's
  context comment at `state_runtime.rs:278` calls this out
  explicitly: "we do NOT auto-reactivate when battery recovers --
  the user must manually toggle back on"). Same rationale: the auto-
  disable was a safety action, not a temporary suspension.

## Open questions resolved during brainstorming

- **Default timeout?** 2 minutes, per the colleague's suggestion.
- **Default opt-in or opt-out?** Opt-in (off by default). Avoids
  surprising existing users on upgrade.
- **Network probe target?** `apple.com`. Stable, not ours, common
  enough that the lookup is effectively free.
- **Apply on AC?** No. The on-battery guard is part of the predicate.
- **Auto-reactivate?** No. Manual re-enable, matching the battery
  threshold's design.
