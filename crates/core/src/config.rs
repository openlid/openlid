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
/// `~/Library/Application Support/open-lid/config.toml` on macOS.
///
/// Fields are partitioned into three groups:
///   * Toggle state: `enabled` (persisted so "Restore last state" on launch
///     works — the default for new installs).
///   * Modifier rules: `modifiers` (legacy from the mode-based design; the
///     only one actively wired in v1 is `min_battery`, exposed via the
///     `battery_threshold_pct` preference).
///   * UX preferences: `start_at_login`, `activate_at_launch`,
///     `default_duration_minutes`, `battery_threshold_pct`.
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
    /// value is restored — matches Caffeine's "Activate at launch" off.
    #[serde(default)]
    pub activate_at_launch: bool,

    /// Default timer duration for single-click activations, in minutes.
    /// `None` means indefinite (no timer set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_duration_minutes: Option<u32>,

    /// Auto-deactivate when battery falls below this percent.
    /// `None` disables this safeguard.
    /// Once auto-deactivated, the toggle stays off until the user manually
    /// reactivates — we don't auto-reactivate on power restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_threshold_pct: Option<u8>,
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

impl Default for Config {
    fn default() -> Self {
        Config {
            version: SCHEMA_VERSION,
            enabled: false,
            modifiers: Modifiers::default(),
            start_at_login: false,
            activate_at_launch: false,
            default_duration_minutes: None,
            battery_threshold_pct: None,
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
        let dirs = ProjectDirs::from("io", "openlid", "open-lid").ok_or(ConfigError::NoHome)?;
        Ok(dirs.config_dir().join("config.toml"))
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
            default_duration_minutes: Some(30),
            battery_threshold_pct: Some(20),
        };
        cfg.save(&p).unwrap();
        let back = Config::load(&p).unwrap();
        assert_eq!(cfg, back);
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
        assert!(cfg.default_duration_minutes.is_none());
        assert!(cfg.battery_threshold_pct.is_none());
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
}
