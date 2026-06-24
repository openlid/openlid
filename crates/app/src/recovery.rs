//! Helper-recovery decision logic.
//!
//! When the privileged helper is unreachable, the app needs to decide
//! what to do based on the `SMAppService` registration status, and how
//! often it is allowed to bother the user. Those decisions are pure
//! functions here so they can be unit-tested; the `HelperRecoverySurface`
//! (added alongside the `HelperPowerController`) and the actual objc
//! calls (`register`, banner, `open_system_settings_login_items`) are the
//! thin glue that consumes them.

use crate::helper_installer::HelperServiceStatus;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Backstop interval between repeated banners within a single unhealthy
/// episode. The normal reset is [`HelperRecoverySurface::mark_healthy`]
/// (called the moment we reconnect); this cooldown only matters if the
/// helper stays broken, so it's deliberately long.
const SURFACE_COOLDOWN: Duration = Duration::from_secs(300);

/// Shared, rate-limited surface for helper-recovery UI and the bounded
/// post-approval follow-up. One instance is created in `menubar::run` and
/// shared between startup registration (`try_register_helper`) and the
/// runtime `HelperPowerController`, so a single cooldown governs all
/// surfacing rather than each path nagging independently.
pub struct HelperRecoverySurface {
    approval_last: Mutex<Option<Instant>>,
    not_found_last: Mutex<Option<Instant>>,
    /// Runtime reconcile trigger, installed after the `StateRuntime` is
    /// built. The bounded approval follow-up calls it so a "user approved
    /// in Settings" is picked up without an incidental hardware event.
    reconcile_cb: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl HelperRecoverySurface {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            approval_last: Mutex::new(None),
            not_found_last: Mutex::new(None),
            reconcile_cb: Mutex::new(None),
        })
    }

    /// Install the runtime reconcile callback. Called once by `menubar::run`
    /// after the runtime exists.
    pub fn set_reconcile_callback(&self, cb: Arc<dyn Fn() + Send + Sync>) {
        *self.reconcile_cb.lock().unwrap() = Some(cb);
    }

    /// The helper is reachable again — clear the surfacing cooldowns so a
    /// future unhealthy episode can post a fresh banner.
    pub fn mark_healthy(&self) {
        *self.approval_last.lock().unwrap() = None;
        *self.not_found_last.lock().unwrap() = None;
    }

    /// Surface "helper needs approval", rate-limited. Posts a banner, or —
    /// if the user has denied notifications — falls back to opening System
    /// Settings directly so the silent failure can't recur.
    pub fn notify_approval_needed(&self) {
        let now = Instant::now();
        {
            let mut last = self.approval_last.lock().unwrap();
            if !should_surface(now, *last, SURFACE_COOLDOWN) {
                return;
            }
            *last = Some(now);
        }
        if crate::notify::auth_denied() {
            tracing::info!(
                "helper needs approval; notifications denied — opening System Settings directly"
            );
            self.on_settings_opened();
        } else {
            tracing::info!("helper needs approval; posting banner");
            crate::notify::post_approval();
        }
    }

    /// Surface "OpenLid must be in /Applications", rate-limited.
    pub fn notify_not_found(&self) {
        let now = Instant::now();
        {
            let mut last = self.not_found_last.lock().unwrap();
            if !should_surface(now, *last, SURFACE_COOLDOWN) {
                return;
            }
            *last = Some(now);
        }
        tracing::warn!("helper SMAppService plist not found; posting banner");
        crate::notify::post_not_found();
    }

    /// The user is being sent to the Login Items pane (via banner tap or
    /// the denied fallback): open Settings and arm the bounded follow-up.
    pub fn on_settings_opened(&self) {
        if let Err(e) = crate::helper_installer::open_system_settings_login_items() {
            tracing::warn!("failed to open System Settings Login Items: {e:#}");
        }
        self.start_approval_follow_up();
    }

    /// Spawn the bounded post-approval follow-up: a single thread that
    /// nudges `reconcile` at the fixed [`approval_recheck_delays`] so a
    /// just-approved helper reconnects without waiting for a hardware
    /// event. Finite by construction — never an always-on poller.
    fn start_approval_follow_up(&self) {
        let Some(cb) = self.reconcile_cb.lock().unwrap().clone() else {
            return; // runtime not wired yet
        };
        std::thread::spawn(move || {
            for delay in approval_recheck_delays() {
                std::thread::sleep(*delay);
                cb();
            }
        });
    }
}

/// What to do about an unreachable helper, given its registration status
/// and whether we're allowed to surface UI (the prevent path) or not
/// (the allow path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Programmatically (re-)register the daemon. Idempotent.
    Register,
    /// Post the "needs approval" banner and start the approval follow-up.
    NotifyApproval,
    /// Post the "move OpenLid to /Applications" banner — a genuine
    /// can't-install, surfaced only after a `register()` attempt fails
    /// and the daemon still isn't registered.
    NotifyNotFound,
    /// Rebuild the XPC connection — the daemon is enabled but our client
    /// latched invalid.
    Reconnect,
    /// Do nothing this cycle.
    Nothing,
}

/// Map the live `SMAppService` status onto a recovery action.
///
/// `surface` is `true` on the prevent-sleep path (where bothering the
/// user is justified) and `false` on the allow-sleep path (where we must
/// never nag). `Register` and `Reconnect` run regardless of `surface`;
/// only the user-visible `Notify*` actions are gated by it.
pub fn recovery_action(status: HelperServiceStatus, surface: bool) -> RecoveryAction {
    match status {
        // NotFound is treated like NotRegistered: an in-place app
        // replacement can orphan the prior registration and report
        // NotFound even when the app is correctly placed. Re-register;
        // the genuine misinstall surfaces only if register() fails.
        HelperServiceStatus::NotRegistered
        | HelperServiceStatus::NotFound
        | HelperServiceStatus::Unknown(_) => RecoveryAction::Register,
        HelperServiceStatus::Enabled => RecoveryAction::Reconnect,
        HelperServiceStatus::RequiresApproval => {
            if surface {
                RecoveryAction::NotifyApproval
            } else {
                RecoveryAction::Nothing
            }
        }
    }
}

/// Decide what to surface AFTER a `register()` attempt has failed, based
/// on the re-checked status. Unlike [`recovery_action`], this never
/// returns `Register` (we already tried). `register()` failing with
/// `EPERM` ("Operation not permitted") commonly means the daemon IS
/// registered but needs the user's Login Items approval — so a
/// `RequiresApproval` re-check routes to the approval banner, not the
/// misinstall one. Only a still-not-registered status is a genuine
/// can't-install.
pub fn post_register_action(status: HelperServiceStatus, surface: bool) -> RecoveryAction {
    match status {
        HelperServiceStatus::Enabled => RecoveryAction::Reconnect,
        HelperServiceStatus::RequiresApproval => {
            if surface {
                RecoveryAction::NotifyApproval
            } else {
                RecoveryAction::Nothing
            }
        }
        HelperServiceStatus::NotRegistered
        | HelperServiceStatus::NotFound
        | HelperServiceStatus::Unknown(_) => {
            if surface {
                RecoveryAction::NotifyNotFound
            } else {
                RecoveryAction::Nothing
            }
        }
    }
}

/// Cooldown gate for user-visible surfacing: fire if we've never surfaced
/// (`last == None`) or the cooldown has elapsed. Keeps one banner per
/// unhealthy episode instead of one per reconcile.
pub fn should_surface(now: Instant, last: Option<Instant>, cooldown: Duration) -> bool {
    match last {
        None => true,
        Some(t) => now.duration_since(t) >= cooldown,
    }
}

/// The bounded recheck schedule fired after opening System Settings, so a
/// "user just approved" can be picked up without an always-on poller.
/// Returned as a fixed slice precisely so the bound is pinned by a test.
pub fn approval_recheck_delays() -> &'static [Duration] {
    static DELAYS: [Duration; 5] = [
        Duration::from_secs(5),
        Duration::from_secs(15),
        Duration::from_secs(30),
        Duration::from_secs(60),
        Duration::from_secs(120),
    ];
    &DELAYS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper_installer::HelperServiceStatus;
    use std::time::{Duration, Instant};

    #[test]
    fn registers_when_not_registered() {
        // Programmatic registration is safe and idempotent, so it runs on
        // either the prevent or allow path (surface is irrelevant here).
        assert_eq!(
            recovery_action(HelperServiceStatus::NotRegistered, true),
            RecoveryAction::Register
        );
        assert_eq!(
            recovery_action(HelperServiceStatus::Unknown(9), false),
            RecoveryAction::Register
        );
    }

    #[test]
    fn reconnects_when_enabled_regardless_of_surface() {
        // Enabled but our XPC client latched invalid → rebuild it. This
        // is the "user approved while the app was running" recovery and
        // must happen on both paths.
        assert_eq!(
            recovery_action(HelperServiceStatus::Enabled, true),
            RecoveryAction::Reconnect
        );
        assert_eq!(
            recovery_action(HelperServiceStatus::Enabled, false),
            RecoveryAction::Reconnect
        );
    }

    #[test]
    fn notifies_for_approval_only_when_surfacing() {
        assert_eq!(
            recovery_action(HelperServiceStatus::RequiresApproval, true),
            RecoveryAction::NotifyApproval
        );
        // allow-sleep path: never post a banner while turning prevention
        // OFF — that would nag the user for no reason.
        assert_eq!(
            recovery_action(HelperServiceStatus::RequiresApproval, false),
            RecoveryAction::Nothing
        );
    }

    #[test]
    fn registers_when_not_found() {
        // NotFound is treated like NotRegistered: an in-place app
        // replacement (Homebrew upgrade or a manual swap) can orphan the
        // prior registration and report NotFound even though the app is
        // correctly placed. We re-register regardless of `surface`; the
        // genuine "can't install the daemon" case is surfaced only if
        // register() itself fails (handled by the executor).
        assert_eq!(
            recovery_action(HelperServiceStatus::NotFound, true),
            RecoveryAction::Register
        );
        assert_eq!(
            recovery_action(HelperServiceStatus::NotFound, false),
            RecoveryAction::Register
        );
    }

    #[test]
    fn post_register_failure_routes_eperm_to_approval() {
        // The wart fix: register() returning EPERM ("Operation not
        // permitted") almost always means the daemon is registered but
        // needs the user's Login Items approval — NOT a misinstall. Once
        // we re-check status and see RequiresApproval, surface the
        // APPROVAL banner (deep-links to the toggle), not "move to
        // /Applications".
        assert_eq!(
            post_register_action(HelperServiceStatus::RequiresApproval, true),
            RecoveryAction::NotifyApproval
        );
    }

    #[test]
    fn post_register_failure_genuine_misinstall_surfaces_not_found() {
        // If register() failed AND the helper still isn't registered,
        // that's a real can't-install (app not in /Applications, signing
        // mismatch) → the misinstall banner is correct here.
        assert_eq!(
            post_register_action(HelperServiceStatus::NotFound, true),
            RecoveryAction::NotifyNotFound
        );
        assert_eq!(
            post_register_action(HelperServiceStatus::NotRegistered, true),
            RecoveryAction::NotifyNotFound
        );
    }

    #[test]
    fn post_register_reconnects_if_somehow_enabled() {
        assert_eq!(
            post_register_action(HelperServiceStatus::Enabled, true),
            RecoveryAction::Reconnect
        );
    }

    #[test]
    fn post_register_never_surfaces_on_allow_path() {
        // surface=false (allow-sleep path): never post a banner.
        assert_eq!(
            post_register_action(HelperServiceStatus::RequiresApproval, false),
            RecoveryAction::Nothing
        );
        assert_eq!(
            post_register_action(HelperServiceStatus::NotFound, false),
            RecoveryAction::Nothing
        );
    }

    #[test]
    fn surfaces_when_never_surfaced_before() {
        assert!(should_surface(
            Instant::now(),
            None,
            Duration::from_secs(60)
        ));
    }

    #[test]
    fn suppresses_surface_within_cooldown() {
        let now = Instant::now();
        let last = now - Duration::from_secs(10);
        assert!(!should_surface(now, Some(last), Duration::from_secs(60)));
    }

    #[test]
    fn surfaces_again_past_cooldown() {
        let now = Instant::now();
        let last = now - Duration::from_secs(120);
        assert!(should_surface(now, Some(last), Duration::from_secs(60)));
    }

    #[test]
    fn approval_recheck_schedule_is_bounded_and_increasing() {
        // Pins the bounded follow-up: finite, strictly increasing. A
        // regression to an unbounded or constant-zero schedule (an
        // always-on poll) would fail here.
        let delays = approval_recheck_delays();
        assert_eq!(delays.len(), 5);
        assert_eq!(delays.first(), Some(&Duration::from_secs(5)));
        assert_eq!(delays.last(), Some(&Duration::from_secs(120)));
        assert!(
            delays.windows(2).all(|w| w[0] < w[1]),
            "delays must be strictly increasing (bounded, no tight loop)"
        );
    }
}
