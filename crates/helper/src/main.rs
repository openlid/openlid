//! open-lid-helper — the privileged daemon.
//!
//! Loaded by launchd as root. Listens for NSXPC connections from the
//! menubar app, validates them by code requirement, toggles
//! `pmset -a disablesleep` on request, and self-exits after 15 s of
//! inactivity.

mod client_validator;
mod idle_exit;
mod ownership_marker;
mod pmset;
mod xpc_listener;

use anyhow::Result;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;

use pmset::Pmset;

const HELPER_MACH_SERVICE_NAME: &str = "io.openlid.helper";

// ─────────────────────────────────────────────────────────────────────────────
// Code-requirement string: who is allowed to send XPC requests to the helper.
//
// The helper validates every incoming connection's signing identity against
// this string (via SecRequirementCreateWithString + SecCodeCheckValidity).
// A mismatch causes the connection to be rejected silently.
//
// Two profiles are available; only one should be active at a time. The
// default is DEV until you have an Apple Developer Program membership and
// know your Team ID, at which point you switch to PROD.
// ─────────────────────────────────────────────────────────────────────────────

/// DEV — permissive. Only requires that the caller claim bundle identifier
/// `io.openlid.app`. Ad-hoc-signed local builds satisfy this. NEVER ship a
/// distributed build with DEV active — any locally compiled "io.openlid.app"
/// could control your root daemon.
#[allow(dead_code)]
const DEV_REQUIREMENT: &str = r#"identifier "io.openlid.app""#;

/// PROD — pins to bundle id + Apple-issued Developer ID Application cert
/// chain + your specific Team ID. To activate:
///   1. Replace `TEAMID` below with your actual 10-character Team ID
///      (e.g., `ABCD123456`). You can find it at
///      https://developer.apple.com/account → Membership → Team ID.
///   2. Decide whether you want notarization to be required (see below).
///   3. Switch the `validator = …` line further down to use PROD_REQUIREMENT.
///
/// The certificate field OIDs in this string are Apple-assigned:
///   • `1.2.840.113635.100.6.2.6`  — "Developer ID" intermediate CA
///   • `1.2.840.113635.100.6.1.13` — "Developer ID Application" leaf cert
///
/// Together they mean: "the binary must be signed by a Developer ID
/// Application certificate issued under Apple's Developer ID CA to my team."
#[allow(dead_code)]
const PROD_REQUIREMENT: &str = r#"identifier "io.openlid.app" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */ and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = "TEAMID""#;

fn main() -> Result<()> {
    setup_logging()?;
    guard_launched_by_launchd()?;
    tracing::info!("open-lid-helper starting (pid {})", std::process::id());

    let pmset = Arc::new(pmset::RealPmset);
    let marker = Arc::new(ownership_marker::OwnershipMarker::new());
    let validator = Arc::new(client_validator::ClientValidator::new(DEV_REQUIREMENT));
    let idle_exit = idle_exit::IdleExit::new();

    // Stale-state recovery: if the marker is present at startup, the previous
    // helper (or app) probably crashed without cleaning up. Restore normal
    // sleep behavior and remove the marker before accepting connections.
    if marker.exists() {
        tracing::warn!("ownership marker present at startup; restoring sleep");
        let _ = pmset.set_disable_sleep(false);
        let _ = marker.remove();
    }

    let helper = xpc_listener::HelperImpl {
        pmset,
        marker,
        idle_exit: idle_exit.clone(),
        validator,
    };

    // Initial arm: if no client connects within 15 s, exit.
    idle_exit.arm(|| {
        tracing::info!("idle-exit timer fired; exiting");
        std::process::exit(0);
    });

    xpc_listener::run_listener(helper, HELPER_MACH_SERVICE_NAME)?;
    Ok(())
}

fn setup_logging() -> Result<()> {
    use tracing_subscriber::EnvFilter;

    let log_dir = std::path::Path::new("/Library/Logs/open-lid");
    std::fs::create_dir_all(log_dir).ok();
    let file = tracing_appender::rolling::daily(log_dir, "helper.log");
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OPEN_LID_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(file)
        .with_ansi(false)
        .init();
    Ok(())
}

fn guard_launched_by_launchd() -> Result<()> {
    let ppid = unsafe { libc::getppid() };
    let stdin_is_tty = unsafe { libc::isatty(std::io::stdin().as_raw_fd()) } == 1;
    if ppid != 1 || stdin_is_tty {
        anyhow::bail!("open-lid-helper must be loaded by launchd, not invoked directly");
    }
    Ok(())
}
