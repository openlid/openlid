//! Detect how openlid was installed on this machine.
//!
//! Three cases:
//!   * `Homebrew` — the .app at `/Applications/OpenLid.app` is a
//!     symlink into either `/usr/local/Caskroom/openlid/...` (Intel
//!     brew) or `/opt/homebrew/Caskroom/openlid/...` (Apple-silicon
//!     brew). For these users we surface `brew upgrade openlid` and
//!     stop -- never fight `brew`.
//!   * `Manual` — the .app lives at `/Applications/OpenLid.app`
//!     directly. We take over the install ourselves.
//!   * `Dev` — running from a non-Applications path (e.g.
//!     `target/bundle/OpenLid.app`). The updater refuses to install
//!     so a dev build doesn't accidentally clobber a checked-out
//!     source tree.
//!
//! The split between the impure `detect()` (which canonicalises a real
//! filesystem path) and the pure `classify(...)` keeps the bulk of the
//! logic unit-testable.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMethod {
    Homebrew,
    Manual,
    /// The running binary is not under `/Applications/OpenLid.app`,
    /// e.g. a developer running from `target/`. The path is carried so
    /// the user can be told exactly what we saw.
    Dev {
        path: PathBuf,
    },
}

/// The canonical install location for end-user builds.
pub const APP_PATH: &str = "/Applications/OpenLid.app";

/// Canonical Homebrew Caskroom subpaths. Both prefixes can coexist on
/// a single machine, but only one of them owns a given install.
const HOMEBREW_CASKROOM_PREFIXES: &[&str] = &[
    "/usr/local/Caskroom/openlid",
    "/opt/homebrew/Caskroom/openlid",
];

/// Classify a *canonical* application path. Pure: takes the path the
/// caller already resolved with `fs::canonicalize`, returns the
/// install method without touching the filesystem.
pub fn classify(canonical: &Path) -> InstallMethod {
    let s = canonical.to_string_lossy();
    for prefix in HOMEBREW_CASKROOM_PREFIXES {
        if s.starts_with(prefix) {
            return InstallMethod::Homebrew;
        }
    }
    if canonical == Path::new(APP_PATH) {
        return InstallMethod::Manual;
    }
    InstallMethod::Dev {
        path: canonical.to_path_buf(),
    }
}

/// Best-effort detection: canonicalize `/Applications/OpenLid.app` and
/// classify the result. If the path doesn't exist (developer environment,
/// uninstalled) we fall back to the current executable's path so the
/// `Dev` branch reports something useful.
pub fn detect() -> InstallMethod {
    let canonical = std::fs::canonicalize(APP_PATH)
        .or_else(|_| std::env::current_exe().and_then(std::fs::canonicalize))
        .unwrap_or_else(|_| PathBuf::from(APP_PATH));
    classify(&canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_intel_homebrew_caskroom_is_homebrew() {
        let p = PathBuf::from("/usr/local/Caskroom/openlid/2.0.0/OpenLid.app");
        assert_eq!(classify(&p), InstallMethod::Homebrew);
    }

    #[test]
    fn classify_arm_homebrew_caskroom_is_homebrew() {
        let p = PathBuf::from("/opt/homebrew/Caskroom/openlid/2.0.0/OpenLid.app");
        assert_eq!(classify(&p), InstallMethod::Homebrew);
    }

    #[test]
    fn classify_applications_root_is_manual() {
        // The DMG-install path: the .app lives directly under
        // /Applications with no Caskroom indirection. This is the
        // common case for users who downloaded the DMG themselves.
        let p = PathBuf::from("/Applications/OpenLid.app");
        assert_eq!(classify(&p), InstallMethod::Manual);
    }

    #[test]
    fn classify_dev_target_path_is_dev() {
        // A developer running from a workspace `target/` directory
        // must not have their checked-out tree replaced by the
        // updater. Classify as Dev so the CLI refuses with a clear
        // message.
        let p = PathBuf::from("/Users/dev/code/openlid/target/bundle/OpenLid.app");
        match classify(&p) {
            InstallMethod::Dev { path } => assert_eq!(path, p),
            other => panic!("expected Dev, got {other:?}"),
        }
    }

    #[test]
    fn classify_non_caskroom_but_homebrew_prefix_is_not_homebrew() {
        // A path that contains `/usr/local/` but NOT
        // `/usr/local/Caskroom/openlid` must not be misclassified. The
        // check is on the Caskroom prefix specifically.
        let p = PathBuf::from("/usr/local/share/something/OpenLid.app");
        assert!(!matches!(classify(&p), InstallMethod::Homebrew));
    }

    #[test]
    fn classify_application_support_path_is_not_misclassified() {
        // The data dir at `~/Library/Application Support/...` shares a
        // prefix substring with `/Applications/OpenLid.app`. The
        // classifier compares full paths, not substrings, so a path
        // like this must NOT classify as `Manual`.
        let p = PathBuf::from("/Users/x/Library/Application Support/io.openlid.app");
        assert!(!matches!(classify(&p), InstallMethod::Manual));
    }
}
