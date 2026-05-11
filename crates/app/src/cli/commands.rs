use crate::cli::{ConfigArg, ModeArg, ModifierArg};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone};
use interprocess::local_socket::{
    GenericFilePath, Stream, ToFsName,
    traits::Stream as StreamTrait,
};
use open_lid_core::config::Config;
use open_lid_core::ipc::control::{ControlRequest, ControlResponse, Snapshot};
use open_lid_core::mode::Mode;
use std::io::{BufRead, BufReader, Write};
use std::time::Duration as StdDuration;

fn socket_path() -> Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("io", "openlid", "open-lid")
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
                    let _ = std::process::Command::new("/usr/bin/open")
                        .args(["-a", "OpenLid"])
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

pub fn set_enabled(enabled: bool) -> Result<()> {
    match send_request(ControlRequest::SetEnabled { enabled }, true)? {
        ControlResponse::Ok { state } => {
            // Report the user's intent (the toggle state), not the
            // moment-to-moment "actually preventing right now" state. In
            // mode `lid-closed` with the lid open, the toggle is on but
            // we're not actively calling pmset yet — that's still "ON"
            // from the user's perspective.
            println!("{}", if state.enabled { "ON" } else { "OFF" });
            Ok(())
        }
        ControlResponse::Error { message } => Err(anyhow!(message)),
        _ => Err(anyhow!("unexpected response")),
    }
}

pub fn status(json: bool) -> Result<()> {
    let resp = send_request(ControlRequest::GetStatus, false);
    match resp {
        Ok(ControlResponse::Ok { state }) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&state)?);
            } else {
                print_status_human(&state);
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

fn print_status_human(s: &Snapshot) {
    let state_label = match (s.enabled, s.preventing_sleep_now) {
        (false, _) => "OFF",
        (true, true) => "ON (preventing sleep now)",
        (true, false) => "ON (armed, idle)",
    };
    println!("Sleep prevention: {state_label}");
    println!("Mode:             {:?}", s.mode);
    println!("Lid:              {:?}", s.lid);
    println!("Power:            {:?}", s.power);
}

pub fn set_mode(mode: ModeArg) -> Result<()> {
    let m = match mode {
        ModeArg::LidClosed => Mode::LidClosed,
        ModeArg::AlwaysAwake => Mode::AlwaysAwake,
    };
    match send_request(ControlRequest::SetMode { mode: m }, true)? {
        ControlResponse::Ok { .. } => Ok(()),
        ControlResponse::Error { message } => Err(anyhow!(message)),
        _ => Err(anyhow!("unexpected response")),
    }
}

pub fn for_duration(s: &str) -> Result<()> {
    let dur = humantime::parse_duration(s).context("invalid duration")?;
    let until: DateTime<Local> = Local::now() + Duration::from_std(dur)?;
    let req = ControlRequest::SetMode { mode: Mode::Timed { until } };
    send_request(req, true)?;
    set_enabled(true)
}

pub fn until(s: &str) -> Result<()> {
    let until = parse_until(s)?;
    let req = ControlRequest::SetMode { mode: Mode::Timed { until } };
    send_request(req, true)?;
    set_enabled(true)
}

fn parse_until(s: &str) -> Result<DateTime<Local>> {
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M") {
        let today = Local::now().date_naive().and_time(t);
        let dt = Local.from_local_datetime(&today).single().unwrap();
        return Ok(if dt > Local::now() { dt } else { dt + Duration::days(1) });
    }
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .context("expected HH:MM or YYYY-MM-DDTHH:MM")?;
    Local.from_local_datetime(&naive).single()
        .ok_or_else(|| anyhow!("ambiguous local time"))
}

pub fn modifier(_m: ModifierArg) -> Result<()> {
    println!("modifier commands are stubbed in MVP; coming in Plan 2.");
    Ok(())
}

pub fn config(c: ConfigArg) -> Result<()> {
    let path = Config::default_path()?;
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

pub fn uninstall() -> Result<()> {
    println!("uninstall is stubbed in MVP; coming in Plan 2.");
    Ok(())
}
