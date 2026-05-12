//! NSXPC client wrapping the connection to `io.openlid.helper`.
//!
//! See `docs/superpowers/notes/2026-05-10-objc2-xpc-spike-findings.md` for
//! the patterns this code uses. The most important takeaway: any reply block
//! we hand to NSXPC must be created with `RcBlock::with_encoding(...)` so its
//! descriptor carries `BLOCK_HAS_SIGNATURE`. `RcBlock::new` produces an
//! "unsigned" block, which NSXPC rejects with the cryptic exception
//! "Block was not compiled using a compiler that inserts type information
//! about arguments".
//!
//! The encoding strings used here mirror exactly what Clang emits for the
//! protocol declared in `crates/helper-protocol/objc/OpenLidHelperProtocol.h`
//! and what `crates/helper/src/xpc_listener.rs` documents in its method
//! handlers. If the protocol changes, BOTH ends must update their encodings.
//!
//! ## Sync wrapper around async NSXPC
//!
//! NSXPC is inherently asynchronous: you obtain a remote-object proxy with
//! `remoteObjectProxyWithErrorHandler:` and then call methods on it, passing
//! a reply block that fires when the helper responds. We bridge this to a
//! synchronous Rust API by using a `mpsc::sync_channel(1)` per call:
//!
//! 1. Build a (tx, rx) channel.
//! 2. The error handler (fires on connection / proxy errors) and the reply
//!    block (fires on success) BOTH try to `send` on a `Mutex<Option<Sender>>`.
//!    Whichever fires first wins; the other observes `None` and is a no-op.
//! 3. The caller blocks on `rx.recv_timeout`.
//!
//! This is the same pattern as the spike's `call_ping`, replicated three times
//! for the three protocol methods.

use block2::{ManualBlockEncoding, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{msg_send, AnyThread};
use objc2_foundation::{
    NSError, NSString, NSXPCConnection, NSXPCConnectionOptions, NSXPCInterface,
};
use open_lid_core::platform::{PlatformError, PowerController};
use std::ffi::CStr;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const HELPER_MACH_SERVICE_NAME: &str = "io.openlid.helper";
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Block encodings — one per reply-block signature.
//
// The strings come from Clang's emission for the protocol; they are reproduced
// in `crates/helper/src/xpc_listener.rs` comments. Changing the protocol means
// regenerating both sides.

/// `void (^)(BOOL ok, NSString * _Nullable error)` — reply for
/// `setSleepPreventionEnabled:withReply:`.
struct ReplyOkErr;
// SAFETY: argument tuple `(Bool, *mut NSString)` matches the block's
// `(BOOL, NSString*)` and return is void. The encoding string is what Clang
// emits when compiling the protocol header.
unsafe impl ManualBlockEncoding for ReplyOkErr {
    type Arguments = (Bool, *mut NSString);
    type Return = ();
    const ENCODING_CSTR: &'static CStr = c"v20@?0B8@\"NSString\"12";
}

/// `void (^)(BOOL ok, BOOL active, NSString * _Nullable error)` — reply for
/// `getSleepPreventionStatusWithReply:`. Used by `HelperClient::get_status`,
/// which the menubar does not yet call (Plan 2 will).
#[allow(dead_code)]
struct ReplyOkActiveErr;
// SAFETY: argument tuple matches `(BOOL, BOOL, NSString*)` and return is void.
unsafe impl ManualBlockEncoding for ReplyOkActiveErr {
    type Arguments = (Bool, Bool, *mut NSString);
    type Return = ();
    const ENCODING_CSTR: &'static CStr = c"v24@?0B8B12@\"NSString\"16";
}

/// `void (^)(void)` — reply for `pingWithReply:`. Used by `HelperClient::ping`,
/// reserved for Plan 2's health check (the MVP doesn't ping explicitly).
#[allow(dead_code)]
struct ReplyEmpty;
// SAFETY: argument tuple is unit (no arguments) and return is void.
unsafe impl ManualBlockEncoding for ReplyEmpty {
    type Arguments = ();
    type Return = ();
    const ENCODING_CSTR: &'static CStr = c"v8@?0";
}

// ---------------------------------------------------------------------------
// Channel reply slot used to bridge async NSXPC -> sync Rust.
//
// The error handler and reply block both try to deposit a result here. The
// mutex / `take()` pattern ensures only the first to fire wins; the second is
// a no-op (because the slot is `None`). This avoids `mpsc::SyncSender::send`
// panicking on a full channel of capacity 1, AND prevents the receiver from
// observing two messages.

type Slot<T> = Arc<Mutex<Option<mpsc::SyncSender<T>>>>;

fn fill_slot<T>(slot: &Slot<T>, value: T) {
    // Take the sender out of the slot exactly once. If it has already been
    // taken (i.e. the other handler already fired), drop `value` silently.
    let sender = match slot.lock() {
        Ok(mut g) => g.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    };
    if let Some(s) = sender {
        // try_send: the channel has capacity 1, but a poisoned `Some` would
        // be impossible (we always `take` before the second sender attempt).
        // Use `send` for clarity; failure (closed receiver) is unrecoverable
        // but harmless — the caller has already given up.
        let _ = s.send(value);
    }
}

// ---------------------------------------------------------------------------
// HelperClient — owns one persistent NSXPCConnection.

/// NSXPC client to the privileged helper. One connection per `HelperClient`,
/// kept open for the lifetime of the value. NSXPC reconnects on demand if the
/// helper exits, so we do not need to recreate this between calls.
pub struct HelperClient {
    conn: Retained<NSXPCConnection>,
    /// Tracks whether the connection has been invalidated. When `true`, all
    /// `set_sleep_prevention` / `get_status` / `ping` calls short-circuit
    /// with `PlatformError::HelperUnavailable` instead of attempting an XPC
    /// send. This is critical when the helper isn't installed: without the
    /// check, `msg_send!` against the degenerate __NSXPCInterfaceProxy panics
    /// with "method not found" rather than returning an error, taking down
    /// the menu bar app before it has a chance to render.
    invalidated: Arc<AtomicBool>,
}

// SAFETY: NSXPCConnection is documented as thread-safe for `remoteObjectProxy`
// calls and message sends. The proxy itself is single-use per call, but
// `HelperClient::*` methods construct a fresh proxy each time.
unsafe impl Send for HelperClient {}
unsafe impl Sync for HelperClient {}

impl HelperClient {
    /// Build a new client bound to the `io.openlid.helper` Mach service.
    ///
    /// Uses `kNSXPCConnectionPrivileged` (`1 << 12`) because the helper is
    /// installed in `/Library/LaunchDaemons/` (system domain). Without this
    /// flag, NSXPC looks up the service in the per-user domain, doesn't find
    /// it, and invalidates the connection before launchd can route the
    /// bootstrap. The Mach lookup itself is non-blocking; the actual
    /// handshake (and helper launch by launchd) happens lazily on first
    /// message send.
    pub fn new() -> anyhow::Result<Self> {
        let service_name = NSString::from_str(HELPER_MACH_SERVICE_NAME);
        let conn = NSXPCConnection::initWithMachServiceName_options(
            NSXPCConnection::alloc(),
            &service_name,
            NSXPCConnectionOptions(1 << 12),
        );

        // The remote interface metadata MUST be a Clang-emitted Protocol —
        // see the spike findings doc, section 7a, on why `ProtocolBuilder`
        // is insufficient for NSXPC.
        // SAFETY: `open_lid_helper_protocol::protocol()` returns a static
        // pointer to the Clang-emitted `OpenLidHelperProtocol` metadata.
        let interface =
            unsafe { NSXPCInterface::interfaceWithProtocol(open_lid_helper_protocol::protocol()) };
        conn.setRemoteObjectInterface(Some(&interface));

        // Lifecycle handlers. These are NOT reply blocks, so NSXPC does not
        // introspect their signatures — plain `RcBlock::new` is sufficient.
        let invalidated = Arc::new(AtomicBool::new(false));
        let invalidation_flag = Arc::clone(&invalidated);
        let invalidation_block = RcBlock::new(move || {
            invalidation_flag.store(true, Ordering::SeqCst);
            tracing::warn!("XPC connection to helper invalidated");
        });
        conn.setInvalidationHandler(Some(&invalidation_block));

        let interruption_block = RcBlock::new(|| {
            tracing::warn!("XPC connection to helper interrupted");
        });
        conn.setInterruptionHandler(Some(&interruption_block));

        conn.resume();

        tracing::info!("HelperClient connected to {HELPER_MACH_SERVICE_NAME}");

        Ok(HelperClient { conn, invalidated })
    }

    /// Returns true if this connection has been observed to be invalid. We
    /// check this proactively before each XPC call to avoid the "method not
    /// found" panic on a degenerate proxy.
    fn is_unavailable(&self) -> bool {
        self.invalidated.load(Ordering::SeqCst)
    }

    /// Toggle sleep prevention on the helper. Blocks up to `REPLY_TIMEOUT`.
    pub fn set_sleep_prevention(&self, enabled: bool) -> Result<(), PlatformError> {
        if self.is_unavailable() {
            return Err(PlatformError::HelperUnavailable);
        }
        let (tx, rx) = mpsc::sync_channel::<Result<(), PlatformError>>(1);
        let slot: Slot<Result<(), PlatformError>> = Arc::new(Mutex::new(Some(tx)));

        // Error handler — fires on proxy / connection errors before the reply
        // could ever be invoked. Either this or the reply block, never both.
        let err_slot = Arc::clone(&slot);
        let err_handler = RcBlock::new(move |err: NonNull<NSError>| {
            let msg = unsafe { err.as_ref() }.localizedDescription().to_string();
            tracing::warn!("set_sleep_prevention proxy error: {msg}");
            fill_slot(
                &err_slot,
                Err(PlatformError::Native(format!("xpc proxy error: {msg}"))),
            );
        });

        // Reply block — fires on success path. Must use `with_encoding` so
        // NSXPC accepts it; see ReplyOkErr above.
        let reply_slot = Arc::clone(&slot);
        let reply_block =
            RcBlock::with_encoding::<_, _, _, ReplyOkErr>(move |ok: Bool, err: *mut NSString| {
                if ok.as_bool() {
                    fill_slot(&reply_slot, Ok(()));
                } else {
                    let msg = if err.is_null() {
                        "helper reported failure with no error message".to_string()
                    } else {
                        // SAFETY: NSXPC guarantees this pointer is either null
                        // or a valid NSString proxy for the duration of the
                        // reply block call. We copy out the UTF-8 immediately.
                        unsafe { &*err }.to_string()
                    };
                    fill_slot(&reply_slot, Err(PlatformError::Native(msg)));
                }
            });

        // `remoteObjectProxyWithErrorHandler:` returns a single-use proxy
        // bound to the error handler. The error handler block must outlive
        // the call below; we keep `err_handler` alive on the stack until
        // after recv_timeout returns.
        let proxy: Retained<AnyObject> = self.conn.remoteObjectProxyWithErrorHandler(&err_handler);

        // SAFETY: the selector matches the Clang-emitted protocol that this
        // connection's remoteObjectInterface was configured with. `Bool::new`
        // gives us the BOOL ABI type, and `&*reply_block` is a `&Block<...>`
        // with the correct signature.
        //
        // catch_unwind: if the connection invalidated between our
        // `is_unavailable` check and now (race window with the async
        // invalidation handler), objc2's runtime check on the degenerate
        // proxy panics with "method not found". Catch it, mark the connection
        // dead, return a clean error.
        let send_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            let _: () = msg_send![
                &*proxy,
                setSleepPreventionEnabled: Bool::new(enabled),
                withReply: &*reply_block
            ];
        }));
        if send_result.is_err() {
            self.invalidated.store(true, Ordering::SeqCst);
            return Err(PlatformError::HelperUnavailable);
        }

        recv_or_timeout(&rx)
    }

    /// Query the current sleep-prevention state from the helper.
    #[allow(dead_code)] // Reserved for Plan 2 health check.
    pub fn get_status(&self) -> Result<bool, PlatformError> {
        if self.is_unavailable() {
            return Err(PlatformError::HelperUnavailable);
        }
        let (tx, rx) = mpsc::sync_channel::<Result<bool, PlatformError>>(1);
        let slot: Slot<Result<bool, PlatformError>> = Arc::new(Mutex::new(Some(tx)));

        let err_slot = Arc::clone(&slot);
        let err_handler = RcBlock::new(move |err: NonNull<NSError>| {
            let msg = unsafe { err.as_ref() }.localizedDescription().to_string();
            tracing::warn!("get_status proxy error: {msg}");
            fill_slot(
                &err_slot,
                Err(PlatformError::Native(format!("xpc proxy error: {msg}"))),
            );
        });

        let reply_slot = Arc::clone(&slot);
        let reply_block = RcBlock::with_encoding::<_, _, _, ReplyOkActiveErr>(
            move |ok: Bool, active: Bool, err: *mut NSString| {
                if ok.as_bool() {
                    fill_slot(&reply_slot, Ok(active.as_bool()));
                } else {
                    let msg = if err.is_null() {
                        "helper reported failure with no error message".to_string()
                    } else {
                        // SAFETY: see note in set_sleep_prevention.
                        unsafe { &*err }.to_string()
                    };
                    fill_slot(&reply_slot, Err(PlatformError::Native(msg)));
                }
            },
        );

        let proxy: Retained<AnyObject> = self.conn.remoteObjectProxyWithErrorHandler(&err_handler);

        // SAFETY: as in set_sleep_prevention.
        let send_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            let _: () = msg_send![
                &*proxy,
                getSleepPreventionStatusWithReply: &*reply_block
            ];
        }));
        if send_result.is_err() {
            self.invalidated.store(true, Ordering::SeqCst);
            return Err(PlatformError::HelperUnavailable);
        }

        recv_or_timeout(&rx)
    }

    /// Round-trip ping. Reserved for Plan 2 (the MVP menubar relies on the
    /// connection coming up implicitly on the first `set_sleep_prevention`).
    #[allow(dead_code)]
    pub fn ping(&self) -> Result<(), PlatformError> {
        if self.is_unavailable() {
            return Err(PlatformError::HelperUnavailable);
        }
        let (tx, rx) = mpsc::sync_channel::<Result<(), PlatformError>>(1);
        let slot: Slot<Result<(), PlatformError>> = Arc::new(Mutex::new(Some(tx)));

        let err_slot = Arc::clone(&slot);
        let err_handler = RcBlock::new(move |err: NonNull<NSError>| {
            let msg = unsafe { err.as_ref() }.localizedDescription().to_string();
            tracing::warn!("ping proxy error: {msg}");
            fill_slot(
                &err_slot,
                Err(PlatformError::Native(format!("xpc proxy error: {msg}"))),
            );
        });

        let reply_slot = Arc::clone(&slot);
        let reply_block = RcBlock::with_encoding::<_, _, _, ReplyEmpty>(move || {
            fill_slot(&reply_slot, Ok(()));
        });

        let proxy: Retained<AnyObject> = self.conn.remoteObjectProxyWithErrorHandler(&err_handler);

        // SAFETY: as in set_sleep_prevention.
        let send_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            let _: () = msg_send![&*proxy, pingWithReply: &*reply_block];
        }));
        if send_result.is_err() {
            self.invalidated.store(true, Ordering::SeqCst);
            return Err(PlatformError::HelperUnavailable);
        }

        recv_or_timeout(&rx)
    }
}

impl Drop for HelperClient {
    fn drop(&mut self) {
        // Politely close the connection. NSXPC would clean up anyway when the
        // last reference drops, but explicit `invalidate()` lets the helper
        // observe the close and arm its idle-exit timer immediately.
        self.conn.invalidate();
    }
}

/// Block on `rx` for at most `REPLY_TIMEOUT`. Maps the two error cases to
/// `PlatformError`. Used by all three RPC methods.
fn recv_or_timeout<T>(rx: &mpsc::Receiver<Result<T, PlatformError>>) -> Result<T, PlatformError> {
    match rx.recv_timeout(REPLY_TIMEOUT) {
        Ok(r) => r,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(PlatformError::Native(format!(
            "helper did not reply within {:?}",
            REPLY_TIMEOUT
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(PlatformError::HelperUnavailable),
    }
}

// ---------------------------------------------------------------------------
// PowerController adapter
//
// The state_runtime (Task 21) talks to `dyn PowerController`. This wrapper
// adapts a shared `HelperClient` into that trait so the runtime can call
// `prevent_sleep` / `allow_sleep` without knowing about XPC at all.

pub struct HelperPowerController {
    client: Arc<HelperClient>,
}

impl HelperPowerController {
    pub fn new(client: Arc<HelperClient>) -> Self {
        Self { client }
    }
}

impl PowerController for HelperPowerController {
    fn prevent_sleep(&self) -> Result<(), PlatformError> {
        self.client.set_sleep_prevention(true)
    }

    fn allow_sleep(&self) -> Result<(), PlatformError> {
        self.client.set_sleep_prevention(false)
    }
}
