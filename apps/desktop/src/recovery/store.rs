//! Snapshots on disk: one folder per vault in the app's data folder (never
//! in the vault, so they don't sync), one folder per note inside it named
//! by the note's path, and one file per snapshot named by when it was
//! taken, in milliseconds since 1970.
//!
//! ```text
//! <data>/editor/snapshots/Vault-1f2e3d4c/Essays/Wave Packets.md/1790514540123.md
//! ```

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Overrides where snapshots go, for portable setups.
pub const DATA_DIR_ENV: &str = "EDITOR_DATA_DIR";

/// Where snapshots go instead of the data folder, once set.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Keeps snapshots under `dir` from now on, as tests do so they never
/// touch the real data folder. Only the first call counts.
pub fn use_data_dir(dir: PathBuf) {
    DATA_DIR.get_or_init(|| dir);
}

const SNAPSHOT_EXTENSION: &str = "md";

/// One saved version of a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub path: PathBuf,
    pub taken: SystemTime,
}

/// A vault's snapshots.
#[derive(Clone, Debug)]
pub struct SnapshotStore {
    dir: PathBuf,
}

impl SnapshotStore {
    /// The store for `vault` in the app's data folder.
    pub fn for_vault(vault: &Path) -> Option<SnapshotStore> {
        let data = DATA_DIR
            .get()
            .cloned()
            .or_else(|| std::env::var_os(DATA_DIR_ENV).map(PathBuf::from))
            .or_else(|| dirs::data_local_dir().map(|dir| dir.join("editor")))?;
        Some(SnapshotStore::at(
            data.join("snapshots").join(vault_key(vault)),
        ))
    }

    /// A store in `dir`.
    pub fn at(dir: PathBuf) -> SnapshotStore {
        SnapshotStore { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn note_dir(&self, note: &Path) -> PathBuf {
        self.dir.join(note)
    }

    /// A note's snapshots, newest first. `note` is relative to the vault.
    pub fn list(&self, note: &Path) -> Vec<Snapshot> {
        let Ok(entries) = std::fs::read_dir(self.note_dir(note)) else {
            return Vec::new();
        };
        let mut snapshots: Vec<Snapshot> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let taken = taken_from_name(&path)?;
                Some(Snapshot { path, taken })
            })
            .collect();
        snapshots.sort_by(|a, b| b.taken.cmp(&a.taken));
        snapshots
    }

    pub fn read(&self, snapshot: &Snapshot) -> io::Result<String> {
        std::fs::read_to_string(&snapshot.path)
    }

    /// Saves `text` as the note's version at `now`, unless the newest
    /// snapshot is younger than `interval` or already holds this text.
    /// `interval` zero always saves a differing text. Answers whether it
    /// saved one.
    pub fn record(
        &self,
        note: &Path,
        text: &str,
        now: SystemTime,
        interval: Duration,
    ) -> io::Result<bool> {
        let newest = self.list(note).into_iter().next();
        if let Some(newest) = &newest {
            let age = now.duration_since(newest.taken).unwrap_or_default();
            if age < interval || self.read(newest).is_ok_and(|saved| saved == text) {
                return Ok(false);
            }
        }
        let dir = self.note_dir(note);
        std::fs::create_dir_all(&dir)?;
        let millis = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = dir.join(format!("{millis}.{SNAPSHOT_EXTENSION}"));
        crate::workspace::files::atomic_write(&path, text)?;
        Ok(true)
    }

    /// Deletes snapshots older than `keep`, and folders left empty.
    /// Answers how many snapshots went.
    pub fn prune(&self, keep: Duration, now: SystemTime) -> usize {
        let cutoff = now.checked_sub(keep).unwrap_or(UNIX_EPOCH);
        prune_dir(&self.dir, cutoff)
    }
}

fn prune_dir(dir: &Path, cutoff: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            removed += prune_dir(&path, cutoff);
            // Fails, harmlessly, while the folder still has snapshots.
            std::fs::remove_dir(&path).ok();
        } else if taken_from_name(&path).is_some_and(|taken| taken < cutoff)
            && std::fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

/// When a snapshot file says it was taken.
fn taken_from_name(path: &Path) -> Option<SystemTime> {
    if path.extension()? != SNAPSHOT_EXTENSION {
        return None;
    }
    let millis: u64 = path.file_stem()?.to_str()?.parse().ok()?;
    Some(UNIX_EPOCH + Duration::from_millis(millis))
}

/// A folder name for a vault: its own name, readable, and a hash of its
/// full path, so two vaults with one name never share snapshots.
fn vault_key(vault: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    vault.hash(&mut hasher);
    let name = vault
        .file_name()
        .map_or_else(|| "Vault".into(), |name| name.to_string_lossy());
    format!("{name}-{:08x}", hasher.finish() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);
    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn at(minutes: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_790_000_000) + MINUTE * minutes as u32
    }

    #[test]
    fn snapshots_wait_out_the_interval_and_skip_repeats() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::at(dir.path().to_path_buf());
        let note = Path::new("Essays/Wave.md");
        assert!(store.record(note, "one", at(0), MINUTE * 5).unwrap());
        // Too soon after the last one.
        assert!(!store.record(note, "two", at(3), MINUTE * 5).unwrap());
        // Late enough, but the same text.
        assert!(!store.record(note, "one", at(6), MINUTE * 5).unwrap());
        assert!(store.record(note, "two", at(7), MINUTE * 5).unwrap());
        let listed = store.list(note);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].taken, at(7));
        assert_eq!(store.read(&listed[0]).unwrap(), "two");
        assert_eq!(store.read(&listed[1]).unwrap(), "one");
        assert!(store.list(Path::new("Other.md")).is_empty());
    }

    #[test]
    fn pruning_keeps_only_recent_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::at(dir.path().to_path_buf());
        let old = Path::new("Old.md");
        let mixed = Path::new("Folder/Mixed.md");
        store.record(old, "a", at(0), Duration::ZERO).unwrap();
        store.record(mixed, "b", at(0), Duration::ZERO).unwrap();
        let later = at(0) + DAY * 8;
        store.record(mixed, "c", later, Duration::ZERO).unwrap();
        assert_eq!(store.prune(DAY * 7, later + MINUTE), 2);
        assert!(store.list(old).is_empty());
        assert!(!dir.path().join("Old.md").exists(), "empty folders go too");
        let kept = store.list(mixed);
        assert_eq!(kept.len(), 1);
        assert_eq!(store.read(&kept[0]).unwrap(), "c");
    }

    #[test]
    fn vaults_with_one_name_get_their_own_folders() {
        let a = vault_key(Path::new("/home/a/Vault"));
        let b = vault_key(Path::new("/home/b/Vault"));
        assert!(a.starts_with("Vault-"));
        assert_ne!(a, b);
    }
}
