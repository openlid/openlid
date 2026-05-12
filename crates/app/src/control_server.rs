//! Unix domain socket server. Accepts one request per connection, replies, closes.
//! Path: ~/Library/Application Support/open-lid/control.sock.

use crate::state_runtime::{PrefsPatch, StateRuntime};
use anyhow::Result;
use interprocess::local_socket::{
    GenericFilePath, ListenerOptions, Stream, ToFsName,
    traits::ListenerExt,
};
use open_lid_core::ipc::control::{ControlRequest, ControlResponse};
use open_lid_core::platform::{
    DisplayController, LidObserver, PowerController, PowerSourceMonitor,
};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub fn control_socket_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("io", "openlid", "open-lid")
        .ok_or_else(|| anyhow::anyhow!("no home"))?;
    let dir = dirs.config_dir();
    std::fs::create_dir_all(dir).ok();
    Ok(dir.join("control.sock"))
}

pub fn spawn<P, L, S, D>(rt: Arc<StateRuntime<P, L, S, D>>) -> Result<()>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    let path = control_socket_path()?;
    let _ = std::fs::remove_file(&path);

    let name = path.clone().to_fs_name::<GenericFilePath>()?;
    let listener = ListenerOptions::new().name(name).create_sync()?;
    tracing::info!("control socket listening at {}", path.display());

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let rt = Arc::clone(&rt);
                    std::thread::spawn(move || {
                        if let Err(e) = handle_one(s, rt) {
                            tracing::warn!("control session error: {e:#}");
                        }
                    });
                }
                Err(e) => tracing::warn!("control accept error: {e}"),
            }
        }
    });
    Ok(())
}

fn handle_one<P, L, S, D>(stream: Stream, rt: Arc<StateRuntime<P, L, S, D>>) -> Result<()>
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 || line.trim().is_empty() {
        // Empty connection — the single-instance probe drops the stream after
        // confirming the server is alive. Not an error.
        return Ok(());
    }
    let req: ControlRequest = serde_json::from_str(line.trim())?;
    let resp = dispatch(req, &rt);
    let mut s = reader.into_inner();
    serde_json::to_writer(&mut s, &resp)?;
    s.write_all(b"\n")?;
    Ok(())
}

fn dispatch<P, L, S, D>(
    req: ControlRequest,
    rt: &Arc<StateRuntime<P, L, S, D>>,
) -> ControlResponse
where
    P: PowerController + Send + Sync + 'static,
    L: LidObserver + Send + Sync + 'static,
    S: PowerSourceMonitor + Send + Sync + 'static,
    D: DisplayController + Send + Sync + 'static,
{
    let result: Result<()> = match req {
        ControlRequest::Ping => return ControlResponse::Pong,
        ControlRequest::GetStatus => Ok(()),
        ControlRequest::SetEnabled { enabled, until } => rt.set_enabled(enabled, until),
        ControlRequest::SetPreferences {
            start_at_login,
            activate_at_launch,
            default_duration_minutes,
            battery_threshold_pct,
        } => rt.set_preferences(PrefsPatch {
            start_at_login,
            activate_at_launch,
            default_duration_minutes,
            battery_threshold_pct,
        }),
        ControlRequest::Uninstall => {
            tracing::info!("uninstall requested via control socket");
            rt.set_enabled(false, None)
        }
    };
    match result {
        Ok(()) => ControlResponse::Ok { state: rt.snapshot() },
        Err(e) => ControlResponse::Error { message: format!("{e:#}") },
    }
}
