//! Where the running app lives, and whether that's somewhere it should
//! offer to move from. Everything here works on paths alone, so it's the
//! same on every platform.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The folder macOS mounts disk images under.
const VOLUMES: &str = "/Volumes";
/// The folder a bundle's path runs through while Gatekeeper runs it from
/// a randomised read-only copy (App Translocation).
const TRANSLOCATION: &str = "AppTranslocation";
const APPLICATIONS: &str = "Applications";
const DOWNLOADS: &str = "Downloads";
const BUNDLE_EXTENSION: &str = "app";

/// A somewhere the app would rather not stay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A mounted disk image, which ejecting takes away.
    DiskImage { mount: PathBuf },
    /// The Downloads folder.
    Downloads,
    /// A translocated copy whose original couldn't be found.
    Translocated,
    /// Anywhere else. Only `--show-install` asks from here.
    Elsewhere,
}

/// The running bundle, where it came from, and why it could move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The bundle as running, which may be a translocated copy.
    pub bundle: PathBuf,
    /// The bundle the user opened, when it's known and not `bundle`.
    pub original: Option<PathBuf>,
    pub source: Source,
}

impl Placement {
    /// What a "Not now" is remembered against: the bundle the user
    /// opened. A translocated path is new every launch, so one whose
    /// original is unknown is remembered as translocation itself.
    pub fn memory_key(&self) -> PathBuf {
        match (&self.original, &self.source) {
            (Some(original), _) => original.clone(),
            (None, Source::Translocated) => PathBuf::from(TRANSLOCATION),
            (None, _) => self.bundle.clone(),
        }
    }

    /// The bundle to put in the Trash once the copy runs, when it was
    /// opened from Downloads.
    pub fn leftover(&self) -> Option<&Path> {
        (self.source == Source::Downloads).then(|| self.original.as_deref().unwrap_or(&self.bundle))
    }

    /// The disk image to eject once the copy runs.
    pub fn mount(&self) -> Option<&Path> {
        match &self.source {
            Source::DiskImage { mount } => Some(mount),
            _ => None,
        }
    }
}

/// The `.app` bundle an executable at `exe` runs from: the folder two
/// above `Contents/MacOS/<exe>`. None for a bare binary, such as a
/// development build in `target/`.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let shaped = macos.file_name()? == "MacOS" && contents.file_name()? == "Contents";
    let named = bundle
        .extension()
        .is_some_and(|ext| ext == BUNDLE_EXTENSION);
    (shaped && named).then(|| bundle.to_path_buf())
}

/// Whether `path` is inside `App Translocation`'s read-only copies.
pub fn is_translocated(path: &Path) -> bool {
    path.components()
        .any(|part| part.as_os_str() == TRANSLOCATION)
}

/// Where `bundle` runs from, when that's a place to move from. `original`
/// is the bundle a translocated one was copied from, if the system said.
/// `home` finds Downloads and `~/Applications`.
pub fn placement(bundle: &Path, original: Option<&Path>, home: Option<&Path>) -> Option<Placement> {
    let opened = original.unwrap_or(bundle);
    if in_applications(opened) {
        return None;
    }
    let source = match source_of(opened, home) {
        Some(source) => source,
        None if is_translocated(bundle) => Source::Translocated,
        None => return None,
    };
    Some(Placement {
        bundle: bundle.to_path_buf(),
        original: original.map(Path::to_path_buf),
        source,
    })
}

/// A placement for `--show-install`: the real one, or one saying the app
/// runs from wherever it is.
pub fn placement_for_preview(
    bundle: &Path,
    original: Option<&Path>,
    home: Option<&Path>,
) -> Placement {
    placement(bundle, original, home).unwrap_or_else(|| Placement {
        bundle: bundle.to_path_buf(),
        original: None,
        source: Source::Elsewhere,
    })
}

fn source_of(path: &Path, home: Option<&Path>) -> Option<Source> {
    if let Ok(rest) = path.strip_prefix(VOLUMES)
        && let Some(Component::Normal(volume)) = rest.components().next()
    {
        return Some(Source::DiskImage {
            mount: Path::new(VOLUMES).join(volume),
        });
    }
    let downloads = home?.join(DOWNLOADS);
    path.starts_with(downloads).then_some(Source::Downloads)
}

/// Inside any folder named Applications: `/Applications`,
/// `~/Applications` or one on another disk, or a folder in one of them.
fn in_applications(path: &Path) -> bool {
    path.ancestors()
        .skip(1)
        .any(|folder| folder.file_name().is_some_and(|name| name == APPLICATIONS))
}

/// A "Not now": the bundle it was said for and the version it was said to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Declined {
    pub bundle: PathBuf,
    pub version: String,
}

/// Whether to ask about `placement` given the last "Not now". A new
/// version, or the app opened from somewhere else, asks again.
pub fn should_offer(placement: &Placement, declined: Option<&Declined>, version: &str) -> bool {
    declined.is_none_or(|declined| {
        declined.bundle != placement.memory_key() || declined.version != version
    })
}

/// What to remember when the user says "Not now" to `placement`.
pub fn declined(placement: &Placement, version: &str) -> Declined {
    Declined {
        bundle: placement.memory_key(),
        version: version.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/ada";

    fn place(bundle: &str) -> Option<Placement> {
        placement(Path::new(bundle), None, Some(Path::new(HOME)))
    }

    #[test]
    fn a_bundle_is_found_from_its_executable() {
        assert_eq!(
            bundle_of(Path::new("/Volumes/Gasp/Gasp.app/Contents/MacOS/gasp")),
            Some(PathBuf::from("/Volumes/Gasp/Gasp.app"))
        );
        assert_eq!(bundle_of(Path::new("/tmp/target/debug/gasp")), None);
        assert_eq!(bundle_of(Path::new("/opt/Gasp/Contents/MacOS/gasp")), None);
        assert_eq!(bundle_of(Path::new("gasp")), None);
    }

    #[test]
    fn applications_folders_stay_put() {
        assert_eq!(place("/Applications/Gasp.app"), None);
        assert_eq!(place("/Users/ada/Applications/Gasp.app"), None);
        assert_eq!(place("/Applications/Writing/Gasp.app"), None);
        assert_eq!(place("/Volumes/Backup/Applications/Gasp.app"), None);
    }

    #[test]
    fn a_disk_image_offers_to_move_and_names_its_mount() {
        let placement = place("/Volumes/Gasp 0.1/Gasp.app").unwrap();
        assert_eq!(
            placement.source,
            Source::DiskImage {
                mount: PathBuf::from("/Volumes/Gasp 0.1")
            }
        );
        assert_eq!(placement.mount(), Some(Path::new("/Volumes/Gasp 0.1")));
        assert_eq!(placement.leftover(), None);
    }

    #[test]
    fn downloads_offers_to_move_and_tidies_up() {
        let placement = place("/Users/ada/Downloads/Gasp.app").unwrap();
        assert_eq!(placement.source, Source::Downloads);
        assert_eq!(
            placement.leftover(),
            Some(Path::new("/Users/ada/Downloads/Gasp.app"))
        );
        assert_eq!(
            place("/Users/ada/Downloads/gasp-0.1/Gasp.app")
                .unwrap()
                .source,
            Source::Downloads
        );
    }

    #[test]
    fn translocation_follows_the_original() {
        let running = Path::new("/private/var/folders/xy/T/AppTranslocation/0A1B-2C3D/d/Gasp.app");
        let home = Some(Path::new(HOME));
        let from_image =
            placement(running, Some(Path::new("/Volumes/Gasp/Gasp.app")), home).unwrap();
        assert_eq!(from_image.mount(), Some(Path::new("/Volumes/Gasp")));
        assert_eq!(from_image.bundle, running);
        let unknown = placement(running, None, home).unwrap();
        assert_eq!(unknown.source, Source::Translocated);
        assert_eq!(unknown.memory_key(), PathBuf::from(TRANSLOCATION));
        let from_downloads = placement(
            running,
            Some(Path::new("/Users/ada/Downloads/Gasp.app")),
            home,
        )
        .unwrap();
        assert_eq!(
            from_downloads.leftover(),
            Some(Path::new("/Users/ada/Downloads/Gasp.app"))
        );
        let installed = placement(running, Some(Path::new("/Applications/Gasp.app")), home);
        assert_eq!(installed, None);
    }

    #[test]
    fn anywhere_else_stays_put() {
        assert_eq!(place("/Users/ada/Tools/Gasp.app"), None);
        assert_eq!(place("/tmp/target-app/dist/Gasp.app"), None);
        let preview = placement_for_preview(Path::new("/tmp/Gasp.app"), None, None);
        assert_eq!(preview.source, Source::Elsewhere);
    }

    #[test]
    fn not_now_is_remembered_for_that_bundle_and_version() {
        let placement = place("/Volumes/Gasp/Gasp.app").unwrap();
        assert!(should_offer(&placement, None, "0.1.0"));
        let said = declined(&placement, "0.1.0");
        assert!(!should_offer(&placement, Some(&said), "0.1.0"));
        assert!(should_offer(&placement, Some(&said), "0.2.0"));
        let other = place("/Users/ada/Downloads/Gasp.app").unwrap();
        assert!(should_offer(&other, Some(&said), "0.1.0"));
    }

    #[test]
    fn not_now_holds_across_translocated_launches() {
        let home = Some(Path::new(HOME));
        let first = placement(
            Path::new("/private/var/AppTranslocation/A/d/Gasp.app"),
            None,
            home,
        )
        .unwrap();
        let second = placement(
            Path::new("/private/var/AppTranslocation/B/d/Gasp.app"),
            None,
            home,
        )
        .unwrap();
        let said = declined(&first, "0.1.0");
        assert!(!should_offer(&second, Some(&said), "0.1.0"));
    }
}
