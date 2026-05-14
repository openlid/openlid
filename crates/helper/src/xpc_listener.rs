//! NSXPC listener: accepts connections from the menubar app, validates the
//! caller via SecCode requirement, and dispatches the three protocol methods
//! into a `HelperImpl<P: Pmset>`.
//!
//! Architecture (mirrors the Phase-0 spike):
//!
//! 1. [`HelperImpl`] holds the live state — pmset wrapper, ownership marker,
//!    idle-exit timer, client validator. Public methods `handle_set_sleep_prevention`
//!    and `handle_get_status` encapsulate the sequencing required by the
//!    crash-recovery design (write marker before disabling sleep, remove
//!    marker after re-enabling sleep).
//! 2. The `define_class!`-generated `Exported` NSObject subclass implements
//!    the three protocol methods. It dispatches into a type-erased
//!    [`HelperHandle`] trait object.
//! 3. The `define_class!`-generated `ListenerDelegate` accepts incoming
//!    connections, performs SecCode-based audit-token validation, and wires
//!    each connection with the protocol interface plus a fresh `Exported`.
//! 4. A connection counter tracks open connections. On every successful
//!    `shouldAcceptNewConnection`, the counter increments and the idle-exit
//!    timer is disarmed. On every invalidation/interruption, the counter
//!    decrements; when it drops to zero, the idle-exit timer is re-armed.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use block2::{Block, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass};
use objc2_foundation::{
    NSObject, NSRunLoop, NSString, NSXPCConnection, NSXPCInterface, NSXPCListener,
    NSXPCListenerDelegate,
};

use crate::client_validator::ClientValidator;
use crate::idle_exit::IdleExit;
use crate::ownership_marker::OwnershipMarker;
use crate::pmset::Pmset;

/// Type-erased view of the three operations the listener needs to call.
/// Lets us hide the `P: Pmset` generic from the Obj-C class boundary.
trait HelperHandle: Send + Sync {
    fn handle_set_sleep_prevention(&self, enabled: bool) -> anyhow::Result<()>;
    fn handle_get_status(&self) -> anyhow::Result<bool>;
    fn validator(&self) -> &ClientValidator;
    fn idle_exit(&self) -> &IdleExit;
}

/// Concrete helper state. Generic over the [`Pmset`] implementation so tests
/// can substitute a fake.
pub struct HelperImpl<P: Pmset + 'static> {
    pub pmset: Arc<P>,
    pub marker: Arc<OwnershipMarker>,
    pub idle_exit: IdleExit,
    pub validator: Arc<ClientValidator>,
}

impl<P: Pmset + 'static> HelperImpl<P> {
    /// Apply a sleep-prevention change. The order matters for crash recovery:
    /// on `enabled=true`, write the marker FIRST; on `enabled=false`, remove
    /// the marker LAST. That way, the marker is present iff the system is in
    /// the "sleep-disabled" state, which is what the startup recovery logic
    /// in `main.rs` checks.
    pub fn handle_set_sleep_prevention(&self, enabled: bool) -> anyhow::Result<()> {
        if enabled {
            self.marker.write()?;
            self.pmset.set_disable_sleep(true)?;
        } else {
            self.pmset.set_disable_sleep(false)?;
            self.marker.remove()?;
        }
        Ok(())
    }

    /// Returns true when sleep is currently disabled by `pmset -a disablesleep 1`.
    pub fn handle_get_status(&self) -> anyhow::Result<bool> {
        self.pmset.read_disable_sleep()
    }
}

impl<P: Pmset + 'static> HelperHandle for HelperImpl<P> {
    fn handle_set_sleep_prevention(&self, enabled: bool) -> anyhow::Result<()> {
        HelperImpl::handle_set_sleep_prevention(self, enabled)
    }
    fn handle_get_status(&self) -> anyhow::Result<bool> {
        HelperImpl::handle_get_status(self)
    }
    fn validator(&self) -> &ClientValidator {
        &self.validator
    }
    fn idle_exit(&self) -> &IdleExit {
        &self.idle_exit
    }
}

/// Shared state stored in the Obj-C ivar of both `Exported` and
/// `ListenerDelegate`. We use a non-generic trait object so the macro-defined
/// classes don't need to be parametric.
struct Shared {
    helper: Arc<dyn HelperHandle>,
    /// Number of currently-active accepted connections. When this drops to
    /// zero, the idle-exit timer is armed; while it is non-zero, the timer
    /// is disarmed.
    active_connections: Arc<AtomicUsize>,
}

impl Shared {
    fn new(helper: Arc<dyn HelperHandle>, active_connections: Arc<AtomicUsize>) -> Self {
        Self {
            helper,
            active_connections,
        }
    }
}

/// Increment connection count and disarm idle-exit. Called on accept.
fn on_connection_opened(shared: &Shared) {
    let prev = shared.active_connections.fetch_add(1, Ordering::SeqCst);
    if prev == 0 {
        shared.helper.idle_exit().disarm();
    }
    tracing::info!("connection opened (active={})", prev + 1);
}

/// Decrement connection count and arm idle-exit when it falls to zero.
fn on_connection_closed(shared: &Shared) {
    let prev = shared.active_connections.fetch_sub(1, Ordering::SeqCst);
    let now = prev.saturating_sub(1);
    tracing::info!("connection closed (active={})", now);
    if now == 0 {
        shared.helper.idle_exit().arm(|| {
            tracing::info!("idle-exit timer fired; exiting");
            std::process::exit(0);
        });
    }
}

// audit_token_t — NSXPCConnection's `auditToken` property returns a 32-byte
// struct. On macOS 10.15+ this property is public:
// https://developer.apple.com/documentation/foundation/nsxpcconnection/audittoken
//
// objc2-foundation 0.3 doesn't expose it in its generated bindings yet, so we
// invoke it via raw `msg_send!`. The struct layout matches `audit_token_t`
// from `<bsm/audit.h>` / `<mach/message.h>`: eight `uint32_t` words.
#[repr(C)]
#[derive(Clone, Copy)]
struct AuditTokenT([u32; 8]);

// SAFETY: AuditTokenT is a plain 32-byte POD value with the same layout as
// Apple's `audit_token_t`, declared as `struct audit_token { unsigned int val[8]; }`.
// We use the matching encoding `{?=[8I]}` so objc2's debug-build signature
// verification accepts it.
unsafe impl objc2::encode::Encode for AuditTokenT {
    const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Struct(
        "?",
        &[objc2::encode::Encoding::Array(
            8,
            &<u32 as objc2::encode::Encode>::ENCODING,
        )],
    );
}

fn extract_audit_token(conn: &NSXPCConnection) -> Option<[u8; 32]> {
    // NSXPCConnection.auditToken is available on macOS 10.15+. Verify the
    // selector exists on this runtime before invoking — guards against the
    // (very unlikely) case that we ship to an older OS without trapping with
    // an unrecognized-selector NSException.
    let sel_audit_token = sel!(auditToken);
    let responds: bool = unsafe { msg_send![conn, respondsToSelector: sel_audit_token] };
    if !responds {
        tracing::warn!("NSXPCConnection does not expose auditToken; cannot validate caller");
        return None;
    }
    // SAFETY: `auditToken` is a public, no-argument property returning a
    // 32-byte `audit_token_t`. The `AuditTokenT` repr matches.
    let token: AuditTokenT = unsafe { msg_send![conn, auditToken] };
    // SAFETY: AuditTokenT is `#[repr(C)]` of `[u32; 8]`, layout-compatible
    // with `[u8; 32]`.
    let bytes: [u8; 32] = unsafe { std::mem::transmute(token) };
    Some(bytes)
}

define_class!(
    /// Object exported on each accepted XPC connection. Implements the three
    /// methods of `OpenLidHelperProtocol` by delegating into the type-erased
    /// `HelperHandle` stored in its ivar.
    #[unsafe(super(NSObject))]
    #[name = "OpenLidHelperExported"]
    #[ivars = Shared]
    struct Exported;

    impl Exported {
        // setSleepPreventionEnabled:withReply:
        //
        // Reply block signature is `void(^)(BOOL ok, NSString * _Nullable error)`,
        // which Clang emits as `v20@?0B8@"NSString"12`. We don't need to set
        // that encoding ourselves — we are the *callee* receiving a block
        // proxy that NSXPC built on the other side. We only need to invoke
        // it correctly.
        //
        // The NSString* error parameter is nullable, so we use `*mut NSString`
        // and pass null on success.
        #[unsafe(method(setSleepPreventionEnabled:withReply:))]
        fn set_sleep_prevention(
            &self,
            enabled: Bool,
            reply: NonNull<Block<dyn Fn(Bool, *mut NSString)>>,
        ) {
            let helper = &self.ivars().helper;
            match helper.handle_set_sleep_prevention(enabled.as_bool()) {
                Ok(()) => {
                    tracing::info!("set_sleep_prevention enabled={} ok", enabled.as_bool());
                    // SAFETY: reply block is provided by NSXPC on the receiving
                    // side; it expects exactly the encoded signature. Calling
                    // with (BOOL, NSString*) where the second is null on
                    // success matches the protocol declaration.
                    unsafe {
                        reply.as_ref().call((Bool::YES, std::ptr::null_mut()));
                    }
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    tracing::warn!("set_sleep_prevention failed: {msg}");
                    let ns_err = NSString::from_str(&msg);
                    // SAFETY: see above. We pass a retained NSString pointer;
                    // NSXPC retains/copies as needed before serializing.
                    unsafe {
                        reply
                            .as_ref()
                            .call((Bool::NO, Retained::as_ptr(&ns_err) as *mut NSString));
                    }
                }
            }
        }

        // getSleepPreventionStatusWithReply:
        //
        // Reply block signature is `void(^)(BOOL ok, BOOL active, NSString * _Nullable error)`,
        // which Clang emits as `v24@?0B8B12@"NSString"16`. Same comment as
        // above re: encoding string.
        #[unsafe(method(getSleepPreventionStatusWithReply:))]
        fn get_status(
            &self,
            reply: NonNull<Block<dyn Fn(Bool, Bool, *mut NSString)>>,
        ) {
            let helper = &self.ivars().helper;
            match helper.handle_get_status() {
                Ok(active) => {
                    tracing::info!("get_status active={active}");
                    // SAFETY: reply block matches protocol declaration.
                    unsafe {
                        reply.as_ref().call((Bool::YES, Bool::new(active), std::ptr::null_mut()));
                    }
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    tracing::warn!("get_status failed: {msg}");
                    let ns_err = NSString::from_str(&msg);
                    // SAFETY: see above.
                    unsafe {
                        reply.as_ref().call((
                            Bool::NO,
                            Bool::NO,
                            Retained::as_ptr(&ns_err) as *mut NSString,
                        ));
                    }
                }
            }
        }

        // pingWithReply:
        //
        // Reply block signature is `void(^)(void)`, which Clang emits as
        // `v8@?0`. We just call it with no arguments.
        #[unsafe(method(pingWithReply:))]
        fn ping(&self, reply: NonNull<Block<dyn Fn()>>) {
            // SAFETY: reply block matches protocol declaration.
            unsafe {
                reply.as_ref().call(());
            }
        }
    }

    unsafe impl NSObjectProtocol for Exported {}
);

impl Exported {
    fn new(shared: Shared) -> Retained<Self> {
        let this = Self::alloc().set_ivars(shared);
        // SAFETY: NSObject's init returns Self.
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    /// NSXPCListenerDelegate. On `shouldAcceptNewConnection`, validates the
    /// caller via SecCode, sets the connection's exported interface and
    /// object, and wires invalidation/interruption handlers that drive the
    /// idle-exit timer.
    #[unsafe(super(NSObject))]
    #[name = "OpenLidHelperListenerDelegate"]
    #[ivars = Shared]
    struct ListenerDelegate;

    impl ListenerDelegate {}

    unsafe impl NSObjectProtocol for ListenerDelegate {}

    unsafe impl NSXPCListenerDelegate for ListenerDelegate {
        // The Obj-C signature is `-(BOOL)listener:shouldAcceptNewConnection:`
        // and `define_class!` rewrites the body so a `bool` becomes a `Bool`
        // at the ABI boundary. We use `Bool` directly here so explicit
        // `return Bool::NO;` paths work cleanly.
        #[unsafe(method(listener:shouldAcceptNewConnection:))]
        fn listener_should_accept_new_connection(
            &self,
            _listener: &NSXPCListener,
            new_connection: &NSXPCConnection,
        ) -> Bool {
            let shared = self.ivars();

            // 1. Validate the caller via SecCode. Without an audit token we
            // cannot uniquely identify the calling process, so reject.
            let token = match extract_audit_token(new_connection) {
                Some(t) => t,
                None => {
                    tracing::warn!("rejecting connection: could not extract audit token");
                    return Bool::NO;
                }
            };
            if !shared.helper.validator().allows(token) {
                tracing::warn!("rejecting connection: client failed code requirement");
                return Bool::NO;
            }

            tracing::info!("accepting new XPC connection");

            // 2. Configure the connection with our protocol and exported object.
            // SAFETY: `protocol()` returns a Clang-emitted Protocol that
            // matches the methods on `Exported` declared above.
            let interface = unsafe {
                NSXPCInterface::interfaceWithProtocol(open_lid_helper_protocol::protocol())
            };
            new_connection.setExportedInterface(Some(&interface));

            // Each connection gets its own Exported instance, but they all
            // share the same Helper (cloned Arc) and connection counter.
            let exported = Exported::new(Shared::new(
                Arc::clone(&shared.helper),
                Arc::clone(&shared.active_connections),
            ));
            // SAFETY: `exported` conforms to OpenLidHelperProtocol via define_class!.
            unsafe {
                new_connection.setExportedObject(Some(&*exported));
            }

            // 3. Wire connection lifecycle handlers. These blocks are invoked
            // by NSXPC when the connection invalidates or interrupts; they
            // are NOT reply blocks, so RcBlock::new (without an encoding) is
            // sufficient — NSXPC does not introspect them.
            let counter_inv = Arc::clone(&shared.active_connections);
            let helper_inv = Arc::clone(&shared.helper);
            let invalidation = RcBlock::new(move || {
                let snapshot = Shared::new(Arc::clone(&helper_inv), Arc::clone(&counter_inv));
                on_connection_closed(&snapshot);
            });
            new_connection.setInvalidationHandler(Some(&invalidation));

            let counter_int = Arc::clone(&shared.active_connections);
            let helper_int = Arc::clone(&shared.helper);
            let interruption = RcBlock::new(move || {
                tracing::warn!("XPC connection interrupted");
                let snapshot = Shared::new(Arc::clone(&helper_int), Arc::clone(&counter_int));
                on_connection_closed(&snapshot);
            });
            new_connection.setInterruptionHandler(Some(&interruption));

            // 4. Mark connection as open and disarm idle-exit. Done last so
            // that any earlier failure path doesn't leave the counter
            // mis-tracking.
            on_connection_opened(shared);

            // 5. Resume the connection so it starts accepting messages.
            new_connection.resume();
            Bool::YES
        }
    }
);

impl ListenerDelegate {
    fn new(shared: Shared) -> Retained<Self> {
        let this = Self::alloc().set_ivars(shared);
        // SAFETY: NSObject's init returns Self.
        unsafe { msg_send![super(this), init] }
    }
}

/// Block forever running an NSXPC listener for `mach_service_name`. The
/// process will exit (via `std::process::exit(0)`) when the idle-exit timer
/// fires after `IDLE_EXIT_SECS` of zero active connections.
pub fn run_listener<P: Pmset + Send + Sync + 'static>(
    helper: HelperImpl<P>,
    mach_service_name: &str,
) -> anyhow::Result<()> {
    let helper_handle: Arc<dyn HelperHandle> = Arc::new(helper);
    let active_connections = Arc::new(AtomicUsize::new(0));

    let service_name = NSString::from_str(mach_service_name);
    let listener = NSXPCListener::initWithMachServiceName(NSXPCListener::alloc(), &service_name);

    let delegate = ListenerDelegate::new(Shared::new(
        Arc::clone(&helper_handle),
        Arc::clone(&active_connections),
    ));
    let delegate_proto: &ProtocolObject<dyn NSXPCListenerDelegate> =
        ProtocolObject::from_ref(&*delegate);
    listener.setDelegate(Some(delegate_proto));

    // setDelegate stores a weak reference. Leak the Retained so the delegate
    // outlives the run loop (which runs forever; the OS reclaims everything
    // when the process exits).
    let _delegate_leak = Box::leak(Box::new(delegate));

    listener.resume();
    tracing::info!("NSXPC listener resumed on Mach service {mach_service_name}; entering run loop");

    // Block forever. The only exit path is `std::process::exit(0)` from the
    // idle-exit timer, or a fatal signal from launchd.
    let run_loop = NSRunLoop::currentRunLoop();
    run_loop.run();

    // run_loop.run() never returns under normal operation. If it ever does,
    // surface that as an error so launchd can observe a non-zero exit.
    Err(anyhow::anyhow!("NSRunLoop returned unexpectedly"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pmset::tests::FakePmset;
    use tempfile::tempdir;

    fn make_impl(marker_path: std::path::PathBuf) -> HelperImpl<FakePmset> {
        HelperImpl {
            pmset: Arc::new(FakePmset::new()),
            marker: Arc::new(OwnershipMarker::at(&marker_path)),
            idle_exit: IdleExit::new(),
            validator: Arc::new(ClientValidator::new(r#"identifier "io.openlid.app""#)),
        }
    }

    #[test]
    fn set_sleep_prevention_true_writes_marker_then_disables_sleep() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker.flag");
        let helper = make_impl(marker.clone());

        helper.handle_set_sleep_prevention(true).unwrap();

        assert!(
            marker.exists(),
            "marker should be written before pmset toggle"
        );
        assert!(
            *helper.pmset.enabled.lock().unwrap(),
            "pmset should be set to disablesleep=1"
        );
        assert_eq!(*helper.pmset.set_calls.lock().unwrap(), vec![true]);
    }

    #[test]
    fn set_sleep_prevention_false_reenables_sleep_then_removes_marker() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker.flag");
        let helper = make_impl(marker.clone());

        // First arm it, so the marker exists and pmset has been toggled.
        helper.handle_set_sleep_prevention(true).unwrap();
        assert!(marker.exists());

        helper.handle_set_sleep_prevention(false).unwrap();

        assert!(
            !marker.exists(),
            "marker should be removed after re-enabling sleep"
        );
        assert!(!*helper.pmset.enabled.lock().unwrap());
        assert_eq!(*helper.pmset.set_calls.lock().unwrap(), vec![true, false]);
    }

    #[test]
    fn get_status_reflects_pmset_state() {
        let dir = tempdir().unwrap();
        let helper = make_impl(dir.path().join("marker.flag"));

        assert!(!helper.handle_get_status().unwrap());
        helper.handle_set_sleep_prevention(true).unwrap();
        assert!(helper.handle_get_status().unwrap());
    }

    // ─────────────────────────────────────────────────────────────────────
    // Failure paths. These tests *codify* the contracts on partial
    // failure of handle_set_sleep_prevention: the marker tracks "we
    // attempted to flip pmset", and it stays on disk until a *successful*
    // re-enable. Removing the marker after a failed pmset call would lie
    // about the system's sleep state.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn set_true_returns_err_when_pmset_fails_and_keeps_marker() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker.flag");
        let helper = make_impl(marker.clone());
        helper.pmset.fail_set.store(true, Ordering::SeqCst);

        let err = helper.handle_set_sleep_prevention(true).unwrap_err();
        assert!(
            format!("{err:#}").contains("pmset set_disable_sleep failure"),
            "unexpected error: {err:#}",
        );

        // CONTRACT: marker was written before the pmset attempt, and it
        // stays on disk.
        assert!(marker.exists(), "marker must remain after partial failure");
        // FakePmset does not record failed calls.
        assert!(helper.pmset.set_calls.lock().unwrap().is_empty());
    }

    #[test]
    fn set_false_returns_err_when_pmset_fails_and_keeps_marker() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker.flag");
        let helper = make_impl(marker.clone());

        // Enable cleanly so marker exists and pmset reports disabled-sleep.
        helper.handle_set_sleep_prevention(true).unwrap();
        assert!(marker.exists());
        assert!(*helper.pmset.enabled.lock().unwrap());

        // Now make the disable-pmset call fail.
        helper.pmset.fail_set.store(true, Ordering::SeqCst);
        let err = helper.handle_set_sleep_prevention(false).unwrap_err();
        assert!(format!("{err:#}").contains("pmset set_disable_sleep failure"));

        // CONTRACT: marker must NOT be removed if we failed to re-enable
        // sleep. The marker existing means pmset is still in the disabled
        // state; removing it now would lie.
        assert!(marker.exists(), "marker must remain when re-enable fails");
        assert!(
            *helper.pmset.enabled.lock().unwrap(),
            "pmset state should still report disabled-sleep",
        );
    }

    #[test]
    fn set_true_returns_err_when_marker_write_fails_and_pmset_not_called() {
        let dir = tempdir().unwrap();
        // Plant a regular file where the marker's parent directory should
        // be — `create_dir_all` inside OwnershipMarker::write will fail
        // because the parent path is a file, not a directory.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"").unwrap();
        let marker = blocker.join("marker.flag");
        let helper = make_impl(marker.clone());

        let _ = helper
            .handle_set_sleep_prevention(true)
            .expect_err("marker.write should fail when parent is a regular file");

        // CONTRACT: pmset must NOT be called if the marker write failed.
        // The marker write is the "claim ownership" step.
        assert!(helper.pmset.set_calls.lock().unwrap().is_empty());
        assert!(!*helper.pmset.enabled.lock().unwrap());
    }

    // ─────────────────────────────────────────────────────────────────────
    // Connection counter — drives idle_exit arm/disarm
    // ─────────────────────────────────────────────────────────────────────

    fn make_shared(marker_path: std::path::PathBuf) -> (Arc<dyn HelperHandle>, Shared) {
        let helper = make_impl(marker_path);
        let helper_handle: Arc<dyn HelperHandle> = Arc::new(helper);
        let active = Arc::new(AtomicUsize::new(0));
        let shared = Shared::new(Arc::clone(&helper_handle), Arc::clone(&active));
        (helper_handle, shared)
    }

    #[test]
    fn open_from_zero_disarms_idle_exit() {
        let dir = tempdir().unwrap();
        let (helper, shared) = make_shared(dir.path().join("marker.flag"));

        // Pre-arm so the disarm is observable. The 15-s exit closure
        // wouldn't fire before the test ends; on_connection_opened disarms
        // it before that anyway.
        helper.idle_exit().arm(|| {});
        assert!(helper.idle_exit().is_armed());

        on_connection_opened(&shared);

        assert_eq!(shared.active_connections.load(Ordering::SeqCst), 1);
        assert!(
            !helper.idle_exit().is_armed(),
            "first connection should disarm idle_exit",
        );
    }

    #[test]
    fn open_while_active_does_not_touch_idle_exit() {
        let dir = tempdir().unwrap();
        let (helper, shared) = make_shared(dir.path().join("marker.flag"));
        // Simulate one already-open connection.
        shared.active_connections.store(1, Ordering::SeqCst);
        assert!(!helper.idle_exit().is_armed());

        on_connection_opened(&shared);

        assert_eq!(shared.active_connections.load(Ordering::SeqCst), 2);
        assert!(
            !helper.idle_exit().is_armed(),
            "subsequent connections must not change idle_exit state",
        );
    }

    #[test]
    fn close_to_zero_arms_idle_exit() {
        let dir = tempdir().unwrap();
        let (helper, shared) = make_shared(dir.path().join("marker.flag"));
        shared.active_connections.store(1, Ordering::SeqCst);
        assert!(!helper.idle_exit().is_armed());

        on_connection_closed(&shared);

        assert_eq!(shared.active_connections.load(Ordering::SeqCst), 0);
        assert!(
            helper.idle_exit().is_armed(),
            "last close should arm idle_exit",
        );
        // Disarm so the 15-s exit timer (which would call
        // std::process::exit(0)) doesn't fire mid-test-suite.
        helper.idle_exit().disarm();
    }

    #[test]
    fn close_while_still_active_does_not_arm() {
        let dir = tempdir().unwrap();
        let (helper, shared) = make_shared(dir.path().join("marker.flag"));
        shared.active_connections.store(2, Ordering::SeqCst);
        assert!(!helper.idle_exit().is_armed());

        on_connection_closed(&shared);

        assert_eq!(shared.active_connections.load(Ordering::SeqCst), 1);
        assert!(
            !helper.idle_exit().is_armed(),
            "must not arm while still active",
        );
    }
}
