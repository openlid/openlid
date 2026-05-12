use crate::mode::Modifiers;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Config {
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
    /// value is restored — matches keep-awake-style's "Activate at launch" off.
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
            Ok(s) => Ok(toml::from_str(&s)?),
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
}
