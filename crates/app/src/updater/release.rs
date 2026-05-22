//! GitHub Releases API client + version comparison.
//!
//! The parsing layer is decoupled from the HTTP fetch so the version-
//! comparison and asset-selection logic stays unit-testable without a
//! network round-trip. The fetch function is a thin wrapper around
//! `ureq`; the manual checklist exercises it end-to-end against the
//! real API.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

/// Subset of the GitHub Releases API response shape we care about.
/// `body` is the release notes (used in the user prompt). Fields not
/// listed here are ignored on deserialize.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ReleaseInfo {
    pub tag_name: String,
    #[serde(default)]
    pub body: String,
    pub assets: Vec<AssetInfo>,
}

/// One downloadable asset attached to a release.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AssetInfo {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    /// GitHub-computed checksum, populated for releases uploaded after
    /// late 2024 in the form `"sha256:<hex>"`. Absent on older releases.
    #[serde(default)]
    pub digest: Option<String>,
}

/// Parse a GitHub API release-JSON payload into `ReleaseInfo`.
pub fn parse_release_json(s: &str) -> Result<ReleaseInfo> {
    serde_json::from_str(s).context("parsing GitHub release JSON")
}

/// Find the DMG among the release's assets. Errors if none is present.
pub fn pick_dmg_asset(assets: &[AssetInfo]) -> Result<&AssetInfo> {
    assets
        .iter()
        .find(|a| a.name.ends_with(".dmg"))
        .ok_or_else(|| anyhow!("no .dmg asset in the latest release"))
}

/// Strip an optional `v` prefix from a release tag name. `v2.1.0` and
/// `2.1.0` both parse; anything else (e.g. `latest`, `nightly`) errors
/// rather than guessing.
pub fn strip_v_prefix(s: &str) -> Result<&str> {
    if let Some(rest) = s.strip_prefix('v') {
        Ok(rest)
    } else if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Ok(s)
    } else {
        Err(anyhow!(
            "expected a version tag like 'v2.1.0' or '2.1.0', got: {s}"
        ))
    }
}

/// Extract the hex portion from a GitHub digest field
/// (`"sha256:<hex>"`). Other algorithms are rejected so a future
/// algorithm change can't silently pass through unverified.
pub fn parse_digest(s: &str) -> Result<String> {
    if let Some(hex) = s.strip_prefix("sha256:") {
        Ok(hex.to_string())
    } else {
        Err(anyhow!("unsupported digest algorithm in '{s}'"))
    }
}

/// Build-time version from `Cargo.toml`. Parses once via semver.
pub fn current_version() -> Result<semver::Version> {
    semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .context("parsing the build's own CARGO_PKG_VERSION")
}

/// True when `release_tag` (`v2.1.0` form) is strictly newer than the
/// current build. Equal versions return `false` -- the user is on the
/// latest.
pub fn is_newer_than_current(release_tag: &str) -> Result<bool> {
    let release_str = strip_v_prefix(release_tag)?;
    let release = semver::Version::parse(release_str)
        .with_context(|| format!("parsing release version '{release_tag}'"))?;
    Ok(release > current_version()?)
}

/// Fetch the latest release from GitHub. Blocking. Uses bundled
/// webpki-roots so this works even on systems without a configured
/// platform keychain.
pub fn fetch_latest() -> Result<ReleaseInfo> {
    let url = "https://api.github.com/repos/openlid/openlid/releases/latest";
    let mut resp = ureq::get(url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("openlid/", env!("CARGO_PKG_VERSION")))
        .call()
        .context("fetching latest release from GitHub")?;
    let body = resp
        .body_mut()
        .read_to_string()
        .context("reading GitHub response body")?;
    parse_release_json(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_release_json() -> String {
        // Trimmed to the fields we deserialize. Real GitHub responses
        // include ~30 more fields that serde drops on the floor.
        //
        // Three-hash delimiters here intentionally: the body value
        // contains `"##` (the Markdown heading), which would terminate
        // a single- or double-hash raw string early.
        r###"{
          "tag_name": "v2.1.0",
          "body": "## Highlights\n- A thing\n- Another thing\n",
          "assets": [
            {
              "name": "OpenLid-v2.1.0.dmg",
              "browser_download_url": "https://example.com/OpenLid-v2.1.0.dmg",
              "size": 1234567,
              "digest": "sha256:abcdef0123456789"
            },
            {
              "name": "OpenLid-v2.1.0.dmg.sha256",
              "browser_download_url": "https://example.com/OpenLid-v2.1.0.dmg.sha256",
              "size": 64
            }
          ]
        }"###
            .to_string()
    }

    #[test]
    fn parse_release_json_extracts_tag_body_and_assets() {
        let r = parse_release_json(&sample_release_json()).unwrap();
        assert_eq!(r.tag_name, "v2.1.0");
        assert!(r.body.contains("Highlights"));
        assert_eq!(r.assets.len(), 2);
    }

    #[test]
    fn parse_release_json_tolerates_missing_optional_fields() {
        // GitHub releases pre-2024 omit `digest`; `body` may be empty
        // or absent. Both must default cleanly so the parser doesn't
        // hard-fail on a perfectly valid older release.
        let json = r#"{
          "tag_name": "v1.0.0",
          "assets": [{
            "name": "OpenLid-v1.0.0.dmg",
            "browser_download_url": "https://example.com/d.dmg",
            "size": 100
          }]
        }"#;
        let r = parse_release_json(json).unwrap();
        assert_eq!(r.tag_name, "v1.0.0");
        assert_eq!(r.body, "");
        assert!(r.assets[0].digest.is_none());
    }

    #[test]
    fn parse_release_json_rejects_malformed_input() {
        // Pin the contract: a malformed payload is an error, not a
        // silent default. Otherwise a network-injected partial response
        // could be interpreted as "no update available".
        let err = parse_release_json("not json").unwrap_err();
        assert!(format!("{err:#}").to_lowercase().contains("parsing"));
    }

    #[test]
    fn pick_dmg_asset_returns_the_dmg_among_mixed_assets() {
        let r = parse_release_json(&sample_release_json()).unwrap();
        let dmg = pick_dmg_asset(&r.assets).unwrap();
        assert_eq!(dmg.name, "OpenLid-v2.1.0.dmg");
        assert_eq!(dmg.size, 1234567);
    }

    #[test]
    fn pick_dmg_asset_errors_when_no_dmg_present() {
        // A release without a DMG isn't installable by us. Surface it
        // as an error rather than picking a random non-DMG asset.
        let assets = vec![AssetInfo {
            name: "checksums.txt".into(),
            browser_download_url: "https://example.com/c.txt".into(),
            size: 100,
            digest: None,
        }];
        let err = pick_dmg_asset(&assets).unwrap_err();
        assert!(format!("{err:#}").contains("no .dmg"));
    }

    #[test]
    fn strip_v_prefix_handles_v_prefix() {
        assert_eq!(strip_v_prefix("v2.1.0").unwrap(), "2.1.0");
    }

    #[test]
    fn strip_v_prefix_accepts_bare_numeric_version() {
        // Some tag conventions omit `v`. Tolerating both keeps the
        // updater forward-compatible with a future tag-style change
        // upstream.
        assert_eq!(strip_v_prefix("2.1.0").unwrap(), "2.1.0");
    }

    #[test]
    fn strip_v_prefix_rejects_non_version_strings() {
        // `latest`, `nightly`, `main` are all conceivable tag names
        // that don't map to a semver -- reject explicitly so the user
        // gets a useful error instead of a downstream parse failure.
        assert!(strip_v_prefix("latest").is_err());
        assert!(strip_v_prefix("nightly").is_err());
    }

    #[test]
    fn parse_digest_extracts_hex_from_sha256_prefix() {
        let hex = parse_digest("sha256:abcdef0123456789").unwrap();
        assert_eq!(hex, "abcdef0123456789");
    }

    #[test]
    fn parse_digest_rejects_other_algorithms() {
        // GitHub may add `sha512:` or `blake2:` later. Refuse rather
        // than silently treating a non-SHA256 digest as a SHA256 hex
        // string -- a length mismatch downstream would cause a
        // confusing verification failure.
        assert!(parse_digest("sha512:abc123").is_err());
        assert!(parse_digest("plain-hex-no-prefix").is_err());
    }

    #[test]
    fn current_version_parses_cargo_pkg_version() {
        // Sanity: CARGO_PKG_VERSION is set at compile time from
        // workspace.package.version. If that ever drifts from a valid
        // semver (e.g. `2.0.0-dev` becomes `2.0.0-`), this test fails
        // loudly at unit-test time rather than at user-run time.
        let v = current_version().unwrap();
        assert!(v.major >= 2, "expected major>=2, got {v}");
    }

    #[test]
    fn is_newer_than_current_returns_false_for_same_version() {
        let same = format!("v{}", env!("CARGO_PKG_VERSION"));
        assert!(!is_newer_than_current(&same).unwrap());
    }

    #[test]
    fn is_newer_than_current_returns_true_for_higher_major() {
        // Construct a tag that is unambiguously newer than the build
        // version regardless of where the build version drifts to.
        let current = current_version().unwrap();
        let newer = format!("v{}.0.0", current.major + 10);
        assert!(is_newer_than_current(&newer).unwrap());
    }

    #[test]
    fn is_newer_than_current_returns_false_for_lower_version() {
        // Lower-than-current must report up-to-date, not "newer".
        // Otherwise a downgrade-named asset would prompt an install.
        assert!(!is_newer_than_current("v0.0.1").unwrap());
    }

    #[test]
    fn is_newer_than_current_errors_on_garbage_tag() {
        // An unparseable tag is an error so the user sees the underlying
        // problem rather than silently never being offered an update.
        assert!(is_newer_than_current("not-a-version").is_err());
    }
}
