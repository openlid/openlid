use crate::cli::{ConfigArg, ScheduleArg, UpdateArg};
use crate::updater::{install_detect, installer, release};
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

/// Compact machine-readable identifier for an install method. Used in
/// the `--json` output so script consumers can branch without parsing
/// human text.
fn install_method_label(method: &install_detect::InstallMethod) -> &'static str {
    match method {
        install_detect::InstallMethod::Homebrew => "homebrew",
        install_detect::InstallMethod::Manual => "manual",
        install_detect::InstallMethod::Dev { .. } => "dev",
    }
}

/// Human-readable update status. Multi-line; suitable for `print!` (no
/// trailing newline policy enforced -- the caller adds one if it needs
/// to follow with prompts).
fn format_update_status_human(
    current: &str,
    latest: &str,
    available: bool,
    method: &install_detect::InstallMethod,
) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    writeln!(out, "Current version:  {current}").unwrap();
    writeln!(out, "Latest version:   {latest}").unwrap();
    let status = if available {
        "Update available"
    } else {
        "Up to date"
    };
    writeln!(out, "Status:           {status}").unwrap();
    let method_label = match method {
        install_detect::InstallMethod::Homebrew => "Homebrew",
        install_detect::InstallMethod::Manual => "Manual install",
        install_detect::InstallMethod::Dev { .. } => "Dev build (not installable)",
    };
    writeln!(out, "Install method:   {method_label}").unwrap();
    out
}

/// Pretty-printed JSON status. Pure -- the actual stdout write happens
/// in the dispatcher.
fn format_update_status_json(
    current: &str,
    latest: &str,
    available: bool,
    method: &install_detect::InstallMethod,
) -> Result<String> {
    let status = serde_json::json!({
        "current": current,
        "latest": latest,
        "update_available": available,
        "install_method": install_method_label(method),
    });
    Ok(serde_json::to_string_pretty(&status)?)
}

fn confirm_install(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N]: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(parse_yes_no(&line))
}

/// Pure parser for the y/N prompt response. Accepts `y`, `yes`,
/// case-insensitive; everything else is a "no". Trims whitespace so an
/// accidental trailing space doesn't trigger an install.
fn parse_yes_no(input: &str) -> bool {
    let trimmed = input.trim().to_ascii_lowercase();
    trimmed == "y" || trimmed == "yes"
}

/// Validate flag combinations. `--check` (which never installs) and
/// `--yes` (non-interactive install) are mutually exclusive. Pure so
/// the conflict message can be regression-tested.
fn validate_update_flags(arg: &UpdateArg) -> Result<()> {
    if arg.check && arg.yes {
        return Err(anyhow!(
            "--check and --yes are mutually exclusive: --check never installs"
        ));
    }
    Ok(())
}

/// What the dispatcher should do next, computed from the install
/// method and the `--yes` flag. Pure -- the actual side-effects
/// (printing, prompting, installing) happen at the caller; this
/// function decides which branch to take.
#[derive(Debug, PartialEq)]
enum InstallAction {
    /// Print the homebrew advice; nothing else.
    HomebrewAdvise,
    /// Refuse with the dev-build error.
    DevRefuse(std::path::PathBuf),
    /// Prompt the user, then install on confirm.
    PromptThenInstall,
    /// Skip the prompt and install immediately.
    InstallNow,
}

fn decide_install_action(
    method: &install_detect::InstallMethod,
    non_interactive: bool,
) -> InstallAction {
    match method {
        install_detect::InstallMethod::Homebrew => InstallAction::HomebrewAdvise,
        install_detect::InstallMethod::Dev { path } => InstallAction::DevRefuse(path.clone()),
        install_detect::InstallMethod::Manual if non_interactive => InstallAction::InstallNow,
        install_detect::InstallMethod::Manual => InstallAction::PromptThenInstall,
    }
}

/// Multi-line human message shown to Homebrew users. Pure so the
/// command-string contract (the exact `brew upgrade openlid`) is
/// pinned by a test.
fn format_homebrew_update_advice() -> String {
    "\nThis is a Homebrew install. To update, run:\n\n  brew upgrade openlid\n".to_string()
}

/// Error message returned for a dev build. Mentions the path so the
/// user can spot which checkout they were running from.
fn format_dev_refusal_message(path: &std::path::Path) -> String {
    format!(
        "you appear to be running a dev build at {}; rebuild from source instead",
        path.display()
    )
}

pub fn update(arg: UpdateArg) -> Result<()> {
    validate_update_flags(&arg)?;

    let release = release::fetch_latest()?;
    let available = release::is_newer_than_current(&release.tag_name)?;
    let method = install_detect::detect();
    let current = release::current_version()?.to_string();
    let latest = release::strip_v_prefix(&release.tag_name)?.to_string();

    if arg.json {
        println!(
            "{}",
            format_update_status_json(&current, &latest, available, &method)?
        );
    } else {
        print!(
            "{}",
            format_update_status_human(&current, &latest, available, &method)
        );
    }

    // --check exit semantics: 0 = up to date, 1 = update available.
    // Use process::exit directly to bypass the standard Err -> exit-1
    // path so we don't print a redundant "Error: ..." line.
    if arg.check {
        if available {
            std::process::exit(1);
        }
        return Ok(());
    }

    if !available {
        return Ok(());
    }

    match decide_install_action(&method, arg.yes) {
        InstallAction::HomebrewAdvise => {
            if !arg.json {
                print!("{}", format_homebrew_update_advice());
            }
            Ok(())
        }
        InstallAction::DevRefuse(path) => Err(anyhow!(format_dev_refusal_message(&path))),
        InstallAction::PromptThenInstall => {
            println!();
            if !confirm_install("Download and install now?")? {
                println!("Aborted.");
                return Ok(());
            }
            install_update(&release)
        }
        InstallAction::InstallNow => install_update(&release),
    }
}

/// Pre-computed plan for `install_update`: which asset to fetch,
/// where to write it, and (optionally) the SHA-256 to verify against.
/// Pure construction means the prep logic gets unit coverage without
/// running the actual download.
#[derive(Debug)]
struct InstallPlan {
    url: String,
    dest: std::path::PathBuf,
    download_message: String,
    digest_hex: Option<String>,
}

fn prepare_install_plan(
    release: &release::ReleaseInfo,
    cache_dir: &std::path::Path,
) -> Result<InstallPlan> {
    let asset = release::pick_dmg_asset(&release.assets)?;
    let dest = cache_dir.join(&asset.name);
    let download_message = format_download_message(&asset.name, asset.size);
    let digest_hex = match asset.digest.as_deref() {
        Some(d) => Some(release::parse_digest(d)?),
        None => None,
    };
    Ok(InstallPlan {
        url: asset.browser_download_url.clone(),
        dest,
        download_message,
        digest_hex,
    })
}

/// `Downloading <name> (<MB>) MB...`. Pure so the exact phrasing the
/// user sees during a real update is pinned by a test.
fn format_download_message(name: &str, size: u64) -> String {
    let mb = (size as f64) / (1024.0 * 1024.0);
    format!("Downloading {name} ({mb:.1} MB)...")
}

/// Message printed when the release asset has no digest field. Pure;
/// pin the wording so a security-relevant log line can't drift.
fn format_no_digest_warning() -> &'static str {
    "Note: release has no published checksum; Gatekeeper will still verify \
     the signature on relaunch."
}

fn format_install_handoff_message(log_path: &std::path::Path) -> String {
    format!(
        "\nOpenLid is installing in the background. It will relaunch in a few seconds.\n\
         If something goes wrong, check the log at:\n  {}",
        log_path.display()
    )
}

/// Download the DMG, verify its SHA, and hand off to a detached
/// installer script. Returns Ok immediately after the spawn; the
/// caller's process exits so the script's wait-for-parent loop can
/// progress.
fn install_update(release: &release::ReleaseInfo) -> Result<()> {
    let cache = installer::cache_dir()?;
    installer::prepare_cache(&cache)?;
    let plan = prepare_install_plan(release, &cache)?;

    println!("{}", plan.download_message);
    installer::download(&plan.url, &plan.dest).context("downloading the DMG")?;

    if let Some(hex) = &plan.digest_hex {
        println!("Verifying checksum...");
        installer::verify_sha256(&plan.dest, hex).context("SHA-256 verification")?;
    } else {
        tracing::warn!(
            "release asset has no digest; relying on Gatekeeper code-signature check on relaunch"
        );
        println!("{}", format_no_digest_warning());
    }

    let log = installer::spawn_detached_installer(
        std::process::id(),
        &plan.dest,
        install_detect::APP_PATH,
    )?;
    println!("{}", format_install_handoff_message(&log));
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

    // ─────────────────────────────────────────────────────────────────────
    // openlid update — pure formatters
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn install_method_label_maps_each_variant() {
        // The JSON consumer keys off these strings; a rename would
        // silently break downstream scripts.
        assert_eq!(
            install_method_label(&install_detect::InstallMethod::Homebrew),
            "homebrew"
        );
        assert_eq!(
            install_method_label(&install_detect::InstallMethod::Manual),
            "manual"
        );
        assert_eq!(
            install_method_label(&install_detect::InstallMethod::Dev {
                path: std::path::PathBuf::from("/tmp/x"),
            }),
            "dev"
        );
    }

    #[test]
    fn format_update_status_human_shows_up_to_date_when_not_available() {
        let out = format_update_status_human(
            "2.0.0",
            "2.0.0",
            false,
            &install_detect::InstallMethod::Manual,
        );
        assert!(out.contains("Current version:  2.0.0"));
        assert!(out.contains("Latest version:   2.0.0"));
        assert!(out.contains("Up to date"));
        assert!(!out.contains("Update available"));
    }

    #[test]
    fn format_update_status_human_shows_update_available_when_newer() {
        let out = format_update_status_human(
            "2.0.0",
            "2.1.0",
            true,
            &install_detect::InstallMethod::Manual,
        );
        assert!(out.contains("Update available"));
        assert!(out.contains("Latest version:   2.1.0"));
    }

    #[test]
    fn format_update_status_human_labels_homebrew_install_method() {
        let out = format_update_status_human(
            "2.0.0",
            "2.0.0",
            false,
            &install_detect::InstallMethod::Homebrew,
        );
        assert!(out.contains("Install method:   Homebrew"));
    }

    #[test]
    fn format_update_status_human_labels_dev_build_explicitly() {
        // A dev build user should see "(not installable)" so the
        // refusal in the dispatcher isn't surprising.
        let out = format_update_status_human(
            "2.0.0",
            "2.0.0",
            false,
            &install_detect::InstallMethod::Dev {
                path: std::path::PathBuf::from("/tmp/x"),
            },
        );
        assert!(out.contains("Dev build (not installable)"));
    }

    #[test]
    fn format_update_status_json_serializes_expected_keys() {
        // Consumer contract: keys are stable. A test that asserted
        // string equality on the whole JSON would be brittle to
        // whitespace; checking key presence is more robust.
        let out = format_update_status_json(
            "2.0.0",
            "2.1.0",
            true,
            &install_detect::InstallMethod::Manual,
        )
        .unwrap();
        assert!(out.contains("\"current\""));
        assert!(out.contains("\"latest\""));
        assert!(out.contains("\"update_available\""));
        assert!(out.contains("\"install_method\""));
        // Sanity: bool serializes as `true` (not `"true"`).
        assert!(out.contains("\"update_available\": true"));
    }

    #[test]
    fn format_update_status_json_emits_homebrew_label() {
        let out = format_update_status_json(
            "2.0.0",
            "2.0.0",
            false,
            &install_detect::InstallMethod::Homebrew,
        )
        .unwrap();
        assert!(out.contains("\"install_method\": \"homebrew\""));
    }

    // ─────────────────────────────────────────────────────────────────────
    // openlid update — flag validation, prompt parsing, install routing
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_yes_no_accepts_y_and_yes_case_insensitive() {
        for s in ["y", "Y", "yes", "YES", "Yes", "  y  ", "\ty\n"] {
            assert!(parse_yes_no(s), "expected yes for {s:?}");
        }
    }

    #[test]
    fn parse_yes_no_rejects_anything_else() {
        for s in ["", "n", "no", "yep", "yeah", "1", "true", "ok"] {
            assert!(!parse_yes_no(s), "expected no for {s:?}");
        }
    }

    #[test]
    fn validate_update_flags_accepts_check_alone() {
        let arg = UpdateArg {
            check: true,
            yes: false,
            json: false,
        };
        validate_update_flags(&arg).expect("check alone should be valid");
    }

    #[test]
    fn validate_update_flags_accepts_yes_alone() {
        let arg = UpdateArg {
            check: false,
            yes: true,
            json: false,
        };
        validate_update_flags(&arg).expect("yes alone should be valid");
    }

    #[test]
    fn validate_update_flags_rejects_check_and_yes_together() {
        // --check never installs; --yes is non-interactive install. The
        // combination is nonsensical and the error message must explain
        // why so the user knows which flag to drop.
        let arg = UpdateArg {
            check: true,
            yes: true,
            json: false,
        };
        let err = validate_update_flags(&arg).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("--check") && msg.contains("--yes"));
        assert!(msg.contains("never installs"));
    }

    #[test]
    fn decide_install_action_homebrew_advises_regardless_of_yes() {
        // Homebrew never auto-installs even with --yes; we never want to
        // race with brew's own metadata.
        let m = install_detect::InstallMethod::Homebrew;
        assert_eq!(
            decide_install_action(&m, false),
            InstallAction::HomebrewAdvise
        );
        assert_eq!(
            decide_install_action(&m, true),
            InstallAction::HomebrewAdvise
        );
    }

    #[test]
    fn decide_install_action_dev_refuses_regardless_of_yes() {
        let p = std::path::PathBuf::from("/tmp/dev/OpenLid.app");
        let m = install_detect::InstallMethod::Dev { path: p.clone() };
        assert_eq!(
            decide_install_action(&m, false),
            InstallAction::DevRefuse(p.clone())
        );
        assert_eq!(decide_install_action(&m, true), InstallAction::DevRefuse(p));
    }

    #[test]
    fn decide_install_action_manual_branches_on_yes_flag() {
        // The --yes flag's whole job is "skip the prompt". A regression
        // that swapped these branches would either prompt under --yes
        // (frustrating automation) or silently install without --yes
        // (frustrating safety).
        let m = install_detect::InstallMethod::Manual;
        assert_eq!(
            decide_install_action(&m, false),
            InstallAction::PromptThenInstall
        );
        assert_eq!(decide_install_action(&m, true), InstallAction::InstallNow);
    }

    #[test]
    fn format_homebrew_update_advice_pins_the_exact_command() {
        // Pin the command string itself: a typo would send users to
        // run something that doesn't exist (e.g. "brew update openlid"
        // would target brew itself, not the cask).
        let out = format_homebrew_update_advice();
        assert!(out.contains("brew upgrade openlid"));
        assert!(out.contains("Homebrew"));
    }

    #[test]
    fn format_dev_refusal_message_includes_path() {
        let path = std::path::PathBuf::from("/Users/dev/openlid/target/bundle/OpenLid.app");
        let msg = format_dev_refusal_message(&path);
        assert!(msg.contains("dev build"));
        assert!(msg.contains(&path.display().to_string()));
        assert!(msg.contains("rebuild from source"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // openlid update — install plan + messages
    // ─────────────────────────────────────────────────────────────────────

    fn sample_release_with_dmg(digest: Option<&str>) -> release::ReleaseInfo {
        release::ReleaseInfo {
            tag_name: "v2.1.0".to_string(),
            body: "".to_string(),
            assets: vec![release::AssetInfo {
                name: "OpenLid-v2.1.0.dmg".to_string(),
                browser_download_url: "https://example.com/OpenLid-v2.1.0.dmg".to_string(),
                size: 5 * 1024 * 1024,
                digest: digest.map(String::from),
            }],
        }
    }

    #[test]
    fn prepare_install_plan_picks_dmg_and_paths_into_cache() {
        let release = sample_release_with_dmg(Some("sha256:abc"));
        let cache = std::path::Path::new("/tmp/cache");
        let plan = prepare_install_plan(&release, cache).unwrap();
        assert_eq!(plan.url, "https://example.com/OpenLid-v2.1.0.dmg");
        assert_eq!(plan.dest, cache.join("OpenLid-v2.1.0.dmg"));
        assert_eq!(plan.digest_hex.as_deref(), Some("abc"));
    }

    #[test]
    fn prepare_install_plan_handles_missing_digest_gracefully() {
        // Older releases lack the digest field. The plan must still
        // succeed; the install loop falls back to Gatekeeper-only
        // verification (warning printed at install time).
        let release = sample_release_with_dmg(None);
        let plan = prepare_install_plan(&release, std::path::Path::new("/tmp/cache")).unwrap();
        assert!(plan.digest_hex.is_none());
    }

    #[test]
    fn prepare_install_plan_propagates_no_dmg_error() {
        // A release with no DMG asset must surface a clear error
        // before any download attempt; otherwise we'd download
        // something else and then fail mysteriously at mount time.
        let release = release::ReleaseInfo {
            tag_name: "v2.1.0".to_string(),
            body: String::new(),
            assets: vec![release::AssetInfo {
                name: "checksums.txt".to_string(),
                browser_download_url: "https://example.com/c.txt".to_string(),
                size: 100,
                digest: None,
            }],
        };
        let err = prepare_install_plan(&release, std::path::Path::new("/tmp")).unwrap_err();
        assert!(format!("{err:#}").contains("no .dmg"));
    }

    #[test]
    fn prepare_install_plan_propagates_bad_digest_format() {
        // A digest with an unrecognised algorithm (e.g. `sha512:`)
        // must fail at plan time, not mid-download. Catching it here
        // means the user sees a clear error before bytes hit disk.
        let release = sample_release_with_dmg(Some("sha512:xx"));
        let err = prepare_install_plan(&release, std::path::Path::new("/tmp")).unwrap_err();
        assert!(format!("{err:#}").contains("unsupported digest"));
    }

    #[test]
    fn format_download_message_shows_size_in_megabytes() {
        let msg = format_download_message("OpenLid-v2.1.0.dmg", 5_242_880);
        assert!(msg.contains("OpenLid-v2.1.0.dmg"));
        assert!(msg.contains("5.0 MB"));
    }

    #[test]
    fn format_download_message_rounds_to_one_decimal_place() {
        // Pin the formatting precision so a future change to "{mb:.0} MB"
        // (lossy on small files) gets caught.
        let msg = format_download_message("a.dmg", 1_500_000);
        // 1_500_000 / (1024*1024) = 1.43...; "1.4 MB" expected.
        assert!(msg.contains("1.4 MB"), "got: {msg}");
    }

    #[test]
    fn format_no_digest_warning_mentions_gatekeeper() {
        // The mitigation that justifies skipping checksum verification
        // is Gatekeeper. If the warning ever drops that word, a
        // security reviewer would have no way to tell from logs that
        // the install was still signature-protected.
        assert!(format_no_digest_warning().contains("Gatekeeper"));
    }

    #[test]
    fn format_install_handoff_message_includes_log_path() {
        let log = std::path::PathBuf::from("/tmp/openlid-installer-12345.log");
        let msg = format_install_handoff_message(&log);
        assert!(msg.contains("/tmp/openlid-installer-12345.log"));
        assert!(msg.contains("relaunch"));
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
