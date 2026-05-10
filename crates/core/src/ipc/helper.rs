//! Protocol between the menubar process and the privileged helper.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "method", rename_all = "kebab-case")]
pub enum HelperRequest {
    SetSleepPrevention { enabled: bool },
    GetStatus,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum HelperResponse {
    SetSleepPreventionOk,
    StatusOk { sleep_prevention_active: bool },
    Pong,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_sleep_prevention_request_serializes() {
        let r = HelperRequest::SetSleepPrevention { enabled: true };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("set-sleep-prevention"));
        let back: HelperRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }
}
