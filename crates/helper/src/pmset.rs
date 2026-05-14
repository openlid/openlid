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
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    pub struct FakePmset {
        pub enabled: Mutex<bool>,
        pub set_calls: Mutex<Vec<bool>>,
        /// Flip to `true` before calling `set_disable_sleep` to make the
        /// next (and subsequent) calls return `Err`. `set_calls` is NOT
        /// updated when the call fails, so tests can assert the failure
        /// happened *before* state was mutated.
        pub fail_set: AtomicBool,
        /// Same idea for `read_disable_sleep`.
        pub fail_read: AtomicBool,
    }
    impl FakePmset {
        pub fn new() -> Self {
            Self {
                enabled: Mutex::new(false),
                set_calls: Mutex::new(Vec::new()),
                fail_set: AtomicBool::new(false),
                fail_read: AtomicBool::new(false),
            }
        }
    }
    impl Pmset for FakePmset {
        fn set_disable_sleep(&self, enabled: bool) -> Result<()> {
            if self.fail_set.load(Ordering::SeqCst) {
                return Err(anyhow!("simulated pmset set_disable_sleep failure"));
            }
            self.set_calls.lock().unwrap().push(enabled);
            *self.enabled.lock().unwrap() = enabled;
            Ok(())
        }
        fn read_disable_sleep(&self) -> Result<bool> {
            if self.fail_read.load(Ordering::SeqCst) {
                return Err(anyhow!("simulated pmset read_disable_sleep failure"));
            }
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

    #[test]
    fn fake_can_simulate_set_failure() {
        let p = FakePmset::new();
        p.fail_set.store(true, Ordering::SeqCst);
        assert!(p.set_disable_sleep(true).is_err());
        // State stays untouched on failure.
        assert!(p.set_calls.lock().unwrap().is_empty());
        assert!(!*p.enabled.lock().unwrap());
    }

    #[test]
    fn fake_can_simulate_read_failure() {
        let p = FakePmset::new();
        p.fail_read.store(true, Ordering::SeqCst);
        assert!(p.read_disable_sleep().is_err());
    }
}
