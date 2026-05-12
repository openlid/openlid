use crate::mode::{LidState, Modifiers, PowerSource};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// Live application state. `enabled` and `modifiers` persist to config;
/// `lid`, `power`, and `until` are runtime-only.
///
/// `until` is the optional timer expiry. When `Some(t)` and `t > now`, sleep
/// prevention is active. When `t <= now`, it's expired (the runtime should
/// clear it on the next reconcile and disable). When `None`, the toggle is
/// indefinite — stays on until the user turns it off.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppState {
    pub enabled: bool,
    pub modifiers: Modifiers,
    #[serde(skip)]
    pub until: Option<DateTime<Local>>,
    #[serde(skip)]
    pub lid: LidState,
    #[serde(skip)]
    pub power: PowerSource,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            enabled: false,
            modifiers: Modifiers::default(),
            until: None,
            lid: LidState::Open,
            power: PowerSource::Ac,
        }
    }
}

/// The single source of truth: "should we be preventing sleep right now?"
///
/// Behavior (post-mode-removal): when `enabled`, prevent sleep — like Caffeine.
/// If a timer is set (`until = Some(t)`), prevention stops at `t`.
pub fn should_prevent_sleep(state: &AppState, now: DateTime<Local>) -> bool {
    if !state.enabled {
        return false;
    }
    if !modifiers_allow(&state.modifiers, now, &state.power) {
        return false;
    }
    if let Some(until) = state.until {
        return now < until;
    }
    true
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
            modifiers: Modifiers::default(),
            until: None,
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
    fn enabled_indefinite_prevents_regardless_of_lid() {
        let mut s = base();
        s.lid = LidState::Open;
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn enabled_with_timer_in_future_prevents() {
        let mut s = base();
        s.until = Some(t() + chrono::Duration::hours(2));
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn enabled_with_timer_in_past_does_not_prevent() {
        let mut s = base();
        s.until = Some(t() - chrono::Duration::hours(1));
        assert!(!should_prevent_sleep(&s, t()));
    }

    #[test]
    fn enabled_with_timer_at_exact_expiry_does_not_prevent() {
        let mut s = base();
        s.until = Some(t());
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
