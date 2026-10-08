//! Conflicts waiting for a person, kept outside git's merge state.
//!
//! A merge that hits a real conflict still finishes: the branch keeps one
//! side's version of the file (its version on the synced branch, so no
//! text there disappears), the work tree shows both versions between
//! conflict markers, and this record remembers the three versions so the
//! resolver can show them. The record lives in the clone's git folder, so
//! it never syncs and survives restarts. Every version it names is a blob
//! reachable from the branch's history, so git never collects them.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use git2::Oid;

use crate::error::SyncResult;

const RECORD_FILE: &str = "editor-sync-conflicts";
const NO_VERSION: &str = "-";

/// Which side's version the branch keeps committed while the conflict waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchKeeps {
    /// The synced branch keeps this device's version. Only records saved
    /// by older versions, which also merged a second branch in, say this.
    ThisDevice,
    /// Merging the synced branch: it keeps the version the remote has.
    OtherDevice,
}

/// One file waiting for a person: the blobs of its three versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParkedConflict {
    pub path: PathBuf,
    pub keeps: BranchKeeps,
    pub base: Option<Oid>,
    pub this_device: Option<Oid>,
    pub other_device: Option<Oid>,
}

/// Every parked conflict of one clone, as saved in its git folder.
#[derive(Debug, Default)]
pub(crate) struct ParkedConflicts {
    entries: Vec<ParkedConflict>,
}

impl ParkedConflicts {
    pub fn load(git_dir: &Path) -> SyncResult<Self> {
        let text = match fs::read_to_string(git_dir.join(RECORD_FILE)) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let entries = text.lines().filter_map(parse_line).collect();
        Ok(Self { entries })
    }

    pub fn save(&self, git_dir: &Path) -> SyncResult<()> {
        let path = git_dir.join(RECORD_FILE);
        if self.entries.is_empty() {
            return match fs::remove_file(path) {
                Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
                _ => Ok(()),
            };
        }
        let text: String = self.entries.iter().map(format_line).collect();
        fs::write(path, text)?;
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &ParkedConflict> {
        self.entries.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.get(path).is_some()
    }

    pub fn get(&self, path: &Path) -> Option<&ParkedConflict> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    /// Adds `conflict`, replacing any earlier one for the same file.
    pub fn insert(&mut self, conflict: ParkedConflict) {
        self.remove(&conflict.path);
        self.entries.push(conflict);
    }

    pub fn remove(&mut self, path: &Path) {
        self.entries.retain(|entry| entry.path != path);
    }
}

/// `<keeps> <base> <this device> <other device> <path>`, with `-` for a
/// missing version. The path goes last because it may hold spaces.
fn format_line(entry: &ParkedConflict) -> String {
    let keeps = match entry.keeps {
        BranchKeeps::ThisDevice => "this",
        BranchKeeps::OtherDevice => "other",
    };
    let version = |oid: Option<Oid>| oid.map_or(NO_VERSION.to_owned(), |oid| oid.to_string());
    format!(
        "{keeps} {} {} {} {}\n",
        version(entry.base),
        version(entry.this_device),
        version(entry.other_device),
        entry.path.to_string_lossy()
    )
}

fn parse_line(line: &str) -> Option<ParkedConflict> {
    let mut fields = line.splitn(5, ' ');
    let keeps = match fields.next()? {
        "this" => BranchKeeps::ThisDevice,
        "other" => BranchKeeps::OtherDevice,
        _ => return None,
    };
    let mut version = || -> Option<Option<Oid>> {
        match fields.next()? {
            NO_VERSION => Some(None),
            hex => Oid::from_str(hex).ok().map(Some),
        }
    };
    let (base, this_device, other_device) = (version()?, version()?, version()?);
    let path = fields.next().filter(|path| !path.is_empty())?;
    Some(ParkedConflict {
        path: PathBuf::from(path),
        keeps,
        base,
        this_device,
        other_device,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(hex_digit: char) -> Oid {
        Oid::from_str(&hex_digit.to_string().repeat(40)).unwrap()
    }

    #[test]
    fn the_record_round_trips_through_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut record = ParkedConflicts::default();
        record.insert(ParkedConflict {
            path: PathBuf::from("Daily notes/Habit Ideas.md"),
            keeps: BranchKeeps::OtherDevice,
            base: None,
            this_device: Some(oid('a')),
            other_device: Some(oid('b')),
        });
        record.insert(ParkedConflict {
            path: PathBuf::from("Lemma.md"),
            keeps: BranchKeeps::ThisDevice,
            base: Some(oid('c')),
            this_device: Some(oid('d')),
            other_device: Some(oid('e')),
        });
        record.save(dir.path()).unwrap();
        let loaded = ParkedConflicts::load(dir.path()).unwrap();
        assert_eq!(loaded.entries, record.entries);
    }

    #[test]
    fn an_empty_record_leaves_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut record = ParkedConflicts::default();
        record.insert(ParkedConflict {
            path: PathBuf::from("a.md"),
            keeps: BranchKeeps::OtherDevice,
            base: None,
            this_device: None,
            other_device: None,
        });
        record.save(dir.path()).unwrap();
        record.remove(Path::new("a.md"));
        record.save(dir.path()).unwrap();
        assert!(!dir.path().join(RECORD_FILE).exists());
        assert!(
            ParkedConflicts::load(dir.path())
                .unwrap()
                .entries
                .is_empty()
        );
    }

    #[test]
    fn unreadable_lines_are_skipped() {
        assert_eq!(parse_line("garbage"), None);
        assert_eq!(parse_line("other - - -"), None);
        assert_eq!(parse_line("other - - - "), None);
    }
}
