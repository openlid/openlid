use crate::cli::{ConfigArg, ScheduleArg};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone};
use interprocess::local_socket::{
    traits::Stream as StreamTrait, GenericFilePath, Stream, ToFsName,
};
use openlid_core::config::Config;
use openlid_core::ipc::control::{ControlRequest, ControlResponse, Snapshot};
use std::io::{BufRead, BufReader, Write};
use std::time::Duration as StdDuration;

fn socket_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("io", "openlid", "openlid")
        .ok_or_else(|| anyhow!("no home"))?;
    Ok(dirs.config_dir().join("control.sock"))
}

fn send_request(req: ControlRequest, auto_launch: bool) -> Result<ControlResponse> {
    let path = socket_path()?;
    let mut attempts = if auto_launch { 6 } else { 1 };
    let mut last_err = None;
    while attempts > 0 {
        match try_send(&path, &req) {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                last_err = Some(e);
                if auto_launch && attempts == 6 {
                    // Look up by bundle identifier (-b), not display name (-a).
                    // -a does a LaunchServices name match that could resolve
                    // to a different user-installed bundle named "OpenLid";
                    // -b matches CFBundleIdentifier which the user can't
                    // collide with for an unsigned/differently-signed app.
                    let _ = std::process::Command::new("/usr/bin/open")
                        .args(["-b", "io.openlid.app"])
                        .status();
                }
                std::thread::sleep(StdDuration::from_millis(500));
                attempts -= 1;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("failed to reach menubar process")))
}

fn try_send(path: &std::path::Path, req: &ControlRequest) -> Result<ControlResponse> {
    let name = path.to_path_buf().to_fs_name::<GenericFilePath>()?;
    let mut stream = Stream::connect(name)?;
    serde_json::to_writer(&mut stream, req)?;
    stream.write_all(b"\n")?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(serde_json::from_str(line.trim())?)
}

fn set(enabled: bool, until: Option<DateTime<Local>>) -> Result<ControlResponse> {
    send_request(ControlRequest::SetEnabled { enabled, until }, true)
}

fn print_set_result(resp: ControlResponse) -> Result<()> {
    match resp {
        ControlResponse::Ok { state } => {
            println!("{}", if state.enabled { "ON" } else { "OFF" });
            Ok(())
        }
        ControlResponse::Error { message } => Err(anyhow!(message)),
        _ => Err(anyhow!("unexpected response")),
    }
}

/// `openlid on` — activate using the user's Default duration preference.
/// If Default duration is None (indefinite), starts an unbounded session.
pub fn on_default_duration() -> Result<()> {
    // We have to ask the running app what the user's default is. The simplest
    // path: GetStatus first, read default_duration_minutes, then SetEnabled.
    let snap = match send_request(ControlRequest::GetStatus, true)? {
        ControlResponse::Ok { state } => state,
        ControlResponse::Error { message } => return Err(anyhow!(message)),
        _ => return Err(anyhow!("unexpected response")),
    };
    let until = snap
        .default_duration_minutes
        .map(|m| Local::now() + Duration::minutes(m as i64));
    print_set_result(set(true, until)?)
}

pub fn off() -> Result<()> {
    print_set_result(send_request(
        ControlRequest::SetEnabled {
            enabled: false,
            until: None,
        },
        true,
    )?)
}

pub fn status(json: bool) -> Result<()> {
    let resp = send_request(ControlRequest::GetStatus, false);
    match resp {
        Ok(ControlResponse::Ok { state }) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&state)?);
            } else {
                print!("{}", format_status_human(&state));
            }
            Ok(())
        }
        Ok(ControlResponse::Error { message }) => Err(anyhow!(message)),
        Ok(_) => Err(anyhow!("unexpected response")),
        Err(_) => {
            if json {
                println!("{}", serde_json::json!({"helper": "not-running"}));
                Ok(())
            } else {
                println!("Open-Lid is not running.");
                std::process::exit(1);
            }
        }
    }
}

fn format_status_human(s: &Snapshot) -> String {
    use std::fmt::Write;
    let state_label = match (s.enabled, s.preventing_sleep_now) {
        (false, _) => "OFF".to_string(),
        (true, true) => {
            if let Some(t) = s.until {
                format!("ON until {} (preventing sleep now)", t.format("%H:%M"))
            } else {
                "ON (preventing sleep now)".to_string()
            }
        }
        (true, false) => "ON (armed, idle)".to_string(),
    };
    let mut out = String::new();
    writeln!(out, "Sleep prevention: {state_label}").unwrap();
    writeln!(out, "Lid:              {:?}", s.lid).unwrap();
    writeln!(out, "Power:            {:?}", s.power).unwrap();
    if let Some(pct) = s.battery_threshold_pct {
        writeln!(out, "Auto-off below:   {pct}% battery").unwrap();
    }
    out
}

pub fn for_duration(s: &str) -> Result<()> {
    let dur = humantime::parse_duration(s).context("invalid duration")?;
    let until: DateTime<Local> = Local::now() + Duration::from_std(dur)?;
    print_set_result(set(true, Some(until))?)
}

pub fn until(s: &str) -> Result<()> {
    let until = parse_until(s)?;
    print_set_result(set(true, Some(until))?)
}

fn parse_until(s: &str) -> Result<DateTime<Local>> {
    parse_until_at(Local::now(), s)
}

fn parse_until_at(now: DateTime<Local>, s: &str) -> Result<DateTime<Local>> {
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M") {
        let today = now.date_naive().and_time(t);
        let dt = Local.from_local_datetime(&today).single().unwrap();
        return Ok(if dt > now { dt } else { dt + Duration::days(1) });
    }
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .context("expected HH:MM or YYYY-MM-DDTHH:MM")?;
    Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| anyhow!("ambiguous local time"))
}

pub fn schedule(_s: ScheduleArg) -> Result<()> {
    // Implemented in the next commit. The parser layer is in place so the
    // help text and shell completion are usable; invoking any subcommand
    // produces a clear error until then.
    Err(anyhow!("schedule subcommand not yet implemented"))
}

pub fn config(c: ConfigArg) -> Result<()> {
    // One-shot v1 → v2 migration on first use after upgrade. No-op once v2
    // config exists. Done here too (not just on menubar launch) so a user
    // who first invokes the CLI before the GUI still gets their settings.
    let path = Config::migrate_v1_to_v2()?;
    match c {
        ConfigArg::Path => {
            println!("{}", path.display());
        }
        ConfigArg::Show => {
            let cfg = Config::load(&path)?;
            println!("{}", toml::to_string_pretty(&cfg)?);
        }
        ConfigArg::Edit => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "open".into());
            std::process::Command::new(&editor).arg(&path).status()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlid_core::ipc::control::HelperStatus;
    use openlid_core::mode::{LidState, Modifiers, PowerSource};

    // ─────────────────────────────────────────────────────────────────────
    // parse_until_at — pure date logic with an injected `now`
    // ─────────────────────────────────────────────────────────────────────

    fn fixed_now() -> DateTime<Local> {
        // Wednesday 2026-05-14, 12:00:00 local. Well clear of midnight,
        // and the date sits outside any DST transition for IANA TZs on
        // any plausible CI runner.
        Local.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap()
    }

    #[test]
    fn parse_until_at_hhmm_in_future_today_rolls_into_today() {
        let got = parse_until_at(fixed_now(), "18:00").unwrap();
        assert_eq!(got, Local.with_ymd_and_hms(2026, 5, 14, 18, 0, 0).unwrap());
    }

    #[test]
    fn parse_until_at_hhmm_in_past_today_rolls_into_tomorrow() {
        let got = parse_until_at(fixed_now(), "09:00").unwrap();
        assert_eq!(got, Local.with_ymd_and_hms(2026, 5, 15, 9, 0, 0).unwrap());
    }

    #[test]
    fn parse_until_at_hhmm_equal_to_now_rolls_into_tomorrow() {
        // Current behavior is `dt > now`; equality lands in the else branch.
        let got = parse_until_at(fixed_now(), "12:00").unwrap();
        assert_eq!(got, Local.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap());
    }

    #[test]
    fn parse_until_at_full_iso_returns_that_exact_datetime() {
        let got = parse_until_at(fixed_now(), "2026-12-25T08:30").unwrap();
        assert_eq!(got, Local.with_ymd_and_hms(2026, 12, 25, 8, 30, 0).unwrap());
    }

    #[test]
    fn parse_until_at_invalid_string_returns_error_with_format_hint() {
        let err = parse_until_at(fixed_now(), "not a time").unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("expected HH:MM"),
            "unexpected error message: {msg}",
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_status_human — pure formatter
    // ─────────────────────────────────────────────────────────────────────

    fn snap() -> Snapshot {
        Snapshot {
            preventing_sleep_now: false,
            enabled: false,
            until: None,
            modifiers: Modifiers::default(),
            lid: LidState::Open,
            power: PowerSource::Ac,
            helper: HelperStatus::Running,
            start_at_login: false,
            activate_at_launch: false,
            default_duration_minutes: None,
            battery_threshold_pct: None,
            prevent_display_sleep: false,
        }
    }

    #[test]
    fn format_status_off() {
        let out = format_status_human(&snap());
        assert!(out.contains("Sleep prevention: OFF"));
        assert!(out.contains("Lid:              Open"));
        assert!(out.contains("Power:            Ac"));
        assert!(!out.contains("Auto-off below"));
    }

    #[test]
    fn format_status_on_preventing_now_with_until() {
        let mut s = snap();
        s.enabled = true;
        s.preventing_sleep_now = true;
        s.until = Some(Local.with_ymd_and_hms(2026, 5, 14, 22, 30, 0).unwrap());
        let out = format_status_human(&s);
        assert!(
            out.contains("Sleep prevention: ON until 22:30 (preventing sleep now)"),
            "got: {out}",
        );
    }

    #[test]
    fn format_status_on_preventing_now_indefinite_has_no_until() {
        let mut s = snap();
        s.enabled = true;
        s.preventing_sleep_now = true;
        s.until = None;
        let out = format_status_human(&s);
        assert!(out.contains("Sleep prevention: ON (preventing sleep now)"));
        assert!(!out.contains("until"));
    }

    #[test]
    fn format_status_on_armed_but_idle() {
        let mut s = snap();
        s.enabled = true;
        s.preventing_sleep_now = false;
        let out = format_status_human(&s);
        assert!(out.contains("Sleep prevention: ON (armed, idle)"));
    }

    #[test]
    fn format_status_includes_battery_threshold_when_set() {
        let mut s = snap();
        s.battery_threshold_pct = Some(20);
        let out = format_status_human(&s);
        assert!(out.contains("Auto-off below:   20% battery"), "got: {out}");
    }

    #[test]
    fn format_status_reflects_lid_and_power_debug_repr() {
        let mut s = snap();
        s.lid = LidState::Closed;
        s.power = PowerSource::Battery { percent: 73 };
        let out = format_status_human(&s);
        assert!(out.contains("Lid:              Closed"));
        assert!(
            out.contains("Power:            Battery { percent: 73 }"),
            "got: {out}",
        );
    }

    // Skip this test when a real v1 config file lives at the legacy path —
    // `config()` calls `migrate_v1_to_v2` which would actually migrate it
    // into the developer's real v2 dir. CI always passes this guard.
    fn has_real_v1_config() -> bool {
        Config::v1_legacy_path().is_some_and(|p| p.exists())
    }

    #[test]
    fn config_path_command_does_not_error() {
        if has_real_v1_config() {
            eprintln!("skipping: real v1 config exists on this machine");
            return;
        }
        // `ConfigArg::Path` just prints the path — no editor, no read of
        // the file. It exercises the `migrate_v1_to_v2 → match → Path arm`
        // path which is otherwise uncovered.
        config(ConfigArg::Path).expect("config path arm should succeed");
    }
}
