use crate::cli::{ConfigArg, ScheduleArg};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Local, NaiveTime};
use interprocess::local_socket::{
    traits::Stream as StreamTrait, GenericFilePath, Stream, ToFsName,
};
use openlid_core::config::Config;
use openlid_core::ipc::control::{ControlRequest, ControlResponse, Snapshot};
use openlid_core::mode::{DaysOfWeek, Schedule};
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

/// `openlid on` — start an indefinite (no-timer) session.
pub fn on() -> Result<()> {
    print_set_result(set(true, None)?)
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
                println!("OpenLid is not running.");
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
    if let Some(sched) = &s.modifiers.schedule {
        writeln!(out, "Schedule:         {}", format_schedule_inline(sched)).unwrap();
    }
    out
}

/// One-liner rendering of a schedule, used by the status output and
/// `schedule show`. Format: `HH:MM-HH:MM (<day-summary>)`.
fn format_schedule_inline(s: &Schedule) -> String {
    format!(
        "{}-{} ({})",
        s.start.format("%H:%M"),
        s.end.format("%H:%M"),
        day_summary(s.days),
    )
}

/// Compact, human-friendly summary of a `DaysOfWeek` set:
/// * all seven   -> "daily"
/// * Mon-Fri exactly -> "Mon-Fri"
/// * Sat,Sun exactly -> "weekends"
/// * otherwise -> three-letter names in Mon->Sun order, comma-separated.
fn day_summary(days: DaysOfWeek) -> String {
    if days == DaysOfWeek::all() {
        return "daily".to_string();
    }
    let weekdays =
        DaysOfWeek::MON | DaysOfWeek::TUE | DaysOfWeek::WED | DaysOfWeek::THU | DaysOfWeek::FRI;
    if days == weekdays {
        return "Mon-Fri".to_string();
    }
    if days == DaysOfWeek::SAT | DaysOfWeek::SUN {
        return "weekends".to_string();
    }
    const ALL: &[(DaysOfWeek, &str)] = &[
        (DaysOfWeek::MON, "Mon"),
        (DaysOfWeek::TUE, "Tue"),
        (DaysOfWeek::WED, "Wed"),
        (DaysOfWeek::THU, "Thu"),
        (DaysOfWeek::FRI, "Fri"),
        (DaysOfWeek::SAT, "Sat"),
        (DaysOfWeek::SUN, "Sun"),
    ];
    let names: Vec<&str> = ALL
        .iter()
        .filter_map(|(f, n)| days.contains(*f).then_some(*n))
        .collect();
    names.join(", ")
}

/// Parse a comma-separated list of three-letter day names into a
/// `DaysOfWeek` bitflag set. Case-insensitive; whitespace around tokens is
/// trimmed. An empty input is an error rather than `empty()` so a typo
/// like `--days ""` can't silently disable the schedule by matching no day.
fn parse_days_csv(s: &str) -> Result<DaysOfWeek> {
    let mut days = DaysOfWeek::empty();
    let mut saw_any = false;
    for token in s.split(',') {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }
        saw_any = true;
        let flag = match t.to_ascii_lowercase().as_str() {
            "mon" => DaysOfWeek::MON,
            "tue" => DaysOfWeek::TUE,
            "wed" => DaysOfWeek::WED,
            "thu" => DaysOfWeek::THU,
            "fri" => DaysOfWeek::FRI,
            "sat" => DaysOfWeek::SAT,
            "sun" => DaysOfWeek::SUN,
            other => {
                return Err(anyhow!(
                    "unknown day '{other}' (expected Mon, Tue, Wed, Thu, Fri, Sat, or Sun)"
                ));
            }
        };
        days |= flag;
    }
    if !saw_any {
        return Err(anyhow!(
            "at least one day required (e.g. --days Mon,Tue,Wed,Thu,Fri)"
        ));
    }
    Ok(days)
}

/// Build a `Schedule` from raw CLI arg strings. Validates that the window is
/// non-empty (start != end). Start > end is *allowed* and signals a window
/// that crosses midnight — `Schedule::contains` already handles that.
fn parse_schedule(from: &str, to: &str, days: Option<&str>) -> Result<Schedule> {
    let start = NaiveTime::parse_from_str(from, "%H:%M").context("expected HH:MM for --from")?;
    let end = NaiveTime::parse_from_str(to, "%H:%M").context("expected HH:MM for --to")?;
    if start == end {
        return Err(anyhow!(
            "schedule window must be non-empty (--from must differ from --to)"
        ));
    }
    let days = match days {
        Some(s) => parse_days_csv(s)?,
        None => DaysOfWeek::all(),
    };
    Ok(Schedule { days, start, end })
}

pub fn schedule(arg: ScheduleArg) -> Result<()> {
    match arg {
        ScheduleArg::Set { from, to, days } => schedule_set(&from, &to, days.as_deref()),
        ScheduleArg::Clear => schedule_clear(),
        ScheduleArg::Show { json } => schedule_show(json),
    }
}

/// Reduce a `ControlResponse` to the embedded snapshot, mapping the
/// `Error` and unexpected-variant branches to anyhow errors. Pure so the
/// three response shapes can be exercised without standing up the IPC.
fn extract_snapshot(resp: ControlResponse) -> Result<Snapshot> {
    match resp {
        ControlResponse::Ok { state } => Ok(state),
        ControlResponse::Error { message } => Err(anyhow!(message)),
        _ => Err(anyhow!("unexpected response")),
    }
}

/// Assert a `ControlResponse` is `Ok` for callers that don't care about
/// the embedded snapshot. Same error mapping as `extract_snapshot`.
fn expect_ok(resp: ControlResponse) -> Result<()> {
    match resp {
        ControlResponse::Ok { .. } => Ok(()),
        ControlResponse::Error { message } => Err(anyhow!(message)),
        _ => Err(anyhow!("unexpected response")),
    }
}

/// Build the IPC request that sets `modifiers.schedule = Some(s)` while
/// leaving every other preference field untouched.
fn build_set_schedule_request(sched: Schedule) -> ControlRequest {
    ControlRequest::SetPreferences {
        start_at_login: None,
        activate_at_launch: None,
        battery_threshold_pct: None,
        prevent_display_sleep: None,
        schedule: Some(Some(sched)),
    }
}

/// Build the IPC request that clears the schedule. The `Some(None)` is
/// the wire-level distinction from "leave alone" (`None`) -- see the
/// custom deserializer on the request variant.
fn build_clear_schedule_request() -> ControlRequest {
    ControlRequest::SetPreferences {
        start_at_login: None,
        activate_at_launch: None,
        battery_threshold_pct: None,
        prevent_display_sleep: None,
        schedule: Some(None),
    }
}

fn schedule_set(from: &str, to: &str, days: Option<&str>) -> Result<()> {
    let sched = parse_schedule(from, to, days)?;
    let snapshot = extract_snapshot(send_request(
        build_set_schedule_request(sched.clone()),
        true,
    )?)?;
    // Implicit-enable bridge: turn the toggle on if it was off, so the
    // newly-persisted schedule has an enabled state to gate. See the
    // design doc for the rationale.
    let was_off = !snapshot.enabled;
    if was_off {
        expect_ok(send_request(
            ControlRequest::SetEnabled {
                enabled: true,
                until: None,
            },
            true,
        )?)?;
    }
    println!("{}", format_schedule_set_confirmation(&sched, was_off));
    Ok(())
}

/// Human-readable confirmation printed after a successful `schedule set`.
/// Pure so the implicit-enable bridge's user-visible contract can be
/// regression-tested without standing up the IPC machinery.
fn format_schedule_set_confirmation(sched: &Schedule, was_off: bool) -> String {
    format!(
        "Schedule: {}{}",
        format_schedule_inline(sched),
        if was_off { "; openlid is now ON" } else { "" }
    )
}

fn schedule_clear() -> Result<()> {
    expect_ok(send_request(build_clear_schedule_request(), true)?)?;
    println!("Schedule cleared.");
    Ok(())
}

fn schedule_show(json: bool) -> Result<()> {
    let snap = extract_snapshot(send_request(ControlRequest::GetStatus, true)?)?;
    let sched = snap.modifiers.schedule.as_ref();
    if json {
        println!("{}", format_schedule_show_json(sched)?);
    } else {
        println!("{}", format_schedule_show_text(sched));
    }
    Ok(())
}

/// Human-readable rendering of `schedule show`. `None` reports the
/// not-configured state explicitly rather than printing an empty line so
/// the output is unambiguous in shell pipelines and screen-reader contexts.
fn format_schedule_show_text(sched: Option<&Schedule>) -> String {
    match sched {
        Some(s) => format!("Schedule: {}", format_schedule_inline(s)),
        None => "No schedule set.".to_string(),
    }
}

/// JSON rendering of `schedule show --json`. Serializes the bare modifier
/// value (or `null`), so the output is identical to
/// `openlid status --json | jq .modifiers.schedule` -- a contract this
/// test pins against accidental wrapping in some envelope.
fn format_schedule_show_json(sched: Option<&Schedule>) -> Result<String> {
    Ok(serde_json::to_string_pretty(&sched)?)
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
    use chrono::TimeZone;
    use openlid_core::ipc::control::HelperStatus;
    use openlid_core::mode::{LidState, Modifiers, PowerSource};

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

    // ─────────────────────────────────────────────────────────────────────
    // parse_days_csv — comma-separated three-letter day names, case-insensitive
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_days_csv_uppercase() {
        use openlid_core::mode::DaysOfWeek;
        let got = parse_days_csv("Mon,Tue,Wed,Thu,Fri").unwrap();
        let want =
            DaysOfWeek::MON | DaysOfWeek::TUE | DaysOfWeek::WED | DaysOfWeek::THU | DaysOfWeek::FRI;
        assert_eq!(got, want);
    }

    #[test]
    fn parse_days_csv_case_insensitive() {
        use openlid_core::mode::DaysOfWeek;
        // Case mixing is the common user mistake: "mon,TUE,WeD" should parse.
        let got = parse_days_csv("mon,TUE,WeD").unwrap();
        assert_eq!(got, DaysOfWeek::MON | DaysOfWeek::TUE | DaysOfWeek::WED);
    }

    #[test]
    fn parse_days_csv_trims_whitespace() {
        use openlid_core::mode::DaysOfWeek;
        let got = parse_days_csv(" Mon , Fri ").unwrap();
        assert_eq!(got, DaysOfWeek::MON | DaysOfWeek::FRI);
    }

    #[test]
    fn parse_days_csv_all_seven_returns_all() {
        use openlid_core::mode::DaysOfWeek;
        let got = parse_days_csv("Mon,Tue,Wed,Thu,Fri,Sat,Sun").unwrap();
        assert_eq!(got, DaysOfWeek::all());
    }

    #[test]
    fn parse_days_csv_rejects_unknown_token_with_name_in_error() {
        let err = parse_days_csv("Mon,Funday").unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("funday"),
            "error should name the bad token, got: {msg}"
        );
    }

    #[test]
    fn parse_days_csv_rejects_empty_string() {
        // Empty after splitting yields zero day flags. Treating this as an
        // error rather than `DaysOfWeek::empty()` prevents a user from
        // accidentally creating an inert schedule (matches no day, so the
        // gate always rejects).
        let err = parse_days_csv("").unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("at least one"),
            "error should mention the empty-set problem, got: {msg}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // parse_schedule — combines from/to/days into a Schedule
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_schedule_basic_same_day_window() {
        use chrono::NaiveTime;
        use openlid_core::mode::DaysOfWeek;
        let s = parse_schedule("08:00", "18:00", Some("Mon,Tue")).unwrap();
        assert_eq!(s.start, NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        assert_eq!(s.end, NaiveTime::from_hms_opt(18, 0, 0).unwrap());
        assert_eq!(s.days, DaysOfWeek::MON | DaysOfWeek::TUE);
    }

    #[test]
    fn parse_schedule_omitted_days_defaults_to_all_seven() {
        use openlid_core::mode::DaysOfWeek;
        let s = parse_schedule("08:00", "18:00", None).unwrap();
        assert_eq!(s.days, DaysOfWeek::all());
    }

    #[test]
    fn parse_schedule_rejects_equal_from_and_to() {
        let err = parse_schedule("09:00", "09:00", None).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("non-empty") || msg.to_lowercase().contains("empty"),
            "error should explain the zero-length window, got: {msg}"
        );
    }

    #[test]
    fn parse_schedule_rejects_invalid_time_format() {
        let err = parse_schedule("nope", "18:00", None).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("HH:MM"),
            "error should hint at HH:MM, got: {msg}"
        );
    }

    #[test]
    fn parse_schedule_allows_cross_midnight_window() {
        // start > end is the cross-midnight signal that Schedule::contains
        // already handles. The CLI must NOT reject it as invalid.
        let s = parse_schedule("22:00", "02:00", Some("Mon")).unwrap();
        assert!(s.start > s.end);
    }

    // ─────────────────────────────────────────────────────────────────────
    // day_summary — human-readable rendering for the status line
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn day_summary_all_seven_is_daily() {
        use openlid_core::mode::DaysOfWeek;
        assert_eq!(day_summary(DaysOfWeek::all()), "daily");
    }

    #[test]
    fn day_summary_weekdays_is_mon_fri() {
        use openlid_core::mode::DaysOfWeek;
        let weekdays =
            DaysOfWeek::MON | DaysOfWeek::TUE | DaysOfWeek::WED | DaysOfWeek::THU | DaysOfWeek::FRI;
        assert_eq!(day_summary(weekdays), "Mon-Fri");
    }

    #[test]
    fn day_summary_weekends_is_named() {
        use openlid_core::mode::DaysOfWeek;
        assert_eq!(day_summary(DaysOfWeek::SAT | DaysOfWeek::SUN), "weekends");
    }

    #[test]
    fn day_summary_arbitrary_subset_is_comma_separated_in_mon_to_sun_order() {
        use openlid_core::mode::DaysOfWeek;
        let s = day_summary(DaysOfWeek::WED | DaysOfWeek::MON | DaysOfWeek::FRI);
        assert_eq!(s, "Mon, Wed, Fri");
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_status_human — schedule line presence/absence
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn format_status_includes_schedule_when_set() {
        use chrono::NaiveTime;
        use openlid_core::mode::{DaysOfWeek, Schedule};
        let mut s = snap();
        s.modifiers.schedule = Some(Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        });
        let out = format_status_human(&s);
        assert!(
            out.contains("Schedule:") && out.contains("09:00-18:00") && out.contains("daily"),
            "got: {out}"
        );
    }

    #[test]
    fn format_status_omits_schedule_line_when_none() {
        // Guard against an accidental always-print regression: a default
        // snapshot must not include the Schedule line at all.
        let out = format_status_human(&snap());
        assert!(!out.contains("Schedule:"), "got: {out}");
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_schedule_inline — direct coverage so a regression here surfaces
    // independently of the larger format_status_human and confirmation
    // wrappers.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn format_schedule_inline_renders_hhmm_with_day_summary() {
        use chrono::NaiveTime;
        use openlid_core::mode::{DaysOfWeek, Schedule};
        let s = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(8, 30, 0).unwrap(),
            end: NaiveTime::from_hms_opt(17, 45, 0).unwrap(),
        };
        assert_eq!(format_schedule_inline(&s), "08:30-17:45 (daily)");
    }

    #[test]
    fn format_schedule_inline_uses_weekdays_summary_for_mon_fri() {
        use chrono::NaiveTime;
        use openlid_core::mode::{DaysOfWeek, Schedule};
        let s = Schedule {
            days: DaysOfWeek::MON
                | DaysOfWeek::TUE
                | DaysOfWeek::WED
                | DaysOfWeek::THU
                | DaysOfWeek::FRI,
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        assert_eq!(format_schedule_inline(&s), "09:00-18:00 (Mon-Fri)");
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_schedule_set_confirmation — the message printed after a
    // successful `schedule set`. The "and openlid is now ON" suffix is the
    // visible signal of the implicit-enable bridge, so a regression here
    // would silently weaken the UX contract called out in the spec.
    // ─────────────────────────────────────────────────────────────────────

    fn sample_schedule() -> Schedule {
        use chrono::NaiveTime;
        use openlid_core::mode::DaysOfWeek;
        Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        }
    }

    #[test]
    fn format_schedule_set_confirmation_when_was_off_appends_now_on_suffix() {
        let out = format_schedule_set_confirmation(&sample_schedule(), true);
        assert!(out.contains("09:00-18:00"), "got: {out}");
        assert!(
            out.contains("openlid is now ON"),
            "expected the now-on suffix when toggle was off, got: {out}"
        );
    }

    #[test]
    fn format_schedule_set_confirmation_when_was_on_omits_now_on_suffix() {
        // Toggle was already on -- don't lie to the user by claiming the
        // command changed it.
        let out = format_schedule_set_confirmation(&sample_schedule(), false);
        assert!(out.contains("09:00-18:00"), "got: {out}");
        assert!(
            !out.contains("now ON"),
            "must not advertise an enable that did not happen, got: {out}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // format_schedule_show_text / _json — `schedule show` output formats.
    // Both branches must round-trip cleanly through the parser they
    // implicitly advertise: a `Schedule:` line a human reads, and a JSON
    // value a shell pipeline can parse.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn format_schedule_show_text_some_prints_schedule_line() {
        let out = format_schedule_show_text(Some(&sample_schedule()));
        assert!(
            out.starts_with("Schedule: "),
            "human form must start with the Schedule prefix, got: {out}"
        );
        assert!(out.contains("09:00-18:00"), "got: {out}");
    }

    #[test]
    fn format_schedule_show_text_none_prints_not_set() {
        let out = format_schedule_show_text(None);
        assert_eq!(out, "No schedule set.");
    }

    #[test]
    fn format_schedule_show_json_some_serializes_to_object() {
        // The JSON branch is what shell pipelines (`openlid schedule show
        // --json | jq ...`) consume. The exact shape is governed by the
        // serde derives on Schedule and DaysOfWeek, but a regression that
        // accidentally serialized to something like an array string would
        // break every downstream consumer.
        let s = sample_schedule();
        let out = format_schedule_show_json(Some(&s)).unwrap();
        assert!(out.contains("\"start\""), "got: {out}");
        assert!(out.contains("\"end\""), "got: {out}");
        assert!(out.contains("\"days\""), "got: {out}");
    }

    #[test]
    fn format_schedule_show_json_none_serializes_to_null() {
        let out = format_schedule_show_json(None).unwrap();
        assert_eq!(out.trim(), "null");
    }

    // ─────────────────────────────────────────────────────────────────────
    // parse_days_csv — `Mon,,Tue` edge case. The empty-token-skipping
    // branch is a deliberate tolerance: humans paste with stray commas all
    // the time, and treating that as the same as the no-comma case keeps
    // the parser forgiving.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_days_csv_skips_empty_tokens_between_commas() {
        use openlid_core::mode::DaysOfWeek;
        let got = parse_days_csv("Mon,,Tue").unwrap();
        assert_eq!(got, DaysOfWeek::MON | DaysOfWeek::TUE);
    }

    // ─────────────────────────────────────────────────────────────────────
    // extract_snapshot / expect_ok — three-arm response reducers. Pinning
    // their behavior directly is the only way to test the schedule
    // handlers' error paths without a live menubar peer.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn extract_snapshot_returns_state_from_ok() {
        let resp = ControlResponse::Ok {
            state: Snapshot {
                enabled: true,
                ..snap()
            },
        };
        let got = extract_snapshot(resp).unwrap();
        assert!(got.enabled);
    }

    #[test]
    fn extract_snapshot_propagates_error_message() {
        let resp = ControlResponse::Error {
            message: "helper offline".to_string(),
        };
        let err = extract_snapshot(resp).unwrap_err();
        assert!(
            format!("{err:#}").contains("helper offline"),
            "error message must include the wire-level reason",
        );
    }

    #[test]
    fn extract_snapshot_rejects_unexpected_variant() {
        // A `Pong` response to a request that asked for state is a
        // wire-level protocol violation. Surface it as an error rather
        // than silently substituting a default snapshot.
        let resp = ControlResponse::Pong;
        let err = extract_snapshot(resp).unwrap_err();
        assert!(format!("{err:#}").contains("unexpected"));
    }

    #[test]
    fn expect_ok_returns_unit_for_ok() {
        let resp = ControlResponse::Ok { state: snap() };
        expect_ok(resp).expect("Ok response must reduce to Ok(())");
    }

    #[test]
    fn expect_ok_propagates_error_message() {
        let resp = ControlResponse::Error {
            message: "no permission".to_string(),
        };
        let err = expect_ok(resp).unwrap_err();
        assert!(format!("{err:#}").contains("no permission"));
    }

    #[test]
    fn expect_ok_rejects_unexpected_variant() {
        let err = expect_ok(ControlResponse::Pong).unwrap_err();
        assert!(format!("{err:#}").contains("unexpected"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // build_set_schedule_request / build_clear_schedule_request — the
    // distinction between `Some(Some(s))` and `Some(None)` is the wire-
    // level signal that distinguishes "set" from "clear". A regression
    // that emitted `None` for clear would silently turn the command into
    // a no-op.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn build_set_schedule_request_uses_some_some_schedule() {
        let s = sample_schedule();
        match build_set_schedule_request(s.clone()) {
            ControlRequest::SetPreferences { schedule, .. } => {
                assert_eq!(schedule, Some(Some(s)));
            }
            other => panic!("expected SetPreferences, got {other:?}"),
        }
    }

    #[test]
    fn build_set_schedule_request_leaves_other_prefs_untouched() {
        // None on every other field means "don't change". A regression
        // that defaulted, say, start_at_login to Some(false) would clobber
        // user preferences every time someone set a schedule.
        match build_set_schedule_request(sample_schedule()) {
            ControlRequest::SetPreferences {
                start_at_login,
                activate_at_launch,
                battery_threshold_pct,
                prevent_display_sleep,
                ..
            } => {
                assert!(start_at_login.is_none());
                assert!(activate_at_launch.is_none());
                assert!(battery_threshold_pct.is_none());
                assert!(prevent_display_sleep.is_none());
            }
            other => panic!("expected SetPreferences, got {other:?}"),
        }
    }

    #[test]
    fn build_clear_schedule_request_uses_some_none_schedule() {
        match build_clear_schedule_request() {
            ControlRequest::SetPreferences { schedule, .. } => {
                assert_eq!(
                    schedule,
                    Some(None),
                    "clear must send Some(None) (the 'clear' signal), \
                     not None (which means 'leave alone')",
                );
            }
            other => panic!("expected SetPreferences, got {other:?}"),
        }
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
