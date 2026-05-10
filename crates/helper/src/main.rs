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

use anyhow::Result;
use std::os::unix::io::AsRawFd;

fn main() -> Result<()> {
    setup_logging()?;
    guard_launched_by_launchd()?;
    tracing::info!("open-lid-helper starting (pid {})", std::process::id());

    // TODO Task 14: install XPC listener, run main loop.
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
