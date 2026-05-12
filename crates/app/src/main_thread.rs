//! Hop a closure to the AppKit main thread via libdispatch.
//!
//! AppKit (NSStatusItem, NSImage, NSMenu) is documented main-thread-only.
//! Callers from background threads (UDS control server, NSXPC reply blocks,
//! anything not on the main run loop) MUST funnel UI updates through here.
//!
//! Implemented with raw `dispatch_async_f` FFI to avoid pulling in a heavier
//! Objective-C block dependency. `dispatch_get_main_queue` is a C macro that
//! expands to `&_dispatch_main_q`, so we reference the global directly.

use std::ffi::c_void;

#[link(name = "System", kind = "dylib")]
unsafe extern "C" {
    static _dispatch_main_q: c_void;

    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: unsafe extern "C" fn(*mut c_void),
    );
}

fn main_queue() -> *const c_void {
    // SAFETY: `_dispatch_main_q` is a stable, process-lifetime global in
    // libdispatch (part of libSystem). Taking its address is always valid.
    unsafe { &_dispatch_main_q as *const c_void }
}

/// Schedule `f` to run on the AppKit main thread at the next run-loop tick.
/// Returns immediately; the closure runs asynchronously.
pub fn run_on_main<F: FnOnce() + Send + 'static>(f: F) {
    // Two-layer box: the inner `Box<dyn FnOnce>` is needed to type-erase the
    // closure; the outer `Box` gives us a stable raw pointer to hand to C.
    let outer: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(f));
    let context = Box::into_raw(outer) as *mut c_void;
    unsafe {
        dispatch_async_f(main_queue(), context, trampoline);
    }
}

unsafe extern "C" fn trampoline(context: *mut c_void) {
    let outer: Box<Box<dyn FnOnce() + Send>> = unsafe { Box::from_raw(context as *mut _) };
    (*outer)();
}
