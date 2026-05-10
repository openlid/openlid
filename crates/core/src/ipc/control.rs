//! Messages exchanged between the CLI role and the menubar role over a
//! Unix domain socket. Line-delimited JSON: one request, one response, close.

use crate::mode::{LidState, Mode, Modifiers, PowerSource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum ControlRequest {
    GetStatus,
    SetEnabled { enabled: bool },
    SetMode { mode: Mode },
    SetModifierOnlyOnAc { enabled: bool },
    SetModifierMinBattery { percent: Option<u8> },
    SetModifierSchedule { enabled: bool },
    Uninstall,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum ControlResponse {
    Ok { state: Snapshot },
    Pong,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub preventing_sleep_now: bool,
    pub enabled: bool,
    pub mode: Mode,
    pub modifiers: Modifiers,
    pub lid: LidState,
    pub power: PowerSource,
    pub helper: HelperStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum HelperStatus {
    NotInstalled,
    NeedsApproval,
    Running,
    Stopped,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_get_status_serializes_to_kebab_case() {
        let r = ControlRequest::GetStatus;
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"cmd":"get-status"}"#);
    }

    #[test]
    fn response_pong_round_trips() {
        let r = ControlResponse::Pong;
        let s = serde_json::to_string(&r).unwrap();
        let back: ControlResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn snapshot_round_trips() {
        let snap = Snapshot {
            preventing_sleep_now: true,
            enabled: true,
            mode: Mode::LidClosed,
            modifiers: Modifiers::default(),
            lid: LidState::Closed,
            power: PowerSource::Battery { percent: 73 },
            helper: HelperStatus::Running,
        };
        let s = serde_json::to_string(&snap).unwrap();
        let back: Snapshot = serde_json::from_str(&s).unwrap();
        assert_eq!(snap, back);
    }
}
