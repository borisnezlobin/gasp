//! Where the running app lives, whether it may check for updates, and
//! the paths an update goes through beside it.

use std::path::{Path, PathBuf};

/// The `.app` bundle the executable at `exe` runs from, such as
/// `/Applications/Gasp.app` for `/Applications/Gasp.app/Contents/MacOS/gasp`.
/// `cargo run`, tests and other loose binaries have none.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let shaped = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app";
    shaped.then(|| bundle.to_path_buf())
}

/// The bundle this process runs from, if any.
pub fn running_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    bundle_of(&exe)
}

/// What decides whether the daily check runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckGate {
    /// Whether the app runs from a `.app` bundle.
    pub in_bundle: bool,
    /// Whether a snapshot run keeps the app from reaching the network.
    pub reaches_outside: bool,
    /// Whether every open vault leaves `updates.check` on.
    pub enabled: bool,
}

impl CheckGate {
    /// Whether the daily check should ask the site now.
    pub fn allows_daily_check(self) -> bool {
        self.allows_any_check() && self.enabled
    }

    /// Whether any check can run, even one asked for from the menu.
    pub fn allows_any_check(self) -> bool {
        self.in_bundle && self.reaches_outside
    }
}

/// Where the verified new version waits beside `bundle`, on the same
/// volume so it can be moved into place: `.Gasp-update.app`.
pub fn staged_path(bundle: &Path) -> PathBuf {
    sibling(bundle, "update")
}

/// Where the old version goes while the new one moves into place:
/// `.Gasp-old.app`.
pub fn aside_path(bundle: &Path) -> PathBuf {
    sibling(bundle, "old")
}

fn sibling(bundle: &Path, role: &str) -> PathBuf {
    let stem = bundle
        .file_stem()
        .map_or_else(|| "Gasp".into(), |stem| stem.to_string_lossy());
    let folder = bundle.parent().unwrap_or(Path::new("/"));
    folder.join(format!(".{stem}-{role}.app"))
}

/// Whether the app's folder takes new files, so the bundle can be
/// replaced without asking for a password.
pub fn folder_is_writable(bundle: &Path) -> bool {
    let Some(folder) = bundle.parent() else {
        return false;
    };
    let probe = folder.join(format!(".gasp-write-probe-{}", std::process::id()));
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if created {
        let _ = std::fs::remove_file(&probe);
    }
    created
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundled_executable_names_its_bundle() {
        let exe = Path::new("/Users/me/My Apps/Gasp.app/Contents/MacOS/gasp");
        assert_eq!(
            bundle_of(exe),
            Some(PathBuf::from("/Users/me/My Apps/Gasp.app"))
        );
    }

    #[test]
    fn loose_binaries_have_no_bundle() {
        assert_eq!(bundle_of(Path::new("/repo/target/debug/gasp")), None);
        assert_eq!(
            bundle_of(Path::new("/repo/target/debug/deps/app-1234")),
            None
        );
        assert_eq!(bundle_of(Path::new("/Gasp/Contents/MacOS/gasp")), None);
        assert_eq!(bundle_of(Path::new("gasp")), None);
        assert_eq!(running_bundle(), None);
    }

    #[test]
    fn checks_need_a_bundle_and_the_network() {
        let gate = |in_bundle, reaches_outside, enabled| CheckGate {
            in_bundle,
            reaches_outside,
            enabled,
        };
        assert!(gate(true, true, true).allows_daily_check());
        assert!(!gate(false, true, true).allows_daily_check());
        assert!(!gate(true, false, true).allows_daily_check());
        assert!(!gate(true, true, false).allows_daily_check());
        assert!(gate(true, true, false).allows_any_check());
        assert!(!gate(false, true, true).allows_any_check());
        assert!(!gate(true, false, true).allows_any_check());
    }

    #[test]
    fn updates_wait_beside_the_bundle() {
        let bundle = Path::new("/Volumes/Work Disk/Apps/Gasp.app");
        assert_eq!(
            staged_path(bundle),
            PathBuf::from("/Volumes/Work Disk/Apps/.Gasp-update.app")
        );
        assert_eq!(
            aside_path(bundle),
            PathBuf::from("/Volumes/Work Disk/Apps/.Gasp-old.app")
        );
    }

    #[test]
    fn a_temporary_folder_is_writable_and_a_missing_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        assert!(folder_is_writable(&dir.path().join("Gasp.app")));
        assert!(!folder_is_writable(&dir.path().join("missing/Gasp.app")));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
