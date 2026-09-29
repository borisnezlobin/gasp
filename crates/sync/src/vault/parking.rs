//! Files a merge left waiting for a person (see [`crate::parked`]): parking
//! them, keeping them out of commits, carrying them through later merges
//! and settling them.

use std::fs;
use std::path::{Path, PathBuf};

use git2::{IndexEntry, IndexTime, Oid, Tree};

use super::{ConflictVersions, MergeRun, Settlement, Vault, Version, is_inside_vault};
use crate::conflict::{ConflictedFile, Resolution};
use crate::error::{SyncError, SyncResult};
use crate::line_merge::{LineMerge, merge_lines};
use crate::parked::{BranchKeeps, ParkedConflict, ParkedConflicts};

/// Git's mode for an ordinary file.
const FILE_MODE: u32 = 0o100_644;

/// A text conflict the merge policy handed to a person, and the merge it
/// came from.
pub(super) struct Parking<'a> {
    pub versions: &'a ConflictVersions,
    pub file: &'a ConflictedFile,
    pub keeps: BranchKeeps,
    pub run: MergeRun,
}

/// A parked file whose committed version a merge is about to change, and
/// what was on disk, put aside while the merge runs.
pub(super) struct LiftedFile {
    conflict: ParkedConflict,
    committed: Option<Oid>,
    on_disk: Vec<u8>,
}

impl Vault {
    pub(super) fn parked(&self) -> SyncResult<ParkedConflicts> {
        ParkedConflicts::load(self.repo.path())
    }

    pub(super) fn save_parked(&self, parked: &ParkedConflicts) -> SyncResult<()> {
        parked.save(self.repo.path())
    }

    /// Parks a file that needs a person: the index gets the version the
    /// branch keeps, the disk shows both versions between markers, and the
    /// record keeps all three. Returns whether the file now waits.
    ///
    /// After an interrupted merge the markers are already on disk, perhaps
    /// edited. A file with none left was settled by hand, so it's staged as
    /// it is instead.
    pub(super) fn settle_by_person(
        &self,
        index: &mut git2::Index,
        parked: &mut ParkedConflicts,
        parking: Parking<'_>,
    ) -> SyncResult<bool> {
        let path = &parking.file.path;
        if parking.run == MergeRun::Interrupted && !self.shows_conflict(path) {
            index.add_path(path)?;
            return Ok(false);
        }
        let versions = parking.versions;
        let kept = match parking.keeps {
            BranchKeeps::ThisDevice => &versions.this_device,
            BranchKeeps::OtherDevice => &versions.other_device,
        };
        let kept = kept
            .as_ref()
            .expect("only a file both devices kept needs a person");
        index.add_frombuffer(&index_entry(path), &kept.bytes)?;
        if parking.run == MergeRun::Fresh {
            self.write_file(path, parking.file.marked_text().text.as_bytes())?;
        }
        let id = |version: &Option<Version>| version.as_ref().map(|version| version.id);
        parked.insert(ParkedConflict {
            path: path.clone(),
            keeps: parking.keeps,
            base: id(&versions.base),
            this_device: id(&versions.this_device),
            other_device: id(&versions.other_device),
        });
        Ok(true)
    }

    /// Lets go of parked files that no longer need a person because they
    /// were settled by hand (no markers left) or deleted; the next commit
    /// takes them as they are. Returns the files still waiting.
    pub(super) fn release_settled_conflicts(&self) -> SyncResult<ParkedConflicts> {
        let mut parked = self.parked()?;
        let settled: Vec<PathBuf> = parked
            .iter()
            .map(|conflict| conflict.path.clone())
            .filter(|path| !self.shows_conflict(path))
            .collect();
        if !settled.is_empty() {
            for path in &settled {
                parked.remove(path);
            }
            self.save_parked(&parked)?;
        }
        Ok(parked)
    }

    /// Before a merge of `incoming`: puts the committed version of each
    /// parked file the merge will change back on disk, so the merge can
    /// update it, and returns what was there.
    pub(super) fn lift_parked(&self, incoming: Oid) -> SyncResult<Vec<LiftedFile>> {
        let parked = self.parked()?;
        if parked.is_empty() {
            return Ok(Vec::new());
        }
        let head_tree = self.local_head()?.map(|head| head.tree()).transpose()?;
        let incoming_tree = self.repo.find_commit(incoming)?.tree()?;
        let mut lifted = Vec::new();
        for conflict in parked.iter() {
            let committed = blob_at(head_tree.as_ref(), &conflict.path);
            if committed == blob_at(Some(&incoming_tree), &conflict.path) {
                continue;
            }
            let Ok(on_disk) = fs::read(self.root().join(&conflict.path)) else {
                continue;
            };
            self.write_version(&conflict.path, committed)?;
            lifted.push(LiftedFile {
                conflict: conflict.clone(),
                committed,
                on_disk,
            });
        }
        Ok(lifted)
    }

    /// After a merge: puts each lifted file back on disk with whatever the
    /// merge brought in folded into it, and keeps it waiting.
    pub(super) fn refold(&self, lifted: Vec<LiftedFile>) -> SyncResult<()> {
        if lifted.is_empty() {
            return Ok(());
        }
        let mut parked = self.parked()?;
        let head_tree = self.local_head()?.map(|head| head.tree()).transpose()?;
        for file in lifted {
            let change = incoming_change(&parked, &file, head_tree.as_ref());
            let text = self.fold(&file, change)?;
            self.write_file(&file.conflict.path, &text)?;
            parked.insert(file.conflict);
        }
        self.save_parked(&parked)
    }

    /// The lifted file's text from disk with the change `(from, to)` merged
    /// into it. Clashes come out as markers nested in the file's own.
    fn fold(
        &self,
        file: &LiftedFile,
        (from, to): (Option<Oid>, Option<Oid>),
    ) -> SyncResult<Vec<u8>> {
        // With nothing new, or the file deleted elsewhere, what was on disk
        // stays: an edit beats a delete.
        let Some(to) = to.filter(|to| Some(*to) != from) else {
            return Ok(file.on_disk.clone());
        };
        let from_text = match from {
            Some(id) => text_of(&self.read_version(id)?.bytes),
            None => String::new(),
        };
        let to_text = text_of(&self.read_version(to)?.bytes);
        let folded = match merge_lines(&from_text, &text_of(&file.on_disk), &to_text) {
            LineMerge::Clean(text) => text,
            LineMerge::Conflicted(segments) => {
                let path = file.conflict.path.clone();
                ConflictedFile { path, segments }.marked_text().text
            }
        };
        Ok(folded.into_bytes())
    }

    fn write_version(&self, path: &Path, version: Option<Oid>) -> SyncResult<()> {
        match version {
            Some(id) => self.write_file(path, &self.read_version(id)?.bytes),
            None => self.remove_file(path),
        }
    }

    /// Every file waiting for a person, read from disk so that edits made
    /// to it since the merge show.
    pub fn conflicts(&self) -> SyncResult<Vec<ConflictedFile>> {
        let parked = self.parked()?;
        Ok(parked
            .iter()
            .filter_map(|conflict| self.waiting_file(conflict))
            .collect())
    }

    /// The file as it's on disk, if it still has conflict markers. Exactly
    /// as the merge wrote it, it comes back with every hunk's base.
    fn waiting_file(&self, conflict: &ParkedConflict) -> Option<ConflictedFile> {
        let on_disk = self.read_text(&conflict.path)?;
        let recorded = self.recorded_file(conflict);
        if let Some(recorded) = &recorded
            && recorded.marked_text().text == on_disk
        {
            return Some(recorded.clone());
        }
        let read =
            ConflictedFile::from_marked_text(conflict.path.clone(), &on_disk, recorded.as_ref());
        (read.hunk_count() > 0).then_some(read)
    }

    /// The conflict as the merge found it, from the recorded versions.
    fn recorded_file(&self, conflict: &ParkedConflict) -> Option<ConflictedFile> {
        let version = |id: Option<Oid>| id.and_then(|id| self.read_version(id).ok());
        let versions = ConflictVersions {
            path: conflict.path.clone(),
            base: version(conflict.base),
            this_device: version(conflict.this_device),
            other_device: version(conflict.other_device),
        };
        match versions.settle() {
            Settlement::NeedsPerson(file) => Some(file),
            _ => None,
        }
    }

    fn read_text(&self, path: &Path) -> Option<String> {
        let bytes = fs::read(self.root().join(path)).ok()?;
        Some(text_of(&bytes))
    }

    fn shows_conflict(&self, path: &Path) -> bool {
        self.read_text(path).is_some_and(|text| {
            ConflictedFile::from_marked_text(path.to_owned(), &text, None).hunk_count() > 0
        })
    }

    /// Settles a waiting file with one resolution per hunk. `file` must be
    /// the file as [`Vault::conflicts`] shows it now; when it has changed
    /// since, nothing is written and the resolution is refused. The next
    /// commit syncs the result.
    pub fn resolve(&self, file: &ConflictedFile, resolutions: &[Resolution]) -> SyncResult<()> {
        let current = self
            .parked()?
            .get(&file.path)
            .and_then(|conflict| self.waiting_file(conflict));
        if current.as_ref() != Some(file) {
            return Err(SyncError::InvalidResolution(format!(
                "{} changed since its conflicts were shown",
                file.path.display()
            )));
        }
        let text = file.resolve(resolutions)?;
        self.resolve_with_text(&file.path, &text)
    }

    /// Replaces a waiting file with `text` the person settled on. The next
    /// commit syncs it.
    pub fn resolve_with_text(&self, path: &Path, text: &str) -> SyncResult<()> {
        if !is_inside_vault(path) {
            return Err(SyncError::OutsideVault(path.to_owned()));
        }
        self.write_file(path, text.as_bytes())?;
        let mut parked = self.parked()?;
        parked.remove(path);
        self.save_parked(&parked)
    }
}

/// What a merge did to a lifted file, as the version it changed from and
/// the version it brought in.
fn incoming_change(
    parked: &ParkedConflicts,
    file: &LiftedFile,
    head_tree: Option<&Tree<'_>>,
) -> (Option<Oid>, Option<Oid>) {
    match parked.get(&file.conflict.path) {
        // The merge parked it afresh, so what came in clashed with what
        // was committed: the change is the incoming side's own.
        Some(fresh) if *fresh != file.conflict => (fresh.base, fresh.other_device),
        _ => (file.committed, blob_at(head_tree, &file.conflict.path)),
    }
}

fn blob_at(tree: Option<&Tree<'_>>, path: &Path) -> Option<Oid> {
    Some(tree?.get_path(path).ok()?.id())
}

fn text_of(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A stage-0 index entry for `path`, for adding a version from memory.
fn index_entry(path: &Path) -> IndexEntry {
    let time = IndexTime::new(0, 0);
    IndexEntry {
        ctime: time,
        mtime: time,
        dev: 0,
        ino: 0,
        mode: FILE_MODE,
        uid: 0,
        gid: 0,
        file_size: 0,
        id: Oid::zero(),
        flags: 0,
        flags_extended: 0,
        path: path.to_string_lossy().replace('\\', "/").into_bytes(),
    }
}
