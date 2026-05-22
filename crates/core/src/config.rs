use crate::mode::Modifiers;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Highest `config.toml` schema version this build knows about.
///
/// Bumped only when fields are removed or renamed (breaking changes).
/// Additive changes — new fields with `#[serde(default)]` — do not require
/// bumping this. See `docs/COMPATIBILITY.md`.
pub const SCHEMA_VERSION: u32 = 1;

/// Persisted user configuration. Lives at
/// `~/Library/Application Support/io.openlid.app/config.toml` on
/// macOS — the path is computed by `directories::ProjectDirs` from the
/// reverse-DNS triple `("io", "openlid", "app")`, which matches the
/// `Info.plist` bundle ID rather than the CLI/cask name. v1.x wrote
/// to `io.openlid.open-lid/`; [`Config::load_with_v1_fallback`] reads
/// from the v1 path on first launch when the v2 path doesn't exist
/// yet, then writes lands at v2 going forward.
///
/// Fields are partitioned into three groups:
///   * Toggle state: `enabled` (persisted so "Restore last state" on launch
///     works — the default for new installs).
///   * Modifier rules: `modifiers` (legacy from the mode-based design; the
///     only one actively wired in v1 is `min_battery`, exposed via the
///     `battery_threshold_pct` preference).
///   * UX preferences: `start_at_login`, `activate_at_launch`,
///     `battery_threshold_pct`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Schema version. Used to detect configs written by a newer binary.
    /// See `SCHEMA_VERSION` and the warn-on-newer branch in `Config::load`.
    #[serde(default = "default_schema_version")]
    pub version: u32,

    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub modifiers: Modifiers,

    /// Auto-launch the app on user login. Wired via SMAppService.loginItem
    /// (or LaunchAgents fallback for unsigned dev builds).
    #[serde(default)]
    pub start_at_login: bool,

    /// On every app launch, force `enabled = true` regardless of last
    /// persisted state. When `false` (the default), the last `enabled`
    /// value is restored — the standard "remember-last-state" convention
    /// users expect from menu-bar utilities.
    #[serde(default)]
    pub activate_at_launch: bool,

    /// Auto-deactivate when battery falls below this percent.
    /// `None` disables this safeguard.
    /// Once auto-deactivated, the toggle stays off until the user manually
    /// reactivates — we don't auto-reactivate on power restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_threshold_pct: Option<u8>,

    /// When `true` (the default), holding an `IOPMAssertion` keeps the
    /// display from going to sleep on idle while sleep prevention is active,
    /// which in turn prevents the screen lock from engaging. Released
    /// automatically when the lid closes and no external display is
    /// attached, so the battery-saving force-display-sleep on lid-close
    /// still takes effect.
    #[serde(default = "default_prevent_display_sleep")]
    pub prevent_display_sleep: bool,
}

/// Default version assumed when a config on disk has no `version` field.
///
/// Always returns `1`, NOT `SCHEMA_VERSION`. A versionless config in the
/// wild is by definition a pre-versioning (v1-era) config; it was written
/// before the `version` field existed. Treating it as the current
/// `SCHEMA_VERSION` would mis-tag old configs as future-schema ones the
/// moment we bump `SCHEMA_VERSION` for a v2.
fn default_schema_version() -> u32 {
    1
}

/// Default for `prevent_display_sleep`. Defaults to `true` because the most
/// common user expectation for a "keep-awake" toggle is that the screen
/// does not lock while the tool is active. Users who explicitly want the
/// screen to lock on idle can set this to `false` in `config.toml`.
fn default_prevent_display_sleep() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: SCHEMA_VERSION,
            enabled: false,
            modifiers: Modifiers::default(),
            start_at_login: false,
            activate_at_launch: false,
            battery_threshold_pct: None,
            prevent_display_sleep: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("home directory not found")]
    NoHome,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

impl Config {
    pub fn default_path() -> Result<PathBuf, ConfigError> {
        let dirs = ProjectDirs::from("io", "openlid", "app").ok_or(ConfigError::NoHome)?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    /// Path the v1.x build wrote to:
    /// `~/Library/Application Support/io.openlid.open-lid/config.toml` on
    /// macOS. Returned only when `ProjectDirs` resolves. Used by
    /// [`Config::load_with_v1_fallback`] to migrate users from v1 to v2 on
    /// first launch without making them copy files by hand.
    pub fn v1_legacy_path() -> Option<PathBuf> {
        ProjectDirs::from("io", "openlid", "open-lid").map(|d| d.config_dir().join("config.toml"))
    }

    /// Ensure the v2 config exists, copying it from the v1 path if needed.
    /// Returns the v2 path so callers can `Config::load` it normally.
    ///
    /// This is the one-shot v1 → v2 migration: when the v2 file is absent
    /// and the v1 file is present, the v1 contents are written to the v2
    /// path. The v1 file is left in place so users can roll back. Once
    /// the v2 file exists, subsequent calls are no-ops.
    ///
    /// Call this at startup before any other config read. Keeping the
    /// migration here (rather than inside `Config::load`) keeps `load`
    /// hermetic for tests and makes the side effect (writing the v2 file)
    /// explicit at call sites that actually want it.
    pub fn migrate_v1_to_v2() -> Result<PathBuf, ConfigError> {
        let v2 = Self::default_path()?;
        let v1 = Self::v1_legacy_path();
        Self::migrate_v1_to_v2_paths(&v2, v1.as_deref())?;
        Ok(v2)
    }

    fn migrate_v1_to_v2_paths(v2: &Path, v1: Option<&Path>) -> Result<(), ConfigError> {
        if v2.try_exists().unwrap_or(false) {
            return Ok(());
        }
        let Some(v1_path) = v1 else {
            return Ok(());
        };
        if !v1_path.try_exists().unwrap_or(false) {
            return Ok(());
        }
        let cfg = Self::load(v1_path)?;
        cfg.save(v2)?;
        // Pre-bind the Display strings so llvm-cov can track them. `tracing`
        // evaluates `%value` lazily inside the macro — if no subscriber is
        // installed (as in tests), the inner expression never runs and the
        // line is reported uncovered. Materializing to a String pulls the
        // call up to a regular line-level statement.
        let from = v1_path.display().to_string();
        let to = v2.display().to_string();
        tracing::info!(legacy = %from, target = %to, "migrated v1 config to v2 path");
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                let cfg: Config = toml::from_str(&s)?;
                if cfg.version > SCHEMA_VERSION {
                    // Forward-compat: a config from a newer schema. Serde has
                    // already dropped any unknown fields. Warn the user so they
                    // know why a downgraded binary may behave unexpectedly, but
                    // don't refuse the load — locking someone out of their own
                    // config because they tested a beta is a worse failure mode
                    // than degraded behavior on a personal-use utility.
                    tracing::warn!(
                        "loaded config has a newer schema version (config={}, build={}); \
                         unknown fields were ignored",
                        cfg.version,
                        SCHEMA_VERSION,
                    );
                }
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError::Io(e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("toml.tmp");
        let body = toml::to_string_pretty(self)?;
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(body.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_default() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("subdir").join("config.toml");
        let cfg = Config {
            version: 1,
            enabled: true,
            modifiers: Modifiers {
                only_on_ac: true,
                min_battery: Some(25),
                schedule: None,
            },
            start_at_login: true,
            activate_at_launch: false,
            battery_threshold_pct: Some(20),
            prevent_display_sleep: false,
        };
        cfg.save(&p).unwrap();
        let back = Config::load(&p).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn migrate_copies_v1_to_v2_when_v2_missing() {
        let dir = tempdir().unwrap();
        let v1 = dir.path().join("v1/config.toml");
        let v2 = dir.path().join("v2/config.toml");

        let v1_cfg = Config {
            version: 1,
            enabled: true,
            modifiers: Modifiers::default(),
            start_at_login: true,
            activate_at_launch: false,
            battery_threshold_pct: None,
            prevent_display_sleep: true,
        };
        v1_cfg.save(&v1).unwrap();

        Config::migrate_v1_to_v2_paths(&v2, Some(&v1)).unwrap();

        assert!(v2.exists(), "migration must materialize the v2 file");
        let migrated = Config::load(&v2).unwrap();
        assert_eq!(migrated, v1_cfg, "v2 contents must match v1");
        assert!(v1.exists(), "v1 must be preserved for rollback");
    }

    #[test]
    fn migrate_is_noop_when_v2_already_exists() {
        let dir = tempdir().unwrap();
        let v1 = dir.path().join("v1/config.toml");
        let v2 = dir.path().join("v2/config.toml");

        let v1_cfg = Config {
            enabled: true,
            ..Config::default()
        };
        v1_cfg.save(&v1).unwrap();

        // v2 already has a different config; migration must not touch it.
        let v2_cfg = Config {
            battery_threshold_pct: Some(15),
            ..Config::default()
        };
        v2_cfg.save(&v2).unwrap();

        Config::migrate_v1_to_v2_paths(&v2, Some(&v1)).unwrap();

        let loaded = Config::load(&v2).unwrap();
        assert_eq!(loaded, v2_cfg, "v2 must not be overwritten by v1");
    }

    #[test]
    fn migrate_is_noop_when_v1_missing() {
        let dir = tempdir().unwrap();
        let v1 = dir.path().join("v1/config.toml");
        let v2 = dir.path().join("v2/config.toml");

        Config::migrate_v1_to_v2_paths(&v2, Some(&v1)).unwrap();

        assert!(!v2.exists(), "no v1 → must not create v2");
    }

    #[test]
    fn migrate_is_noop_when_v1_path_is_none() {
        let dir = tempdir().unwrap();
        let v2 = dir.path().join("v2/config.toml");

        Config::migrate_v1_to_v2_paths(&v2, None).unwrap();

        assert!(!v2.exists());
    }

    /// True when the running machine has a real v1 config file at the
    /// `ProjectDirs`-derived legacy path. The `migrate_v1_to_v2()` wrapper
    /// would mutate that real v2 dir on this machine, which is unsafe to
    /// do from a unit test. CI always passes this guard.
    fn has_real_v1_config() -> bool {
        Config::v1_legacy_path().is_some_and(|p| p.exists())
    }

    #[test]
    fn v1_legacy_path_resolves_to_io_openlid_open_lid() {
        let p = Config::v1_legacy_path().expect("ProjectDirs should resolve in a test env");
        let s = p.to_string_lossy();
        assert!(
            s.contains("io.openlid.open-lid"),
            "v1 legacy path must point at the v1 reverse-DNS dir, got {s}"
        );
        assert_eq!(p.file_name().and_then(|s| s.to_str()), Some("config.toml"));
    }

    #[test]
    fn migrate_v1_to_v2_smoke_returns_default_path() {
        // Skip on dev machines that have an actual v1 config file —
        // calling the wrapper there would write to the developer's real v2
        // config dir. CI runs in a clean env where this guard is a no-op.
        if has_real_v1_config() {
            eprintln!("skipping: real v1 config exists on this machine");
            return;
        }
        let migrated = Config::migrate_v1_to_v2().expect("wrapper should succeed");
        let v2 = Config::default_path().expect("default_path should succeed");
        assert_eq!(migrated, v2, "wrapper must return the v2 path");
    }

    #[test]
    fn save_is_atomic_no_tmp_file_left() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        Config::default().save(&p).unwrap();
        let tmp = p.with_extension("toml.tmp");
        assert!(!tmp.exists());
        assert!(p.exists());
    }

    #[test]
    fn default_has_no_optional_fields_set() {
        let cfg = Config::default();
        assert!(!cfg.start_at_login);
        assert!(!cfg.activate_at_launch);
        assert!(cfg.battery_threshold_pct.is_none());
    }

    #[test]
    fn default_prevents_display_sleep() {
        // The whole point of this field is to ship a keep-awake-style
        // default: when sleep prevention is active, the display also stays
        // awake (and so the screen doesn't lock). Flipping this assertion
        // would silently regress that contract.
        assert!(Config::default().prevent_display_sleep);
    }

    #[test]
    fn config_missing_prevent_display_sleep_loads_as_true() {
        // Back-compat: a config file written by a v0.x build (or by a user
        // who omitted the field) must load with `prevent_display_sleep =
        // true`, matching the new default. If serde silently fell through
        // to bool's natural default (false), upgrading users would lose
        // the new behavior they didn't opt out of.
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        std::fs::write(&p, "enabled = true\n").unwrap();
        let cfg = Config::load(&p).unwrap();
        assert!(cfg.prevent_display_sleep);
    }

    #[test]
    fn default_config_has_schema_version() {
        // The manual Default impl must set version to SCHEMA_VERSION, not
        // Rust's integer default (0). Asserting against the constant rather
        // than the literal 1 keeps this test correct across version bumps.
        let cfg = Config::default();
        assert_eq!(cfg.version, SCHEMA_VERSION);
    }

    #[test]
    fn save_writes_version_field_into_toml() {
        // The saved TOML must carry the current schema version so older
        // binaries (post-v1.0, seeing an unknown future version) can detect
        // the schema bump.
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        Config::default().save(&p).unwrap();
        let raw = std::fs::read_to_string(&p).unwrap();
        let expected = format!("version = {SCHEMA_VERSION}");
        assert!(
            raw.contains(&expected),
            "expected '{expected}' in saved TOML, got:\n{raw}"
        );
    }

    #[test]
    fn config_missing_version_field_loads_as_version_one() {
        // Back-compat: a v0.1 config file with no `version` line must load
        // cleanly under v1.0 as if it had `version = 1`.
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        std::fs::write(&p, "enabled = true\nstart_at_login = false\n").unwrap();
        let cfg = Config::load(&p).unwrap();
        assert_eq!(cfg.version, 1);
        assert!(cfg.enabled);
    }

    #[test]
    fn load_with_newer_version_preserves_known_fields() {
        // A future schema (version > SCHEMA_VERSION) is loaded under the
        // current binary. Under Option A (warn-and-continue), known fields
        // are still parsed and the user is not locked out of their config.
        // A `tracing::warn!` is emitted as a side effect — not asserted
        // here because doing so would require pulling in `tracing-test` as
        // a dev-dependency for one assertion. The behavior under test is
        // "doesn't fail and preserves known fields".
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        std::fs::write(&p, "version = 999\nenabled = true\nstart_at_login = true\n").unwrap();
        let cfg = Config::load(&p).expect("load should not error on newer version");
        assert_eq!(cfg.version, 999);
        assert!(
            cfg.enabled,
            "known field should be preserved under Option A"
        );
        assert!(
            cfg.start_at_login,
            "known field should be preserved under Option A"
        );
    }

    #[test]
    fn default_path_resolves_under_project_dirs() {
        // `default_path` is the only documented way callers find the
        // config file. Pin two contracts: (1) it succeeds on a normal
        // developer/CI machine (where ProjectDirs::from returns Some),
        // and (2) the resolved file is named `config.toml`. If a future
        // refactor renamed the file (e.g., to `config.yaml`), CLI
        // `openlid config show` would silently look at the wrong path.
        let p = Config::default_path().expect("ProjectDirs should resolve in a test env");
        assert_eq!(p.file_name().and_then(|s| s.to_str()), Some("config.toml"));
    }

    #[test]
    fn load_propagates_non_not_found_io_errors() {
        // Pin the contract: `load()` swallows `NotFound` (returns the
        // default config) but must bubble up every other IO error.
        // Pointing it at a directory triggers a non-NotFound error
        // (`IsADirectory` on Linux, sometimes `PermissionDenied` /
        // similar on macOS). If a regression made `load()` treat all
        // IO errors as missing-file, a half-broken filesystem state
        // would silently overwrite the user's real config with the
        // default on next save.
        let dir = tempdir().unwrap();
        let err = Config::load(dir.path()).expect_err("expected non-NotFound IO error");
        assert!(matches!(err, ConfigError::Io(_)), "got: {err:?}");
    }
}
