use bitflags::bitflags;
use chrono::{DateTime, Local, NaiveTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    LidClosed,
    AlwaysAwake,
    Timed { until: DateTime<Local> },
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LidState {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PowerSource {
    Ac,
    Battery { percent: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn mode_lid_closed_round_trips() {
        let m = Mode::LidClosed;
        let s = toml::to_string(&m).unwrap();
        let back: Mode = toml::from_str(&s).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn mode_timed_round_trips() {
        let until = Local.with_ymd_and_hms(2026, 5, 10, 18, 0, 0).unwrap();
        let m = Mode::Timed { until };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("timed"));
        let back: Mode = serde_json::from_str(&s).unwrap();
        assert_eq!(m, back);
    }

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
}
