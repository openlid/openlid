use objc2::runtime::AnyProtocol;

extern "C" {
    fn OpenLidHelperProtocol_get() -> *const AnyProtocol;
}

/// Return a static reference to the Clang-emitted `OpenLidHelperProtocol`
/// metadata. Both the helper (NSXPC listener) and the app (NSXPC client)
/// must call this to configure `NSXPCInterface::interfaceWithProtocol:`.
///
/// The underlying C function is defined in `objc/OpenLidHelperProtocol.m` and
/// compiled by `build.rs` via the `cc` crate. Clang emits the extended method
/// signature information that NSXPC requires; no runtime registration can
/// substitute for it.
///
/// # Safety
///
/// The returned pointer is a static Objective-C Protocol object interned by
/// the runtime. Its lifetime is `'static` and it is safe to dereference.
pub fn protocol() -> &'static AnyProtocol {
    // SAFETY: OpenLidHelperProtocol_get returns a pointer to a static Protocol
    // object that the Objective-C runtime interns at load time. The pointer is
    // never null and its lifetime is 'static.
    unsafe { &*OpenLidHelperProtocol_get() }
}
