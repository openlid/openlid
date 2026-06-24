//! System-activity probe for the in-transit auto-disable detector.
//!
//! The in-transit rule treats "lid closed, on battery, no display, no
//! network for N minutes" as "probably in a backpack." A headless agent
//! running with the lid shut is the same shape — so we add an activity
//! guard: if the machine is doing real work, defer the auto-disable
//! instead of sleeping the Mac out from under the agent.
//!
//! The signal is the 1-minute load average per online CPU. The pure
//! threshold check ([`load_busy`]) is unit-tested; the FFI read in
//! [`system_busy`] is thin glue.

/// Load-average-per-CPU at or above which the machine counts as "doing
/// work." Intentionally low: a false "busy" only means a cool idle Mac
/// won't auto-disable in transit, while a false "idle" kills a running
/// agent. One sustained agent process should clear this on a many-core
/// Apple Silicon machine. Tunable.
const BUSY_LOAD_PER_CPU: f64 = 0.05;

/// Pure threshold check, split from the FFI so it can be unit-tested.
/// `online_cpus` is clamped to `>= 1` to avoid division by zero.
fn load_busy(load_1min: f64, online_cpus: usize) -> bool {
    let cpus = online_cpus.max(1) as f64;
    load_1min / cpus >= BUSY_LOAD_PER_CPU
}

/// True when the 1-minute load average per online CPU is at or above
/// [`BUSY_LOAD_PER_CPU`]. Reads `getloadavg(3)` and
/// `sysconf(_SC_NPROCESSORS_ONLN)`. On any read failure it fails safe to
/// `true` (busy), so the in-transit detector never auto-disables on a
/// bad reading — consistent with the agent-first bias.
pub fn system_busy() -> bool {
    let mut loads = [0f64; 3];
    // SAFETY: getloadavg writes up to `nelem` doubles into the buffer; we
    // pass a 3-element buffer and request 1.
    let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 1) };
    if n < 1 {
        return true;
    }
    // SAFETY: sysconf with a valid name returns the value or -1.
    let cpus = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    let cpus = if cpus > 0 { cpus as usize } else { 1 };
    load_busy(loads[0], cpus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_when_load_per_cpu_at_or_above_threshold() {
        // 0.5 load across 10 CPUs == 0.05/CPU == the threshold. `>=`
        // means the boundary counts as busy — bias toward keeping the
        // agent alive.
        assert!(load_busy(0.5, 10));
        assert!(load_busy(2.0, 10)); // comfortably above
    }

    #[test]
    fn idle_when_load_per_cpu_below_threshold() {
        // 0.2 load across 10 CPUs == 0.02/CPU < 0.05 → idle.
        assert!(!load_busy(0.2, 10));
        assert!(!load_busy(0.0, 8));
    }

    #[test]
    fn zero_cpus_does_not_divide_by_zero() {
        // Defensive: sysconf could theoretically return <= 0. Treat as 1
        // CPU so a positive load reads as busy rather than NaN/∞.
        assert!(load_busy(1.0, 0));
    }
}
