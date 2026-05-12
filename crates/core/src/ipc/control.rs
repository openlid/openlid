//! Messages exchanged between the CLI role and the menubar role over a
//! Unix domain socket. Line-delimited JSON: one request, one response, close.

use crate::mode::{LidState, Modifiers, PowerSource};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum ControlRequest {
    GetStatus,
    /// Set the toggle plus an optional auto-expiry instant. `until = None`
    /// means indefinite (no timer); `Some(t)` means deactivate at `t`.
    SetEnabled {
        enabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        until: Option<DateTime<Local>>,
    },
    /// Update the persisted preferences from the preferences UI or the CLI
    /// `config edit` path. Fields are all optional — only `Some(_)` ones are
    /// applied; `None` leaves the existing value untouched.
    SetPreferences {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_at_login: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activate_at_launch: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_duration_minutes: Option<Option<u32>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        battery_threshold_pct: Option<Option<u8>>,
    },
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Local>>,
    pub modifiers: Modifiers,
    pub lid: LidState,
    pub power: PowerSource,
    pub helper: HelperStatus,
    /// User preferences mirrored so the UI can render them without an
    /// extra round-trip.
    pub start_at_login: bool,
    pub activate_at_launch: bool,
    pub default_duration_minutes: Option<u32>,
    pub battery_threshold_pct: Option<u8>,
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
            until: None,
            modifiers: Modifiers::default(),
            lid: LidState::Closed,
            power: PowerSource::Battery { percent: 73 },
            helper: HelperStatus::Running,
            start_at_login: false,
            activate_at_launch: false,
            default_duration_minutes: None,
            battery_threshold_pct: Some(20),
        };
        let s = serde_json::to_string(&snap).unwrap();
        let back: Snapshot = serde_json::from_str(&s).unwrap();
        assert_eq!(snap, back);
    }

    #[test]
    fn set_enabled_with_timer_round_trip() {
        use chrono::TimeZone;
        let until = Local.with_ymd_and_hms(2026, 5, 12, 18, 0, 0).unwrap();
        let r = ControlRequest::SetEnabled {
            enabled: true,
            until: Some(until),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: ControlRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }
}
