//! Register and check status of the privileged helper via
//! `SMAppService.daemon(plistName:)`.
//!
//! macOS 13+ introduced `SMAppService` as the modern replacement for the
//! deprecated `SMJobBless` flow. The daemon factory installs a system-domain
//! launchd job from a plist embedded inside the app bundle at
//! `Contents/Library/LaunchDaemons/<plist-name>`. macOS validates the
//! bundle's code signature + Team ID matches the helper plist's
//! `AssociatedBundleIdentifiers` before accepting the registration.
//!
//! User flow:
//!
//!   1. App calls [`register`] → returns Ok if accepted by macOS.
//!   2. The helper enters the "Requires Approval" state — visible in
//!      System Settings → General → Login Items → Allow in the Background.
//!   3. The user flips the toggle ON.
//!   4. launchd starts the helper as root on next XPC connection.
//!
//! [`status`] reports which of those phases we're currently in. The menubar
//! app calls `status()` after every `register()` to decide whether to nudge
//! the user toward System Settings.
//!
//! Failures here are user-actionable, not fatal. The most common reasons
//! for register to fail are:
//!   - App is not installed in /Applications (macOS rejects daemon installs
//!     from arbitrary paths to prevent a malicious downloaded app from
//!     installing a root daemon)
//!   - Code signature doesn't match the Team ID baked into the helper's
//!     code-requirement string
//!   - The .app is unsigned or ad-hoc-signed (Developer ID required for
//!     daemon registration; ad-hoc works only for the main-app login item)

use anyhow::{anyhow, Result};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSError, NSString};

/// Filename of the helper plist embedded in `OpenLid.app/Contents/Library/LaunchDaemons/`.
/// MUST match what `scripts/build-app-bundle.sh` copies into the bundle.
const HELPER_PLIST_NAME: &str = "io.openlid.helper.plist";

/// Status of the helper's `SMAppService` registration as of right now.
/// Maps to Apple's `SMAppServiceStatus` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperServiceStatus {
    /// Never registered, or unregister succeeded. `register()` is the next step.
    NotRegistered,
    /// Registered but the user hasn't enabled it in System Settings yet.
    /// The menubar should surface a "Open System Settings" hint.
    RequiresApproval,
    /// Enabled and ready to receive XPC connections.
    Enabled,
    /// macOS couldn't find the plist or the bundle layout is wrong.
    /// Usually indicates the .app isn't in /Applications, or the build
    /// pipeline skipped copying the plist into Contents/Library/LaunchDaemons.
    NotFound,
    /// Apple may add new states in future macOS versions; we surface them
    /// as a numeric fallback rather than crashing.
    Unknown(i64),
}

impl HelperServiceStatus {
    fn from_raw(raw: i64) -> Self {
        // Per Apple's SMAppServiceStatus header:
        //   0 = .notRegistered
        //   1 = .enabled
        //   2 = .requiresApproval
        //   3 = .notFound
        match raw {
            0 => Self::NotRegistered,
            1 => Self::Enabled,
            2 => Self::RequiresApproval,
            3 => Self::NotFound,
            other => Self::Unknown(other),
        }
    }
}

/// Register the privileged helper daemon. After success, the user must
/// approve it in System Settings → Login Items before launchd will run it.
pub fn register() -> Result<()> {
    let service = daemon_service()?;
    let mut err_ptr: *mut NSError = std::ptr::null_mut();
    // SAFETY: -registerAndReturnError: is the documented SMAppService API.
    let ok: objc2::runtime::Bool =
        unsafe { msg_send![&*service, registerAndReturnError: &mut err_ptr] };

    if ok.as_bool() {
        tracing::info!("SMAppService daemon register: ok");
        return Ok(());
    }
    Err(unwrap_error(err_ptr, "register"))
}

/// Unregister the helper. Called from the uninstall path.
#[allow(dead_code)] // Wired in the uninstall command (v0.2.x menu + CLI).
pub fn unregister() -> Result<()> {
    let service = daemon_service()?;
    let mut err_ptr: *mut NSError = std::ptr::null_mut();
    // SAFETY: -unregisterAndReturnError: is the documented SMAppService API.
    let ok: objc2::runtime::Bool =
        unsafe { msg_send![&*service, unregisterAndReturnError: &mut err_ptr] };

    if ok.as_bool() {
        tracing::info!("SMAppService daemon unregister: ok");
        return Ok(());
    }
    Err(unwrap_error(err_ptr, "unregister"))
}

/// Read the current status. Cheap; safe to call frequently (e.g., on every
/// status menu refresh) — macOS caches internally.
pub fn status() -> Result<HelperServiceStatus> {
    let service = daemon_service()?;
    // SAFETY: -status returns SMAppServiceStatus (NSInteger).
    let raw: i64 = unsafe { msg_send![&*service, status] };
    Ok(HelperServiceStatus::from_raw(raw))
}

/// Open System Settings on the Login Items pane so the user can flip the
/// approval toggle. Returns immediately; the system opens the pane async.
#[allow(dead_code)] // Wired into the preferences window in v0.2.x (status row hint).
pub fn open_system_settings_login_items() -> Result<()> {
    let cls = objc2::runtime::AnyClass::get(c"SMAppService")
        .ok_or_else(|| anyhow!("SMAppService class not available (macOS 13+ required)"))?;
    // SAFETY: +openSystemSettingsLoginItems is documented on macOS 13+.
    let _: () = unsafe { msg_send![cls, openSystemSettingsLoginItems] };
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────────────

fn daemon_service() -> Result<Retained<AnyObject>> {
    let cls = objc2::runtime::AnyClass::get(c"SMAppService")
        .ok_or_else(|| anyhow!("SMAppService class not available (macOS 13+ required)"))?;
    let plist_name = NSString::from_str(HELPER_PLIST_NAME);
    // SAFETY: +daemonWithPlistName: returns a Retained SMAppService.
    let service: Retained<AnyObject> = unsafe {
        let raw: *mut AnyObject = msg_send![cls, daemonWithPlistName: &*plist_name];
        Retained::from_raw(raw)
            .ok_or_else(|| anyhow!("SMAppService.daemon(plistName:) returned nil"))?
    };
    Ok(service)
}

fn unwrap_error(err_ptr: *mut NSError, op: &str) -> anyhow::Error {
    if err_ptr.is_null() {
        return anyhow!("SMAppService {op} returned NO with no error info");
    }
    // SAFETY: err_ptr now owns an autoreleased NSError; retain it.
    let err = unsafe { Retained::retain(err_ptr) };
    match err {
        Some(e) => {
            let desc = e.localizedDescription();
            anyhow!("SMAppService {op} failed: {desc}")
        }
        None => anyhow!("SMAppService {op} failed: unknown"),
    }
}
