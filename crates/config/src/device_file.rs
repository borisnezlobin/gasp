//! Where this device keeps its own state for a vault: open tabs, window
//! size and the like, which never sync. A vault syncing with git keeps it
//! in `.gasp/device.toml`, which git leaves on each device. iCloud copies
//! the whole folder, so a vault in iCloud gives each device a file of its
//! own, `.gasp/device-<name>.toml`, and no two devices ever write the same
//! one.

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use crate::CONFIG_DIR;

/// The state file's name in a vault that isn't in iCloud.
pub const DEVICE_FILE: &str = "device.toml";

/// The folder iCloud keeps in step: iCloud Drive and every app's iCloud
/// container live under it, on the Mac and the iPhone alike.
const MOBILE_DOCUMENTS: &str = "Mobile Documents";

static DEVICE_NAME: OnceLock<String> = OnceLock::new();

/// Names this device for its state file, once, as the app starts: letters,
/// digits and dashes only. Later calls change nothing.
pub fn set_device_name(name: &str) {
    let slug: String = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    if !slug.is_empty() {
        DEVICE_NAME.get_or_init(|| slug.to_owned());
    }
}

/// Whether `path` is somewhere iCloud keeps in step.
pub fn is_in_icloud(path: &Path) -> bool {
    path.components()
        .any(|part| part == Component::Normal(MOBILE_DOCUMENTS.as_ref()))
}

/// The state file's name for the vault whose config folder is `dir`.
pub fn device_file_name(dir: &Path) -> String {
    match DEVICE_NAME.get() {
        Some(name) if is_in_icloud(dir) => format!("device-{name}.toml"),
        _ => DEVICE_FILE.to_owned(),
    }
}

/// This device's state file for the vault at `vault`.
pub fn device_file(vault: &Path) -> PathBuf {
    let dir = vault.join(CONFIG_DIR);
    let name = device_file_name(&dir);
    dir.join(name)
}

/// Whether `name` is a device's state file, this device's or another's.
pub fn is_device_file_name(name: &str) -> bool {
    name == DEVICE_FILE || (name.starts_with("device-") && name.ends_with(".toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_vault_in_icloud_gets_a_file_per_device() {
        set_device_name("Boris's MacBook Pro");
        let local = Path::new("/Users/b/Documents/Notes");
        let icloud = Path::new("/Users/b/Library/Mobile Documents/com~apple~CloudDocs/Gasp");
        assert_eq!(device_file(local), local.join(".gasp/device.toml"));
        assert_eq!(
            device_file(icloud),
            icloud.join(".gasp/device-boris-s-macbook-pro.toml")
        );
    }

    #[test]
    fn every_device_s_file_is_recognised() {
        assert!(is_device_file_name("device.toml"));
        assert!(is_device_file_name("device-iphone-3fa2b1.toml"));
        assert!(!is_device_file_name("settings.toml"));
    }
}
