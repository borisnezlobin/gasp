//! File recovery on the phone: while a note is edited, the version it's
//! replacing is kept as a snapshot every few minutes, in the app's own
//! folder and never in the vault, as the desktop keeps them.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use editor_vault::recovery::{Snapshot, SnapshotStore, use_data_dir};

use crate::vault::{VaultError, VaultFolder};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// Keeps snapshots under `path`, the app's own support folder. Call it
/// once, before opening a vault.
#[uniffi::export]
pub fn use_data_folder(path: String) {
    use_data_dir(PathBuf::from(path));
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SnapshotInfo {
    /// Names the snapshot for [`VaultFolder::snapshot_text`].
    pub id: String,
    /// When it was kept, in seconds since 1970.
    pub taken: i64,
}

#[uniffi::export]
impl VaultFolder {
    /// The note's snapshots, newest first.
    pub fn snapshots(&self, path: String) -> Vec<SnapshotInfo> {
        let Some(store) = self.snapshot_store() else {
            return Vec::new();
        };
        store
            .list(Path::new(&path))
            .iter()
            .map(|snapshot| SnapshotInfo {
                id: snapshot_id(snapshot),
                taken: seconds(snapshot.taken),
            })
            .collect()
    }

    pub fn snapshot_text(&self, path: String, id: String) -> Result<String, VaultError> {
        let store = self.snapshot_store().ok_or_else(no_store)?;
        let snapshot = store
            .list(Path::new(&path))
            .into_iter()
            .find(|snapshot| snapshot_id(snapshot) == id)
            .ok_or_else(|| VaultError::Refused {
                message: "That version is gone.".to_owned(),
            })?;
        Ok(store.read(&snapshot)?)
    }

    /// Keeps `text` as the note's newest snapshot once the
    /// `recovery.interval-minutes` setting has passed since the last one,
    /// or right away with `now`. Answers whether it kept one.
    pub fn record_snapshot(&self, path: String, text: String, now: bool) -> bool {
        let Some(store) = self.snapshot_store() else {
            return false;
        };
        let minutes = u64::from(self.config().settings.recovery.interval_minutes);
        let interval = match now {
            true => Duration::ZERO,
            false => Duration::from_secs(minutes * 60),
        };
        store
            .record(Path::new(&path), &text, SystemTime::now(), interval)
            .unwrap_or(false)
    }

    /// Deletes snapshots older than the `recovery.keep-days` setting.
    pub fn prune_snapshots(&self) {
        let days = u32::max(self.config().settings.recovery.keep_days, 1);
        if let Some(store) = self.snapshot_store() {
            store.prune(DAY * days, SystemTime::now());
        }
    }
}

impl VaultFolder {
    pub(crate) fn snapshot_store(&self) -> Option<SnapshotStore> {
        SnapshotStore::for_vault(&self.root)
    }
}

fn snapshot_id(snapshot: &Snapshot) -> String {
    snapshot
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

fn no_store() -> VaultError {
    VaultError::Refused {
        message: "There's nowhere to keep versions.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use crate::vault::tests::vault_with;

    #[test]
    fn a_kept_version_reads_back() {
        let (_dir, vault) = vault_with(&[("Plan.md", "first")]);
        assert!(vault.record_snapshot("Plan.md".into(), "first".into(), true));
        assert!(!vault.record_snapshot("Plan.md".into(), "second".into(), false));
        let snapshots = vault.snapshots("Plan.md".into());
        assert_eq!(snapshots.len(), 1);
        let text = vault
            .snapshot_text("Plan.md".into(), snapshots[0].id.clone())
            .unwrap();
        assert_eq!(text, "first");
    }
}
