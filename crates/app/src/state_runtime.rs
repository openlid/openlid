//! Owns the live AppState. Subscribes to lid, power, and timer events;
//! re-evaluates `should_prevent_sleep` after each event; calls into the
//! PowerController to reconcile the helper with the desired state.

use anyhow::Result;
use chrono::{DateTime, Local};
use openlid_core::config::Config;
use openlid_core::ipc::control::Snapshot;
use openlid_core::mode::{LidState, PowerSource, Schedule};
use openlid_core::platform::{DisplayController, LidObserver, PowerController, PowerSourceMonitor};
use openlid_core::state::{should_prevent_sleep, AppState};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

/// Patch struct for updating preferences atomically. Each `Some(_)` replaces
/// the corresponding field; `None` leaves it untouched.
///
/// `schedule` uses an extra `Option` layer: outer `None` means "leave alone",
/// `Some(None)` means "clear", `Some(Some(s))` means "set to s". The
/// `set_preferences` apply path mirrors the value into
/// `state.modifiers.schedule` so `should_prevent_sleep` sees the gate without
/// an extra reload.
#[derive(Debug, Default, Clone)]
pub struct PrefsPatch {
    pub start_at_login: Option<bool>,
    pub activate_at_launch: Option<bool>,
    pub battery_threshold_pct: Option<Option<u8>>,
    pub prevent_display_sleep: Option<bool>,
    pub schedule: Option<Option<Schedule>>,
    /// In-transit auto-disable threshold (minutes). Three-state:
    /// outer `None` = leave alone, `Some(None)` = clear (feature off),
    /// `Some(Some(n))` = set to n.
    pub in_transit_timeout_minutes: Option<Option<u32>>,
}

/// Notification fired whenever any state-affecting field changes. Listeners
/// run on whichever thread triggered the change (menu click = main thread,
/// CLI = control-server worker thread, lid/power events = main thread via
/// IOKit run-loop sources). UI listeners MUST hop to main themselves before
/// touching AppKit.
pub type StateListener = Arc<dyn Fn(&Snapshot) + Send + Sync>;

pub struct StateRuntime<P, L, S, D>
where
    P: PowerController + 'static,
    L: LidObserver + 'static,
    S: PowerSourceMonitor + 'static,
    D: DisplayController + 'static,
{
    pub state: Arc<Mutex<AppState>>,
    last_applied: Arc<Mutex<bool>>,
    /// Tracks whether the display-sleep assertion is currently held, so
    /// reconcile() can skip a redundant FFI call when nothing changed. The
    /// underlying `prevent_display_sleep`/`allow_display_sleep` methods are
    /// already idempotent — this cache is purely a noise-reduction layer
    /// that keeps log lines down and matches the `last_applied` pattern
    /// used for system-sleep reconciliation.
    last_assertion_held: Arc<Mutex<bool>>,
    config: Mutex<Config>,
    power: Arc<P>,
    display: Arc<D>,
    _lid: Arc<L>,
    _power_source: Arc<S>,
    config_path: PathBuf,
    listeners: Mutex<Vec<StateListener>>,
    /// Monotonic counter bumped every time the timer is re-armed. A scheduled
    /// expiry thread captures the generation it was spawned for and refuses
    /// to fire if a newer generation has been written. See `arm_timer`.
    timer_generation: AtomicU64,
    /// Generation counter for the in-transit auto-disable timer. Bumped
    /// every time the network reachability flips (in either direction) so
    /// an in-flight sleeper from a previous reachability window becomes a
    /// no-op. Same pattern as `timer_generation` but for a separate
    /// timer that the user-driven enable/disable path doesn't touch.
    in_transit_generation: AtomicU64,
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

        // Decision 1: "Restore last state" by default. If `activate_at_launch`
        // is set, override with `enabled = true` regardless of persisted state.
        let enabled = if cfg.activate_at_launch {
            true
        } else {
            cfg.enabled
        };

        let state = AppState {
            enabled,
            modifiers: cfg.modifiers.clone(),
            until: None, // timers are transient — never persisted
            lid: lid.current(),
            power: power_source.current(),
            network_reachable: true, // optimistic; the monitor publishes the real value at startup
            network_unreachable_since: None,
        };

        let rt = Arc::new(Self {
            state: Arc::new(Mutex::new(state)),
            last_applied: Arc::new(Mutex::new(false)),
            last_assertion_held: Arc::new(Mutex::new(false)),
            config: Mutex::new(cfg),
            power,
            display,
            _lid: lid.clone(),
            _power_source: power_source.clone(),
            config_path,
            listeners: Mutex::new(Vec::new()),
            timer_generation: AtomicU64::new(0),
            in_transit_generation: AtomicU64::new(0),
        });

        let rt_for_lid = Arc::clone(&rt);
        lid.subscribe(Arc::new(move |new_lid| {
            rt_for_lid.on_lid_change(new_lid);
        }));
        let rt_for_ps = Arc::clone(&rt);
        power_source.subscribe(Arc::new(move |new_ps| {
            rt_for_ps.on_power_change(new_ps);
        }));

        rt.reconcile();
        Ok(rt)
    }

    /// Subscribe to state changes. UI listeners must dispatch back to the
    /// main thread before touching AppKit — see `crate::main_thread::run_on_main`.
    pub fn add_listener(&self, listener: StateListener) {
        self.listeners.lock().unwrap().push(listener);
    }

    fn notify_listeners(&self) {
        let snap = self.snapshot();
        let listeners: Vec<StateListener> = self.listeners.lock().unwrap().clone();
        for l in &listeners {
            l(&snap);
        }
    }

    /// Set the toggle plus an optional auto-expiry instant.
    /// `until = None` means indefinite (no timer); `Some(t)` deactivates at `t`.
    /// Disabling clears any pending timer.
    pub fn set_enabled(
        self: &Arc<Self>,
        enabled: bool,
        until: Option<DateTime<Local>>,
    ) -> Result<()> {
        let effective_until = if enabled { until } else { None };
        {
            let mut s = self.state.lock().unwrap();
            s.enabled = enabled;
            s.until = effective_until;
        }
        // Re-arm the expiry scheduler. Any previously-scheduled thread becomes
        // a no-op once the generation counter ticks past its captured value.
        self.arm_timer(effective_until);
        self.persist_and_reconcile()
    }

    /// Spawn a one-shot thread that wakes at `until` and forces a reconcile.
    /// Cheap: when no timer is set, this just bumps the generation counter
    /// (which invalidates any in-flight sleeper).
    fn arm_timer(self: &Arc<Self>, until: Option<DateTime<Local>>) {
        let gen = self.timer_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let Some(until) = until else {
            return;
        };
        let sleep_for = (until - Local::now()).to_std().unwrap_or(StdDuration::ZERO);
        let rt = Arc::clone(self);
        std::thread::spawn(move || {
            std::thread::sleep(sleep_for);
            // Stale-check: if a newer arm() has been called, do nothing.
            if rt.timer_generation.load(Ordering::SeqCst) != gen {
                return;
            }
            tracing::info!("timer expired (gen {gen}); forcing reconcile");
            // The reconcile path already clears `enabled` and `until` when it
            // detects the expiry, then calls allow_sleep on the power
            // controller. We just need to trigger it.
            rt.reconcile();
            rt.notify_listeners();
        });
    }

    /// Network reachability flipped. Update state, arm or cancel the
    /// in-transit timer accordingly, then notify listeners so any UI
    /// (snapshot-driven indicators) refreshes.
    ///
    /// Called by the `MacNetworkMonitor` subscription wired up at
    /// menubar startup. Tests drive this method directly.
    pub fn on_network_change(self: &Arc<Self>, reachable: bool) {
        let now = std::time::Instant::now();
        let arm = {
            let mut s = self.state.lock().unwrap();
            s.network_reachable = reachable;
            if reachable {
                s.network_unreachable_since = None;
                // Bump the generation so any in-flight sleeper from a
                // previous unreachable window becomes a no-op.
                self.in_transit_generation.fetch_add(1, Ordering::SeqCst);
                false
            } else if s.network_unreachable_since.is_none() {
                s.network_unreachable_since = Some(now);
                true
            } else {
                // Already in an unreachable window; an earlier call armed
                // the timer. Don't double-arm.
                false
            }
        };
        if arm {
            self.arm_in_transit_timer();
        }
        self.notify_listeners();
    }

    /// Spawn a one-shot thread that wakes after the configured
    /// in-transit timeout. If, at fire time, all five guards still
    /// hold, set `enabled = false` and persist.
    ///
    /// Same generation-counter pattern as `arm_timer`: a reachability
    /// flip back to `true` bumps the generation and the in-flight
    /// thread becomes a no-op.
    fn arm_in_transit_timer(self: &Arc<Self>) {
        let timeout_minutes = match self.config.lock().unwrap().in_transit_timeout_minutes {
            Some(n) if n > 0 => n,
            _ => return, // feature disabled or zero -- nothing to arm
        };
        let timeout = StdDuration::from_secs(u64::from(timeout_minutes) * 60);
        let gen = self.in_transit_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let rt = Arc::clone(self);
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            if rt.in_transit_generation.load(Ordering::SeqCst) != gen {
                // Reachability flipped back during the wait window;
                // a newer arm() superseded us (or cancelled).
                return;
            }
            rt.maybe_fire_in_transit_auto_disable(timeout);
        });
    }

    /// Read live state + the configured timeout and decide whether to
    /// fire the in-transit auto-disable. Pure-decision-function call
    /// happens here; on a `true` return we persist and reconcile.
    fn maybe_fire_in_transit_auto_disable(self: &Arc<Self>, timeout: StdDuration) {
        let should_fire = {
            let s = self.state.lock().unwrap();
            openlid_core::state::should_auto_disable_in_transit(
                &s,
                self.display.has_external_display(),
                timeout,
                std::time::Instant::now(),
            )
        };
        if !should_fire {
            return;
        }
        tracing::info!(
            "in-transit auto-disable: lid closed, on battery, no external display, \
             no network for {}s; turning off",
            timeout.as_secs(),
        );
        {
            let mut s = self.state.lock().unwrap();
            s.enabled = false;
            s.until = None;
        }
        // Best-effort persist; ignore errors so a transient disk issue
        // doesn't prevent the safety deactivation from happening.
        let _ = self.persist_and_reconcile_inner();
    }

    /// Apply a preferences patch — atomic per-field update.
    pub fn set_preferences(self: &Arc<Self>, patch: PrefsPatch) -> Result<()> {
        // Capture the side-effect we may need to perform AFTER releasing the
        // config lock. We don't want to hold the lock across SMAppService FFI
        // calls (they can block briefly waiting for the launchd database).
        let mut start_at_login_change: Option<bool> = None;

        {
            let mut cfg = self.config.lock().unwrap();
            if let Some(v) = patch.start_at_login {
                if cfg.start_at_login != v {
                    start_at_login_change = Some(v);
                }
                cfg.start_at_login = v;
            }
            if let Some(v) = patch.activate_at_launch {
                cfg.activate_at_launch = v;
            }
            if let Some(v) = patch.battery_threshold_pct {
                cfg.battery_threshold_pct = v;
                // Propagate to the modifier the decision function reads.
                self.state.lock().unwrap().modifiers.min_battery = v;
            }
            if let Some(v) = patch.prevent_display_sleep {
                cfg.prevent_display_sleep = v;
            }
            if let Some(v) = patch.schedule {
                // Propagate to the live modifiers (read by should_prevent_sleep)
                // before persist_and_reconcile copies cfg.modifiers back out.
                self.state.lock().unwrap().modifiers.schedule = v.clone();
                cfg.modifiers.schedule = v;
            }
            if let Some(v) = patch.in_transit_timeout_minutes {
                cfg.in_transit_timeout_minutes = v;
                // If the user disables the detector while we're in an
                // unreachable window, clear the timestamp so a future
                // re-enable doesn't see stale data and fire prematurely.
                if v.is_none() {
                    self.state.lock().unwrap().network_unreachable_since = None;
                    // Invalidate any in-flight sleeper too.
                    self.in_transit_generation.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        // Persist the new config + reconcile the helper *first*, so even if
        // the SMAppService call below fails, the user's preference is saved.
        self.persist_and_reconcile()?;

        if let Some(enable) = start_at_login_change {
            let result = if enable {
                crate::launch_at_login::enable()
            } else {
                crate::launch_at_login::disable()
            };
            if let Err(e) = result {
                // Log but don't fail the whole set_preferences — the UI
                // already shows the preference as flipped, and the user can
                // re-try after fixing the underlying issue (e.g., move app
                // into /Applications). Returning an error here would roll
                // back the UI checkbox, which is a confusing UX.
                tracing::warn!("launch-at-login change failed: {e:#}");
            }
        }

        Ok(())
    }

    pub fn snapshot(&self) -> Snapshot {
        let s = self.state.lock().unwrap();
        let cfg = self.config.lock().unwrap();
        Snapshot {
            preventing_sleep_now: should_prevent_sleep(&s, Local::now()),
            enabled: s.enabled,
            until: s.until,
            modifiers: s.modifiers.clone(),
            lid: s.lid,
            power: s.power,
            helper: openlid_core::ipc::control::HelperStatus::Running,
            start_at_login: cfg.start_at_login,
            activate_at_launch: cfg.activate_at_launch,
            battery_threshold_pct: cfg.battery_threshold_pct,
            prevent_display_sleep: cfg.prevent_display_sleep,
            in_transit_timeout_minutes: cfg.in_transit_timeout_minutes,
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
        // Original Open-Lid value-prop: if we're keeping the system awake and
        // the user just closed the lid, force the display off to save battery
        // and reduce heat. Skipped if an external display is attached.
        if was_closing
            && self.snapshot().preventing_sleep_now
            && !self.display.has_external_display()
        {
            let _ = self.display.force_display_sleep();
        }
        self.notify_listeners();
    }

    fn on_power_change(self: &Arc<Self>, new_ps: PowerSource) {
        self.state.lock().unwrap().power = new_ps;
        // Battery threshold: if we drop below the configured threshold AND we
        // are currently enabled, auto-deactivate. Per Decision 2, we do NOT
        // auto-reactivate when battery recovers — the user must manually
        // toggle back on.
        let should_auto_deactivate = {
            let cfg = self.config.lock().unwrap();
            let s = self.state.lock().unwrap();
            matches!(
                (cfg.battery_threshold_pct, new_ps, s.enabled),
                (Some(threshold), PowerSource::Battery { percent }, true) if percent < threshold
            )
        };
        if should_auto_deactivate {
            tracing::info!("battery threshold reached; auto-deactivating sleep prevention");
            let mut s = self.state.lock().unwrap();
            s.enabled = false;
            s.until = None;
            drop(s);
            // Best-effort persist; ignore errors so a transient disk issue
            // doesn't prevent the safety deactivation from happening.
            let _ = self.persist_and_reconcile_inner();
        }
        self.reconcile();
        self.notify_listeners();
    }

    fn persist_and_reconcile(&self) -> Result<()> {
        self.persist_and_reconcile_inner()
    }

    fn persist_and_reconcile_inner(&self) -> Result<()> {
        let cfg_to_save = {
            let mut cfg = self.config.lock().unwrap();
            let s = self.state.lock().unwrap();
            // Sync state fields that should persist
            cfg.enabled = s.enabled;
            cfg.modifiers = s.modifiers.clone();
            cfg.clone()
        };
        cfg_to_save.save(&self.config_path)?;
        self.reconcile();
        self.notify_listeners();
        Ok(())
    }

    /// Release runtime side-effects (helper sleep prevention + display
    /// IOPMAssertion) without mutating `AppState` or persisting to disk.
    /// Used by the quit handler so quitting the app does NOT silently
    /// overwrite the user's `enabled` toggle. The assertion is also
    /// released by macOS automatically on process exit; doing it
    /// explicitly here just keeps `pmset -g assertions` and
    /// `pmset -g | grep SleepDisabled` clean immediately instead of
    /// waiting for the helper's 15-second idle-exit.
    pub fn shutdown_cleanup(&self) {
        if let Err(e) = self.power.allow_sleep() {
            tracing::warn!("shutdown_cleanup: allow_sleep failed: {e:#}");
        }
        if let Err(e) = self.display.allow_display_sleep() {
            tracing::warn!("shutdown_cleanup: allow_display_sleep failed: {e:#}");
        }
    }

    fn reconcile(&self) {
        // Check for expired timer first
        let timer_expired = {
            let s = self.state.lock().unwrap();
            matches!(s.until, Some(t) if Local::now() >= t)
        };
        if timer_expired {
            tracing::info!("timer expired; auto-deactivating");
            let mut s = self.state.lock().unwrap();
            s.enabled = false;
            s.until = None;
        }

        let (desired, lid_open) = {
            let s = self.state.lock().unwrap();
            (
                should_prevent_sleep(&s, Local::now()),
                s.lid == LidState::Open,
            )
        };

        // (1) System-sleep prevention via the helper. Same as before.
        {
            let mut last = self.last_applied.lock().unwrap();
            if *last != desired {
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

        // (2) Display-sleep prevention via IOPMAssertion. Option-B condition:
        // hold the assertion only while we're preventing sleep, the user
        // hasn't opted out, AND there's actually a display worth keeping
        // awake — i.e. the lid is open, or an external display is attached.
        // When the lid closes with no external display, releasing the
        // assertion lets the `force_display_sleep` branch in `on_lid_change`
        // do its battery-saving job uncontested.
        let pref = self.config.lock().unwrap().prevent_display_sleep;
        let want_assertion = desired && pref && (lid_open || self.display.has_external_display());
        let mut last_assert = self.last_assertion_held.lock().unwrap();
        if *last_assert != want_assertion {
            let r = if want_assertion {
                self.display.prevent_display_sleep()
            } else {
                self.display.allow_display_sleep()
            };
            match r {
                Ok(()) => {
                    tracing::info!("reconcile: display_assertion_held = {want_assertion}");
                    *last_assert = want_assertion;
                }
                Err(e) => {
                    tracing::error!("display-assertion reconcile failed: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlid_core::platform::PlatformError;
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
        cb: Mutex<Option<openlid_core::platform::LidStateCallback>>,
    }
    impl MockLid {
        fn new(s: LidState) -> Self {
            Self {
                state: Mutex::new(s),
                cb: Mutex::new(None),
            }
        }
        /// Simulate a lid-state change by invoking the stored callback, the
        /// same way the real IOKit clamshell observer does.
        fn fire(&self, new: LidState) {
            *self.state.lock().unwrap() = new;
            let guard = self.cb.lock().unwrap();
            if let Some(cb) = guard.as_ref() {
                cb(new);
            }
        }
    }
    impl LidObserver for MockLid {
        fn current(&self) -> LidState {
            *self.state.lock().unwrap()
        }
        fn subscribe(&self, cb: openlid_core::platform::LidStateCallback) {
            *self.cb.lock().unwrap() = Some(cb);
        }
    }

    #[derive(Default)]
    struct MockPs {
        cb: Mutex<Option<openlid_core::platform::PowerSourceCallback>>,
        initial: Mutex<PowerSource>,
    }
    impl MockPs {
        /// Simulate a power-source change by invoking the stored callback,
        /// the same way the real IOKit monitor does.
        fn fire(&self, new: PowerSource) {
            let guard = self.cb.lock().unwrap();
            if let Some(cb) = guard.as_ref() {
                cb(new);
            }
        }
    }
    impl PowerSourceMonitor for MockPs {
        fn current(&self) -> PowerSource {
            *self.initial.lock().unwrap()
        }
        fn subscribe(&self, cb: openlid_core::platform::PowerSourceCallback) {
            *self.cb.lock().unwrap() = Some(cb);
        }
    }

    #[derive(Default)]
    struct MockDisplay {
        external: AtomicBool,
        sleep_calls: AtomicU32,
        // Assertion-lifecycle bookkeeping. `held` reflects the current state
        // as the runtime believes it; the two counters give tests a way to
        // assert how many transitions happened, separately from the final
        // state.
        held: AtomicBool,
        acquire_calls: AtomicU32,
        release_calls: AtomicU32,
    }
    impl DisplayController for MockDisplay {
        fn has_external_display(&self) -> bool {
            self.external.load(Ordering::SeqCst)
        }
        fn force_display_sleep(&self) -> Result<(), PlatformError> {
            self.sleep_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn prevent_display_sleep(&self) -> Result<(), PlatformError> {
            self.acquire_calls.fetch_add(1, Ordering::SeqCst);
            self.held.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn allow_display_sleep(&self) -> Result<(), PlatformError> {
            self.release_calls.fetch_add(1, Ordering::SeqCst);
            self.held.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn enabling_calls_prevent_once_regardless_of_lid() {
        // Post-mode-removal: enable always means prevent sleep, lid-agnostic.
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        let power = Arc::new(MockPower::default());
        let lid = Arc::new(MockLid::new(LidState::Open));
        let ps = Arc::new(MockPs::default());
        let disp = Arc::new(MockDisplay::default());
        let rt = StateRuntime::new(power.clone(), lid, ps, disp, cfg).unwrap();
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 0);
        rt.set_enabled(true, None).unwrap();
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 1);
        // Idempotent: re-enabling doesn't re-call
        rt.set_enabled(true, None).unwrap();
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn disabling_clears_pending_timer() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        let power = Arc::new(MockPower::default());
        let lid = Arc::new(MockLid::new(LidState::Closed));
        let ps = Arc::new(MockPs::default());
        let disp = Arc::new(MockDisplay::default());
        let rt = StateRuntime::new(power, lid, ps, disp, cfg).unwrap();
        rt.set_enabled(true, Some(Local::now() + chrono::Duration::hours(1)))
            .unwrap();
        assert!(rt.snapshot().until.is_some());
        rt.set_enabled(false, None).unwrap();
        assert!(rt.snapshot().until.is_none());
    }

    // --- helpers ------------------------------------------------------------

    type TestRt = Arc<StateRuntime<MockPower, MockLid, MockPs, MockDisplay>>;
    type TestFixture = (
        TestRt,
        Arc<MockPower>,
        Arc<MockPs>,
        Arc<MockDisplay>,
        tempfile::TempDir,
    );

    fn fresh_runtime() -> TestFixture {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        let power = Arc::new(MockPower::default());
        let lid = Arc::new(MockLid::new(LidState::Open));
        let ps = Arc::new(MockPs::default());
        let disp = Arc::new(MockDisplay::default());
        let rt = StateRuntime::new(power.clone(), lid, ps.clone(), disp.clone(), cfg).unwrap();
        (rt, power, ps, disp, dir)
    }

    // --- timer expiry -------------------------------------------------------

    #[test]
    fn snapshot_reports_preventing_false_when_timer_is_in_the_past() {
        // Verify the pure decision function via the runtime: if `until` is in
        // the past, `preventing_sleep_now` is false even when `enabled = true`.
        // We avoid sleeping on a thread by writing a past `until` directly.
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        {
            let mut s = rt.state.lock().unwrap();
            s.enabled = true;
            s.until = Some(Local::now() - chrono::Duration::seconds(1));
        }
        assert!(!rt.snapshot().preventing_sleep_now);
    }

    #[test]
    fn reconcile_clears_enabled_when_timer_expired() {
        // Drive the runtime's own reconcile path (not just the pure helper):
        // set a past `until`, call `set_enabled` to force reconcile, and
        // observe that the runtime cleared `enabled` and `until`.
        let (rt, power, _ps, _disp, _dir) = fresh_runtime();
        // Set a future timer so reconcile arms prevent_sleep first.
        rt.set_enabled(true, Some(Local::now() + chrono::Duration::hours(1)))
            .unwrap();
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 1);
        // Now rewrite `until` to the past directly, then bounce reconcile.
        {
            let mut s = rt.state.lock().unwrap();
            s.until = Some(Local::now() - chrono::Duration::seconds(1));
        }
        // Toggling enabled triggers reconcile, which sees the expired timer
        // and clears `enabled`.
        rt.set_enabled(true, Some(Local::now() - chrono::Duration::seconds(1)))
            .unwrap();
        let snap = rt.snapshot();
        assert!(!snap.enabled);
        assert!(snap.until.is_none());
        assert!(!snap.preventing_sleep_now);
        // We should have called allow_sleep at least once now that we
        // transitioned from prevented to not-prevented.
        assert!(power.allow_calls.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn future_timer_keeps_prevention_active() {
        let (rt, power, _ps, _disp, _dir) = fresh_runtime();
        rt.set_enabled(true, Some(Local::now() + chrono::Duration::hours(1)))
            .unwrap();
        assert!(rt.snapshot().preventing_sleep_now);
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 1);
    }

    // --- battery threshold --------------------------------------------------

    #[test]
    fn battery_below_threshold_auto_deactivates_when_enabled() {
        let (rt, power, ps, _disp, _dir) = fresh_runtime();
        rt.set_preferences(PrefsPatch {
            battery_threshold_pct: Some(Some(20)),
            ..Default::default()
        })
        .unwrap();
        rt.set_enabled(true, None).unwrap();
        assert!(rt.snapshot().enabled);
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 1);

        // Fire a power-change event: dropped to 15% battery.
        ps.fire(PowerSource::Battery { percent: 15 });

        let snap = rt.snapshot();
        assert!(
            !snap.enabled,
            "should auto-deactivate at battery < threshold"
        );
        assert!(snap.until.is_none());
        assert!(power.allow_calls.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn battery_at_or_above_threshold_does_not_deactivate() {
        let (rt, _power, ps, _disp, _dir) = fresh_runtime();
        rt.set_preferences(PrefsPatch {
            battery_threshold_pct: Some(Some(20)),
            ..Default::default()
        })
        .unwrap();
        rt.set_enabled(true, None).unwrap();
        // 25% is above the 20% threshold — should NOT auto-deactivate.
        ps.fire(PowerSource::Battery { percent: 25 });
        assert!(rt.snapshot().enabled);
        // 20% is at the threshold — strict `<` means it should also stay on.
        ps.fire(PowerSource::Battery { percent: 20 });
        assert!(rt.snapshot().enabled);
    }

    #[test]
    fn battery_threshold_does_not_reactivate_after_recovery() {
        // Per Decision 2: once auto-deactivated, recovering battery does
        // NOT flip enabled back on — the user must do that explicitly.
        let (rt, _power, ps, _disp, _dir) = fresh_runtime();
        rt.set_preferences(PrefsPatch {
            battery_threshold_pct: Some(Some(20)),
            ..Default::default()
        })
        .unwrap();
        rt.set_enabled(true, None).unwrap();
        ps.fire(PowerSource::Battery { percent: 10 }); // drops below
        assert!(!rt.snapshot().enabled);
        ps.fire(PowerSource::Ac); // plugged back in
        assert!(!rt.snapshot().enabled, "must not auto-reactivate");
    }

    #[test]
    fn battery_threshold_does_not_deactivate_when_already_disabled() {
        // Edge: if enabled is already false, the power-change handler must
        // not flip anything just because battery is low.
        let (rt, power, ps, _disp, _dir) = fresh_runtime();
        rt.set_preferences(PrefsPatch {
            battery_threshold_pct: Some(Some(50)),
            ..Default::default()
        })
        .unwrap();
        assert!(!rt.snapshot().enabled);
        ps.fire(PowerSource::Battery { percent: 5 });
        assert!(!rt.snapshot().enabled);
        // Should never have called prevent_sleep, and allow_sleep was either
        // never called or only on the initial reconcile (mock starts in
        // "no last applied" state, so first reconcile may be a no-op).
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 0);
    }

    // --- listener notification ---------------------------------------------

    #[test]
    fn listener_fires_on_state_change_with_snapshot() {
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        let fires = Arc::new(AtomicU32::new(0));
        let last_enabled = Arc::new(AtomicBool::new(false));
        let f = fires.clone();
        let le = last_enabled.clone();
        rt.add_listener(Arc::new(move |snap| {
            f.fetch_add(1, Ordering::SeqCst);
            le.store(snap.enabled, Ordering::SeqCst);
        }));
        rt.set_enabled(true, None).unwrap();
        assert!(fires.load(Ordering::SeqCst) >= 1);
        assert!(last_enabled.load(Ordering::SeqCst));
        rt.set_enabled(false, None).unwrap();
        assert!(!last_enabled.load(Ordering::SeqCst));
    }

    #[test]
    fn listener_fires_on_power_source_event() {
        // Power-source events go through on_power_change, which always calls
        // notify_listeners at the end.
        let (rt, _power, ps, _disp, _dir) = fresh_runtime();
        let fires = Arc::new(AtomicU32::new(0));
        let f = fires.clone();
        rt.add_listener(Arc::new(move |_snap| {
            f.fetch_add(1, Ordering::SeqCst);
        }));
        let before = fires.load(Ordering::SeqCst);
        ps.fire(PowerSource::Battery { percent: 80 });
        assert!(fires.load(Ordering::SeqCst) > before);
    }

    #[test]
    fn multiple_listeners_all_fire() {
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        let a = Arc::new(AtomicU32::new(0));
        let b = Arc::new(AtomicU32::new(0));
        let aa = a.clone();
        let bb = b.clone();
        rt.add_listener(Arc::new(move |_| {
            aa.fetch_add(1, Ordering::SeqCst);
        }));
        rt.add_listener(Arc::new(move |_| {
            bb.fetch_add(1, Ordering::SeqCst);
        }));
        rt.set_enabled(true, None).unwrap();
        assert!(a.load(Ordering::SeqCst) >= 1);
        assert!(b.load(Ordering::SeqCst) >= 1);
    }

    // --- PrefsPatch ---------------------------------------------------------

    #[test]
    fn prefs_patch_updates_only_some_fields() {
        // Each `None` in the patch leaves the corresponding field untouched.
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        rt.set_preferences(PrefsPatch {
            start_at_login: Some(true),
            ..Default::default()
        })
        .unwrap();
        let snap1 = rt.snapshot();
        assert!(snap1.start_at_login);

        // Patch with all-None should not alter prior settings.
        rt.set_preferences(PrefsPatch::default()).unwrap();
        let snap2 = rt.snapshot();
        assert_eq!(snap2.start_at_login, snap1.start_at_login);
        assert_eq!(snap2.activate_at_launch, snap1.activate_at_launch);
        assert_eq!(snap2.battery_threshold_pct, snap1.battery_threshold_pct);
    }

    #[test]
    fn prefs_patch_battery_threshold_propagates_to_modifiers() {
        // Setting the threshold via PrefsPatch should also push the value
        // into state.modifiers.min_battery so should_prevent_sleep sees it.
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        rt.set_preferences(PrefsPatch {
            battery_threshold_pct: Some(Some(40)),
            ..Default::default()
        })
        .unwrap();
        let s = rt.state.lock().unwrap();
        assert_eq!(s.modifiers.min_battery, Some(40));
    }

    #[test]
    fn prefs_patch_schedule_some_some_applies() {
        // Applying Some(Some(schedule)) must (1) update the persisted modifier
        // so the snapshot exposes it to clients, and (2) push it into
        // state.modifiers.schedule so should_prevent_sleep sees the gate.
        use chrono::NaiveTime;
        use openlid_core::mode::{DaysOfWeek, Schedule};
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        rt.set_preferences(PrefsPatch {
            schedule: Some(Some(sched.clone())),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            rt.state.lock().unwrap().modifiers.schedule.as_ref(),
            Some(&sched),
            "schedule must be reflected in live state.modifiers",
        );
        assert_eq!(
            rt.snapshot().modifiers.schedule.as_ref(),
            Some(&sched),
            "schedule must be reflected in the snapshot served to clients",
        );
    }

    #[test]
    fn prefs_patch_schedule_some_none_clears() {
        // Some(None) is the explicit "clear" signal. Setting then clearing
        // must remove the schedule from the live state and the snapshot.
        use chrono::NaiveTime;
        use openlid_core::mode::{DaysOfWeek, Schedule};
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        rt.set_preferences(PrefsPatch {
            schedule: Some(Some(Schedule {
                days: DaysOfWeek::all(),
                start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
            })),
            ..Default::default()
        })
        .unwrap();
        assert!(rt.state.lock().unwrap().modifiers.schedule.is_some());
        rt.set_preferences(PrefsPatch {
            schedule: Some(None),
            ..Default::default()
        })
        .unwrap();
        assert!(rt.state.lock().unwrap().modifiers.schedule.is_none());
        assert!(rt.snapshot().modifiers.schedule.is_none());
    }

    #[test]
    fn prefs_patch_schedule_none_leaves_alone() {
        // Outer None on the schedule field means "do not touch". A
        // PrefsPatch::default() must not wipe a previously-set schedule.
        // This is the round-trip that the menubar UI relies on: changing
        // one preference shouldn't accidentally clear another.
        use chrono::NaiveTime;
        use openlid_core::mode::{DaysOfWeek, Schedule};
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        rt.set_preferences(PrefsPatch {
            schedule: Some(Some(sched.clone())),
            ..Default::default()
        })
        .unwrap();
        rt.set_preferences(PrefsPatch {
            start_at_login: Some(true),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            rt.state.lock().unwrap().modifiers.schedule.as_ref(),
            Some(&sched),
            "unrelated PrefsPatch must not clear the schedule",
        );
    }

    #[test]
    fn prefs_patch_activate_at_launch_toggles_independently() {
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        rt.set_preferences(PrefsPatch {
            activate_at_launch: Some(true),
            ..Default::default()
        })
        .unwrap();
        assert!(rt.snapshot().activate_at_launch);
        rt.set_preferences(PrefsPatch {
            activate_at_launch: Some(false),
            ..Default::default()
        })
        .unwrap();
        assert!(!rt.snapshot().activate_at_launch);
    }

    // --- display-sleep assertion (Option B) --------------------------------

    /// Like `fresh_runtime`, but also returns the lid so tests can fire
    /// lid-change events. Default lid state is Open.
    fn fresh_runtime_with_lid() -> (
        TestRt,
        Arc<MockPower>,
        Arc<MockLid>,
        Arc<MockPs>,
        Arc<MockDisplay>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.toml");
        let power = Arc::new(MockPower::default());
        let lid = Arc::new(MockLid::new(LidState::Open));
        let ps = Arc::new(MockPs::default());
        let disp = Arc::new(MockDisplay::default());
        let rt =
            StateRuntime::new(power.clone(), lid.clone(), ps.clone(), disp.clone(), cfg).unwrap();
        (rt, power, lid, ps, disp, dir)
    }

    #[test]
    fn enabling_acquires_display_assertion_by_default() {
        // Default config has prevent_display_sleep=true and the fixture
        // starts with lid open / no external display. Enabling sleep
        // prevention must therefore acquire the display assertion exactly
        // once.
        let (rt, _power, _lid, _ps, disp, _dir) = fresh_runtime_with_lid();
        rt.set_enabled(true, None).unwrap();
        assert!(disp.held.load(Ordering::SeqCst));
        assert_eq!(disp.acquire_calls.load(Ordering::SeqCst), 1);
        // Idempotent: re-enabling does not re-acquire.
        rt.set_enabled(true, None).unwrap();
        assert_eq!(disp.acquire_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn disabling_releases_display_assertion() {
        let (rt, _power, _lid, _ps, disp, _dir) = fresh_runtime_with_lid();
        rt.set_enabled(true, None).unwrap();
        assert!(disp.held.load(Ordering::SeqCst));
        rt.set_enabled(false, None).unwrap();
        assert!(!disp.held.load(Ordering::SeqCst));
        assert_eq!(disp.release_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn closing_lid_with_no_external_display_releases_assertion() {
        // Per Option B, closing the lid while no external display is
        // attached releases the assertion so that on_lid_change's
        // force_display_sleep call lands uncontested. Re-opening must
        // re-acquire.
        let (rt, _power, lid, _ps, disp, _dir) = fresh_runtime_with_lid();
        rt.set_enabled(true, None).unwrap();
        assert!(disp.held.load(Ordering::SeqCst));

        lid.fire(LidState::Closed);
        assert!(
            !disp.held.load(Ordering::SeqCst),
            "lid closed, no external display => assertion must be released"
        );

        lid.fire(LidState::Open);
        assert!(
            disp.held.load(Ordering::SeqCst),
            "lid re-opened => assertion must be re-acquired"
        );
        // We should have seen exactly two acquires and one release.
        assert_eq!(disp.acquire_calls.load(Ordering::SeqCst), 2);
        assert_eq!(disp.release_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn closing_lid_with_external_display_keeps_assertion_held() {
        // External display present: closing the lid must NOT release the
        // assertion — the external display still needs to stay awake.
        let (rt, _power, lid, _ps, disp, _dir) = fresh_runtime_with_lid();
        disp.external.store(true, Ordering::SeqCst);
        rt.set_enabled(true, None).unwrap();
        assert!(disp.held.load(Ordering::SeqCst));

        lid.fire(LidState::Closed);
        assert!(
            disp.held.load(Ordering::SeqCst),
            "external display attached => assertion stays held when lid closes"
        );
        assert_eq!(disp.release_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn opt_out_means_no_assertion_even_when_enabled() {
        // Users who want the screen to lock on idle set
        // prevent_display_sleep=false. The runtime must respect this and
        // skip the assertion entirely, even though system-sleep prevention
        // is still on.
        let (rt, power, _lid, _ps, disp, _dir) = fresh_runtime_with_lid();
        rt.set_preferences(PrefsPatch {
            prevent_display_sleep: Some(false),
            ..Default::default()
        })
        .unwrap();
        rt.set_enabled(true, None).unwrap();
        assert!(rt.snapshot().preventing_sleep_now);
        assert_eq!(power.prevent_calls.load(Ordering::SeqCst), 1);
        assert!(!disp.held.load(Ordering::SeqCst));
        assert_eq!(disp.acquire_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn flipping_opt_out_to_true_acquires_assertion_immediately() {
        // Re-enabling the preference while already-enabled must take effect
        // without requiring the user to toggle the main switch.
        let (rt, _power, _lid, _ps, disp, _dir) = fresh_runtime_with_lid();
        rt.set_preferences(PrefsPatch {
            prevent_display_sleep: Some(false),
            ..Default::default()
        })
        .unwrap();
        rt.set_enabled(true, None).unwrap();
        assert!(!disp.held.load(Ordering::SeqCst));
        rt.set_preferences(PrefsPatch {
            prevent_display_sleep: Some(true),
            ..Default::default()
        })
        .unwrap();
        assert!(disp.held.load(Ordering::SeqCst));
    }

    #[test]
    fn shutdown_cleanup_releases_runtime_resources_but_preserves_persisted_enabled() {
        // Regression for the quit-bug: the menubar Quit handler used to
        // call `set_enabled(false, None)` to release the helper, but that
        // also persisted `enabled = false` to disk — so every relaunch
        // came up as Off, silently overwriting the user's last toggle.
        // The replacement `shutdown_cleanup` must release the runtime
        // side-effects WITHOUT touching state or disk.
        let (rt, power, _lid, _ps, disp, dir) = fresh_runtime_with_lid();
        let cfg_path = dir.path().join("config.toml");

        rt.set_enabled(true, None).unwrap();
        assert!(disp.held.load(Ordering::SeqCst));
        assert!(
            openlid_core::config::Config::load(&cfg_path)
                .unwrap()
                .enabled,
            "precondition: set_enabled(true) should persist enabled = true"
        );

        rt.shutdown_cleanup();

        // Runtime side-effects released.
        assert!(power.allow_calls.load(Ordering::SeqCst) >= 1);
        assert!(disp.release_calls.load(Ordering::SeqCst) >= 1);
        assert!(!disp.held.load(Ordering::SeqCst));

        // But the persisted toggle survives, so the next launch's
        // "restore last state" comes up as On — which is what the user
        // had configured.
        assert!(
            openlid_core::config::Config::load(&cfg_path)
                .unwrap()
                .enabled,
            "shutdown_cleanup must not flip persisted enabled"
        );
        assert!(
            rt.snapshot().enabled,
            "shutdown_cleanup must not flip in-memory enabled either"
        );
    }

    #[test]
    fn lid_closes_with_no_external_display_still_forces_display_sleep() {
        // Sanity guard: regardless of the new assertion machinery, the
        // original Open-Lid value-prop of force-display-sleep on lid close
        // (with no external display) must still fire. This is the behavior
        // the user explicitly asked us to preserve.
        let (rt, _power, lid, _ps, disp, _dir) = fresh_runtime_with_lid();
        rt.set_enabled(true, None).unwrap();
        lid.fire(LidState::Closed);
        assert_eq!(
            disp.sleep_calls.load(Ordering::SeqCst),
            1,
            "force_display_sleep should fire on lid-close without external display"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // In-transit auto-disable: on_network_change bookkeeping +
    // maybe_fire_in_transit_auto_disable predicate-driven fire.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn on_network_change_unreachable_sets_unreachable_since() {
        // The Instant timestamp is what the duration check measures
        // from. Without it the timer would never trip.
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        assert!(rt.state.lock().unwrap().network_unreachable_since.is_none());
        rt.on_network_change(false);
        assert!(rt.state.lock().unwrap().network_unreachable_since.is_some());
        assert!(!rt.state.lock().unwrap().network_reachable);
    }

    #[test]
    fn on_network_change_reachable_clears_unreachable_since() {
        // Network came back -- cancel the in-flight timer (via the
        // generation counter) and clear the timestamp so a future
        // unreachable starts a fresh measurement.
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        rt.on_network_change(false);
        let gen_before = rt.in_transit_generation.load(Ordering::SeqCst);
        rt.on_network_change(true);
        assert!(rt.state.lock().unwrap().network_unreachable_since.is_none());
        assert!(rt.state.lock().unwrap().network_reachable);
        assert!(
            rt.in_transit_generation.load(Ordering::SeqCst) > gen_before,
            "reachable flip must bump the generation to invalidate in-flight sleepers"
        );
    }

    #[test]
    fn on_network_change_repeated_unreachable_does_not_reset_timestamp() {
        // If we get two `false` callbacks in a row without an
        // intervening `true`, the second must not re-arm or reset the
        // timestamp (which would push the auto-disable out by another
        // full window).
        let (rt, _power, _ps, _disp, _dir) = fresh_runtime();
        rt.on_network_change(false);
        let first = rt.state.lock().unwrap().network_unreachable_since;
        std::thread::sleep(StdDuration::from_millis(5));
        rt.on_network_change(false);
        let second = rt.state.lock().unwrap().network_unreachable_since;
        assert_eq!(
            first, second,
            "two unreachable callbacks in a row must keep the original timestamp"
        );
    }

    #[test]
    fn maybe_fire_in_transit_disables_when_all_guards_pass() {
        // The integration path: arrange state so the pure predicate
        // returns true, call the fire helper, observe enabled is now
        // false and persisted.
        let (rt, _power, _ps, disp, _dir) = fresh_runtime();
        // Set the toggle on and put the world in the in-transit shape.
        rt.set_enabled(true, None).unwrap();
        {
            let mut s = rt.state.lock().unwrap();
            s.lid = LidState::Closed;
            s.power = PowerSource::Battery { percent: 50 };
            s.network_reachable = false;
            s.network_unreachable_since =
                Some(std::time::Instant::now() - StdDuration::from_secs(300));
        }
        disp.external.store(false, Ordering::SeqCst);
        rt.maybe_fire_in_transit_auto_disable(StdDuration::from_secs(120));
        assert!(
            !rt.state.lock().unwrap().enabled,
            "should have auto-disabled"
        );
    }

    #[test]
    fn maybe_fire_in_transit_noop_when_external_display_attached() {
        // Clamshell mode -- must not fire even if all other guards hold.
        let (rt, _power, _ps, disp, _dir) = fresh_runtime();
        rt.set_enabled(true, None).unwrap();
        {
            let mut s = rt.state.lock().unwrap();
            s.lid = LidState::Closed;
            s.power = PowerSource::Battery { percent: 50 };
            s.network_unreachable_since =
                Some(std::time::Instant::now() - StdDuration::from_secs(300));
        }
        disp.external.store(true, Ordering::SeqCst);
        rt.maybe_fire_in_transit_auto_disable(StdDuration::from_secs(120));
        assert!(
            rt.state.lock().unwrap().enabled,
            "must not auto-disable in clamshell mode"
        );
    }

    #[test]
    fn maybe_fire_in_transit_noop_when_on_ac() {
        // The on-battery guard is the strongest "in transit" signal --
        // a plugged-in laptop with a network drop is at a desk, not
        // in a backpack.
        let (rt, _power, _ps, disp, _dir) = fresh_runtime();
        rt.set_enabled(true, None).unwrap();
        {
            let mut s = rt.state.lock().unwrap();
            s.lid = LidState::Closed;
            s.power = PowerSource::Ac;
            s.network_unreachable_since =
                Some(std::time::Instant::now() - StdDuration::from_secs(300));
        }
        disp.external.store(false, Ordering::SeqCst);
        rt.maybe_fire_in_transit_auto_disable(StdDuration::from_secs(120));
        assert!(rt.state.lock().unwrap().enabled, "must not fire on AC");
    }

    #[test]
    fn maybe_fire_in_transit_noop_when_duration_under_threshold() {
        // Sub-threshold: 30 s elapsed, 120 s required. Must not fire.
        let (rt, _power, _ps, disp, _dir) = fresh_runtime();
        rt.set_enabled(true, None).unwrap();
        {
            let mut s = rt.state.lock().unwrap();
            s.lid = LidState::Closed;
            s.power = PowerSource::Battery { percent: 50 };
            s.network_unreachable_since =
                Some(std::time::Instant::now() - StdDuration::from_secs(30));
        }
        disp.external.store(false, Ordering::SeqCst);
        rt.maybe_fire_in_transit_auto_disable(StdDuration::from_secs(120));
        assert!(
            rt.state.lock().unwrap().enabled,
            "must not fire under threshold"
        );
    }
}
