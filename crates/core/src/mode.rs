use bitflags::bitflags;
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DaysOfWeek: u8 {
        const MON = 1 << 0;
        const TUE = 1 << 1;
        const WED = 1 << 2;
        const THU = 1 << 3;
        const FRI = 1 << 4;
        const SAT = 1 << 5;
        const SUN = 1 << 6;
    }
}

impl Default for DaysOfWeek {
    fn default() -> Self {
        DaysOfWeek::empty()
    }
}

// Serialize as an array of three-letter day strings.
impl Serialize for DaysOfWeek {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        const ALL: [(DaysOfWeek, &str); 7] = [
            (DaysOfWeek::MON, "Mon"),
            (DaysOfWeek::TUE, "Tue"),
            (DaysOfWeek::WED, "Wed"),
            (DaysOfWeek::THU, "Thu"),
            (DaysOfWeek::FRI, "Fri"),
            (DaysOfWeek::SAT, "Sat"),
            (DaysOfWeek::SUN, "Sun"),
        ];
        let count = ALL.iter().filter(|(d, _)| self.contains(*d)).count();
        let mut seq = s.serialize_seq(Some(count))?;
        for (flag, name) in ALL.iter() {
            if self.contains(*flag) {
                seq.serialize_element(name)?;
            }
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for DaysOfWeek {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let names: Vec<String> = Vec::deserialize(d)?;
        let mut days = DaysOfWeek::empty();
        for name in names {
            match name.as_str() {
                "Mon" => days |= DaysOfWeek::MON,
                "Tue" => days |= DaysOfWeek::TUE,
                "Wed" => days |= DaysOfWeek::WED,
                "Thu" => days |= DaysOfWeek::THU,
                "Fri" => days |= DaysOfWeek::FRI,
                "Sat" => days |= DaysOfWeek::SAT,
                "Sun" => days |= DaysOfWeek::SUN,
                other => return Err(serde::de::Error::custom(format!("unknown day: {other}"))),
            }
        }
        Ok(days)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Modifiers {
    #[serde(default)]
    pub only_on_ac: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_battery: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Schedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schedule {
    pub days: DaysOfWeek,
    pub start: NaiveTime,
    pub end: NaiveTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LidState {
    #[default]
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PowerSource {
    #[default]
    Ac,
    Battery {
        percent: u8,
    },
}

impl Schedule {
    /// Returns true if `now` falls within this schedule window.
    pub fn contains(&self, now: chrono::DateTime<chrono::Local>) -> bool {
        use chrono::{Datelike, Timelike, Weekday};

        let today_flag = match now.weekday() {
            Weekday::Mon => DaysOfWeek::MON,
            Weekday::Tue => DaysOfWeek::TUE,
            Weekday::Wed => DaysOfWeek::WED,
            Weekday::Thu => DaysOfWeek::THU,
            Weekday::Fri => DaysOfWeek::FRI,
            Weekday::Sat => DaysOfWeek::SAT,
            Weekday::Sun => DaysOfWeek::SUN,
        };

        let now_t = chrono::NaiveTime::from_hms_opt(now.hour(), now.minute(), now.second())
            .expect("valid clock time");

        if self.start <= self.end {
            self.days.contains(today_flag) && now_t >= self.start && now_t < self.end
        } else {
            if self.days.contains(today_flag) && now_t >= self.start {
                return true;
            }
            let yesterday = match now.weekday() {
                Weekday::Mon => DaysOfWeek::SUN,
                Weekday::Tue => DaysOfWeek::MON,
                Weekday::Wed => DaysOfWeek::TUE,
                Weekday::Thu => DaysOfWeek::WED,
                Weekday::Fri => DaysOfWeek::THU,
                Weekday::Sat => DaysOfWeek::FRI,
                Weekday::Sun => DaysOfWeek::SAT,
            };
            self.days.contains(yesterday) && now_t < self.end
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    #[test]
    fn modifiers_default_is_all_off() {
        let m = Modifiers::default();
        assert!(!m.only_on_ac);
        assert!(m.min_battery.is_none());
        assert!(m.schedule.is_none());
    }

    #[test]
    fn days_of_week_round_trip_via_json() {
        let d = DaysOfWeek::MON | DaysOfWeek::WED | DaysOfWeek::FRI;
        let s = serde_json::to_string(&d).unwrap();
        assert_eq!(s, r#"["Mon","Wed","Fri"]"#);
        let back: DaysOfWeek = serde_json::from_str(&s).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn days_of_week_rejects_unknown_day() {
        let r: Result<DaysOfWeek, _> = serde_json::from_str(r#"["Funday"]"#);
        assert!(r.is_err());
    }

    #[test]
    fn schedule_same_day_inside_window_active() {
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 11, 12, 0, 0).unwrap();
        assert!(sched.contains(now));
    }

    #[test]
    fn schedule_same_day_outside_window_inactive() {
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 11, 20, 0, 0).unwrap();
        assert!(!sched.contains(now));
    }

    #[test]
    fn schedule_day_not_in_flags_inactive() {
        let sched = Schedule {
            days: DaysOfWeek::MON
                | DaysOfWeek::TUE
                | DaysOfWeek::WED
                | DaysOfWeek::THU
                | DaysOfWeek::FRI,
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 9, 12, 0, 0).unwrap();
        assert!(!sched.contains(now));
    }

    #[test]
    fn schedule_crosses_midnight_late_evening_active() {
        let sched = Schedule {
            days: DaysOfWeek::MON,
            start: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 11, 23, 30, 0).unwrap();
        assert!(sched.contains(now));
    }

    #[test]
    fn schedule_crosses_midnight_early_morning_uses_yesterday_flag() {
        let sched = Schedule {
            days: DaysOfWeek::MON,
            start: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 12, 1, 0, 0).unwrap();
        assert!(sched.contains(now));
    }

    #[test]
    fn schedule_at_exact_end_inactive() {
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        };
        let now = Local.with_ymd_and_hms(2026, 5, 11, 18, 0, 0).unwrap();
        assert!(!sched.contains(now));
    }

    #[test]
    fn days_of_week_default_is_empty() {
        // Pins the contract that an unconfigured `DaysOfWeek` is the
        // empty set, not an arbitrary sentinel. `Modifiers::default()`
        // relies on this so a fresh-install config has no implicit
        // active days that would silently widen the schedule semantics.
        assert_eq!(DaysOfWeek::default(), DaysOfWeek::empty());
        assert!(!DaysOfWeek::default().contains(DaysOfWeek::MON));
    }

    #[test]
    fn schedule_contains_dispatches_all_weekdays_today() {
        // The today_flag match in `Schedule::contains` has an arm per
        // weekday. Walking a 7-day stretch (Mon → Sun, 2026-05-11..17)
        // forces every arm to dispatch. With days=all() and a window
        // covering noon, every iteration should return true. If a
        // future edit reordered or dropped an arm, this test would
        // catch it for every weekday at once instead of only the
        // specific day the existing tests happen to hit.
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(23, 59, 59).unwrap(),
        };
        for day in 11..=17u32 {
            let now = Local.with_ymd_and_hms(2026, 5, day, 12, 0, 0).unwrap();
            assert!(
                sched.contains(now),
                "noon on 2026-05-{day} should be inside an all-days, all-day window",
            );
        }
    }

    #[test]
    fn schedule_cross_midnight_dispatches_all_yesterday_arms() {
        // When start > end the schedule wraps midnight, and the
        // implementation looks at YESTERDAY's day-of-week for early-
        // morning samples. Cycling 7 consecutive days at 01:00 dispatches
        // every arm of the yesterday match. Companion to the today-arm
        // test above — together they pin the full weekday-dispatch
        // surface of `Schedule::contains`.
        let sched = Schedule {
            days: DaysOfWeek::all(),
            start: NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        };
        for day in 12..=18u32 {
            let now = Local.with_ymd_and_hms(2026, 5, day, 1, 0, 0).unwrap();
            assert!(
                sched.contains(now),
                "01:00 on 2026-05-{day} should be inside a 22:00-02:00 window with all days set",
            );
        }
    }
}
