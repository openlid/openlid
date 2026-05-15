mod commands;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "openlid", version, about = "Keep your Mac awake.")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_menubar() {
        let cli = Cli::try_parse_from(["openlid", "menubar"]).unwrap();
        assert!(matches!(cli.command, Command::Menubar));
    }

    #[test]
    fn parses_helper() {
        let cli = Cli::try_parse_from(["openlid", "helper"]).unwrap();
        assert!(matches!(cli.command, Command::Helper));
    }

    #[test]
    fn parses_on() {
        let cli = Cli::try_parse_from(["openlid", "on"]).unwrap();
        assert!(matches!(cli.command, Command::On));
    }

    #[test]
    fn parses_off() {
        let cli = Cli::try_parse_from(["openlid", "off"]).unwrap();
        assert!(matches!(cli.command, Command::Off));
    }

    #[test]
    fn parses_status_default_is_human() {
        let cli = Cli::try_parse_from(["openlid", "status"]).unwrap();
        assert!(matches!(cli.command, Command::Status { json: false }));
    }

    #[test]
    fn parses_status_with_json_flag() {
        let cli = Cli::try_parse_from(["openlid", "status", "--json"]).unwrap();
        assert!(matches!(cli.command, Command::Status { json: true }));
    }

    #[test]
    fn parses_for_with_duration_arg() {
        let cli = Cli::try_parse_from(["openlid", "for", "2h"]).unwrap();
        match cli.command {
            Command::For { duration } => assert_eq!(duration, "2h"),
            other => panic!("expected For, got {other:?}"),
        }
    }

    #[test]
    fn parses_until_with_time_arg() {
        let cli = Cli::try_parse_from(["openlid", "until", "22:00"]).unwrap();
        match cli.command {
            Command::Until { time } => assert_eq!(time, "22:00"),
            other => panic!("expected Until, got {other:?}"),
        }
    }

    #[test]
    fn parses_config_show() {
        let cli = Cli::try_parse_from(["openlid", "config", "show"]).unwrap();
        assert!(matches!(cli.command, Command::Config(ConfigArg::Show)));
    }

    #[test]
    fn parses_config_path() {
        let cli = Cli::try_parse_from(["openlid", "config", "path"]).unwrap();
        assert!(matches!(cli.command, Command::Config(ConfigArg::Path)));
    }

    #[test]
    fn parses_config_edit() {
        let cli = Cli::try_parse_from(["openlid", "config", "edit"]).unwrap();
        assert!(matches!(cli.command, Command::Config(ConfigArg::Edit)));
    }

    #[test]
    fn rejects_config_without_subcommand() {
        assert!(Cli::try_parse_from(["openlid", "config"]).is_err());
    }

    #[test]
    fn rejects_unknown_subcommand() {
        assert!(Cli::try_parse_from(["openlid", "thisdoesnotexist"]).is_err());
    }

    #[test]
    fn rejects_for_without_duration_arg() {
        assert!(Cli::try_parse_from(["openlid", "for"]).is_err());
    }
}
