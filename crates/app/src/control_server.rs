//! Unix domain socket server. Accepts one request per connection, replies, closes.
//! Path: ~/Library/Application Support/io.openlid.app/control.sock.
//!
//! Access control:
//!   1. The socket lives inside `~/Library/Application Support` which is
//!      `0700` for the owning user, so cross-UID traversal is blocked at the
//!      filesystem layer (this is the primary defense).
//!   2. Every accepted connection has its peer's effective UID checked
//!      against ours via `SO_PEERCRED`/`xucred` (defense-in-depth, race-free).
//!
//! We deliberately do NOT set an explicit socket file mode via
//! `ListenerOptionsExt::mode`: on macOS that path calls `fchmod` on the
//! unbound socket fd, which returns `EINVAL` and is mapped to
//! `io::ErrorKind::Unsupported` by `interprocess`, breaking listener
//! creation. The peer-creds check above is the authoritative defense and
//! makes the mode bits redundant.

use crate::state_runtime::{PrefsPatch, StateRuntime};
use anyhow::Result;
use interprocess::local_socket::{
    traits::{ListenerExt, StreamCommon},
    GenericFilePath, ListenerOptions, Stream, ToFsName,
};
use openlid_core::ipc::control::{ControlRequest, ControlResponse};
use openlid_core::platform::{DisplayController, LidObserver, PowerController, PowerSourceMonitor};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub fn control_socket_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("io", "openlid", "openlid")
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
    // SAFETY: getpeereid via interprocess's xucred wrapper — the kernel
    // attaches the peer's effective UID at connect time, so this is
    // race-free regardless of PID reuse. We reject the connection if the
    // peer is a different user, or if we can't determine its UID. Returning
    // Ok(()) drops the stream without writing — the peer just sees EOF.
    let our_euid = unsafe { libc::geteuid() };
    match stream.peer_creds().ok().and_then(|c| c.euid()) {
        Some(peer_euid) if peer_euid == our_euid => {}
        Some(peer_euid) => {
            tracing::warn!(
                "rejecting control connection: peer euid {peer_euid} != ours {our_euid}",
            );
            return Ok(());
        }
        None => {
            tracing::warn!("rejecting control connection: could not read peer euid");
            return Ok(());
        }
    }

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

fn dispatch<P, L, S, D>(req: ControlRequest, rt: &Arc<StateRuntime<P, L, S, D>>) -> ControlResponse
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
            prevent_display_sleep,
        } => rt.set_preferences(PrefsPatch {
            start_at_login,
            activate_at_launch,
            default_duration_minutes,
            battery_threshold_pct,
            prevent_display_sleep,
        }),
    };
    match result {
        Ok(()) => ControlResponse::Ok {
            state: rt.snapshot(),
        },
        Err(e) => ControlResponse::Error {
            message: format!("{e:#}"),
        },
    }
}
