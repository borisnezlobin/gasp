//! iCloud Drive on this Mac: where it is, whether a vault can move into
//! it, and asking it for files it hasn't downloaded. The vault goes in a
//! folder called Gasp at the top of iCloud Drive, which the iPhone opens
//! once through the Files picker. A non-sandboxed app needs no
//! entitlement to write there, so this works in every build, signed or
//! not. A snapshot run gets a folder of its own in the sandbox instead.

use std::path::{Path, PathBuf};

use gasp_sync::icloud::{self, is_git_clone};
use gpui::{App, Global};

/// A folder standing in for iCloud Drive, set by tests.
struct DriveGlobal(PathBuf);

impl Global for DriveGlobal {}

/// Treats `folder` as iCloud Drive from now on, as tests do so nothing
/// reaches the real one.
pub fn use_drive(folder: PathBuf, cx: &mut App) {
    cx.set_global(DriveGlobal(folder));
}

/// The folder standing in for iCloud Drive: a test's, or a snapshot
/// run's own.
fn stand_in(cx: &App) -> Option<PathBuf> {
    cx.try_global::<DriveGlobal>()
        .map(|drive| drive.0.clone())
        .or_else(|| crate::sandbox::folder("icloud-drive"))
}

/// iCloud Drive's folder on this Mac, when iCloud Drive is turned on.
pub fn drive(cx: &App) -> Option<PathBuf> {
    if let Some(stand_in) = stand_in(cx) {
        std::fs::create_dir_all(&stand_in).ok()?;
        return Some(stand_in);
    }
    if !cfg!(target_os = "macos") {
        return None;
    }
    let drive = icloud::icloud_drive(&dirs::home_dir()?);
    drive.is_dir().then_some(drive)
}

/// Whether the vault at `root` lives in iCloud, so iCloud keeps it in step
/// rather than git.
pub fn is_icloud_vault(root: &Path, cx: &App) -> bool {
    if is_git_clone(root) {
        return false;
    }
    let in_stand_in =
        stand_in(cx).is_some_and(|drive| canonical(root).starts_with(canonical(&drive)));
    in_stand_in || icloud::is_in_icloud(root)
}

/// `path` with its links resolved, such as macOS's `/var` for `/private/var`.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Whether a vault can move into iCloud, and where to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ICloudReadiness {
    /// It can, into this folder, which already holds `notes` notes.
    Ready { folder: PathBuf, notes: usize },
    /// iCloud Drive is off on this Mac.
    NoDrive,
    /// The vault syncs with git and stays on it.
    SyncsWithGit,
    /// It's in iCloud already.
    AlreadyThere,
}

/// Where the vault at `root` stands with iCloud.
pub fn readiness(root: &Path, cx: &App) -> ICloudReadiness {
    if is_git_clone(root) {
        return ICloudReadiness::SyncsWithGit;
    }
    if is_icloud_vault(root, cx) {
        return ICloudReadiness::AlreadyThere;
    }
    let Some(drive) = drive(cx) else {
        return ICloudReadiness::NoDrive;
    };
    let folder = icloud::icloud_vault(&drive);
    let notes = count_notes(&folder);
    ICloudReadiness::Ready { folder, notes }
}

/// How many notes are in `folder`, leaving out hidden folders.
pub fn count_notes(folder: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => count_notes(&path),
                _ => usize::from(path.extension().is_some_and(|ext| ext == "md")),
            }
        })
        .sum()
}

/// Asks iCloud to download the files the vault at `root` only has
/// placeholders for. macOS 14 and later download a file when it's read,
/// so this matters on older systems, which leave `.Name.md.icloud` files.
pub fn request_downloads(root: &Path) -> usize {
    let waiting = icloud::waiting_downloads(root);
    if crate::sandbox::is_active() {
        return waiting.len();
    }
    for path in &waiting {
        let _ = std::process::Command::new("/usr/bin/brctl")
            .arg("download")
            .arg(root.join(path))
            .status();
    }
    waiting.len()
}

/// How a folder in iCloud reads to a person: `iCloud Drive › Gasp`.
pub fn shown_location(folder: &Path) -> String {
    let name = folder.file_name().map_or_else(
        || icloud::ICLOUD_FOLDER_NAME.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    format!("iCloud Drive › {name}")
}

/// A folder in the home folder as `~/Documents/Notes`.
pub fn shown_path(folder: &Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    match folder.strip_prefix(&home) {
        Ok(inside) if !home.as_os_str().is_empty() => format!("~/{}", inside.display()),
        _ => folder.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_notes_in_every_visible_folder() {
        let vault = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(vault.path().join("Daily")).unwrap();
        std::fs::create_dir_all(vault.path().join(".gasp")).unwrap();
        std::fs::write(vault.path().join("Plan.md"), "").unwrap();
        std::fs::write(vault.path().join("Daily/Monday.md"), "").unwrap();
        std::fs::write(vault.path().join("Daily/photo.png"), "").unwrap();
        std::fs::write(vault.path().join(".gasp/notes.md"), "").unwrap();
        assert_eq!(count_notes(vault.path()), 2);
    }

    #[gpui::test]
    fn a_vault_that_syncs_with_git_is_never_an_icloud_vault(cx: &mut gpui::TestAppContext) {
        let vault = tempfile::tempdir().unwrap();
        std::fs::create_dir(vault.path().join(".git")).unwrap();
        cx.update(|cx| {
            use_drive(vault.path().to_path_buf(), cx);
            assert_eq!(readiness(vault.path(), cx), ICloudReadiness::SyncsWithGit);
            assert!(!is_icloud_vault(vault.path(), cx));
        });
    }
}
