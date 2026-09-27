//! Every note's text, kept between vault searches so the panel opens with
//! results at once. The first search reads every note; after that only the
//! notes the vault watcher reported changed are read again.
//!
//! Reading (`load`) runs off the main thread and never holds the lock
//! while it reads, so the main thread can record changes (`apply`) at any
//! time.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::vault_search::engine::{Note, NoteCache};
use crate::workspace::watcher::DiskChange;

/// A vault's note texts, shared between the workspace and its search
/// panels.
#[derive(Clone)]
pub struct NoteTexts {
    root: PathBuf,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    /// The texts; `None` while a load has them out, or before the first.
    cache: Option<NoteCache>,
    /// Whether the texts were loaded at least once.
    loaded: bool,
    /// Whether a vault watcher reports changes here. Until one does, each
    /// load looks at every note's modification time.
    followed: bool,
    /// Paths (relative) changed on disk since the last load.
    changed: BTreeSet<PathBuf>,
    /// The texts as of the last load.
    snapshot: Option<Arc<Vec<Note>>>,
}

impl NoteTexts {
    pub fn new(root: &Path) -> NoteTexts {
        NoteTexts {
            root: root.to_path_buf(),
            state: Arc::default(),
        }
    }

    /// The vault folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Notes that a vault watcher now reports every change through
    /// [`NoteTexts::apply`].
    pub fn set_followed(&self, followed: bool) {
        self.state().followed = followed;
    }

    /// Records changes the vault watcher saw, for the next load to read.
    pub fn apply(&self, changes: &[DiskChange]) {
        let mut state = self.state();
        let relative = changes
            .iter()
            .flat_map(DiskChange::paths)
            .filter_map(|path| path.strip_prefix(&self.root).ok().map(Path::to_path_buf));
        state.changed.extend(relative);
    }

    /// Marks notes (relative paths) as changed, as after a replace wrote
    /// them, before the watcher reports it.
    pub fn mark_changed(&self, paths: &[PathBuf]) {
        self.state().changed.extend(paths.iter().cloned());
    }

    /// The texts as last loaded, which may be behind the disk.
    pub fn snapshot(&self) -> Option<Arc<Vec<Note>>> {
        self.state().snapshot.clone()
    }

    /// Brings the texts up to date and returns them: the first time by
    /// reading every note, then by reading only the notes that changed
    /// (those the watcher reported, or without one, those whose
    /// modification time or size moved). Blocks on the disk, so call it
    /// off the main thread.
    pub fn load(&self) -> Arc<Vec<Note>> {
        let (cache, changed, current) = {
            let mut state = self.state();
            let changed: Vec<PathBuf> = std::mem::take(&mut state.changed).into_iter().collect();
            (state.cache.take(), changed, state.loaded && state.followed)
        };
        let mut cache = cache.unwrap_or_default();
        if current {
            cache.reload(&self.root, &changed);
        } else {
            cache.refresh(&self.root);
        }
        let notes = Arc::new(cache.notes());
        let mut state = self.state();
        state.cache = Some(cache);
        state.loaded = true;
        state.snapshot = Some(notes.clone());
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, name: &str, text: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn texts(notes: &[Note]) -> Vec<(String, String)> {
        notes
            .iter()
            .map(|note| {
                (
                    note.path.to_string_lossy().into_owned(),
                    note.text.to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn loads_read_only_what_changed() {
        let vault = tempfile::tempdir().unwrap();
        let root = vault.path();
        write(root, "a.md", "A");
        write(root, "Folder/b.md", "B");
        let notes = NoteTexts::new(root);
        notes.set_followed(true);
        assert!(notes.snapshot().is_none());
        assert_eq!(
            texts(&notes.load()),
            [
                ("Folder/b.md".into(), "B".into()),
                ("a.md".into(), "A".into())
            ]
        );
        write(root, "a.md", "A2");
        std::fs::remove_dir_all(root.join("Folder")).unwrap();
        notes.apply(&[
            DiskChange::Changed(root.join("a.md")),
            DiskChange::Removed(root.join("Folder")),
        ]);
        assert_eq!(texts(&notes.load()), [("a.md".into(), "A2".into())]);
        // Without a reported change, the texts stay as they were read.
        write(root, "a.md", "A3");
        assert_eq!(texts(&notes.load()), [("a.md".into(), "A2".into())]);
        notes.mark_changed(&[PathBuf::from("a.md")]);
        assert_eq!(texts(&notes.load()), [("a.md".into(), "A3".into())]);
        assert_eq!(
            texts(&notes.snapshot().unwrap()),
            [("a.md".into(), "A3".into())]
        );
    }

    #[test]
    fn unfollowed_loads_notice_changes_themselves() {
        let vault = tempfile::tempdir().unwrap();
        let root = vault.path();
        write(root, "a.md", "A");
        let notes = NoteTexts::new(root);
        notes.load();
        write(root, "a.md", "A, longer");
        write(root, "b.md", "B");
        assert_eq!(
            texts(&notes.load()),
            [
                ("a.md".into(), "A, longer".into()),
                ("b.md".into(), "B".into())
            ]
        );
    }
}
