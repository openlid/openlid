//! Wraps `SCNetworkReachability` to observe Internet reachability and
//! deliver bool callbacks when interface state changes.
//!
//! The target host is `apple.com` -- a stable, non-openlid host so a
//! user inspecting their network activity does not see openlid making
//! reachability lookups against our own infrastructure. The API is
//! passive: it observes interface link / routing state and does not
//! generate outbound traffic on its own.
//!
//! Threading: callbacks fire on the main run loop. The user-supplied
//! callback is invoked on that same thread, so consumers should not
//! perform blocking work inside it -- spawn a worker if needed.

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoopGetMain};
use openlid_core::platform::{NetworkMonitor, NetworkStateCallback};
use std::ffi::{c_void, CString};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};

// ─────────────────────────────────────────────────────────────────────
// FFI declarations. `SCNetworkReachability` lives in the
// SystemConfiguration framework, which is part of every macOS system.
// We only need a handful of functions and the flags-bitfield contract.
// ─────────────────────────────────────────────────────────────────────

#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    fn SCNetworkReachabilityCreateWithName(
        allocator: *const c_void,
        nodename: *const c_char,
    ) -> *mut c_void;

    fn SCNetworkReachabilityGetFlags(target: *const c_void, flags: *mut u32) -> bool;

    fn SCNetworkReachabilitySetCallback(
        target: *const c_void,
        callout: Option<extern "C" fn(target: *const c_void, flags: u32, info: *mut c_void)>,
        context: *const SCNetworkReachabilityContext,
    ) -> bool;

    fn SCNetworkReachabilityScheduleWithRunLoop(
        target: *const c_void,
        runloop: *mut c_void,
        mode: *const c_void,
    ) -> bool;

    fn CFRelease(cf: *const c_void);
}

#[repr(C)]
struct SCNetworkReachabilityContext {
    version: i64,
    info: *mut c_void,
    retain: Option<extern "C" fn(*const c_void) -> *const c_void>,
    release: Option<extern "C" fn(*const c_void)>,
    copy_description: Option<extern "C" fn(*const c_void) -> *const c_void>,
}

// Reachability flags. Apple's headers define more, but we only need
// the two that determine "is the Internet currently reachable from
// any interface?". `Reachable` says a route exists; `ConnectionRequired`
// says the route requires bringing up a connection (e.g. dial-up) that
// hasn't been attempted -- treat that as unreachable until the system
// proves otherwise.
const SC_REACHABLE: u32 = 1 << 1; // kSCNetworkReachabilityFlagsReachable
const SC_CONNECTION_REQUIRED: u32 = 1 << 2; // kSCNetworkReachabilityFlagsConnectionRequired

/// Translate raw reachability flags into a single bool. Pure -- the
/// SCNetworkReachability framework's combinatorial-flag API is
/// notoriously easy to misread, so we pin the interpretation in a
/// unit test rather than scattering bit-ops at every call site.
fn flags_to_reachable(flags: u32) -> bool {
    (flags & SC_REACHABLE) != 0 && (flags & SC_CONNECTION_REQUIRED) == 0
}

pub struct MacNetworkMonitor {
    inner: Arc<Mutex<Inner>>,
    // The SCNetworkReachability target. Held so its CFRelease fires on
    // drop. Not Send/Sync naturally; we assert it's only touched on the
    // main thread via the inner-arc handoff pattern.
    target: *mut c_void,
}

struct Inner {
    callback: Option<NetworkStateCallback>,
}

unsafe impl Send for MacNetworkMonitor {}
unsafe impl Sync for MacNetworkMonitor {}
unsafe impl Send for Inner {}

impl MacNetworkMonitor {
    pub fn start() -> anyhow::Result<Self> {
        let inner = Arc::new(Mutex::new(Inner { callback: None }));

        // The probe target. apple.com is stable, non-openlid, and
        // cached by the OS resolver -- the lookup is effectively free
        // after the first call.
        let host = CString::new("apple.com").expect("string literal contains no NUL");
        let target =
            unsafe { SCNetworkReachabilityCreateWithName(std::ptr::null(), host.as_ptr()) };
        if target.is_null() {
            anyhow::bail!("SCNetworkReachabilityCreateWithName returned null");
        }

        // Register the callback. `info` is a raw pointer to the Arc<Mutex<Inner>>.
        // We leak one ref into the framework and reclaim it on drop.
        let refcon = Arc::into_raw(Arc::clone(&inner)) as *mut c_void;
        let ctx = SCNetworkReachabilityContext {
            version: 0,
            info: refcon,
            retain: None,
            release: None,
            copy_description: None,
        };
        let ok = unsafe { SCNetworkReachabilitySetCallback(target, Some(Self::on_change), &ctx) };
        if !ok {
            unsafe { CFRelease(target as *const c_void) };
            // Reclaim the leaked Arc so we don't grow the strong-count.
            let _ = unsafe { Arc::from_raw(refcon as *const Mutex<Inner>) };
            anyhow::bail!("SCNetworkReachabilitySetCallback failed");
        }

        let ok = unsafe {
            SCNetworkReachabilityScheduleWithRunLoop(
                target,
                CFRunLoopGetMain() as *mut c_void,
                kCFRunLoopCommonModes as *const c_void,
            )
        };
        if !ok {
            unsafe { CFRelease(target as *const c_void) };
            let _ = unsafe { Arc::from_raw(refcon as *const Mutex<Inner>) };
            anyhow::bail!("SCNetworkReachabilityScheduleWithRunLoop failed");
        }

        Ok(Self { inner, target })
    }

    /// Read flags from the target right now. Returns `true` if any
    /// interface reports a reachable, no-connection-required path.
    fn read_current(target: *const c_void) -> bool {
        if target.is_null() {
            // Defensive: never claim unreachable when we don't know.
            // The in-transit detector would fire incorrectly otherwise.
            return true;
        }
        let mut flags: u32 = 0;
        let ok = unsafe { SCNetworkReachabilityGetFlags(target, &mut flags) };
        if !ok {
            return true;
        }
        flags_to_reachable(flags)
    }

    extern "C" fn on_change(_target: *const c_void, flags: u32, info: *mut c_void) {
        if info.is_null() {
            return;
        }
        let inner = unsafe { Arc::from_raw(info as *const Mutex<Inner>) };
        let cb = inner.lock().unwrap().callback.clone();
        // Don't drop the ref -- we're handing it back to the framework.
        std::mem::forget(inner);
        if let Some(cb) = cb {
            cb(flags_to_reachable(flags));
        }
    }
}

impl Drop for MacNetworkMonitor {
    fn drop(&mut self) {
        // Best-effort cleanup of the CF target. The leaked Arc held by
        // the framework is not reclaimed here -- doing so safely would
        // require unscheduling first, which would touch the main
        // run loop from drop. For an app-lifetime singleton this is
        // acceptable; the OS reclaims everything on process exit.
        if !self.target.is_null() {
            unsafe { CFRelease(self.target as *const c_void) };
            self.target = std::ptr::null_mut();
        }
    }
}

impl NetworkMonitor for MacNetworkMonitor {
    fn is_reachable(&self) -> bool {
        Self::read_current(self.target)
    }

    fn subscribe(&self, callback: NetworkStateCallback) {
        self.inner.lock().unwrap().callback = Some(callback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_to_reachable_true_for_clean_reachable_flag() {
        assert!(flags_to_reachable(SC_REACHABLE));
    }

    #[test]
    fn flags_to_reachable_false_when_only_connection_required_set() {
        // "Reachable but requires a connection" means we don't have
        // a live route yet -- treat as unreachable until the system
        // proves otherwise. Mis-reading this bit is a classic
        // SCNetworkReachability footgun.
        assert!(!flags_to_reachable(SC_REACHABLE | SC_CONNECTION_REQUIRED));
    }

    #[test]
    fn flags_to_reachable_false_when_nothing_set() {
        // The all-zero case is the "no route at all" reading; must
        // be unreachable.
        assert!(!flags_to_reachable(0));
    }

    #[test]
    fn flags_to_reachable_ignores_unrelated_bits() {
        // SCNetworkReachability defines a dozen other flags
        // (TransientConnection, IsLocalAddress, etc). Our predicate
        // only cares about two of them; future additions must not
        // accidentally flip our boolean.
        let other_bits: u32 = 0xF0; // some unrelated high bits
        assert!(flags_to_reachable(SC_REACHABLE | other_bits));
        assert!(!flags_to_reachable(other_bits));
    }
}
