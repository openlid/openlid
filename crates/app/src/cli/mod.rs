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
    /// Turn on sleep prevention (indefinite — use `schedule` for time windows)
    On,
    /// Turn off sleep prevention
    Off,
    /// Show current status
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Config operations
    #[command(subcommand)]
    Config(ConfigArg),
    /// Schedule operations — set, clear, or inspect a recurring time window
    #[command(subcommand)]
    Schedule(ScheduleArg),
    /// Check for and install a newer release. Homebrew installs are
    /// routed to Homebrew's cask upgrade command; manual installs are handled
    /// in-process.
    Update(UpdateArg),
}

#[derive(clap::Subcommand, Debug)]
pub enum ConfigArg {
    Show,
    Path,
    Edit,
}

#[derive(clap::Subcommand, Debug)]
pub enum ScheduleArg {
    /// Set the recurring window (e.g. `--from 08:00 --to 18:00`).
    /// Setting a schedule also turns sleep prevention ON if it's off.
    Set {
        /// Start of the window, HH:MM (24h).
        #[arg(long)]
        from: String,
        /// End of the window, HH:MM (24h). If `--to` <= `--from`, the window
        /// crosses midnight (e.g. `--from 22:00 --to 02:00`).
        #[arg(long)]
        to: String,
        /// Comma-separated days (case-insensitive): Mon,Tue,Wed,Thu,Fri,Sat,Sun.
        /// Omit for every day of the week.
        #[arg(long)]
        days: Option<String>,
    },
    /// Remove the schedule. Leaves the on/off toggle untouched.
    Clear,
    /// Print the current schedule.
    Show {
        #[arg(long)]
        json: bool,
    },
}

#[derive(clap::Args, Debug)]
pub struct UpdateArg {
    /// Only check; never install. Exits 0 when up to date, 1 when an
    /// update is available, 2+ on any error reaching the update server.
    #[arg(long)]
    pub check: bool,
    /// Non-interactive: skip the confirm prompt. Honored only for
    /// manual installs (Homebrew installs always exit with an
    /// instruction; dev builds are always refused).
    #[arg(long)]
    pub yes: bool,
    /// Emit machine-readable JSON status. Does not affect the
    /// install path -- `--json` alone is equivalent to `--check
    /// --json`.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: Vec<String>) -> Result<()> {
    let cli = Cli::try_parse_from(&args)?;
    match cli.command {
        Command::Menubar => crate::menubar::run(),
        Command::Helper => anyhow::bail!("the 'helper' role is for launchd only"),
        Command::On => commands::on(),
        Command::Off => commands::off(),
        Command::Status { json } => commands::status(json),
        Command::Config(c) => commands::config(c),
        Command::Schedule(s) => commands::schedule(s),
        Command::Update(arg) => commands::update(arg),
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
    fn parses_schedule_set_with_all_flags() {
        let cli = Cli::try_parse_from([
            "openlid",
            "schedule",
            "set",
            "--from",
            "09:00",
            "--to",
            "18:00",
            "--days",
            "Mon,Tue,Wed,Thu,Fri",
        ])
        .unwrap();
        match cli.command {
            Command::Schedule(ScheduleArg::Set { from, to, days }) => {
                assert_eq!(from, "09:00");
                assert_eq!(to, "18:00");
                assert_eq!(days.as_deref(), Some("Mon,Tue,Wed,Thu,Fri"));
            }
            other => panic!("expected Schedule(Set), got {other:?}"),
        }
    }

    #[test]
    fn parses_schedule_set_without_days_leaves_days_none() {
        // The default-to-all-days behavior lives in the CLI command layer,
        // not the parser. Parser must keep `days` as `None` so the command
        // layer can distinguish "user omitted" from "user passed empty".
        let cli = Cli::try_parse_from([
            "openlid", "schedule", "set", "--from", "08:00", "--to", "18:00",
        ])
        .unwrap();
        match cli.command {
            Command::Schedule(ScheduleArg::Set { days, .. }) => assert!(days.is_none()),
            other => panic!("expected Schedule(Set), got {other:?}"),
        }
    }

    #[test]
    fn parses_schedule_clear() {
        let cli = Cli::try_parse_from(["openlid", "schedule", "clear"]).unwrap();
        assert!(matches!(cli.command, Command::Schedule(ScheduleArg::Clear)));
    }

    #[test]
    fn parses_schedule_show_default_human() {
        let cli = Cli::try_parse_from(["openlid", "schedule", "show"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Schedule(ScheduleArg::Show { json: false })
        ));
    }

    #[test]
    fn parses_schedule_show_with_json_flag() {
        let cli = Cli::try_parse_from(["openlid", "schedule", "show", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Schedule(ScheduleArg::Show { json: true })
        ));
    }

    #[test]
    fn rejects_schedule_set_without_from() {
        assert!(Cli::try_parse_from(["openlid", "schedule", "set", "--to", "18:00"]).is_err());
    }

    #[test]
    fn rejects_schedule_set_without_to() {
        assert!(Cli::try_parse_from(["openlid", "schedule", "set", "--from", "08:00"]).is_err());
    }

    #[test]
    fn rejects_schedule_without_subcommand() {
        assert!(Cli::try_parse_from(["openlid", "schedule"]).is_err());
    }

    #[test]
    fn parses_update_default_flags_all_false() {
        let cli = Cli::try_parse_from(["openlid", "update"]).unwrap();
        match cli.command {
            Command::Update(a) => {
                assert!(!a.check);
                assert!(!a.yes);
                assert!(!a.json);
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn parses_update_with_check_flag() {
        let cli = Cli::try_parse_from(["openlid", "update", "--check"]).unwrap();
        match cli.command {
            Command::Update(a) => {
                assert!(a.check);
                assert!(!a.yes);
                assert!(!a.json);
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn parses_update_with_yes_flag() {
        let cli = Cli::try_parse_from(["openlid", "update", "--yes"]).unwrap();
        match cli.command {
            Command::Update(a) => {
                assert!(!a.check);
                assert!(a.yes);
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn parses_update_with_json_flag() {
        let cli = Cli::try_parse_from(["openlid", "update", "--json"]).unwrap();
        match cli.command {
            Command::Update(a) => assert!(a.json),
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn parses_update_with_all_flags() {
        // All three flags coexist at the parser level -- the conflict
        // between --check and --yes is enforced in the command layer
        // (so the error message can explain *why* they conflict).
        let cli = Cli::try_parse_from(["openlid", "update", "--check", "--yes", "--json"]).unwrap();
        match cli.command {
            Command::Update(a) => {
                assert!(a.check);
                assert!(a.yes);
                assert!(a.json);
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }
}
