//! DMG download, SHA-256 verification, and detached-installer-script
//! generation.
//!
//! The actual filesystem swap happens in a shell script (see
//! `installer_script.sh`) that this module renders and runs detached.
//! That gives us:
//!   * Survival of the parent's exit -- the parent is the `openlid
//!     update` process running INSIDE the bundle we're about to
//!     replace. We need a separate process whose binary isn't being
//!     swapped under it.
//!   * A single source of truth for the install steps. Other update
//!     entry points (menubar "Check for updates...") render and run
//!     the same script.

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the cache directory used to store the in-flight DMG.
/// `~/Library/Caches/io.openlid.app/updates/` on macOS via
/// `ProjectDirs`. Errors only if no home directory could be resolved.
pub fn cache_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("io", "openlid", "app")
        .ok_or_else(|| anyhow!("no home directory"))?;
    Ok(dirs.cache_dir().join("updates"))
}

/// Wipe and re-create the cache directory so we always download into
/// a known-empty location. Idempotent: safe to call when the cache
/// dir is missing or already empty.
pub fn prepare_cache(dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)
            .with_context(|| format!("removing stale cache dir at {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating cache dir at {}", dir.display()))?;
    Ok(())
}

/// Stream the asset at `url` to `dest`. Blocks until complete.
/// `ureq`'s default rustls backend handles HTTPS.
pub fn download(url: &str, dest: &Path) -> Result<()> {
    let mut resp = ureq::get(url)
        .header("User-Agent", concat!("openlid/", env!("CARGO_PKG_VERSION")))
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let mut reader = resp.body_mut().as_reader();
    let mut out =
        std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    std::io::copy(&mut reader, &mut out)
        .with_context(|| format!("writing to {}", dest.display()))?;
    Ok(())
}

/// Verify that `path` hashes to `expected_hex` (lowercase hex by
/// convention, but compared case-insensitively to be safe).
/// Returns an error spelling out both expected and actual values so a
/// mismatch is debuggable from a single log line.
pub fn verify_sha256(path: &Path, expected_hex: &str) -> Result<()> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("opening {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).context("reading file for hashing")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(anyhow!(
            "SHA-256 mismatch on {}: expected {expected_hex}, got {actual}",
            path.display()
        ))
    }
}

/// Render the installer shell script with the three runtime parameters
/// substituted in. Pure string templating -- snapshot-testable. The
/// resulting script body is what gets written to /tmp and run.
pub fn render_installer_script(parent_pid: u32, dmg_path: &Path, app_path: &str) -> String {
    let template = include_str!("installer_script.sh");
    template
        .replace("__PARENT_PID__", &parent_pid.to_string())
        .replace("__DMG_PATH__", &dmg_path.display().to_string())
        .replace("__APP_PATH__", app_path)
}

/// Pure path-builder for the installer script file. Lives under
/// `tmp_dir` (typically `std::env::temp_dir()`); the PID-based name
/// guards against concurrent updater invocations stepping on each
/// other's scripts.
pub fn installer_script_path(tmp_dir: &Path, parent_pid: u32) -> PathBuf {
    tmp_dir.join(format!("openlid-installer-{parent_pid}.sh"))
}

/// Pure path-builder for the installer log file. Same layout as the
/// script path; the user is pointed at this path when something goes
/// wrong so they can attach it to a bug report.
pub fn installer_log_path(tmp_dir: &Path, parent_pid: u32) -> PathBuf {
    tmp_dir.join(format!("openlid-installer-{parent_pid}.log"))
}

/// Write the rendered installer to /tmp and spawn it detached. After
/// this call returns, the parent process should exit promptly so the
/// installer can proceed past its `kill -0` wait loop.
pub fn spawn_detached_installer(
    parent_pid: u32,
    dmg_path: &Path,
    app_path: &str,
) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;

    let script = render_installer_script(parent_pid, dmg_path, app_path);
    let tmp = std::env::temp_dir();
    let script_path = installer_script_path(&tmp, parent_pid);
    let log_path = installer_log_path(&tmp, parent_pid);

    {
        let mut f = std::fs::File::create(&script_path)
            .with_context(|| format!("creating {}", script_path.display()))?;
        f.write_all(script.as_bytes())
            .context("writing installer script")?;
        let mut perms = f.metadata()?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }

    let log_file = std::fs::File::create(&log_path)?;
    let log_clone = log_file.try_clone()?;
    let mut cmd = Command::new("/bin/sh");
    cmd.arg(&script_path)
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_clone);
    unsafe {
        cmd.pre_exec(|| {
            // Detach from the controlling terminal so the script
            // survives the parent's exit and any TTY close.
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()
        .with_context(|| format!("spawning {}", script_path.display()))?;
    Ok(log_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_sha256_accepts_matching_hash() {
        // Compute the expected hash at test time so a regression in
        // the hasher surfaces here rather than via a hand-typed
        // constant going stale.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let content = b"openlid update test\n";
        std::fs::write(tmp.path(), content).unwrap();
        let expected = format!("{:x}", Sha256::digest(content));
        verify_sha256(tmp.path(), &expected).expect("known hash should match");
    }

    #[test]
    fn verify_sha256_rejects_mismatched_hash() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"some bytes").unwrap();
        let err = verify_sha256(tmp.path(), "deadbeef").unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("mismatch"),
            "error should call out the mismatch, got: {msg}"
        );
        assert!(
            msg.contains("deadbeef"),
            "error should include the expected value, got: {msg}"
        );
    }

    #[test]
    fn verify_sha256_is_case_insensitive() {
        // GitHub digests are lowercase by convention but other sources
        // emit uppercase hex. Comparing case-insensitively keeps a
        // future format change from causing spurious failures.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"hi").unwrap();
        let lower = format!("{:x}", Sha256::digest(b"hi"));
        let upper = lower.to_uppercase();
        verify_sha256(tmp.path(), &upper).expect("uppercase hash should match");
    }

    #[test]
    fn verify_sha256_errors_on_missing_file() {
        // A missing file is the most likely real failure mode (e.g.
        // download was aborted). Must surface as an error, not a
        // success-with-default-hash.
        let bogus = PathBuf::from("/nonexistent/openlid-test.dmg");
        let err = verify_sha256(&bogus, "deadbeef").unwrap_err();
        assert!(
            format!("{err:#}").contains("opening"),
            "error should mention opening the file"
        );
    }

    #[test]
    fn render_installer_script_substitutes_all_placeholders() {
        let out = render_installer_script(
            12345,
            Path::new("/tmp/OpenLid-v2.1.0.dmg"),
            "/Applications/OpenLid.app",
        );
        assert!(out.contains("PARENT_PID=\"12345\""));
        assert!(out.contains("DMG_PATH=\"/tmp/OpenLid-v2.1.0.dmg\""));
        assert!(out.contains("APP_PATH=\"/Applications/OpenLid.app\""));
    }

    #[test]
    fn render_installer_script_leaves_no_placeholder_strings() {
        // Defence against a typo in the template: if a future edit
        // misnames a placeholder, this test would catch it before the
        // wrong literal got embedded in /tmp.
        let out = render_installer_script(1, Path::new("/tmp/x.dmg"), "/Applications/OpenLid.app");
        assert!(
            !out.contains("__"),
            "output still contains a `__` placeholder: {out}"
        );
    }

    #[test]
    fn render_installer_script_starts_with_shebang() {
        // Sanity: a missing shebang line would make the kernel reject
        // the script. Pin the contract so a future edit can't drop it.
        let out = render_installer_script(1, Path::new("/tmp/x.dmg"), "/Applications/OpenLid.app");
        assert!(out.starts_with("#!/bin/sh"), "got: {}", &out[..40]);
    }

    #[test]
    fn render_installer_script_preserves_atomic_swap_pattern() {
        // The destructive `rm -rf` of the live app path must be
        // preceded by a stage step. Pin this so a future refactor
        // can't accidentally remove the staging step and leave a
        // partial install on disk.
        let out = render_installer_script(1, Path::new("/tmp/x.dmg"), "/Applications/OpenLid.app");
        let staging = out
            .find("cp -R \"$VOLUME_PATH/OpenLid.app\" \"${APP_PATH}.new\"")
            .expect("staging step missing");
        let destroy = out
            .find("rm -rf \"$APP_PATH\"")
            .expect("destroy step missing");
        let swap = out
            .find("mv \"${APP_PATH}.new\" \"$APP_PATH\"")
            .expect("swap step missing");
        assert!(staging < destroy, "staging must precede destroy");
        assert!(destroy < swap, "destroy must precede swap");
    }

    #[test]
    fn render_installer_script_verifies_signature_before_destructive_swap() {
        // Security contract: the installer downloads the DMG with an HTTP
        // client, so the bundle is NOT quarantined and macOS runs no
        // Gatekeeper assessment when we `open` it. The script must therefore
        // verify the Developer ID signature itself, pinned to OpenLid's Team
        // ID — the same anchor the privileged helper requires of XPC clients
        // — and it MUST do so before the destructive `rm -rf "$APP_PATH"`, so
        // a foreign-signed bundle is rejected with the user's app intact.
        let out = render_installer_script(1, Path::new("/tmp/x.dmg"), "/Applications/OpenLid.app");
        // Pin the full invocation, not loose substrings: the requirement must
        // actually be bound to the `codesign` call via -R (a defined-but-
        // unused requirement variable would silently degrade the check to
        // "any valid signature"), and --deep must stay so verification also
        // covers the nested openlid-helper binary that launchd runs as root.
        let verify = out
            .find(r#"codesign --verify --deep --strict -R="$OPENLID_CODE_REQUIREMENT" "$VOLUME_PATH/OpenLid.app""#)
            .expect("pinned codesign invocation missing");
        // The requirement is byte-for-byte the helper's PROD_REQUIREMENT
        // (crates/helper/src/main.rs) — keep the two in lockstep. The full
        // Developer ID chain matters: a check weakened to bare
        // `anchor apple generic` + Team ID would also accept bundles signed
        // with the team's Apple *development* certs, and dropping the Team
        // ID pin entirely would accept any Apple-signed app.
        assert!(
            out.contains(
                r#"OPENLID_CODE_REQUIREMENT='identifier "io.openlid.app" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */ and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = "X5SZL4562S"'"#
            ),
            "requirement must match the helper's PROD_REQUIREMENT, got: {out}"
        );
        let destroy = out
            .find("rm -rf \"$APP_PATH\"")
            .expect("destroy step missing");
        assert!(
            verify < destroy,
            "signature verification must precede the destructive swap"
        );
    }

    #[test]
    fn render_installer_script_verifies_dmg_signature_before_mounting() {
        // Security contract: the DMG arrives via the updater's HTTP client,
        // so nothing has assessed it when the script starts. Release DMGs are
        // codesign-signed in CI, so the script must verify the image's own
        // Developer ID signature BEFORE `hdiutil attach` — the disk-image
        // parser never touches unverified bytes (a tampered image could
        // otherwise probe for diskimages parsing bugs), and a rejected
        // download aborts before the script kills the user's running app.
        // This is a layer in FRONT of the bundle check, not a replacement:
        // the bundle check (pinned by the test above) must stay, since the
        // bundle is what actually gets installed.
        let out = render_installer_script(1, Path::new("/tmp/x.dmg"), "/Applications/OpenLid.app");
        // Pin the full invocation so the requirement can't silently come
        // unbound from the `codesign` call (same rationale as the bundle
        // check's test). No --deep: a disk image is a single code object.
        let verify = out
            .find(r#"codesign --verify --strict -R="$OPENLID_DMG_REQUIREMENT" "$DMG_PATH""#)
            .expect("pinned DMG codesign invocation missing");
        // The requirement is the helper's PROD_REQUIREMENT minus the
        // identifier clause: a DMG's signing identifier derives from its
        // filename (`OpenLid-v2` for OpenLid-v2.3.2.dmg), so it isn't stable
        // across releases. The Developer ID chain + Team ID pin is what makes
        // the check OpenLid-specific; dropping the Team ID would accept any
        // Apple-signed image.
        assert!(
            out.contains(
                r#"OPENLID_DMG_REQUIREMENT='anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */ and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = "X5SZL4562S"'"#
            ),
            "DMG requirement must pin the Developer ID chain + Team ID, got: {out}"
        );
        let mount = out.find("hdiutil attach").expect("mount step missing");
        assert!(
            verify < mount,
            "DMG signature verification must precede mounting"
        );
    }

    #[test]
    fn installer_requirements_stay_in_lockstep_with_helper_prod_requirement() {
        // Both codesign requirements in installer_script.sh derive from ONE
        // source of truth — the helper's `PROD_REQUIREMENT`
        // (crates/helper/src/main.rs), the anchor the root daemon enforces on
        // its XPC clients:
        //   * OPENLID_CODE_REQUIREMENT — the bundle check — is PROD_REQUIREMENT
        //     verbatim.
        //   * OPENLID_DMG_REQUIREMENT — the DMG-file check — is PROD_REQUIREMENT
        //     with the leading `identifier "io.openlid.app" and ` clause
        //     dropped (a DMG's signing identifier derives from its filename, so
        //     it isn't stable across releases).
        // The two tests above pin each requirement against a hardcoded literal,
        // so a Team-ID change or added cert pin in the *helper* would drift
        // silently — every test stays green while the updater verifies against
        // a different anchor than the root daemon. This closes that gap: extract
        // the helper's literal from source and assert both requirements track
        // it, so changing the helper without the installer fails the build.
        let helper_src = include_str!("../../../helper/src/main.rs");
        let marker = "const PROD_REQUIREMENT: &str = r#\"";
        let start = helper_src
            .find(marker)
            .expect("PROD_REQUIREMENT literal not found in helper/src/main.rs (renamed?)")
            + marker.len();
        let len = helper_src[start..]
            .find("\"#")
            .expect("unterminated PROD_REQUIREMENT raw string in helper/src/main.rs");
        let prod = &helper_src[start..start + len];
        // Sanity: we captured the real requirement, not an empty slice.
        assert!(
            prod.contains("X5SZL4562S") && prod.starts_with(r#"identifier "io.openlid.app""#),
            "extracted an unexpected PROD_REQUIREMENT: {prod:?}"
        );
        // The DMG requirement is PROD_REQUIREMENT minus the identifier clause.
        let dmg_req = prod
            .strip_prefix(r#"identifier "io.openlid.app" and "#)
            .expect("PROD_REQUIREMENT must start with the identifier clause the DMG check drops");

        let out = render_installer_script(1, Path::new("/tmp/x.dmg"), "/Applications/OpenLid.app");
        assert!(
            out.contains(prod),
            "the bundle check (OPENLID_CODE_REQUIREMENT) drifted from the helper's \
             PROD_REQUIREMENT.\n  helper: {prod}\n  must appear verbatim in installer_script.sh"
        );
        assert!(
            out.contains(dmg_req),
            "the DMG check (OPENLID_DMG_REQUIREMENT) drifted from the helper's \
             PROD_REQUIREMENT (minus the identifier clause).\n  expected: {dmg_req}"
        );
    }

    #[test]
    fn render_installer_script_waits_for_old_menubar_before_relaunch() {
        // `open -b io.openlid.app` can hit the single-instance guard if
        // the old menubar still owns the control socket. The installer
        // must wait for the old app process to disappear before swapping
        // and relaunching.
        let out = render_installer_script(1, Path::new("/tmp/x.dmg"), "/Applications/OpenLid.app");
        let exact_app_pattern = "APP_EXEC_RE=\"$APP_PATH/Contents/MacOS/openlid([[:space:]]|$)\"";
        let term = out.find("pkill -TERM -f \"$APP_EXEC_RE\"");
        let wait = out.find("while pgrep -f \"$APP_EXEC_RE\"");
        let swap = out
            .find("log \"swapping in the new bundle\"")
            .expect("swap step missing");
        let relaunch = out
            .find("log \"relaunching openlid\"")
            .expect("relaunch step missing");

        assert!(
            out.contains(exact_app_pattern),
            "script should match the app executable without also matching openlid-helper"
        );
        assert!(term.is_some(), "script should TERM the old menubar");
        assert!(
            wait.is_some(),
            "script should wait for the old menubar to exit"
        );
        assert!(term.unwrap() < wait.unwrap(), "wait must happen after TERM");
        assert!(
            wait.unwrap() < swap,
            "old menubar must be gone before swapping"
        );
        assert!(swap < relaunch, "relaunch must happen after the swap");
    }

    #[test]
    fn cache_dir_resolves_under_io_openlid_app() {
        // Pin the path location: a typo (e.g. `io.openlid.openlid`)
        // would point the cache somewhere users don't see when looking
        // in `~/Library/Caches/io.openlid.app/`.
        let dir = cache_dir().expect("ProjectDirs should resolve");
        let s = dir.to_string_lossy();
        assert!(
            s.contains("io.openlid.app") && s.ends_with("updates"),
            "got: {s}"
        );
    }

    #[test]
    fn prepare_cache_creates_missing_directory() {
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("updates");
        assert!(!dir.exists());
        prepare_cache(&dir).unwrap();
        assert!(dir.exists() && dir.is_dir());
    }

    #[test]
    fn installer_script_path_uses_parent_pid_for_uniqueness() {
        // Two concurrent updater invocations must not collide on the
        // same script path. The PID is the natural per-invocation ID.
        let tmp = PathBuf::from("/tmp");
        let a = installer_script_path(&tmp, 12345);
        let b = installer_script_path(&tmp, 67890);
        assert_ne!(a, b);
        assert!(a.to_string_lossy().contains("12345"));
        assert!(a.to_string_lossy().ends_with(".sh"));
    }

    #[test]
    fn installer_log_path_pairs_with_script_path() {
        // A user reading the log path printed at install time should
        // be able to find the script next to it by swapping the
        // extension. Pin that contract.
        let tmp = PathBuf::from("/tmp");
        let script = installer_script_path(&tmp, 42);
        let log = installer_log_path(&tmp, 42);
        assert_eq!(script.with_extension("log"), log);
    }

    #[test]
    fn prepare_cache_wipes_stale_contents() {
        // A previous failed download could leave a partial DMG behind.
        // Re-running `openlid update` must not see stale files.
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("updates");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("stale.dmg"), b"old bytes").unwrap();
        assert!(dir.join("stale.dmg").exists());
        prepare_cache(&dir).unwrap();
        assert!(!dir.join("stale.dmg").exists());
    }
}
