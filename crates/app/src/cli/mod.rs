mod commands;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "open-lid", version, about = "Prevent macOS sleep on lid close.")]
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
    /// Turn on sleep prevention with current mode
    On,
    /// Turn off sleep prevention
    Off,
    /// Show current status
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Switch mode
    Mode {
        #[arg(value_enum)]
        mode: ModeArg,
    },
    /// Switch to Timed mode for a duration (e.g., 2h, 30m)
    For {
        duration: String,
    },
    /// Switch to Timed mode until a time (HH:MM or YYYY-MM-DDTHH:MM)
    Until {
        time: String,
    },
    /// Modifier operations (MVP placeholder; full impl in Plan 2)
    #[command(subcommand)]
    Modifier(ModifierArg),
    /// Config operations
    #[command(subcommand)]
    Config(ConfigArg),
    /// Uninstall (MVP placeholder; full impl in Plan 2)
    Uninstall,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ModeArg {
    LidClosed,
    AlwaysAwake,
}

#[derive(clap::Subcommand, Debug)]
pub enum ModifierArg {
    OnlyOnAc { value: BoolArg },
    MinBattery { value: String },
    Schedule { value: BoolArg },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum BoolArg { On, Off }

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
        Command::On => commands::set_enabled(true),
        Command::Off => commands::set_enabled(false),
        Command::Status { json } => commands::status(json),
        Command::Mode { mode } => commands::set_mode(mode),
        Command::For { duration } => commands::for_duration(&duration),
        Command::Until { time } => commands::until(&time),
        Command::Modifier(m) => commands::modifier(m),
        Command::Config(c) => commands::config(c),
        Command::Uninstall => commands::uninstall(),
    }
}
