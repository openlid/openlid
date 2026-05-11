//! Owns the live AppState. Subscribes to lid, power, and timer events;
//! re-evaluates `should_prevent_sleep` after each event; calls into the
//! PowerController to reconcile the helper with the desired state.

use anyhow::Result;
use chrono::Local;
use open_lid_core::config::Config;
use open_lid_core::ipc::control::Snapshot;
use open_lid_core::mode::{LidState, Mode, Modifiers, PowerSource};
use open_lid_core::platform::{
    DisplayController, LidObserver, PowerController, PowerSourceMonitor,
};
use open_lid_core::state::{should_prevent_sleep, AppState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
    power: Arc<P>,
    display: Arc<D>,
    _lid: Arc<L>,
    _power_source: Arc<S>,
    config_path: PathBuf,
    listeners: Mutex<Vec<StateListener>>,
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
            listeners: Mutex::new(Vec::new()),
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

    /// Subscribe to state changes. Listener is called after every successful
    /// `set_enabled`/`set_mode`/`set_modifiers` and after every lid/power event
    /// reconciliation. UI listeners must dispatch back to the main thread
    /// before touching AppKit — see `crate::main_thread::run_on_main`.
    pub fn add_listener(&self, listener: StateListener) {
        self.listeners.lock().unwrap().push(listener);
    }

    fn notify_listeners(&self) {
        let snap = self.snapshot();
        // Clone the Arc list and drop the lock before invoking any listener,
        // so a re-entrant `add_listener` during a callback can't deadlock.
        let listeners: Vec<StateListener> = self.listeners.lock().unwrap().clone();
        for l in &listeners {
            l(&snap);
        }
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
        if was_closing && !self.display.has_external_display() {
            let _ = self.display.force_display_sleep();
        }
        self.notify_listeners();
    }

    fn on_power_change(self: &Arc<Self>, new_ps: PowerSource) {
        self.state.lock().unwrap().power = new_ps;
        self.reconcile();
        self.notify_listeners();
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
        self.notify_listeners();
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
