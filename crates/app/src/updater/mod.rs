//! Self-update support for openlid.
//!
//! Two entry points (both user-initiated, never automatic):
//!   * `openlid update` from the CLI
//!   * "Check for updates…" from the menubar menu
//!
//! Flow:
//!   1. `release::fetch_latest` queries GitHub for the latest release.
//!   2. `release::is_newer_than_current` compares against the build's
//!      `CARGO_PKG_VERSION` via `semver`.
//!   3. `install_detect::is_homebrew_install` decides whether to defer
//!      to `brew upgrade` or take over the install ourselves.
//!   4. For manual installs, `installer` downloads the DMG to a cache
//!      directory, verifies SHA-256 against `assets[i].digest`, and
//!      spawns a detached shell script that swaps the .app bundle and
//!      relaunches the menubar.
//!
//! State preservation is automatic: the config lives at
//! `~/Library/Application Support/io.openlid.app/config.toml`, outside
//! the .app bundle. Swapping the bundle doesn't touch any user state.

pub mod install_detect;
pub mod release;
