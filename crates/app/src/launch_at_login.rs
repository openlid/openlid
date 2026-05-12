//! Register or unregister Open-Lid as a Login Item using `SMAppService.mainApp()`.
//!
//! Unlike daemon registration (which requires Apple Developer ID signing),
//! the main-app login-item path works for any app installed in /Applications,
//! including ad-hoc-signed local builds. macOS sees a request to autolaunch a
//! user-space app and validates only that the bundle exists and is launchable.
//!
//! When `start_at_login = true`:
//!   - We call `SMAppService.mainApp().register()`.
//!   - On success, the app appears in System Settings → General → Login Items
//!     with the toggle ON. macOS will launch it on next user login.
//!
//! When `start_at_login = false`:
//!   - We call `SMAppService.mainApp().unregister()`.
//!   - The entry remains in System Settings but the toggle goes OFF (so it
//!     won't auto-launch). Some macOS versions also remove the entry entirely.
//!
//! Errors are surfaced via `Result` but the state runtime treats them as
//! non-fatal — the preference still persists, the launch behavior just
//! doesn't take effect until the user re-tries (e.g., after moving the app
//! into /Applications).

use anyhow::{anyhow, Result};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::NSError;

/// Register the running app to launch at user login.
pub fn enable() -> Result<()> {
    call_register_or_unregister(/* register = */ true)
}

/// Cancel the previous registration. Safe to call even if not currently
/// registered.
pub fn disable() -> Result<()> {
    call_register_or_unregister(/* register = */ false)
}

fn call_register_or_unregister(register: bool) -> Result<()> {
    // SMAppService is part of the ServiceManagement framework. The class is
    // resolved by name; the `mainApp` factory returns an instance representing
    // the running app's bundle. Calling -register / -unregister on it returns
    // YES (true) on success and writes to an out-error pointer on failure.
    //
    // We avoid pulling in a heavy ServiceManagement crate because we only need
    // this one call site.
    let cls = objc2::runtime::AnyClass::get(c"SMAppService")
        .ok_or_else(|| anyhow!("SMAppService class not available (macOS 13+ required)"))?;

    // SAFETY: +mainApp is a documented class method returning a Retained
    // SMAppService instance.
    let service: Retained<AnyObject> = unsafe {
        let raw: *mut AnyObject = msg_send![cls, mainApp];
        Retained::from_raw(raw).ok_or_else(|| anyhow!("SMAppService.mainApp returned nil"))?
    };

    let mut err_ptr: *mut NSError = std::ptr::null_mut();
    // SAFETY: -registerAndReturnError: / -unregisterAndReturnError: both take
    // an inout NSError** and return BOOL (objc2::runtime::Bool).
    let ok: objc2::runtime::Bool = unsafe {
        if register {
            msg_send![&*service, registerAndReturnError: &mut err_ptr]
        } else {
            msg_send![&*service, unregisterAndReturnError: &mut err_ptr]
        }
    };

    if ok.as_bool() {
        tracing::info!(
            "launch-at-login: {} succeeded",
            if register { "register" } else { "unregister" }
        );
        return Ok(());
    }

    // Translate the NSError, if any, into a friendly message.
    let msg = if err_ptr.is_null() {
        format!(
            "SMAppService {} returned NO with no error info",
            if register { "register" } else { "unregister" }
        )
    } else {
        // SAFETY: err_ptr now owns an autoreleased NSError; retain it.
        let err = unsafe { Retained::retain(err_ptr) };
        match err {
            Some(e) => {
                let desc = e.localizedDescription();
                format!(
                    "SMAppService {} failed: {}",
                    if register { "register" } else { "unregister" },
                    desc
                )
            }
            None => "SMAppService failed: unknown".to_string(),
        }
    };

    Err(anyhow!(msg))
}
