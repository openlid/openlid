//! Wraps `/usr/bin/pmset` invocations. Pulled out so we can stub in tests.

use anyhow::{anyhow, Result};
use std::process::Command;

pub trait Pmset: Send + Sync {
    fn set_disable_sleep(&self, enabled: bool) -> Result<()>;
    fn read_disable_sleep(&self) -> Result<bool>;
}

pub struct RealPmset;

impl Pmset for RealPmset {
    fn set_disable_sleep(&self, enabled: bool) -> Result<()> {
        let arg = if enabled { "1" } else { "0" };
        let out = Command::new("/usr/bin/pmset")
            .args(["-a", "disablesleep", arg])
            .output()?;
        if !out.status.success() {
            return Err(anyhow!(
                "pmset disablesleep {arg} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(())
    }

    fn read_disable_sleep(&self) -> Result<bool> {
        let out = Command::new("/usr/bin/pmset").arg("-g").output()?;
        if !out.status.success() {
            return Err(anyhow!(
                "pmset -g failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.trim_start().starts_with("SleepDisabled") {
                let last = line.split_whitespace().last().unwrap_or("");
                return Ok(last == "1");
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    pub struct FakePmset {
        pub enabled: Mutex<bool>,
        pub set_calls: Mutex<Vec<bool>>,
    }
    impl FakePmset {
        pub fn new() -> Self {
            Self {
                enabled: Mutex::new(false),
                set_calls: Mutex::new(Vec::new()),
            }
        }
    }
    impl Pmset for FakePmset {
        fn set_disable_sleep(&self, enabled: bool) -> Result<()> {
            self.set_calls.lock().unwrap().push(enabled);
            *self.enabled.lock().unwrap() = enabled;
            Ok(())
        }
        fn read_disable_sleep(&self) -> Result<bool> {
            Ok(*self.enabled.lock().unwrap())
        }
    }

    #[test]
    fn fake_records_calls() {
        let p = FakePmset::new();
        p.set_disable_sleep(true).unwrap();
        p.set_disable_sleep(false).unwrap();
        assert_eq!(*p.set_calls.lock().unwrap(), vec![true, false]);
        assert!(!p.read_disable_sleep().unwrap());
    }
}
