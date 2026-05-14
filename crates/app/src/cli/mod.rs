mod commands;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "open-lid", version, about = "Keep your Mac awake.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run as menubar app
    Menubar,
    /// Run as privileged helper (used by launchd)
    Helper,
    /// Turn on sleep prevention (uses Default duration from Preferences)
    On,
    /// Turn off sleep prevention
    Off,
    /// Show current status
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Activate with a timer for a duration (e.g., 2h, 30m, 1h30m)
    For { duration: String },
    /// Activate with a timer until a wall-clock time (HH:MM or YYYY-MM-DDTHH:MM)
    Until { time: String },
    /// Config operations
    #[command(subcommand)]
    Config(ConfigArg),
}

#[derive(clap::Subcommand, Debug)]
pub enum ConfigArg {
    Show,
    Path,
    Edit,
}

pub fn run(args: Vec<String>) -> Result<()> {
    let cli = Cli::try_parse_from(&args)?;
    match cli.command {
        Command::Menubar => crate::menubar::run(),
        Command::Helper => anyhow::bail!("the 'helper' role is for launchd only"),
        Command::On => commands::on_default_duration(),
        Command::Off => commands::off(),
        Command::Status { json } => commands::status(json),
        Command::For { duration } => commands::for_duration(&duration),
        Command::Until { time } => commands::until(&time),
        Command::Config(c) => commands::config(c),
    }
}
