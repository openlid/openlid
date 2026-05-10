mod cli;
mod helper_client;     // stub — Task 20 fills this in
mod menubar;
mod platform;
mod state_runtime;     // stub — Task 21 fills this in
mod control_server;    // stub — Task 23 fills this in

use anyhow::Result;
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("open-lid: {e:#}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    setup_logging()?;
    let args: Vec<String> = std::env::args().collect();

    let subcommand = args.get(1).map(String::as_str);
    match subcommand {
        None | Some("menubar") => menubar::run(),
        Some(_) => cli::run(args),
    }
}

fn setup_logging() -> Result<()> {
    use directories::ProjectDirs;
    use tracing_subscriber::EnvFilter;
    let dirs = ProjectDirs::from("io", "openlid", "open-lid")
        .ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    let log_dir = dirs.data_dir().parent().unwrap_or(dirs.data_dir()).join("Logs/open-lid");
    std::fs::create_dir_all(&log_dir).ok();
    let file = tracing_appender::rolling::daily(&log_dir, "app.log");
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OPEN_LID_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(file)
        .with_ansi(false)
        .init();
    Ok(())
}
