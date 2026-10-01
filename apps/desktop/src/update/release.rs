//! What gaspmd.com/api/version says about the newest published release,
//! and whether it's newer than the app that's running.

use semver::Version;
use serde::Deserialize;

/// Where the app asks for the newest version.
pub const VERSION_URL: &str = "https://gaspmd.com/api/version";

/// The newest published release, as the site describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The release's disk image.
    pub url: String,
    /// The release's page on GitHub.
    pub notes: String,
    pub published: String,
    /// The disk image's size in bytes.
    pub size: u64,
    /// The disk image's SHA-256 in lowercase hex, when GitHub gave one.
    pub sha256: Option<String>,
}

/// Why the site's answer couldn't be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReleaseError {
    NotJson,
    BadVersion(String),
    NotHttps(String),
    BadDigest(String),
}

impl std::fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReleaseError::NotJson => f.write_str("the answer isn’t a release"),
            ReleaseError::BadVersion(version) => write!(f, "“{version}” isn’t a version number"),
            ReleaseError::NotHttps(url) => write!(f, "{url} isn’t an https address"),
            ReleaseError::BadDigest(digest) => write!(f, "“{digest}” isn’t a SHA-256"),
        }
    }
}

#[derive(Deserialize)]
struct ReleaseJson {
    version: String,
    url: String,
    notes: String,
    published: String,
    size: u64,
    #[serde(default)]
    sha256: Option<String>,
}

/// Reads the site's JSON.
pub fn parse_release(json: &str) -> Result<Release, ReleaseError> {
    let raw: ReleaseJson = serde_json::from_str(json).map_err(|_| ReleaseError::NotJson)?;
    let version = parse_version(&raw.version)?;
    let url = https(raw.url)?;
    let notes = https(raw.notes)?;
    let sha256 = raw.sha256.map(sha256_hex).transpose()?;
    Ok(Release {
        version,
        url,
        notes,
        published: raw.published,
        size: raw.size,
        sha256,
    })
}

/// `0.2.0` or `v0.2.0` as a version.
pub fn parse_version(text: &str) -> Result<Version, ReleaseError> {
    let trimmed = text.trim();
    let bare = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(bare).map_err(|_| ReleaseError::BadVersion(text.to_string()))
}

/// The version of the app that's running.
pub fn running_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

/// Whether `release` is newer than `running`, by semver's ordering, so
/// `0.10.0` follows `0.9.0` and `1.0.0` follows `1.0.0-beta.2`.
pub fn is_newer(release: &Version, running: &Version) -> bool {
    release > running
}

fn https(url: String) -> Result<String, ReleaseError> {
    if url.starts_with("https://") && url.len() > "https://".len() {
        Ok(url)
    } else {
        Err(ReleaseError::NotHttps(url))
    }
}

fn sha256_hex(digest: String) -> Result<String, ReleaseError> {
    let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(hex.to_ascii_lowercase())
    } else {
        Err(ReleaseError::BadDigest(digest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
        "version": "0.2.0",
        "url": "https://github.com/borisnezlobin/gasp/releases/download/v0.2.0/Gasp-0.2.0.dmg",
        "notes": "https://github.com/borisnezlobin/gasp/releases/tag/v0.2.0",
        "published": "2026-10-01T09:00:00Z",
        "size": 96746643,
        "sha256": "51CA523E8E19CAF5DDA7A9466A5C55BCD2D4E0FA3DD99E9D77FED2CEA27B7C1B"
    }"#;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn semver_orders_numerically_and_puts_prereleases_first() {
        assert!(is_newer(&version("0.10.0"), &version("0.9.0")));
        assert!(is_newer(&version("0.2.0"), &version("0.1.9")));
        assert!(is_newer(&version("1.0.0"), &version("1.0.0-beta.2")));
        assert!(is_newer(
            &version("1.0.0-beta.10"),
            &version("1.0.0-beta.2")
        ));
        assert!(!is_newer(&version("0.1.0"), &version("0.1.0")));
        assert!(!is_newer(&version("0.1.0"), &version("0.2.0")));
    }

    #[test]
    fn a_good_answer_parses() {
        let release = parse_release(GOOD).unwrap();
        assert_eq!(release.version, version("0.2.0"));
        assert!(release.url.ends_with("Gasp-0.2.0.dmg"));
        assert!(release.notes.ends_with("/tag/v0.2.0"));
        assert_eq!(release.size, 96746643);
        assert_eq!(
            release.sha256.as_deref(),
            Some("51ca523e8e19caf5dda7a9466a5c55bcd2d4e0fa3dd99e9d77fed2cea27b7c1b")
        );
    }

    #[test]
    fn the_digest_is_optional_and_may_carry_githubs_prefix() {
        let without = GOOD.replace(
            r#""sha256": "51CA523E8E19CAF5DDA7A9466A5C55BCD2D4E0FA3DD99E9D77FED2CEA27B7C1B""#,
            r#""extra": true"#,
        );
        assert_eq!(parse_release(&without).unwrap().sha256, None);
        let prefixed = GOOD.replace(r#""sha256": ""#, r#""sha256": "sha256:"#);
        assert!(parse_release(&prefixed).unwrap().sha256.is_some());
    }

    #[test]
    fn bad_answers_are_refused() {
        assert_eq!(parse_release("not json"), Err(ReleaseError::NotJson));
        assert_eq!(parse_release("{}"), Err(ReleaseError::NotJson));
        assert_eq!(
            parse_release(r#"{"version": "0.2.0"}"#),
            Err(ReleaseError::NotJson)
        );
        let bad_version = GOOD.replace(r#""0.2.0""#, r#""soon""#);
        assert_eq!(
            parse_release(&bad_version),
            Err(ReleaseError::BadVersion("soon".into()))
        );
        let http = GOOD.replace(
            "https://github.com/borisnezlobin/gasp/releases/download",
            "http://example.com",
        );
        assert!(matches!(
            parse_release(&http),
            Err(ReleaseError::NotHttps(_))
        ));
        let short_digest = GOOD.replace("51CA523E", "");
        assert!(matches!(
            parse_release(&short_digest),
            Err(ReleaseError::BadDigest(_))
        ));
        let negative_size = GOOD.replace("96746643", "-1");
        assert_eq!(parse_release(&negative_size), Err(ReleaseError::NotJson));
    }

    #[test]
    fn versions_may_start_with_v() {
        assert_eq!(parse_version("v0.2.0"), Ok(version("0.2.0")));
        assert_eq!(parse_version(" 1.2.3 "), Ok(version("1.2.3")));
        assert!(parse_version("1.2").is_err());
    }

    #[test]
    fn the_running_version_is_the_crates() {
        assert_eq!(running_version().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
