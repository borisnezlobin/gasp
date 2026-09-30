//! A vault in iCloud Drive on the phone: moving this iPhone's notes into
//! the Gasp folder, the files iCloud hasn't downloaded yet, and the
//! copies it leaves when two devices changed a note at once. The app
//! does iCloud's own part (the Files picker, downloads, coordinated
//! writes); this is the part that's the same on every device.

use std::path::{Path, PathBuf};

use gasp_sync::icloud::{self, MoveError};

use crate::vault::VaultError;

/// What moving notes into iCloud did.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ICloudMove {
    /// How many notes are in the iCloud folder from this vault, copied
    /// or already there.
    pub notes: u32,
    /// Files the iCloud folder had with other contents, whose version
    /// from this vault went beside them under this name.
    pub kept_beside: Vec<String>,
}

/// A note iCloud found changed on two devices at once, and its copy.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ICloudCopyPair {
    /// Both vault-relative, with `/` between folders.
    pub original: String,
    pub copy: String,
}

/// The folder at the top of iCloud Drive that holds the vault.
#[uniffi::export]
pub fn icloud_folder_name() -> String {
    icloud::ICLOUD_FOLDER_NAME.to_owned()
}

/// Whether `path` is in iCloud Drive or an app's iCloud container.
/// Names this device for its own state file in a vault kept in iCloud (see
/// `gasp_config::device_file`), once, as the app starts.
#[uniffi::export]
pub fn set_device_name(name: String) {
    gasp_config::device_file::set_device_name(&name);
}

#[uniffi::export]
pub fn is_in_icloud(path: String) -> bool {
    icloud::is_in_icloud(Path::new(&path))
}

/// Copies the vault at `from` into the iCloud folder `to`, reads each
/// copy back, and leaves `from` as it was. A note `to` has already with
/// other text keeps its place, and this vault's goes beside it.
#[uniffi::export]
pub fn icloud_move_vault(
    from: String,
    to: String,
    device: String,
) -> Result<ICloudMove, VaultError> {
    let report = icloud::move_vault(Path::new(&from), Path::new(&to), &device).map_err(refused)?;
    Ok(ICloudMove {
        notes: u32::try_from(report.notes()).unwrap_or(u32::MAX),
        kept_beside: report
            .kept_beside
            .iter()
            .map(|(_, beside)| slashed(beside))
            .collect(),
    })
}

/// Files in the vault at `root` that iCloud hasn't downloaded yet, by the
/// vault-relative path each will have.
#[uniffi::export]
pub fn icloud_waiting_downloads(root: String) -> Vec<String> {
    icloud::waiting_downloads(Path::new(&root))
        .iter()
        .map(PathBuf::as_path)
        .map(slashed)
        .collect()
}

/// The copies iCloud has left beside notes in the vault at `root`.
#[uniffi::export]
pub fn icloud_copies(root: String) -> Vec<ICloudCopyPair> {
    icloud::icloud_copies(Path::new(&root))
        .into_iter()
        .map(|pair| ICloudCopyPair {
            original: slashed(&pair.original),
            copy: slashed(&pair.copy),
        })
        .collect()
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn refused(error: MoveError) -> VaultError {
    VaultError::Refused {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_the_phones_notes_into_the_icloud_folder() {
        let phone = tempfile::tempdir().unwrap();
        let drive = tempfile::tempdir().unwrap();
        std::fs::write(phone.path().join("Plan.md"), "# Plan\n").unwrap();
        let gasp = drive.path().join("Gasp");
        std::fs::create_dir(&gasp).unwrap();
        std::fs::write(gasp.join("Plan.md"), "# The Mac's plan\n").unwrap();

        let moved = icloud_move_vault(
            phone.path().to_string_lossy().into_owned(),
            gasp.to_string_lossy().into_owned(),
            "iphone".into(),
        )
        .unwrap();

        assert_eq!(moved.notes, 1);
        assert_eq!(moved.kept_beside, ["Plan (from iphone).md"]);
        assert!(phone.path().join("Plan.md").exists());
    }
}
