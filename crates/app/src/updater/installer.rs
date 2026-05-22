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
    let mut out = std::fs::File::create(dest)
        .with_context(|| format!("creating {}", dest.display()))?;
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
    let script_path =
        std::env::temp_dir().join(format!("openlid-installer-{parent_pid}.sh"));
    let log_path =
        std::env::temp_dir().join(format!("openlid-installer-{parent_pid}.log"));

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
        let out = render_installer_script(
            1,
            Path::new("/tmp/x.dmg"),
            "/Applications/OpenLid.app",
        );
        assert!(
            !out.contains("__"),
            "output still contains a `__` placeholder: {out}"
        );
    }

    #[test]
    fn render_installer_script_starts_with_shebang() {
        // Sanity: a missing shebang line would make the kernel reject
        // the script. Pin the contract so a future edit can't drop it.
        let out = render_installer_script(
            1,
            Path::new("/tmp/x.dmg"),
            "/Applications/OpenLid.app",
        );
        assert!(out.starts_with("#!/bin/sh"), "got: {}", &out[..40]);
    }

    #[test]
    fn render_installer_script_preserves_atomic_swap_pattern() {
        // The destructive `rm -rf` of the live app path must be
        // preceded by a stage step. Pin this so a future refactor
        // can't accidentally remove the staging step and leave a
        // partial install on disk.
        let out = render_installer_script(
            1,
            Path::new("/tmp/x.dmg"),
            "/Applications/OpenLid.app",
        );
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
