//! The checks a downloaded version passes before the app will use it:
//! the disk image matches the site's SHA-256, and the app in it has an
//! intact signature from the same developer and bundle ID as the running
//! app, is notarized, and is newer.

use std::io::Read;
use std::path::{Path, PathBuf};

use semver::Version;
use sha2::{Digest, Sha256};

use super::release::parse_version;
use super::tools::SystemTools;

/// Who signed an app, as `codesign -dv` reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningIdentity {
    /// The bundle ID the signature covers, such as `com.borisnezlobin.gasp`.
    pub identifier: String,
    /// The Apple developer team; `None` for an ad hoc signature.
    pub team_id: Option<String>,
}

/// Why a downloaded version was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    DigestMismatch,
    NoApp,
    BrokenSignature,
    /// The running app has no team to compare with, as a local build.
    RunningUnsigned,
    OtherDeveloper,
    NotNotarized,
    NotNewer,
    /// The release has no SHA-256, which Linux, with no signature to
    /// check, needs.
    NoChecksum,
    /// The new version doesn't start on this computer.
    WontRun,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Refusal::DigestMismatch => "the download doesn’t match the release’s checksum",
            Refusal::NoApp => "the download has no app in it",
            Refusal::BrokenSignature => "the new app’s signature is broken",
            Refusal::RunningUnsigned => {
                "this copy of Gasp isn’t signed, so there’s nothing to compare the new one with"
            }
            Refusal::OtherDeveloper => "the new app isn’t signed by Gasp’s developer",
            Refusal::NotNotarized => "macOS doesn’t accept the new app as notarized",
            Refusal::NotNewer => "the new app isn’t newer than this one",
            Refusal::NoChecksum => "the release has no checksum to check the download against",
            Refusal::WontRun => "the new version doesn’t run on this computer",
        })
    }
}

/// The team and bundle ID in `codesign -dv --verbose=2`'s output.
pub fn parse_signing_identity(output: &str) -> Option<SigningIdentity> {
    let field = |name: &str| {
        output
            .lines()
            .find_map(|line| line.trim().strip_prefix(name))
            .map(str::trim)
    };
    let identifier = field("Identifier=")?.to_string();
    let team_id = field("TeamIdentifier=")
        .filter(|team| !team.is_empty() && *team != "not set")
        .map(str::to_string);
    Some(SigningIdentity {
        identifier,
        team_id,
    })
}

/// Where `hdiutil attach -plist` mounted the image's volume.
pub fn mount_point(plist: &[u8]) -> Option<PathBuf> {
    let root = plist::Value::from_reader_xml(plist).ok()?;
    root.as_dictionary()?
        .get("system-entities")?
        .as_array()?
        .iter()
        .filter_map(|entity| entity.as_dictionary()?.get("mount-point")?.as_string())
        .map(PathBuf::from)
        .next()
}

/// The app at the top of a mounted image, passing over the shortcut to
/// Applications beside it.
pub fn find_app(mount: &Path) -> Option<PathBuf> {
    let mut apps: Vec<PathBuf> = std::fs::read_dir(mount)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .collect();
    apps.sort();
    apps.into_iter().next()
}

/// The app's `CFBundleShortVersionString`.
pub fn bundle_version(app: &Path) -> Option<Version> {
    let info = plist::Value::from_file(app.join("Contents/Info.plist")).ok()?;
    let text = info
        .as_dictionary()?
        .get("CFBundleShortVersionString")?
        .as_string()?;
    parse_version(text).ok()
}

/// The SHA-256 of the file at `path`, in lowercase hex.
pub fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Checks the disk image against the release's SHA-256, when it has one.
pub fn check_digest(dmg: &Path, expected: Option<&str>) -> Result<(), Refusal> {
    let Some(expected) = expected else {
        return Ok(());
    };
    match sha256_of(dmg) {
        Ok(actual) if actual == expected => Ok(()),
        _ => Err(Refusal::DigestMismatch),
    }
}

/// Checks that `app` may replace the running app, which `running` signed
/// and which runs version `running_version`.
pub fn verify_app(
    app: &Path,
    running: &SigningIdentity,
    running_version: &Version,
    tools: &dyn SystemTools,
) -> Result<(), Refusal> {
    if running.team_id.is_none() {
        return Err(Refusal::RunningUnsigned);
    }
    if !tools.signature_is_valid(app) {
        return Err(Refusal::BrokenSignature);
    }
    if tools.signing_identity(app).as_ref() != Some(running) {
        return Err(Refusal::OtherDeveloper);
    }
    if !tools.gatekeeper_accepts(app) {
        return Err(Refusal::NotNotarized);
    }
    match bundle_version(app) {
        Some(version) if version > *running_version => Ok(()),
        _ => Err(Refusal::NotNewer),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVELOPER_ID: &str = "Executable=/Applications/Gasp.app/Contents/MacOS/gasp
Identifier=com.borisnezlobin.gasp
Format=app bundle with Mach-O universal (x86_64 arm64)
CodeDirectory v=20500 size=179106 flags=0x10000(runtime) hashes=5590+3 location=embedded
Signature size=8968
Authority=Developer ID Application: Someone (K2MB68Z582)
Authority=Developer ID Certification Authority
Authority=Apple Root CA
Info.plist entries=12
TeamIdentifier=K2MB68Z582
Runtime Version=26.4.0
";

    const AD_HOC: &str = "Executable=/tmp/Gasp.app/Contents/MacOS/gasp
Identifier=com.borisnezlobin.gasp
Signature=adhoc
TeamIdentifier=not set
";

    #[test]
    fn the_team_id_comes_from_codesign() {
        assert_eq!(
            parse_signing_identity(DEVELOPER_ID),
            Some(SigningIdentity {
                identifier: "com.borisnezlobin.gasp".into(),
                team_id: Some("K2MB68Z582".into()),
            })
        );
    }

    #[test]
    fn an_ad_hoc_signature_has_no_team() {
        let identity = parse_signing_identity(AD_HOC).unwrap();
        assert_eq!(identity.team_id, None);
        assert_eq!(
            parse_signing_identity("code object is not signed at all"),
            None
        );
    }

    #[test]
    fn the_mount_point_comes_from_hdiutils_plist() {
        let plist = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>system-entities</key>
	<array>
		<dict>
			<key>content-hint</key><string>GUID_partition_scheme</string>
			<key>dev-entry</key><string>/dev/disk16</string>
			<key>potentially-mountable</key><false/>
		</dict>
		<dict>
			<key>dev-entry</key><string>/dev/disk17s1</string>
			<key>mount-point</key><string>/Volumes/Gasp 1</string>
			<key>volume-kind</key><string>apfs</string>
		</dict>
	</array>
</dict>
</plist>"#;
        assert_eq!(mount_point(plist), Some(PathBuf::from("/Volumes/Gasp 1")));
        assert_eq!(mount_point(b"<plist><dict/></plist>"), None);
        assert_eq!(mount_point(b"hdiutil: attach failed"), None);
    }

    #[test]
    fn the_app_is_found_beside_the_applications_shortcut() {
        let mount = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(mount.path().join("Gasp.app/Contents")).unwrap();
        std::fs::write(mount.path().join("README.app"), "not a bundle").unwrap();
        std::os::unix::fs::symlink("/Applications", mount.path().join("Applications")).unwrap();
        assert_eq!(find_app(mount.path()), Some(mount.path().join("Gasp.app")));
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(find_app(empty.path()), None);
    }

    #[test]
    fn the_digest_is_checked_when_there_is_one() {
        let dir = tempfile::tempdir().unwrap();
        let dmg = dir.path().join("Gasp.dmg");
        std::fs::write(&dmg, "abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_of(&dmg).unwrap(), abc);
        assert_eq!(check_digest(&dmg, Some(abc)), Ok(()));
        assert_eq!(check_digest(&dmg, None), Ok(()));
        assert_eq!(
            check_digest(&dmg, Some(&"0".repeat(64))),
            Err(Refusal::DigestMismatch)
        );
        assert_eq!(
            check_digest(&dir.path().join("missing.dmg"), Some(abc)),
            Err(Refusal::DigestMismatch)
        );
    }
}
