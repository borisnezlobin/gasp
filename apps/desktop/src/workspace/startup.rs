//! What a vault window reads from disk before it can draw: the vault's
//! config and its most recently changed notes. `editor` reads them on a
//! background thread while GPUI connects to the display and loads fonts,
//! so the first frame doesn't wait on the disk.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use editor_config::{Config, ConfigLoader};

use super::files::notes_by_recency;
use super::launcher::{MAX_RECENT, RECENT_SCAN_FOLDERS};

/// A vault's config and recent notes, read once.
pub struct VaultStart {
    /// The vault folder, canonicalized.
    pub vault: PathBuf,
    pub config: Config,
    /// Notes by modification time, newest first, for the launcher. Left
    /// unread when the window will reopen saved tabs instead.
    pub recent: Option<Vec<PathBuf>>,
}

impl VaultStart {
    /// Reads everything now, on this thread. Config problems are logged.
    pub fn load(vault: &Path) -> VaultStart {
        let _span = crate::trace::span("vault-start");
        let vault = std::fs::canonicalize(vault).unwrap_or_else(|_| vault.to_path_buf());
        let mut loader = ConfigLoader::for_vault(&vault);
        for diagnostic in loader.load_all() {
            eprintln!("{diagnostic:?}");
        }
        let reopens_tabs = loader
            .config()
            .device
            .open_tabs
            .iter()
            .any(|name| vault.join(name).is_file());
        let recent = (!reopens_tabs).then(|| {
            let mut notes = notes_by_recency(&vault, RECENT_SCAN_FOLDERS);
            notes.truncate(MAX_RECENT);
            notes
        });
        VaultStart {
            config: loader.config().clone(),
            recent,
            vault,
        }
    }

    /// Starts reading on a background thread.
    pub fn spawn(vault: PathBuf) -> PendingStart {
        let thread = std::thread::Builder::new()
            .name("vault-start".into())
            .spawn({
                let vault = vault.clone();
                move || VaultStart::load(&vault)
            })
            .ok();
        PendingStart { vault, thread }
    }
}

/// A [`VaultStart`] being read on another thread.
pub struct PendingStart {
    vault: PathBuf,
    thread: Option<JoinHandle<VaultStart>>,
}

impl PendingStart {
    /// The vault being read, as given.
    pub fn vault(&self) -> &Path {
        &self.vault
    }

    /// Waits for the read, or reads here when the thread couldn't start.
    pub fn wait(self) -> VaultStart {
        let _span = crate::trace::span("vault-start-wait");
        self.thread
            .and_then(|thread| thread.join().ok())
            .unwrap_or_else(|| VaultStart::load(&self.vault))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_background_read_matches_a_direct_one() {
        let vault = tempfile::tempdir().unwrap();
        std::fs::write(vault.path().join("a.md"), "A").unwrap();
        std::fs::create_dir(vault.path().join(".editor")).unwrap();
        std::fs::write(
            vault.path().join(".editor/device.toml"),
            "open-tabs = [\"a.md\"]\n",
        )
        .unwrap();
        let direct = VaultStart::load(vault.path());
        let background = VaultStart::spawn(vault.path().to_path_buf()).wait();
        assert_eq!(background.vault, direct.vault);
        assert_eq!(background.config.device.open_tabs, ["a.md"]);
        // The saved tab reopens, so there's no launcher to fill.
        assert_eq!(direct.recent, None);
        std::fs::remove_file(vault.path().join(".editor/device.toml")).unwrap();
        let fresh = VaultStart::load(vault.path());
        assert_eq!(fresh.recent, Some(vec![fresh.vault.join("a.md")]));
    }
}
