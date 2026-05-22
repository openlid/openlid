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
    display::MacDisplayController, lid_monitor::MacLidMonitor, network_monitor::MacNetworkMonitor,
    power_source::MacPowerSourceMonitor,
};
use crate::state_runtime::{PrefsPatch, StateRuntime};
use anyhow::Result;
use menu::MenuActions;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;
use openlid_core::config::Config;
use openlid_core::mode::Schedule;
use openlid_core::platform::{
    DisplayController, LidObserver, NetworkMonitor, PowerController, PowerSourceMonitor,
};
use preferences::{PreferencesWindow, PrefsActions};
use status_item::{StatusItemUI, UIShared};
use std::sync::{Arc, OnceLock};

pub fn run() -> Result<()> {
    tracing::info!("menubar: starting");

    // Single-instance guard. If another `openlid menubar` is already running,
    // it owns the control socket; we silently exit so launching the .app a
    // second time is a no-op (the standard menu-bar-utility behavior).
    // Without this, every `open -a OpenLid` would spawn a fresh process,
    // leading to multiple menu bar icons and a clobbered control socket.
    if another_instance_running() {
        tracing::info!("menubar: another instance is already running; exiting");
        return Ok(());
    }

    let mtm = MainThreadMarker::new()
        .ok_or_else(|| anyhow::anyhow!("menubar::run must be called on the main thread"))?;

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // Helper auto-registration via SMAppService. Best-effort: failure here
    // is user-actionable (e.g., the .app isn't in /Applications, or the
    // build wasn't signed with a Developer ID), but the menubar should
    // still launch so the user can see the status menu. The XPC client's
    // panic-guard ensures we degrade gracefully if the helper isn't reachable.
    try_register_helper();

    // Platform impls.
    let lid = Arc::new(MacLidMonitor::start()?);
    let ps = Arc::new(MacPowerSourceMonitor::start()?);
    let display = Arc::new(MacDisplayController::new());
    let client = Arc::new(HelperClient::new()?);
    let power = Arc::new(HelperPowerController::new(client.clone()));
    // Reachability monitor. Held for app lifetime. Best-effort:
    // failure here just logs and continues with the in-transit
    // detector effectively disabled (the runtime never receives
    // network-change callbacks).
    let network = match MacNetworkMonitor::start() {
        Ok(m) => Some(Arc::new(m)),
        Err(e) => {
            tracing::warn!("network monitor failed to start: {e:#}");
            None
        }
    };

    // State runtime. `migrate_v1_to_v2` is a one-shot no-op once v2's
    // config exists; on a fresh v1 → v2 upgrade it copies the v1 config to
    // the v2 path so the user keeps their settings without manual work.
    let config_path = Config::migrate_v1_to_v2()?;
    let runtime = StateRuntime::new(power, lid, ps, display, config_path)?;

    // Subscribe the runtime to network reachability changes. The
    // callback fires on the SCNetworkReachability main-runloop
    // delivery thread; `on_network_change` is internally synchronized
    // via the state mutex. Publish the initial reachability reading
    // immediately so the runtime starts with the actual state rather
    // than the optimistic default.
    if let Some(network) = network.as_ref() {
        let initial = network.is_reachable();
        runtime.on_network_change(initial);
        let rt_for_net = Arc::clone(&runtime);
        network.subscribe(Arc::new(move |reachable| {
            rt_for_net.on_network_change(reachable);
        }));
    }

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
/// another `openlid menubar` is already alive. A non-existent socket file
/// or `ECONNREFUSED` (stale socket from a crashed instance) both return
/// `false`, in which case the caller can safely take over.
///
/// Uses `std::os::unix::net::UnixStream` directly rather than the
/// `interprocess` crate the control server uses. The interprocess wrapper
/// returned spurious failures when the socket path contained spaces
/// (e.g. "Application Support"); a raw UNIX-domain connect matches what
/// `nc -U "$SOCK"` does and behaves predictably.
/// Best-effort SMAppService daemon registration. Called at menubar startup
/// so the helper is wired into launchd before the first XPC call. Logs
/// outcomes but never fails the launch — the user-facing path through the
/// preferences window can re-trigger registration if needed.
fn try_register_helper() {
    use crate::helper_installer::{self, HelperServiceStatus};

    match helper_installer::status() {
        Ok(HelperServiceStatus::Enabled) => {
            tracing::info!("helper SMAppService: already enabled");
            return;
        }
        Ok(HelperServiceStatus::RequiresApproval) => {
            tracing::info!(
                "helper SMAppService: registered but requires approval in System Settings"
            );
            return;
        }
        Ok(HelperServiceStatus::NotFound) => {
            tracing::warn!(
                "helper SMAppService: plist not found — verify OpenLid.app is in /Applications \
                 and Contents/Library/LaunchDaemons/io.openlid.helper.plist is present"
            );
            return;
        }
        Ok(HelperServiceStatus::NotRegistered) => {
            // Continue to register below.
        }
        Ok(HelperServiceStatus::Unknown(raw)) => {
            tracing::warn!("helper SMAppService: unknown status {raw}; attempting register");
        }
        Err(e) => {
            tracing::warn!("helper SMAppService status check failed: {e:#}");
            return;
        }
    }

    match helper_installer::register() {
        Ok(()) => match helper_installer::status() {
            Ok(HelperServiceStatus::Enabled) => {
                tracing::info!("helper SMAppService: registered and enabled");
            }
            Ok(HelperServiceStatus::RequiresApproval) => {
                tracing::info!(
                    "helper SMAppService: registered; user approval required in System Settings"
                );
            }
            Ok(other) => {
                tracing::warn!("helper SMAppService: registered but unexpected status {other:?}");
            }
            Err(e) => {
                tracing::warn!("helper SMAppService: post-register status check failed: {e:#}")
            }
        },
        Err(e) => tracing::warn!("helper SMAppService register failed: {e:#}"),
    }
}

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
        // Indefinite in both directions — no timer.
        let result = self.runtime.set_enabled(!snap.enabled, None);
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

    fn check_for_updates(&self) {
        // The HTTP fetch blocks; do it on a worker thread to keep
        // AppKit responsive. When the fetch returns, hop back to the
        // main thread for the NSAlert.
        std::thread::spawn(move || {
            let result = update_check_for_menubar();
            crate::main_thread::run_on_main(move || {
                show_update_alert(result);
            });
        });
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

    fn set_schedule(&self, schedule: Option<Schedule>) {
        // Implicit-enable bridge: setting (Some) on an OFF toggle also turns
        // it on, so the new gate has an enabled state to constrain. This
        // mirrors the CLI behavior of `openlid schedule set`. Clearing
        // (None) leaves `enabled` alone.
        let was_setting = schedule.is_some();
        let patch = PrefsPatch {
            schedule: Some(schedule),
            ..Default::default()
        };
        if let Err(e) = self.runtime.set_preferences(patch) {
            tracing::error!("set_schedule failed: {e:#}");
            self.refresh();
            return;
        }
        if was_setting && !self.runtime.snapshot().enabled {
            if let Err(e) = self.runtime.set_enabled(true, None) {
                tracing::error!("set_schedule: implicit enable failed: {e:#}");
            }
        }
        self.refresh();
    }
}

// ─────────────────────────────────────────────────────────────────────
// "Check for updates…" worker + alert. These run as free functions
// because they don't need any RuntimeActions state -- only the
// updater module and the AppKit main-thread NSAlert.
// ─────────────────────────────────────────────────────────────────────

/// Result handed from the worker thread to the main-thread alert
/// presenter. Carries everything needed to render the dialog without
/// any more IO from the main thread.
enum UpdateCheckOutcome {
    UpToDate {
        current: String,
    },
    HomebrewUpdate {
        latest: String,
    },
    ManualUpdate {
        latest: String,
        release: crate::updater::release::ReleaseInfo,
    },
    DevBuildRefused {
        path: std::path::PathBuf,
    },
    Error {
        message: String,
    },
}

fn update_check_for_menubar() -> UpdateCheckOutcome {
    use crate::updater::{install_detect, release};
    let release = match release::fetch_latest() {
        Ok(r) => r,
        Err(e) => {
            return UpdateCheckOutcome::Error {
                message: format!("Couldn't reach the update server: {e:#}"),
            };
        }
    };
    let available = match release::is_newer_than_current(&release.tag_name) {
        Ok(v) => v,
        Err(e) => {
            return UpdateCheckOutcome::Error {
                message: format!("Couldn't parse the latest version: {e:#}"),
            };
        }
    };
    let current = match release::current_version() {
        Ok(v) => v.to_string(),
        Err(e) => {
            return UpdateCheckOutcome::Error {
                message: format!("Couldn't parse the current version: {e:#}"),
            };
        }
    };
    let latest = match release::strip_v_prefix(&release.tag_name) {
        Ok(s) => s.to_string(),
        Err(e) => {
            return UpdateCheckOutcome::Error {
                message: format!("Couldn't parse the release tag: {e:#}"),
            };
        }
    };
    if !available {
        return UpdateCheckOutcome::UpToDate { current };
    }
    match install_detect::detect() {
        install_detect::InstallMethod::Homebrew => UpdateCheckOutcome::HomebrewUpdate { latest },
        install_detect::InstallMethod::Dev { path } => UpdateCheckOutcome::DevBuildRefused { path },
        install_detect::InstallMethod::Manual => {
            UpdateCheckOutcome::ManualUpdate { latest, release }
        }
    }
}

fn show_update_alert(outcome: UpdateCheckOutcome) {
    use objc2_app_kit::{NSAlert, NSAlertStyle, NSApplication};
    use objc2_foundation::NSString;
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::error!("show_update_alert: not on main thread; dropping result");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.activate();
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Informational);

    match outcome {
        UpdateCheckOutcome::UpToDate { current } => {
            alert.setMessageText(&NSString::from_str("OpenLid is up to date"));
            alert.setInformativeText(&NSString::from_str(&format!(
                "You're on the latest version (v{current})."
            )));
            alert.addButtonWithTitle(&NSString::from_str("OK"));
            alert.runModal();
        }
        UpdateCheckOutcome::HomebrewUpdate { latest } => {
            alert.setMessageText(&NSString::from_str(&format!("Update available: v{latest}")));
            alert.setInformativeText(&NSString::from_str(
                "This is a Homebrew install. To update, run:\n\n  brew upgrade openlid",
            ));
            alert.addButtonWithTitle(&NSString::from_str("OK"));
            alert.runModal();
        }
        UpdateCheckOutcome::DevBuildRefused { path } => {
            alert.setMessageText(&NSString::from_str("Dev build detected"));
            alert.setInformativeText(&NSString::from_str(&format!(
                "OpenLid is running from {} -- the updater refuses to \
                 replace a dev build. Rebuild from source instead.",
                path.display()
            )));
            alert.addButtonWithTitle(&NSString::from_str("OK"));
            alert.runModal();
        }
        UpdateCheckOutcome::Error { message } => {
            alert.setAlertStyle(NSAlertStyle::Warning);
            alert.setMessageText(&NSString::from_str("Update check failed"));
            alert.setInformativeText(&NSString::from_str(&message));
            alert.addButtonWithTitle(&NSString::from_str("OK"));
            alert.runModal();
        }
        UpdateCheckOutcome::ManualUpdate { latest, release } => {
            alert.setMessageText(&NSString::from_str(&format!("Update available: v{latest}")));
            // Trim release notes to a manageable size for the alert
            // info pane. NSAlert grows tall with long text; ~600 chars
            // keeps it screen-friendly.
            let notes = release.body.chars().take(600).collect::<String>();
            let info = if notes.is_empty() {
                "Install the new version? Your settings will be preserved.".to_string()
            } else {
                format!(
                    "Release notes:\n\n{notes}\n\nInstall the new version? \
                     Your settings will be preserved."
                )
            };
            alert.setInformativeText(&NSString::from_str(&info));
            alert.addButtonWithTitle(&NSString::from_str("Install Now"));
            alert.addButtonWithTitle(&NSString::from_str("Later"));
            let response = alert.runModal();
            // NSAlertFirstButtonReturn is 1000; first button is "Install Now".
            if response == 1000 {
                if let Err(e) = run_manual_install_from_menubar(&release) {
                    let err_alert = NSAlert::new(mtm);
                    err_alert.setAlertStyle(NSAlertStyle::Critical);
                    err_alert.setMessageText(&NSString::from_str("Install failed"));
                    err_alert.setInformativeText(&NSString::from_str(&format!("{e:#}")));
                    err_alert.addButtonWithTitle(&NSString::from_str("OK"));
                    err_alert.runModal();
                }
            }
        }
    }
}

/// Drive the install from the menubar's main thread after the user
/// clicks "Install Now". On success we terminate NSApplication so the
/// detached installer's `kill -0 $PARENT_PID` wait unblocks.
fn run_manual_install_from_menubar(
    release: &crate::updater::release::ReleaseInfo,
) -> anyhow::Result<()> {
    use crate::updater::{install_detect, installer, release as release_mod};
    let asset = release_mod::pick_dmg_asset(&release.assets)?;
    let cache = installer::cache_dir()?;
    installer::prepare_cache(&cache)?;
    let dmg_path = cache.join(&asset.name);
    installer::download(&asset.browser_download_url, &dmg_path)?;
    if let Some(digest) = asset.digest.as_deref() {
        let hex = release_mod::parse_digest(digest)?;
        installer::verify_sha256(&dmg_path, &hex)?;
    }
    installer::spawn_detached_installer(std::process::id(), &dmg_path, install_detect::APP_PATH)?;
    // Tell AppKit to exit so the installer's wait-for-parent loop
    // unblocks. The installer relaunches via `open -b io.openlid.app`.
    if let Some(mtm) = MainThreadMarker::new() {
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        app.terminate(None);
    }
    Ok(())
}
