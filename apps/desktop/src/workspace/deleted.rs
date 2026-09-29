//! Notes moved to the trash, so `note.restore-deleted` (the Undo on the
//! notice a deletion shows) can bring the last one back.
//!
//! Each deletion is recorded in the vault's `device.toml`, which never
//! syncs: where the note was, where in the trash it went and when. After
//! a restart the note comes back by moving it out of the trash, as long as
//! it's still there. A note deleted this session also keeps the text it
//! had, unsaved edits included, which it comes back with.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use gasp_config::device::{DeletedNoteRecord, KEPT_DELETED_NOTES};
use gasp_config::settings::TrashMode;
use gpui::{Context, Window};

use super::files::note_title;
use super::{OpenIn, Workspace};
use crate::notices::{self, Notice};

pub const RESTORE_COMMAND: &str = "note.restore-deleted";

/// Where a deleted note went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrashedTo {
    /// A known place: the vault's trash, or the system's on macOS.
    Place(PathBuf),
    /// The system's trash, which is searched by the note's old place.
    SystemTrash,
    /// Deleted outright: only the text kept this session can bring it back.
    Nowhere,
}

impl TrashedTo {
    /// Where a note deleted the way `mode` says went: the place the trash
    /// gave, else the system's trash if that's where it went.
    pub fn of(place: Option<PathBuf>, mode: TrashMode) -> TrashedTo {
        match (place, crate::sandbox::trash_mode(mode)) {
            (Some(place), _) => TrashedTo::Place(place),
            (None, TrashMode::System) => TrashedTo::SystemTrash,
            (None, _) => TrashedTo::Nowhere,
        }
    }
}

/// A note that went to the trash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletedNote {
    pub path: PathBuf,
    /// The text it had, for a note deleted this session.
    pub text: Option<String>,
    pub trashed_to: TrashedTo,
    /// Seconds since the Unix epoch.
    pub deleted_at: u64,
    /// The notice offering to undo it, taken away once it's restored.
    notice: Option<u64>,
}

impl DeletedNote {
    pub fn new(path: PathBuf, text: Option<String>, trashed_to: TrashedTo) -> DeletedNote {
        let deleted_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        DeletedNote {
            path,
            text,
            trashed_to,
            deleted_at,
            notice: None,
        }
    }

    /// Whether anything could bring it back.
    fn can_come_back(&self) -> bool {
        self.text.is_some() || self.trashed_to != TrashedTo::Nowhere
    }

    /// The note as `device.toml` keeps it, if it could come back after a
    /// restart.
    fn record(&self, vault: &Path) -> Option<DeletedNoteRecord> {
        let trashed_to = match &self.trashed_to {
            TrashedTo::Place(place) => place
                .strip_prefix(vault)
                .unwrap_or(place)
                .to_string_lossy()
                .into_owned(),
            TrashedTo::SystemTrash => String::new(),
            TrashedTo::Nowhere => return None,
        };
        Some(DeletedNoteRecord {
            path: relative_text(vault, &self.path),
            trashed_to,
            deleted_at: self.deleted_at,
        })
    }

    /// A note recorded in `device.toml` by an earlier session.
    fn from_record(vault: &Path, record: &DeletedNoteRecord) -> DeletedNote {
        let trashed_to = match record.trashed_to.as_str() {
            "" => TrashedTo::SystemTrash,
            place => TrashedTo::Place(vault.join(place)),
        };
        DeletedNote {
            path: vault.join(&record.path),
            text: None,
            trashed_to,
            deleted_at: record.deleted_at,
            notice: None,
        }
    }
}

fn relative_text(vault: &Path, path: &Path) -> String {
    path.strip_prefix(vault)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The deleted notes `device.toml` recorded, oldest first.
pub fn remembered(vault: &Path, records: &[DeletedNoteRecord]) -> Vec<DeletedNote> {
    records
        .iter()
        .map(|record| DeletedNote::from_record(vault, record))
        .collect()
}

/// Why a deleted note couldn't come back.
enum RestoreFailure {
    /// It's no longer in the trash.
    Gone,
    Failed(io::Error),
}

impl From<io::Error> for RestoreFailure {
    fn from(error: io::Error) -> Self {
        RestoreFailure::Failed(error)
    }
}

impl Workspace {
    /// The text `path` has now: the open editor's, else the file's.
    pub(crate) fn text_before_delete(&self, path: &Path, cx: &gpui::App) -> Option<String> {
        match self.doc_for_path(path, cx) {
            Some(doc) => Some(doc.read(cx).current_text(cx)),
            None => std::fs::read_to_string(path).ok(),
        }
    }

    /// Remembers a note that just went to the trash and says so, with an
    /// Undo.
    pub(crate) fn remember_deleted(&mut self, mut note: DeletedNote, cx: &mut Context<Self>) {
        if !note.can_come_back() {
            return;
        }
        let message = format!("Moved “{}” to the trash.", note_title(&note.path));
        note.notice = Some(notices::show(
            Notice::done(message).with_action("Undo", RESTORE_COMMAND),
            cx,
        ));
        self.deleted.push(note);
        let over = self.deleted.len().saturating_sub(KEPT_DELETED_NOTES);
        self.deleted.drain(..over);
        self.keep_deleted_records(cx);
    }

    /// Writes the deleted notes that could come back after a restart to
    /// `device.toml`.
    fn keep_deleted_records(&mut self, cx: &mut Context<Self>) {
        let records: Vec<DeletedNoteRecord> = self
            .deleted
            .iter()
            .filter_map(|note| note.record(&self.vault))
            .collect();
        if records != self.config.device.deleted_notes {
            self.config.device.deleted_notes = records;
            self.save_device_now(cx);
        }
    }

    /// Deleted notes that can still come back, oldest first.
    pub fn deleted_notes(&self) -> &[DeletedNote] {
        &self.deleted
    }

    /// `note.restore-deleted`: brings the last deleted note back where it
    /// was (beside it, if something new took its name) and opens it. One
    /// that has left the trash since is forgotten, and a notice says so.
    pub fn restore_deleted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(note) = self.deleted.pop() else {
            notices::show(Notice::done("There’s no deleted note to bring back."), cx);
            return;
        };
        let title = note_title(&note.path);
        let restored = match bring_back(&note) {
            Ok(path) => path,
            Err(RestoreFailure::Gone) => {
                self.keep_deleted_records(cx);
                let message =
                    format!("“{title}” isn’t in the trash any more, so it can’t come back.");
                notices::show(Notice::problem(message), cx);
                return;
            }
            Err(RestoreFailure::Failed(error)) => {
                self.deleted.push(note);
                notices::problem(format!("Couldn’t restore “{title}”: {error}"), cx);
                return;
            }
        };
        self.keep_deleted_records(cx);
        if let Some(id) = note.notice {
            notices::dismiss(id, cx);
        }
        if let Err(error) = self.open_path(&restored, OpenIn::ActiveTab, window, cx) {
            notices::open_failed(&restored, error, cx);
            return;
        }
        self.focus_active(window, cx);
        let message = format!("Restored “{}”.", note_title(&restored));
        notices::show(Notice::done(message), cx);
    }
}

/// Puts `note` back and returns where it is now: out of the trash, then
/// with the text it had this session, if it had any.
fn bring_back(note: &DeletedNote) -> Result<PathBuf, RestoreFailure> {
    let target = free_path(&note.path);
    let Some(text) = &note.text else {
        return take_from_trash(note, &target);
    };
    if let TrashedTo::Place(place) = &note.trashed_to
        && place.exists()
    {
        crate::trashing::put_back(place, &target)?;
    }
    write_note(&target, text)?;
    Ok(target)
}

/// Moves the note out of the trash to `target`. A note that has left the
/// trash but is back in its place (the Finder's Put Back) counts as back.
fn take_from_trash(note: &DeletedNote, target: &Path) -> Result<PathBuf, RestoreFailure> {
    match &note.trashed_to {
        TrashedTo::Place(place) if place.exists() => {
            crate::trashing::put_back(place, target)?;
            Ok(target.to_path_buf())
        }
        _ if note.path.exists() => Ok(note.path.clone()),
        TrashedTo::SystemTrash if crate::trashing::restore_from_system_trash(&note.path)? => {
            Ok(note.path.clone())
        }
        _ => Err(RestoreFailure::Gone),
    }
}

fn write_note(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}

/// `path` if nothing's there, else the first free "Name (restored N).md"
/// beside it.
pub fn free_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|number| {
            let suffix = if number == 1 {
                " (restored)".to_owned()
            } else {
                format!(" (restored {number})")
            };
            path.with_file_name(format!("{stem}{suffix}{extension}"))
        })
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_gets_a_restored_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("Plan.md");
        assert_eq!(free_path(&note), note);
        std::fs::write(&note, "new").unwrap();
        assert_eq!(free_path(&note), dir.path().join("Plan (restored).md"));
        std::fs::write(dir.path().join("Plan (restored).md"), "").unwrap();
        assert_eq!(free_path(&note), dir.path().join("Plan (restored 2).md"));
    }

    #[test]
    fn records_keep_the_vaults_trash_relative_and_leave_out_the_unrecoverable() {
        let vault = Path::new("/vault");
        let in_vault = DeletedNote::new(
            vault.join("Daily/Plan.md"),
            Some("text".into()),
            TrashedTo::Place(vault.join(".trash/Plan.md")),
        );
        let record = in_vault.record(vault).unwrap();
        assert_eq!(record.path, "Daily/Plan.md");
        assert_eq!(record.trashed_to, ".trash/Plan.md");
        let back = DeletedNote::from_record(vault, &record);
        assert_eq!(back.trashed_to, in_vault.trashed_to);
        assert_eq!(back.text, None);
        let system = DeletedNote::new(vault.join("A.md"), None, TrashedTo::SystemTrash);
        assert_eq!(system.record(vault).unwrap().trashed_to, "");
        let gone = DeletedNote::new(vault.join("B.md"), Some("b".into()), TrashedTo::Nowhere);
        assert_eq!(gone.record(vault), None);
    }
}
