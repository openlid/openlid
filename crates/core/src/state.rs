use crate::mode::{LidState, Modifiers, PowerSource};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Live application state. `enabled` and `modifiers` persist to config;
/// every other field is runtime-only (`#[serde(skip)]`).
///
/// `until` is the optional timer expiry. When `Some(t)` and `t > now`, sleep
/// prevention is active. When `t <= now`, it's expired (the runtime should
/// clear it on the next reconcile and disable). When `None`, the toggle is
/// indefinite — stays on until the user turns it off.
///
/// `network_reachable` tracks whether any interface can currently reach
/// the public Internet. `network_unreachable_since` is the `Instant` at
/// which reachability flipped to `false`, or `None` when reachable. The
/// in-transit auto-disable path reads these together with `lid`, `power`,
/// and the display state to decide whether the laptop is in a backpack.
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
    #[serde(skip, default = "default_reachable")]
    pub network_reachable: bool,
    #[serde(skip)]
    pub network_unreachable_since: Option<Instant>,
}

/// Default for `network_reachable`: optimistic. A fresh state should
/// not trip the in-transit detector before the platform monitor has
/// reported actual reachability.
fn default_reachable() -> bool {
    true
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            enabled: false,
            modifiers: Modifiers::default(),
            until: None,
            lid: LidState::Open,
            power: PowerSource::Ac,
            network_reachable: true,
            network_unreachable_since: None,
        }
    }
}

/// The single source of truth: "should we be preventing sleep right now?"
///
/// Behavior: when `enabled`, prevent sleep unconditionally (subject to
/// modifiers + the optional auto-expiry timer).
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

/// Should the in-transit auto-disable fire RIGHT NOW?
///
/// Returns `true` iff every guard holds:
///   * `enabled` -- nothing to disable when already off
///   * `lid == Closed` -- the laptop isn't visibly in front of the user
///   * `power == Battery {..}` -- not plugged in (the strongest "at a
///     desk" signal we have)
///   * `!has_external_display` -- not in clamshell mode
///   * network has been unreachable for at least `timeout`
///
/// The function is pure; the runtime mutates `state.enabled` based on
/// the return value and persists. Exhaustively unit-tested for each
/// guard's negative case.
pub fn should_auto_disable_in_transit(
    state: &AppState,
    has_external_display: bool,
    timeout: Duration,
    now: Instant,
) -> bool {
    if !state.enabled {
        return false;
    }
    if state.lid != LidState::Closed {
        return false;
    }
    if !matches!(state.power, PowerSource::Battery { .. }) {
        return false;
    }
    if has_external_display {
        return false;
    }
    let Some(since) = state.network_unreachable_since else {
        return false;
    };
    now.duration_since(since) >= timeout
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
            network_reachable: true,
            network_unreachable_since: None,
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

    #[test]
    fn schedule_allows_inside_window() {
        // Companion to schedule_blocks_outside_window. The "outside"
        // case exercises the `return false` arm of the schedule check;
        // this one exercises the fall-through where the schedule is
        // present and satisfied. Without both, a regression that always
        // returned false would only fail the outside test and pass
        // when no schedule was configured — masking the bug.
        let mut s = base();
        s.modifiers.schedule = Some(Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        });
        // Same noon timestamp as `t()` — inside [09:00, 18:00).
        assert!(should_prevent_sleep(&s, t()));
    }

    #[test]
    fn default_app_state_is_off_with_openlid_on_ac() {
        // Default is the "fresh install" baseline. The runtime relies on
        // these specific defaults: a brand-new install must NOT start
        // preventing sleep, and the runtime-only fields must resolve to
        // their conservative readings (lid Open, power AC) so the first
        // reconcile doesn't trigger a spurious state change before the
        // platform monitors have published real values.
        let s = AppState::default();
        assert!(!s.enabled);
        assert_eq!(s.modifiers, Modifiers::default());
        assert!(s.until.is_none());
        assert_eq!(s.lid, LidState::Open);
        assert_eq!(s.power, PowerSource::Ac);
        assert!(s.network_reachable, "default must be reachable: optimistic");
        assert!(s.network_unreachable_since.is_none());
        // And the default must produce no sleep prevention.
        assert!(!should_prevent_sleep(&s, t()));
    }

    // ─────────────────────────────────────────────────────────────────────
    // should_auto_disable_in_transit — all five guards individually
    // ─────────────────────────────────────────────────────────────────────

    fn in_transit_base() -> AppState {
        // The setup that DOES trip the detector: enabled, lid closed,
        // on battery, network unreachable for 5 minutes. Tests then
        // flip individual guards and assert the function refuses.
        AppState {
            enabled: true,
            modifiers: Modifiers::default(),
            until: None,
            lid: LidState::Closed,
            power: PowerSource::Battery { percent: 50 },
            network_reachable: false,
            network_unreachable_since: Some(Instant::now() - Duration::from_secs(300)),
        }
    }

    #[test]
    fn in_transit_fires_when_all_guards_pass() {
        let s = in_transit_base();
        let now = Instant::now();
        assert!(should_auto_disable_in_transit(
            &s,
            false, // no external display
            Duration::from_secs(120),
            now,
        ));
    }

    #[test]
    fn in_transit_skips_when_disabled() {
        // Already off -- nothing to do.
        let mut s = in_transit_base();
        s.enabled = false;
        assert!(!should_auto_disable_in_transit(
            &s,
            false,
            Duration::from_secs(120),
            Instant::now(),
        ));
    }

    #[test]
    fn in_transit_skips_when_lid_open() {
        // Lid open => laptop is in front of the user, not in a bag.
        let mut s = in_transit_base();
        s.lid = LidState::Open;
        assert!(!should_auto_disable_in_transit(
            &s,
            false,
            Duration::from_secs(120),
            Instant::now(),
        ));
    }

    #[test]
    fn in_transit_skips_when_on_ac() {
        // Plugged in => almost certainly at a desk; network may just
        // have flapped. Don't fire.
        let mut s = in_transit_base();
        s.power = PowerSource::Ac;
        assert!(!should_auto_disable_in_transit(
            &s,
            false,
            Duration::from_secs(120),
            Instant::now(),
        ));
    }

    #[test]
    fn in_transit_skips_when_external_display_present() {
        // Clamshell mode. Lid closed but the user is actively working
        // on a monitor. Don't fire even if the network drops.
        let s = in_transit_base();
        assert!(!should_auto_disable_in_transit(
            &s,
            true, // external display attached
            Duration::from_secs(120),
            Instant::now(),
        ));
    }

    #[test]
    fn in_transit_skips_when_network_reachable() {
        // `network_unreachable_since: None` is the runtime's signal
        // for "reachable" -- the timer can only fire when we have a
        // start time to measure from.
        let mut s = in_transit_base();
        s.network_reachable = true;
        s.network_unreachable_since = None;
        assert!(!should_auto_disable_in_transit(
            &s,
            false,
            Duration::from_secs(120),
            Instant::now(),
        ));
    }

    #[test]
    fn in_transit_skips_when_duration_under_threshold() {
        // Network dropped 30 seconds ago; timeout is 120s. Don't fire
        // yet -- the user may be in an elevator.
        let now = Instant::now();
        let s = AppState {
            network_unreachable_since: Some(now - Duration::from_secs(30)),
            ..in_transit_base()
        };
        assert!(!should_auto_disable_in_transit(
            &s,
            false,
            Duration::from_secs(120),
            now,
        ));
    }

    #[test]
    fn in_transit_fires_at_exact_threshold_boundary() {
        // duration_since >= timeout. Equal must fire (>= not >) so
        // the user's chosen threshold isn't off-by-an-instant.
        let now = Instant::now();
        let s = AppState {
            network_unreachable_since: Some(now - Duration::from_secs(120)),
            ..in_transit_base()
        };
        assert!(should_auto_disable_in_transit(
            &s,
            false,
            Duration::from_secs(120),
            now,
        ));
    }
}
