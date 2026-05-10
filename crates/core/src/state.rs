use crate::mode::{LidState, Mode, Modifiers, PowerSource};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppState {
    pub enabled: bool,
    pub mode: Mode,
    pub modifiers: Modifiers,
    #[serde(skip)]
    pub lid: LidState,
    #[serde(skip)]
    pub power: PowerSource,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: Mode::default(),
            modifiers: Modifiers::default(),
            lid: LidState::Open,
            power: PowerSource::Ac,
        }
    }
}

/// The single source of truth: "should we be preventing sleep right now?"
pub fn should_prevent_sleep(state: &AppState, now: DateTime<Local>) -> bool {
    if !state.enabled {
        return false;
    }
    if !modifiers_allow(&state.modifiers, now, &state.power) {
        return false;
    }
    match &state.mode {
        Mode::LidClosed => state.lid == LidState::Closed,
        Mode::AlwaysAwake => true,
        Mode::Timed { until } => now < *until,
    }
}

fn modifiers_allow(m: &Modifiers, now: DateTime<Local>, power: &PowerSource) -> bool {
    if m.only_on_ac && !matches!(power, PowerSource::Ac) {
        return false;
    }
    if let Some(min) = m.min_battery {
        if let PowerSource::Battery { percent } = power {
            if *percent < min {
                return false;
            }
        }
    }
    if let Some(sched) = &m.schedule {
        if !sched.contains(now) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::{DaysOfWeek, Schedule};
    use chrono::{NaiveTime, TimeZone};

    fn t() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 5, 11, 12, 0, 0).unwrap()
    }

    fn base() -> AppState {
        AppState {
            enabled: true,
            mode: Mode::LidClosed,
            modifiers: Modifiers::default(),
            lid: LidState::Closed,
            power: PowerSource::Ac,
        }
    }

    #[test]
    fn disabled_never_prevents() {
        let mut s = base();
        s.enabled = false;
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn lid_closed_mode_with_lid_open_does_not_prevent() {
        let mut s = base();
        s.lid = LidState::Open;
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn lid_closed_mode_with_lid_closed_prevents() {
        let s = base();
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn always_awake_prevents_regardless_of_lid() {
        let mut s = base();
        s.mode = Mode::AlwaysAwake;
        s.lid = LidState::Open;
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn timed_mode_before_until_prevents() {
        let mut s = base();
        s.mode = Mode::Timed {
            until: t() + chrono::Duration::hours(2),
        };
        s.lid = LidState::Open;
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn timed_mode_after_until_does_not_prevent() {
        let mut s = base();
        s.mode = Mode::Timed {
            until: t() - chrono::Duration::hours(1),
        };
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn only_on_ac_blocks_when_on_battery() {
        let mut s = base();
        s.modifiers.only_on_ac = true;
        s.power = PowerSource::Battery { percent: 80 };
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn only_on_ac_allows_when_on_ac() {
        let mut s = base();
        s.modifiers.only_on_ac = true;
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn min_battery_blocks_below_threshold() {
        let mut s = base();
        s.modifiers.min_battery = Some(50);
        s.power = PowerSource::Battery { percent: 30 };
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn min_battery_allows_above_threshold() {
        let mut s = base();
        s.modifiers.min_battery = Some(50);
        s.power = PowerSource::Battery { percent: 80 };
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn min_battery_does_not_apply_on_ac() {
        let mut s = base();
        s.modifiers.min_battery = Some(50);
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn schedule_blocks_outside_window() {
        let mut s = base();
        s.modifiers.schedule = Some(Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        });
        let outside = Local.with_ymd_and_hms(2026, 5, 11, 20, 0, 0).unwrap();
        assert!(!should_prevent_sleep(&s, outside));
    }
}
