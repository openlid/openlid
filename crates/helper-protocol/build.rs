// Compile a tiny Objective-C file that declares (and forces emission of) the
// `OpenLidHelperProtocol`. NSXPC requires the protocol's extended type
// information to be emitted by Clang — the dynamic `protocol_addMethodDescription`
// runtime API does NOT produce that metadata, so we let Clang do it.
fn main() {
    println!("cargo:rerun-if-changed=objc/OpenLidHelperProtocol.m");
    println!("cargo:rerun-if-changed=objc/OpenLidHelperProtocol.h");

    cc::Build::new()
        .file("objc/OpenLidHelperProtocol.m")
        .flag("-fobjc-arc")
        .compile("openlidhelperprotocol");

    println!("cargo:rustc-link-lib=framework=Foundation");
}
