//! Messages exchanged between the CLI role and the menubar role over a
//! Unix domain socket. Line-delimited JSON: one request, one response, close.

use crate::mode::{LidState, Modifiers, PowerSource, Schedule};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// Custom deserializer that preserves the three-state semantics of a
/// `Option<Option<T>>` field across JSON:
///   * missing key  -> `None`       (leave alone)
///   * `"key": null` -> `Some(None)` (clear)
///   * `"key": value` -> `Some(Some(value))` (set)
///
/// Without this, serde's default `Option<T>` deserializer collapses any
/// JSON `null` to the outer `None`, making "missing" and "null" indistinguishable
/// on the wire. The `#[serde(default)]` attribute on the field handles the
/// missing-key case; this function handles the present-but-null case.
fn deserialize_double_option_schedule<'de, D>(d: D) -> Result<Option<Option<Schedule>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Schedule>::deserialize(d).map(Some)
}

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
    ///
    /// `schedule` uses an extra `Option` layer: outer `None` means "leave
    /// alone", `Some(None)` means "clear", `Some(Some(s))` means "set to s".
    SetPreferences {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_at_login: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activate_at_launch: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_duration_minutes: Option<Option<u32>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        battery_threshold_pct: Option<Option<u8>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prevent_display_sleep: Option<bool>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_double_option_schedule"
        )]
        schedule: Option<Option<Schedule>>,
    },
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
    /// Mirrors `Config::prevent_display_sleep`. When `true`, the runtime
    /// holds an IOPMAssertion that keeps the display awake (and therefore
    /// the screen unlocked) whenever sleep prevention is active and the lid
    /// is open (or an external display is attached). Defaults to `true` for
    /// new installs; older clients deserializing a Snapshot without this
    /// field will see `false`, which is safe (they were never aware of the
    /// feature in the first place).
    #[serde(default)]
    pub prevent_display_sleep: bool,
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
            prevent_display_sleep: true,
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

    #[test]
    fn set_preferences_with_schedule_round_trips() {
        // Three-state field semantics: `Some(Some(s))` means "set to s".
        // Without serde-default + skip-if-none on the field, this round-trip
        // would lose `schedule` when no other prefs are being patched.
        use crate::mode::{DaysOfWeek, Schedule};
        use chrono::NaiveTime;
        let sched = Schedule {
            days: DaysOfWeek::MON | DaysOfWeek::FRI,
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let r = ControlRequest::SetPreferences {
            start_at_login: None,
            activate_at_launch: None,
            default_duration_minutes: None,
            battery_threshold_pct: None,
            prevent_display_sleep: None,
            schedule: Some(Some(sched)),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: ControlRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn set_preferences_omitting_schedule_field_loads_as_none() {
        // Back-compat: a client written against an older schema sends no
        // `schedule` key. Deserialization must yield `schedule: None` (the
        // "leave alone" branch), NOT fail or set to `Some(None)` (which
        // would mean "clear the schedule").
        let json = r#"{"cmd":"set-preferences"}"#;
        let req: ControlRequest = serde_json::from_str(json).unwrap();
        match req {
            ControlRequest::SetPreferences { schedule, .. } => {
                assert!(schedule.is_none(), "missing field must mean leave-alone");
            }
            other => panic!("expected SetPreferences, got {other:?}"),
        }
    }

    #[test]
    fn set_preferences_with_schedule_none_means_clear() {
        // `Some(None)` vs missing-field distinction matters: the server uses
        // it as the "clear the schedule" signal. Round-trip preserves it.
        let r = ControlRequest::SetPreferences {
            start_at_login: None,
            activate_at_launch: None,
            default_duration_minutes: None,
            battery_threshold_pct: None,
            prevent_display_sleep: None,
            schedule: Some(None),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: ControlRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }
}
