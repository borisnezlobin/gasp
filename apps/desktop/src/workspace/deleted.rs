//! Notes moved to the trash this session, kept as the text they had, so
//! `note.restore-deleted` (the Undo on the notice a deletion shows) can
//! bring the last one back, unsaved edits included.

use std::io;
use std::path::{Path, PathBuf};

use gpui::{Context, Window};

use super::files::note_title;
use super::{OpenIn, Workspace};
use crate::notices::{self, Notice};

/// Deleted notes remembered at most; older ones are only in the trash.
pub const REMEMBERED: usize = 20;

pub const RESTORE_COMMAND: &str = "note.restore-deleted";

/// A note that went to the trash, as it read when it went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletedNote {
    pub path: PathBuf,
    pub text: String,
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
    pub(crate) fn remember_deleted(&mut self, note: DeletedNote, cx: &mut Context<Self>) {
        let message = format!("Moved “{}” to the trash.", note_title(&note.path));
        self.deleted.push(note);
        if self.deleted.len() > REMEMBERED {
            self.deleted.remove(0);
        }
        notices::show(
            Notice::done(message).with_action("Undo", RESTORE_COMMAND),
            cx,
        );
    }

    /// Notes deleted this session, oldest first.
    pub fn deleted_notes(&self) -> &[DeletedNote] {
        &self.deleted
    }

    /// `note.restore-deleted`: writes the last deleted note back where it
    /// was (beside it, if something new took its name) and opens it.
    pub fn restore_deleted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(note) = self.deleted.pop() else {
            let message = "No note has gone to the trash since this window opened.";
            notices::show(Notice::done(message), cx);
            return;
        };
        let path = free_path(&note.path);
        if let Err(error) = write_note(&path, &note.text) {
            let message = format!("Couldn’t restore “{}”: {error}", note_title(&note.path));
            self.deleted.push(note);
            notices::problem(message, cx);
            return;
        }
        if let Err(error) = self.open_path(&path, OpenIn::ActiveTab, window, cx) {
            notices::open_failed(&path, error, cx);
            return;
        }
        self.focus_active(window, cx);
        let message = format!("Restored “{}”.", note_title(&path));
        notices::show(Notice::done(message), cx);
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
}
