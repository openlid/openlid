//! Menu-bar role entry point and `MenuActions` glue.
//!
//! `run()` initialises NSApplication in accessory mode, builds the platform
//! traits (lid, power source, display, helper-backed power controller), wires
//! up the state runtime and the control-socket server, then constructs the
//! `NSStatusItem` UI and pumps the AppKit event loop.
mod icons;
mod menu;
mod preferences;
mod status_item;

use crate::control_server;
use crate::helper_client::{HelperClient, HelperPowerController};
use crate::platform::macos::{
    display::MacDisplayController, lid_monitor::MacLidMonitor, power_source::MacPowerSourceMonitor,
};
use crate::state_runtime::{PrefsPatch, StateRuntime};
use anyhow::Result;
use chrono::{Duration, Local};
use menu::MenuActions;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;
use open_lid_core::config::Config;
use open_lid_core::platform::{
    DisplayController, LidObserver, PowerController, PowerSourceMonitor,
};
use preferences::{PreferencesWindow, PrefsActions};
use status_item::{StatusItemUI, UIShared};
use std::sync::{Arc, OnceLock};

pub fn run() -> Result<()> {
    tracing::info!("menubar: starting");

    // Single-instance guard. If another `open-lid menubar` is already running,
    // it owns the control socket; we silently exit so launching the .app a
    // second time is a no-op (matching Caffeine's behavior). Without this,
    // every `open -a OpenLid` would spawn a fresh process, leading to
    // multiple menu bar icons and a clobbered control socket.
    if another_instance_running() {
        tracing::info!("menubar: another instance is already running; exiting");
        return Ok(());
    }

    let mtm = MainThreadMarker::new()
        .ok_or_else(|| anyhow::anyhow!("menubar::run must be called on the main thread"))?;

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // Platform impls.
    let lid = Arc::new(MacLidMonitor::start()?);
    let ps = Arc::new(MacPowerSourceMonitor::start()?);
    let display = Arc::new(MacDisplayController::new());
    let client = Arc::new(HelperClient::new()?);
    let power = Arc::new(HelperPowerController::new(client.clone()));

    // State runtime.
    let config_path = Config::default_path()?;
    let runtime = StateRuntime::new(power, lid, ps, display, config_path)?;

    // Spawn the control server (background thread).
    control_server::spawn(runtime.clone())?;

    // Build the actions object with a deferred UI handle. We set the UI
    // reference back into the actions after the UI is built; the menu cannot
    // fire actions before the event loop starts, so this is safe.
    let actions = Arc::new(RuntimeActions::new(runtime.clone()));

    // Build UI. The handler stores `Arc<dyn MenuActions>` — the same one we
    // hold here, type-erased.
    let ui = StatusItemUI::new(mtm, actions.clone() as Arc<dyn MenuActions>)?;
    // Hand the UI's shared bundle back to the actions so click handlers can
    // refresh the icon and menu items.
    actions
        .install_ui(ui.shared())
        .map_err(|_| anyhow::anyhow!("actions UI installed twice"))?;
    // Stash a self-Arc that `open_preferences` can hand to the prefs window
    // handler. The cycle (RuntimeActions → Arc<dyn PrefsActions> → same
    // RuntimeActions) is intentional and lives for the rest of the process.
    actions
        .install_self_prefs(actions.clone() as Arc<dyn PrefsActions>)
        .map_err(|_| anyhow::anyhow!("self_prefs installed twice"))?;

    // First paint reflects the persisted state.
    ui.refresh(&runtime.snapshot(), mtm);

    // Subscribe to runtime changes so the UI refreshes when the CLI (via
    // the control socket) or a lid/power event modifies state from a
    // non-main thread. Each listener invocation hops to the main thread
    // before touching AppKit.
    let ui_for_listener = ui.shared();
    runtime.add_listener(Arc::new(move |snap| {
        let ui = Arc::clone(&ui_for_listener);
        let snap = snap.clone();
        crate::main_thread::run_on_main(move || {
            if let Some(mtm) = MainThreadMarker::new() {
                ui.refresh(&snap, mtm);
            }
        });
    }));

    // Run the event loop. This returns when -[NSApplication terminate:]
    // is invoked (from the Quit menu item).
    app.run();

    // Best-effort socket cleanup on the way out.
    if let Ok(p) = control_server::control_socket_path() {
        let _ = std::fs::remove_file(p);
    }

    Ok(())
}

/// Probe whether a running menubar instance owns the control socket. We
/// attempt to connect; success means another process is listening — i.e.,
/// another `open-lid menubar` is already alive. A non-existent socket file
/// or `ECONNREFUSED` (stale socket from a crashed instance) both return
/// `false`, in which case the caller can safely take over.
///
/// Uses `std::os::unix::net::UnixStream` directly rather than the
/// `interprocess` crate the control server uses. The interprocess wrapper
/// returned spurious failures when the socket path contained spaces
/// (e.g. "Application Support"); a raw UNIX-domain connect matches what
/// `nc -U "$SOCK"` does and behaves predictably.
fn another_instance_running() -> bool {
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let Ok(path) = control_server::control_socket_path() else {
        return false;
    };
    if !path.exists() {
        return false;
    }
    match UnixStream::connect(&path) {
        Ok(stream) => {
            // Set a short timeout so a hung peer can't pin us at startup.
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            true
        }
        Err(e) => {
            tracing::debug!("another_instance_running: connect failed: {e}");
            false
        }
    }
}

/// Concrete `MenuActions` impl wrapping the (generic) `StateRuntime` plus a
/// post-action UI refresh. The runtime's many generic parameters are erased
/// here so the AppKit-facing menu handler sees a single trait object.
struct RuntimeActions<P, L, S, D>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    runtime: Arc<StateRuntime<P, L, S, D>>,
    ui: OnceLock<Arc<UIShared>>,
    /// Self-Arc used to hand `Arc<dyn PrefsActions>` to `PreferencesWindow`
    /// from inside `&self` methods. Installed once at startup (alongside the
    /// UI handle); used only on the main thread.
    self_prefs: OnceLock<Arc<dyn PrefsActions>>,
    /// Lazily-constructed preferences window. Built on first
    /// `MenuActions::open_preferences` call (always main-thread) and reused
    /// for the rest of the process's life.
    prefs_window: OnceLock<PreferencesWindow>,
}

impl<P, L, S, D> RuntimeActions<P, L, S, D>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    fn new(runtime: Arc<StateRuntime<P, L, S, D>>) -> Self {
        Self {
            runtime,
            ui: OnceLock::new(),
            self_prefs: OnceLock::new(),
            prefs_window: OnceLock::new(),
        }
    }

    /// Install a self-Arc for handing to `PreferencesWindow`. Must be called
    /// exactly once at startup.
    fn install_self_prefs(
        &self,
        self_arc: Arc<dyn PrefsActions>,
    ) -> Result<(), Arc<dyn PrefsActions>> {
        self.self_prefs.set(self_arc)
    }

    /// Install the UI handle. Must be called exactly once; returns the value
    /// back as `Err` if already set.
    fn install_ui(&self, ui: Arc<UIShared>) -> Result<(), Arc<UIShared>> {
        self.ui.set(ui)
    }

    fn refresh(&self) {
        // The menu click handlers always run on the main thread, so this
        // marker is safe to construct here. If `MainThreadMarker::new()`
        // somehow returns None (e.g. we are wrong about the dispatch
        // context), skip the refresh rather than panic.
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::warn!("RuntimeActions::refresh: not on main thread, skipping UI update");
            return;
        };
        let Some(ui) = self.ui.get() else {
            // Should not happen post-`install_ui`; harmless if it does.
            return;
        };
        ui.refresh(&self.runtime.snapshot(), mtm);
    }
}

impl<P, L, S, D> MenuActions for RuntimeActions<P, L, S, D>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    fn toggle(&self) {
        let snap = self.runtime.snapshot();
        let result = if snap.enabled {
            // Currently on → turn off (clears any timer too).
            self.runtime.set_enabled(false, None)
        } else {
            // Currently off → turn on, using default duration from prefs.
            let until = snap
                .default_duration_minutes
                .map(|m| Local::now() + Duration::minutes(m as i64));
            self.runtime.set_enabled(true, until)
        };
        if let Err(e) = result {
            tracing::error!("toggle failed: {e:#}");
        }
        self.refresh();
    }

    fn show_menu(&self) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if let Some(ui) = self.ui.get() {
            ui.show_menu(mtm);
        }
    }

    fn activate_for_minutes(&self, minutes: Option<u32>) {
        let until = minutes.map(|m| Local::now() + Duration::minutes(m as i64));
        if let Err(e) = self.runtime.set_enabled(true, until) {
            tracing::error!("activate_for_minutes({minutes:?}) failed: {e:#}");
        }
        self.refresh();
    }

    fn open_preferences(&self) {
        // Menu click handlers run on the main thread, so an MTM should be
        // obtainable. If not, log and bail — calling AppKit from the wrong
        // thread is worse than not opening the window.
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::error!("open_preferences: not on main thread; cannot open window");
            return;
        };
        let window = self.prefs_window.get_or_init(|| {
            // The self-Arc must have been installed at startup. If not, we
            // can't construct the window because the handler needs an
            // `Arc<dyn PrefsActions>` for its ivars.
            let actions = self
                .self_prefs
                .get()
                .expect("self_prefs not installed before open_preferences")
                .clone();
            PreferencesWindow::new(mtm, actions)
        });
        window.show(&self.runtime.snapshot(), mtm);
    }

    fn quit(&self) {
        // 1. Release runtime side-effects (helper sleep prevention + display
        //    assertion) without persisting `enabled = false` to disk.
        //    `set_enabled(false, None)` would also write through to the
        //    config, which would silently overwrite the user's last toggle
        //    on every quit — restoring "last state" on relaunch would
        //    always come up as Off.
        self.runtime.shutdown_cleanup();

        // 2. Best-effort socket cleanup.
        if let Ok(p) = control_server::control_socket_path() {
            let _ = std::fs::remove_file(p);
        }

        // 3. Terminate the AppKit event loop. NSApplication::terminate must
        //    run on the main thread; the menu handler selectors always do.
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            app.terminate(None);
        } else {
            tracing::error!("quit: not on main thread; cannot terminate cleanly");
        }
    }
}

impl<P, L, S, D> PrefsActions for RuntimeActions<P, L, S, D>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    fn set_start_at_login(&self, enabled: bool) {
        let patch = PrefsPatch {
            start_at_login: Some(enabled),
            ..Default::default()
        };
        if let Err(e) = self.runtime.set_preferences(patch) {
            tracing::error!("set_start_at_login failed: {e:#}");
        }
        self.refresh();
    }

    fn set_activate_at_launch(&self, enabled: bool) {
        let patch = PrefsPatch {
            activate_at_launch: Some(enabled),
            ..Default::default()
        };
        if let Err(e) = self.runtime.set_preferences(patch) {
            tracing::error!("set_activate_at_launch failed: {e:#}");
        }
        self.refresh();
    }

    fn set_default_duration(&self, minutes: Option<u32>) {
        let patch = PrefsPatch {
            default_duration_minutes: Some(minutes),
            ..Default::default()
        };
        if let Err(e) = self.runtime.set_preferences(patch) {
            tracing::error!("set_default_duration failed: {e:#}");
        }
        self.refresh();
    }

    fn set_battery_threshold(&self, pct: Option<u8>) {
        let patch = PrefsPatch {
            battery_threshold_pct: Some(pct),
            ..Default::default()
        };
        if let Err(e) = self.runtime.set_preferences(patch) {
            tracing::error!("set_battery_threshold failed: {e:#}");
        }
        self.refresh();
    }

    fn set_prevent_display_sleep(&self, enabled: bool) {
        let patch = PrefsPatch {
            prevent_display_sleep: Some(enabled),
            ..Default::default()
        };
        if let Err(e) = self.runtime.set_preferences(patch) {
            tracing::error!("set_prevent_display_sleep failed: {e:#}");
        }
        self.refresh();
    }
}
