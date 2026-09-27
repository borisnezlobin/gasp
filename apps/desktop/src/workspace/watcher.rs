//! Watching the vault folder for notes changed, moved or deleted by other
//! programs (or by sync).

use std::path::{Path, PathBuf};

use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind};

use super::files::is_hidden;
use crate::vault_watch::WatchHandle;

/// A change to the vault that open notes care about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiskChange {
    /// A file's contents may have changed.
    Changed(PathBuf),
    /// A file or folder may be gone.
    Removed(PathBuf),
    /// A file or folder moved.
    Renamed { from: PathBuf, to: PathBuf },
}

impl DiskChange {
    /// Every path the change touches.
    pub fn paths(&self) -> Vec<PathBuf> {
        match self {
            DiskChange::Changed(path) | DiskChange::Removed(path) => vec![path.clone()],
            DiskChange::Renamed { from, to } => vec![from.clone(), to.clone()],
        }
    }

    /// Moves first, so a rename reported as a move and a removal is seen
    /// as the move.
    fn order(&self) -> u8 {
        match self {
            DiskChange::Renamed { .. } => 0,
            DiskChange::Changed(_) => 1,
            DiskChange::Removed(_) => 2,
        }
    }
}

/// The changes one notify event describes, leaving out hidden files such
/// as our own temporary files and `.editor/`.
pub fn classify(event: &Event, vault: &Path) -> Vec<DiskChange> {
    let visible: Vec<&PathBuf> = event
        .paths
        .iter()
        .filter(|path| !is_hidden(path, vault))
        .collect();
    match event.kind {
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => rename(&event.paths, vault),
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) | EventKind::Remove(_) => visible
            .into_iter()
            .map(|path| DiskChange::Removed(path.clone()))
            .collect(),
        EventKind::Modify(ModifyKind::Metadata(_)) | EventKind::Access(_) => Vec::new(),
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any | EventKind::Other => visible
            .into_iter()
            .map(|path| DiskChange::Changed(path.clone()))
            .collect(),
    }
}

/// A rename, or a plain change when a save's hidden temporary file was
/// moved over a note.
fn rename(paths: &[PathBuf], vault: &Path) -> Vec<DiskChange> {
    let [from, to] = paths else {
        return Vec::new();
    };
    match (is_hidden(from, vault), is_hidden(to, vault)) {
        (false, false) => vec![DiskChange::Renamed {
            from: from.clone(),
            to: to.clone(),
        }],
        (true, false) => vec![DiskChange::Changed(to.clone())],
        (false, true) => vec![DiskChange::Removed(from.clone())],
        (true, true) => Vec::new(),
    }
}

/// Sorts a batch so moves come first and drops repeats.
pub fn normalize(mut changes: Vec<DiskChange>) -> Vec<DiskChange> {
    changes.sort_by_key(DiskChange::order);
    let mut seen = Vec::with_capacity(changes.len());
    for change in changes {
        if !seen.contains(&change) {
            seen.push(change);
        }
    }
    seen
}

/// Starts watching `vault`. Events arrive on the returned receiver; the
/// watch stops when the handle is dropped.
pub fn watch(vault: &Path) -> notify::Result<(WatchHandle, UnboundedReceiver<Vec<DiskChange>>)> {
    let (sender, receiver) = unbounded();
    let root = vault.to_path_buf();
    let handle = crate::vault_watch::watch(vault, move |event| {
        let changes = classify(event, &root);
        if !changes.is_empty() {
            let _ = sender.unbounded_send(changes);
        }
    })?;
    Ok((handle, receiver))
}

#[cfg(test)]
mod tests {
    use notify::event::{CreateKind, DataChange, RemoveKind};

    use super::*;

    fn event(kind: EventKind, paths: &[&str]) -> Event {
        let mut event = Event::new(kind);
        for path in paths {
            event = event.add_path(PathBuf::from(path));
        }
        event
    }

    #[test]
    fn saves_through_a_temporary_file_are_changes() {
        let vault = Path::new("/v");
        let saved = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            &["/v/.a.md.1.tmp", "/v/a.md"],
        );
        assert_eq!(
            classify(&saved, vault),
            vec![DiskChange::Changed("/v/a.md".into())]
        );
    }

    #[test]
    fn renames_removals_and_writes_are_told_apart() {
        let vault = Path::new("/v");
        let moved = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            &["/v/a.md", "/v/b.md"],
        );
        let removed = event(EventKind::Remove(RemoveKind::File), &["/v/a.md"]);
        let written = event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            &["/v/a.md"],
        );
        let hidden = event(
            EventKind::Create(CreateKind::File),
            &["/v/.editor/device.toml"],
        );
        let batch: Vec<DiskChange> = [removed, written, moved, hidden]
            .iter()
            .flat_map(|event| classify(event, vault))
            .collect();
        assert_eq!(
            normalize(batch),
            vec![
                DiskChange::Renamed {
                    from: "/v/a.md".into(),
                    to: "/v/b.md".into()
                },
                DiskChange::Changed("/v/a.md".into()),
                DiskChange::Removed("/v/a.md".into()),
            ]
        );
    }
}
