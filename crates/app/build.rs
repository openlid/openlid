// Link ServiceManagement.framework so the `SMAppService` class is available
// at runtime.
//
// `helper_installer` resolves SMAppService dynamically via
// `AnyClass::get("SMAppService")` to avoid pulling in a wrapper crate, but a
// runtime class lookup only succeeds if the framework is already loaded into
// the process. Nothing else in the dependency graph links ServiceManagement,
// so without this directive `objc_getClass("SMAppService")` returns NULL and
// every helper registration fails with "class not available (macOS 13+
// required)". Older macOS loaded it transitively (via AppKit); macOS 26 does
// not, which turned the missing link into a hard failure: the privileged
// helper never registers, so openlid silently stops preventing sleep.
// See crates/app/src/helper_installer.rs.
fn main() {
    // Only link the Apple framework for macOS targets, so a future
    // Linux/Windows port doesn't try to resolve it.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=ServiceManagement");
    }
}
