//! Owns the live AppState. Subscribes to lid, power, and timer events;
//! re-evaluates `should_prevent_sleep` after each event; calls into the
//! PowerController to reconcile the helper with the desired state.

use anyhow::Result;
use chrono::{DateTime, Local};
use open_lid_core::config::Config;
use open_lid_core::ipc::control::Snapshot;
use open_lid_core::mode::{LidState, PowerSource};
use open_lid_core::platform::{
    DisplayController, LidObserver, PowerController, PowerSourceMonitor,
};
use open_lid_core::state::{should_prevent_sleep, AppState};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

/// Patch struct for updating preferences atomically. Each `Some(_)` replaces
/// the corresponding field; `None` leaves it untouched.
#[derive(Debug, Default, Clone)]
pub struct PrefsPatch {
    pub start_at_login: Option<bool>,
    pub activate_at_launch: Option<bool>,
    pub default_duration_minutes: Option<Option<u32>>,
    pub battery_threshold_pct: Option<Option<u8>>,
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
        };

        let rt = Arc::new(Self {
            state: Arc::new(Mutex::new(state)),
            last_applied: Arc::new(Mutex::new(false)),
            config: Mutex::new(cfg),
            power,
            display,
            _lid: lid.clone(),
            _power_source: power_source.clone(),
            config_path,
            listeners: Mutex::new(Vec::new()),
            timer_generation: AtomicU64::new(0),
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
            if let Some(v) = patch.default_duration_minutes {
                cfg.default_duration_minutes = v;
            }
            if let Some(v) = patch.battery_threshold_pct {
                cfg.battery_threshold_pct = v;
                // Propagate to the modifier the decision function reads.
                self.state.lock().unwrap().modifiers.min_battery = v;
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
            helper: open_lid_core::ipc::control::HelperStatus::Running,
            start_at_login: cfg.start_at_login,
            activate_at_launch: cfg.activate_at_launch,
            default_duration_minutes: cfg.default_duration_minutes,
            battery_threshold_pct: cfg.battery_threshold_pct,
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
        if was_closing && self.snapshot().preventing_sleep_now && !self.display.has_external_display() {
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
            tracing::info!(
                "battery threshold reached; auto-deactivating sleep prevention"
            );
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
            Self {
                state: Mutex::new(s),
                cb: Mutex::new(None),
            }
        }
    }
    impl LidObserver for MockLid {
        fn current(&self) -> LidState {
            *self.state.lock().unwrap()
        }
        fn subscribe(&self, cb: open_lid_core::platform::LidStateCallback) {
            *self.cb.lock().unwrap() = Some(cb);
        }
    }

    #[derive(Default)]
    struct MockPs(Mutex<Option<open_lid_core::platform::PowerSourceCallback>>);
    impl PowerSourceMonitor for MockPs {
        fn current(&self) -> PowerSource {
            PowerSource::Ac
        }
        fn subscribe(&self, cb: open_lid_core::platform::PowerSourceCallback) {
            *self.0.lock().unwrap() = Some(cb);
        }
    }

    #[derive(Default)]
    struct MockDisplay {
        external: AtomicBool,
        sleep_calls: AtomicU32,
    }
    impl DisplayController for MockDisplay {
        fn has_external_display(&self) -> bool {
            self.external.load(Ordering::SeqCst)
        }
        fn force_display_sleep(&self) -> Result<(), PlatformError> {
            self.sleep_calls.fetch_add(1, Ordering::SeqCst);
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
}
