//! Translates `OPEN_LID_HELPER_PROFILE=dev` into a `cfg(helper_profile_dev)`
//! at compile time. Anything else (or unset) keeps the production-strict
//! code-requirement string active — fail-safe by default.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(helper_profile_dev)");
    println!("cargo:rerun-if-env-changed=OPEN_LID_HELPER_PROFILE");

    let profile = std::env::var("OPEN_LID_HELPER_PROFILE").unwrap_or_default();
    if profile == "dev" {
        println!("cargo:rustc-cfg=helper_profile_dev");
        println!(
            "cargo:warning=Building openlid-helper with DEV code-requirement \
             (permissive). Set OPEN_LID_HELPER_PROFILE=prod or leave it unset \
             for release builds."
        );
    }
}
