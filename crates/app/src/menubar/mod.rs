//! Menu-bar role entry point and `MenuActions` glue.
//!
//! `run()` initialises NSApplication in accessory mode, builds the platform
//! traits (lid, power source, display, helper-backed power controller), wires
//! up the state runtime and the control-socket server, then constructs the
//! `NSStatusItem` UI and pumps the AppKit event loop.
mod icons;
mod menu;
mod status_item;

use crate::control_server;
use crate::helper_client::{HelperClient, HelperPowerController};
use crate::platform::macos::{
    display::MacDisplayController, lid_monitor::MacLidMonitor, power_source::MacPowerSourceMonitor,
};
use crate::state_runtime::StateRuntime;
use anyhow::Result;
use menu::MenuActions;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;
use open_lid_core::config::Config;
use open_lid_core::mode::Mode;
use open_lid_core::platform::{
    DisplayController, LidObserver, PowerController, PowerSourceMonitor,
};
use status_item::{StatusItemUI, UIShared};
use std::sync::{Arc, OnceLock};

pub fn run() -> Result<()> {
    tracing::info!("menubar: starting");

    let mtm = MainThreadMarker::new()
        .ok_or_else(|| anyhow::anyhow!("menubar::run must be called on the main thread"))?;

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // Platform impls.
    let lid = Arc::new(MacLidMonitor::start()?);
    let ps = Arc::new(MacPowerSourceMonitor::start()?);
    let display = Arc::new(MacDisplayController);
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
        }
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
        let currently_enabled = self.runtime.snapshot().enabled;
        if let Err(e) = self.runtime.set_enabled(!currently_enabled) {
            tracing::error!("toggle failed: {e:#}");
        }
        self.refresh();
    }

    fn set_mode_lid_closed(&self) {
        if let Err(e) = self.runtime.set_mode(Mode::LidClosed) {
            tracing::error!("set_mode(LidClosed) failed: {e:#}");
        }
        self.refresh();
    }

    fn set_mode_always_awake(&self) {
        if let Err(e) = self.runtime.set_mode(Mode::AlwaysAwake) {
            tracing::error!("set_mode(AlwaysAwake) failed: {e:#}");
        }
        self.refresh();
    }

    fn quit(&self) {
        // 1. Disarm sleep prevention so the helper doesn't keep the machine
        //    awake after we exit.
        if let Err(e) = self.runtime.set_enabled(false) {
            tracing::warn!("quit: failed to disarm sleep prevention: {e:#}");
        }

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
