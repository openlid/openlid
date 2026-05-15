mod cli;
mod control_server;
mod helper_client;
mod helper_installer;
mod launch_at_login;
mod main_thread;
mod menubar;
mod platform;
mod state_runtime;

use anyhow::Result;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("openlid: {e:#}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    setup_logging()?;
    let args: Vec<String> = std::env::args().collect();

    let subcommand = args.get(1).map(String::as_str);
    match subcommand {
        // Explicit `menubar` always runs foreground — the .app bundle uses this.
        Some("menubar") => menubar::run(),
        Some(_) => cli::run(args),
        None => dispatch_no_args(),
    }
}

/// `openlid` invoked with no arguments. Choose between:
///   * Foreground menubar — when launched from inside an .app bundle
///     (LSUIElement) or any non-TTY context (launchd, supervisors).
///   * Detach to background — when invoked from an interactive shell. We
///     re-spawn ourselves with `menubar` in a new session so the calling
///     terminal stays free.
fn dispatch_no_args() -> Result<()> {
    if is_running_from_app_bundle() || !is_stdin_a_tty() {
        return menubar::run();
    }
    spawn_self_in_background()
}

fn is_running_from_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.contains(".app/Contents/MacOS/")))
        .unwrap_or(false)
}

fn is_stdin_a_tty() -> bool {
    use std::os::unix::io::AsRawFd;
    unsafe { libc::isatty(std::io::stdin().as_raw_fd()) == 1 }
}

fn spawn_self_in_background() -> Result<()> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("menubar")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // setsid() detaches the child from the controlling terminal so it
    // survives shell exit and doesn't get SIGHUP when the terminal closes.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    println!("openlid started in background (pid {}).", child.id());
    println!("  openlid status   - show state");
    println!("  openlid off      - disable");
    println!("  pkill openlid    - stop the menubar app");
    Ok(())
}

fn setup_logging() -> Result<()> {
    use directories::ProjectDirs;
    use tracing_subscriber::EnvFilter;
    let dirs = ProjectDirs::from("io", "openlid", "openlid")
        .ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    let log_dir = dirs
        .data_dir()
        .parent()
        .unwrap_or(dirs.data_dir())
        .join("Logs/openlid");
    std::fs::create_dir_all(&log_dir).ok();
    let file = tracing_appender::rolling::daily(&log_dir, "app.log");
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("OPEN_LID_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(file)
        .with_ansi(false)
        .init();
    Ok(())
}
