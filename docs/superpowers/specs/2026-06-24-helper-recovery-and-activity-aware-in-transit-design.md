# Helper auto-recovery + activity-aware in-transit — design

## Summary

Two fixes to the same underlying failure: openlid silently not keeping
a headless machine awake, which kills locally-running agents the moment
the lid closes.

1. **Helper auto-recovery.** openlid's real sleep-prevention mechanism
   (`pmset -a disablesleep 1`, run by the root helper over XPC) can sit
   broken for days when the helper's `SMAppService` daemon is in the
   `RequiresApproval` state — because today the app logs that to a file
   no one reads, never surfaces it, and never reconnects even after the
   user approves. We make the failure user-visible (banner notification
   that deep-links to the approval toggle) and make the XPC client
   recover without an app relaunch.

2. **Activity-aware in-transit.** The in-transit auto-disable
   (`2026-05-22-in-transit-auto-disable-design.md`) sleeps the Mac after
   N minutes of "no network" while lid-closed on battery. A headless
   agent running with the lid shut is exactly that shape — and a Wi-Fi
   blip on lid-close can seed the countdown. We add an activity guard:
   if the machine is actually doing work, it is not "forgotten in a
   backpack," so the detector defers and rechecks instead of
   auto-disabling immediately.

Both are surgical extensions of existing patterns; neither changes the
default-off posture of in-transit or the helper's privilege model.

## Goals

- A user whose helper is stuck in `RequiresApproval` gets a banner
  ("OpenLid isn't keeping your Mac awake — approve in System Settings")
  that, when tapped, opens the Login Items pane. No more silent 4-day
  outages.
- After the user approves through the surfaced Settings action, the
  **already-running** app reconnects and begins actually preventing
  sleep — without requiring a relaunch.
- Recovery rides the existing reconcile path (lid/power/network
  events) and adds only a bounded approval follow-up after opening
  System Settings. There is no always-on helper polling thread.
- A headless agent running on battery with the lid closed and no
  network is not auto-disabled by the in-transit detector while it is
  actively working.
- An idle laptop genuinely forgotten in a backpack still auto-disables:
  activity defers the decision and rechecks, it does not permanently
  cancel the in-transit detector.

## Non-goals

- The broad "fail loud" status surface. The snapshot's hardcoded
  `HelperStatus::Running` and the optimistic `preventing_sleep_now`
  (computed purely from the toggle, ignoring helper health) are left
  **as-is**. Flagged as a known gap; out of scope by explicit choice.
- An `openlid doctor` / reinstall-helper CLI command.
- Bypassing the macOS approval requirement. A `RequiresApproval` daemon
  cannot be enabled by code — a manual user toggle is mandatory. We can
  only surface and deep-link to it.
- Changing the in-transit feature's default (stays opt-in, off) or its
  no-auto-reactivate semantics.
- Motion/orientation/thermal sensing to distinguish "busy in a bag"
  from "busy on a desk." We bias toward keeping the agent alive.
- Process allowlists. The activity signal is system-level, not
  "is Claude/Codex running?"

---

# Part A — Helper auto-recovery

## Problem

`HelperPowerController.prevent_sleep()` → `HelperClient.set_sleep_prevention(true)`
returns `PlatformError::HelperUnavailable` whenever the Mach service
`io.openlid.helper` doesn't resolve (daemon not `Enabled`). Two defects
compound it:

1. **Sticky invalidation.** `HelperClient.invalidated` latches `true` on
   the first failure and is never cleared; there is no reconnect path.
   So once the helper is unreachable at launch, the app stays broken for
   its whole lifetime — even after the user approves the daemon.
2. **Silent approval state.** `try_register_helper()` (menubar startup)
   logs `RequiresApproval`/`NotFound` to `app.log` and returns. The user
   gets no signal.

## A1 · Recoverable connection

`HelperPowerController` changes from `client: Arc<HelperClient>` to
`client: Mutex<Arc<HelperClient>>`. It also receives a shared
`Arc<HelperRecoverySurface>` created by `menubar::run` and passed both
to `try_register_helper(&surface)` and
`HelperPowerController::new(client.clone(), surface.clone())`.
That keeps startup surfacing and runtime recovery behind the same
rate-limit state instead of duplicating notification logic.

A `reconnect()` rebuilds the client
(`HelperClient::new()` makes a fresh `NSXPCConnection` with a clean
`invalidated` flag) and swaps the Arc in.

`HelperClient` itself is unchanged except that nothing else holds it
directly — the controller owns the swappable handle.

## A2 · Self-recovering power calls

`prevent_sleep`/`allow_sleep` route through one helper:

```rust
fn attempt(&self, enabled: bool) -> Result<(), PlatformError> {
    let client = self.client.lock().unwrap().clone();
    match client.set_sleep_prevention(enabled) {
        Ok(()) => Ok(()),
        Err(PlatformError::HelperUnavailable) => {
            // Surfacing only on the prevent path: never nag while
            // turning OFF (disablesleep is already 0 if the helper
            // never set it).
            self.recover(/* surface = */ enabled);
            // Retry once on the possibly-rebuilt client.
            let client = self.client.lock().unwrap().clone();
            client.set_sleep_prevention(enabled)
        }
        Err(e) => Err(e),
    }
}
```

`recover()` reads `helper_installer::status()` and dispatches via a pure
decision function:

```rust
enum RecoveryAction { Register, NotifyApproval, NotifyNotFound, Reconnect, Nothing }

fn recovery_action(status: HelperServiceStatus) -> RecoveryAction {
    match status {
        NotRegistered | Unknown(_) => Register,
        RequiresApproval           => NotifyApproval,
        NotFound                   => NotifyNotFound,
        Enabled                    => Reconnect, // client was stale-invalidated
    }
}
```

- `Register` → `helper_installer::register()` (transitions to
  `RequiresApproval`; the next event surfaces it).
- `NotifyApproval` → ask the shared recovery surface to post the banner
  (A3), rate-limited.
- `NotifyNotFound` → post a "move OpenLid to /Applications" banner,
  rate-limited (no Settings deep-link — it wouldn't help).
- `Reconnect` → `self.reconnect()`; the retry in `attempt` then
  succeeds. A successful reconnect marks the shared recovery surface
  healthy so a later unhealthy episode can surface again.

`recovery_action` is pure (status → action). The impure `recover(surface)`
wrapper executes it, but when `surface == false` (the `allow_sleep`
path) the `NotifyApproval`/`NotifyNotFound` actions degrade to
`Nothing` — `Register` and `Reconnect` still run, but we never post a
banner while *turning sleep prevention off*.

**Why this is event-driven and self-retrying:** `reconcile()` only
advances `last_applied` on a *successful* power call, so every reconcile
(fired by lid/power/network events) re-invokes `attempt`, which re-runs
recovery until it reconnects.

**Why the approval follow-up exists:** changing the Login Items toggle
does not emit a signal that `StateRuntime` currently observes. Relying
only on lid/power/network events would make "approved while already
running" opportunistic, not deterministic. The notification action and
the notification-denied fallback both call
`open_system_settings_login_items()` and arm a bounded follow-up:
5s/15s/30s/60s/120s rechecks that call `runtime.request_reconcile()`
once the runtime installs that callback on the shared recovery surface.
If the helper becomes `Enabled`, the next attempt reconnects. If not,
the follow-up stops and normal event-driven recovery remains.

## A3 · Banner notification

New module `crates/app/src/notify.rs` wrapping `UNUserNotificationCenter`
(via `objc2-user-notifications = "0.3.2"`, which exposes
`UNUserNotificationCenter`, notification actions/categories, and the
delegate protocol):

- `request_authorization_if_needed()` — called lazily by the recovery
  surface before the first banner, not unconditionally for healthy
  installs. Async; completion logged.
- A delegate (`objc2::define_class!`) implementing
  `userNotificationCenter:didReceiveNotificationResponse:` and
  `willPresentNotification:` (so the banner shows even though OpenLid is
  an Accessory/menubar app). The response handler calls
  `helper_installer::open_system_settings_login_items()` (currently
  `#[allow(dead_code)]` — this un-deadens it) and starts the bounded
  approval follow-up.
- Register a notification category before posting:
  `UNNotificationAction` with identifier `open_settings`, a
  `UNNotificationCategory`, `setNotificationCategories`, and
  `content.categoryIdentifier = helper_recovery`. Without the category,
  the custom action will not appear reliably.
- `notify_helper_needs_approval()` builds a `UNMutableNotificationContent`
  with an "Open System Settings" `UNNotificationAction` and adds the
  request.

**Rate limiting** lives in `HelperRecoverySurface`: separate
`Mutex<Option<Instant>>` timestamps for approval and not-found banners,
plus a cooldown constant. Pure `should_surface(now, last, cooldown) ->
bool` is unit-tested. One banner per unhealthy episode; reset to allow a
fresh banner once the helper has been healthy again.

**Startup determinism:** `try_register_helper()` is updated so its
`RequiresApproval`/`NotFound` arms call the same rate-limited notify
path, so the user is told at launch rather than waiting for the first
reconcile. This requires changing its signature to
`try_register_helper(surface: &HelperRecoverySurface)`.

**Authorization denied fallback:** if the user has denied notifications,
the recovery surface logs a warning and performs a single best-effort
`open_system_settings_login_items()` per episode, then starts the same
bounded approval follow-up. (We accept that a user who denies
notifications, ignores the Settings deep-link, and never causes another
runtime event can remain broken — that is outside the chosen scope.)

## A4 · Testable seams

Pure, unit-tested in `helper_client.rs` (or a small `recovery` module):

- `recovery_action(status) -> RecoveryAction` — one case per status.
- `should_surface(now, last, cooldown) -> bool` — cooldown gate
  (none-yet fires; within-cooldown suppresses; past-cooldown fires).
- `approval_recheck_delays() -> &'static [Duration]` — pins the bounded
  follow-up schedule and prevents accidental always-on polling.

The objc glue (`reconnect`, the `notify` module, `register`/`status`)
stays manual-checklist only, matching the existing FFI testing posture
(`helper_installer::from_raw` is unit-tested; its `msg_send` calls are
not).

---

# Part B — Activity-aware in-transit

## Updated decision

The in-transit path needs to distinguish "do it now" from "all safety
guards match, but the machine is busy; check again soon." A plain bool
would hide that reason and would make it easy to skip once at the
timeout and never re-arm.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InTransitAutoDisableDecision {
    Fire,
    DeferBusy,
    Skip,
}

pub fn in_transit_auto_disable_decision(
    state: &AppState,
    has_external_display: bool,
    system_busy: bool,            // NEW
    timeout: Duration,
    now: Instant,
) -> InTransitAutoDisableDecision {
    if !state.enabled { return InTransitAutoDisableDecision::Skip; }
    if state.lid != LidState::Closed { return InTransitAutoDisableDecision::Skip; }
    if !matches!(state.power, PowerSource::Battery { .. }) {
        return InTransitAutoDisableDecision::Skip;
    }
    if has_external_display { return InTransitAutoDisableDecision::Skip; }
    let Some(since) = state.network_unreachable_since else {
        return InTransitAutoDisableDecision::Skip;
    };
    if now.duration_since(since) < timeout {
        return InTransitAutoDisableDecision::Skip;
    }
    if system_busy {
        return InTransitAutoDisableDecision::DeferBusy;
    }
    InTransitAutoDisableDecision::Fire
}

pub fn should_auto_disable_in_transit(
    state: &AppState,
    has_external_display: bool,
    system_busy: bool,
    timeout: Duration,
    now: Instant,
) -> bool {
    matches!(
        in_transit_auto_disable_decision(
            state,
            has_external_display,
            system_busy,
            timeout,
            now,
        ),
        InTransitAutoDisableDecision::Fire,
    )
}
```

The guard table (extending the original spec):

| Guard | Eliminates |
|-------|------------|
| `enabled` | Don't fire when openlid is already off. |
| `lid == Closed` | Don't fire when the laptop is open in front of the user. |
| `Battery` | "On AC" is the strongest "at a desk" signal we have. |
| `!has_external_display` | Clamshell mode (laptop closed, monitor attached). |
| `duration >= N` | Wi-Fi blips of < N minutes do not trip. |
| **`!system_busy`** | **A process actively using the CPU (a headless agent) is working, not abandoned in a backpack. Busy returns `DeferBusy`, not `Skip`, so the runtime rechecks.** |

## Activity probe

New `crates/app/src/platform/macos/activity.rs`:

```rust
/// True if the 1-minute load average per online CPU exceeds the
/// idle threshold. Biased low: the cost of a false "busy" is a cool
/// idle Mac that doesn't sleep; the cost of a false "idle" is a
/// killed agent — the bug we're fixing.
pub fn system_busy() -> bool { /* libc::getloadavg / _SC_NPROCESSORS_ONLN */ }

const BUSY_LOAD_PER_CPU: f64 = 0.05; // tunable
```

`getloadavg` is a single non-sampling read; no `host_processor_info`
two-sample dance. The threshold is intentionally below "one full core
divided by all cores" on common Apple Silicon machines, so one sustained
agent process counts as busy. The constant is named and tunable.

## Injection (no generic creep)

Following the network-monitor pattern (wired externally in
`menubar::run`, not a `StateRuntime` generic), `StateRuntime` gains a
plain field, not a type parameter:

```rust
busy_probe: Arc<dyn Fn() -> bool + Send + Sync>,
```

- `StateRuntime::new()` defaults it to `Arc::new(|| false)` (idle) — so
  **every existing test keeps its current behavior unchanged** and the
  default is the conservative "never busy → behaves like today."
- `menubar::run` calls
  `runtime.set_busy_probe(Arc::new(|| macos::activity::system_busy()))`
  after construction, exactly where it wires the network monitor.
- `maybe_fire_in_transit_auto_disable` reads `(self.busy_probe)()` and
  passes the bool into `in_transit_auto_disable_decision` alongside the
  existing `self.display.has_external_display()` read.
- On `Fire`, behavior stays as today: set `enabled = false`, clear
  `until`, persist, reconcile.
- On `DeferBusy`, do not mutate state and do not clear
  `network_unreachable_since`; arm another generation-checked sleeper
  for `IN_TRANSIT_BUSY_RECHECK_INTERVAL` (60s). If the network comes
  back meanwhile, the existing generation bump cancels the recheck. If
  the machine later goes idle while still lid-closed/on-battery/no
  display/no network, the next recheck fires the auto-disable.
- On `Skip`, do nothing.

## Tests

Pure predicate (`state.rs`):
- `in_transit_defers_when_system_busy_at_timeout` — all other guards
  pass and duration is met, busy=true → `DeferBusy`.
- `in_transit_skips_when_system_busy_before_timeout` — busy does not
  matter until the configured duration has elapsed.
- existing `in_transit_fires_when_all_guards_pass` → pass `system_busy=false`.
- keep the exact-boundary test (busy=false).

Runtime (`state_runtime.rs`):
- new: inject `set_busy_probe(|| true)`, arrange the in-transit shape,
  call `maybe_fire_in_transit_auto_disable`, assert `enabled` stays true
  and the busy recheck generation advanced.
- existing `maybe_fire_in_transit_*` tests are unaffected (default
  probe is `|| false`).

## Files touched (estimate)

NEW:
- `crates/app/src/notify.rs` — `UNUserNotificationCenter` banner + delegate
- `crates/app/src/platform/macos/activity.rs` — `getloadavg` busy probe
- `docs/manual-test-checklist.md` — restore/create the manual checklist
  file referenced by prior specs and add the cases below
- `docs/superpowers/specs/2026-06-24-helper-recovery-and-activity-aware-in-transit-design.md`

MODIFIED:
- `Cargo.toml` (workspace + app) — add `objc2-user-notifications`
- `crates/app/src/helper_client.rs` — `Mutex<Arc<HelperClient>>`,
  self-recovery in `attempt`, `reconnect`, shared recovery surface, pure
  `recovery_action` + `should_surface` + follow-up schedule tests
- `crates/app/src/helper_installer.rs` — un-dead-code
  `open_system_settings_login_items`; expose any shared status helper
- `crates/app/src/menubar/mod.rs` — install the notification delegate at
  startup, keep authorization lazy, have `try_register_helper` surface
  approval/not-found via the shared recovery surface, install the runtime
  reconcile callback, and wire `set_busy_probe`
- `crates/app/src/main.rs` — declare `notify` module
- `crates/app/src/platform/macos/mod.rs` — declare `activity` module
- `crates/core/src/state.rs` — in-transit decision enum + `system_busy`
  input + tests
- `crates/app/src/state_runtime.rs` — `busy_probe` field + `set_busy_probe`
  + thread it into the decision; `request_reconcile`; busy recheck; new
  busy tests

Net estimate: ~500–700 lines including tests, concentrated in
`helper_client.rs` (recovery + tests) and `notify.rs` (objc glue).

## Manual checklist additions

Helper recovery:
- With the helper in `RequiresApproval`, launch OpenLid → banner appears;
  tapping it opens Login Items.
- Approve the toggle while the app is running → within one lid/power
  event or bounded approval follow-up,
  `pmset -g | grep SleepDisabled` flips to `1` (no relaunch).
- Close the lid on battery → machine stays awake, network survives.

In-transit activity guard:
- Lid closed, battery, no display, no network, **idle** → auto-disables
  after N min (unchanged).
- Same setup but with a CPU-busy workload running → does NOT auto-disable.
- Keep the same setup, stop the busy workload while still offline →
  auto-disables on the next busy recheck.

## Risks

- **Notification authorization denied.** Banner can't show; we fall back
  to a single best-effort Settings deep-link per episode, bounded
  approval follow-up, and a log line. Re-introduces silent failure only
  for users who deny notifications and ignore the Settings deep-link.
- **Approval follow-up is bounded.** If the user opens Settings and
  approves much later than the follow-up window, recovery waits for the
  next reconcile event or relaunch. This is the trade-off for avoiding
  an always-on helper poller.
- **`objc2-user-notifications` selectors.** Per the project's objc2
  selector gotchas, prefer the typed crate bindings over raw `msg_send`
  for the notification center/delegate to avoid `NS_SWIFT_NAME`
  mismatches.
- **Load-average threshold.** Coarse signal; biased low to protect
  agents. A genuinely busy laptop forgotten in a bag would not
  auto-disable until it becomes idle — accepted trade-off given the
  agent-first priority.
- **Reconnect cost.** Rebuilding `NSXPCConnection` happens only on the
  failure path; negligible.

## Open questions resolved during brainstorming

- **Approval surface mechanism?** Banner notification (not NSAlert),
  with an "Open System Settings" action.
- **Recovery cadence?** Event-driven via the reconcile path, plus a
  bounded approval follow-up after opening System Settings; no always-on
  polling thread.
- **In-transit fix approach?** Activity-aware — defer auto-disable while
  the machine is doing real work, then recheck.
- **Activity signal?** 1-minute load average per CPU via `getloadavg`,
  biased low.
- **Broad fail-loud UI / `openlid doctor`?** Out of scope by choice.
