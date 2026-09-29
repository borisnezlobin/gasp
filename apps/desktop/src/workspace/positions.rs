//! Where the reader was in each note: the cursor, the line at the top
//! of the view and the folded headings, so a note opens again as it was
//! left, in this session or the next. Kept in the vault's `device.toml`,
//! which never syncs, for the notes shown most recently.

use std::path::Path;

use gasp_config::device::NotePosition;
use gpui::{Context, Entity};

use super::Workspace;
use crate::editor::EditorView;

pub use gasp_config::device::{move_positions as moved, remember_position as remember};

impl Workspace {
    /// Where the reader is in `editor`'s note at `path`.
    pub(crate) fn position_of(
        &self,
        path: &Path,
        editor: &Entity<EditorView>,
        cx: &gpui::App,
    ) -> NotePosition {
        let editor = editor.read(cx);
        let (cursor, top) = editor.position();
        NotePosition {
            path: self.relative_name(path),
            cursor,
            top,
            folds: editor.folded_heading_lines(),
        }
    }

    /// Remembers where the reader is in `editor`'s note, as it leaves.
    pub(crate) fn remember_position(
        &mut self,
        path: &Path,
        editor: &Entity<EditorView>,
        cx: &gpui::App,
    ) {
        let position = self.position_of(path, editor, cx);
        remember(&mut self.positions, position);
    }

    /// Puts `editor` back where the reader left the note at `path`, if
    /// they've been in it. Answers whether it did.
    pub(crate) fn restore_position(
        &self,
        path: &Path,
        editor: &Entity<EditorView>,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = self.relative_name(path);
        let Some(kept) = self.positions.iter().rev().find(|kept| kept.path == name) else {
            return false;
        };
        let (cursor, top, folds) = (kept.cursor, kept.top, kept.folds.clone());
        editor.update(cx, |editor, cx| {
            editor.restore_folded_headings(folds, cx);
            editor.restore_position(cursor, top, cx);
        });
        true
    }

    /// The kept positions with every open note's current one folded in,
    /// for saving.
    pub(crate) fn positions_now(&self, cx: &gpui::App) -> Vec<NotePosition> {
        let mut positions = self.positions.clone();
        for pane in self.panes.panes() {
            for tab in pane.read(cx).tabs() {
                if let Some(note) = tab.note() {
                    let path = note.doc.read(cx).path().to_path_buf();
                    remember(&mut positions, self.position_of(&path, &note.editor, cx));
                }
            }
        }
        positions
    }

    /// Positions follow a note or folder that moved.
    pub(crate) fn positions_moved(&mut self, from: &Path, to: &Path) {
        let (from, to) = (self.relative_name(from), self.relative_name(to));
        moved(&mut self.positions, &from, &to);
    }
}

#[cfg(test)]
mod tests {
    use gasp_config::device::KEPT_POSITIONS;

    use super::*;

    fn at(path: &str, cursor: usize) -> NotePosition {
        NotePosition {
            path: path.into(),
            cursor,
            top: 0,
            folds: Vec::new(),
        }
    }

    #[test]
    fn the_latest_counts_and_the_oldest_go_past_the_cap() {
        let mut positions = vec![at("a.md", 1), at("b.md", 2)];
        remember(&mut positions, at("a.md", 5));
        assert_eq!(positions, [at("b.md", 2), at("a.md", 5)]);
        for index in 0..KEPT_POSITIONS {
            remember(&mut positions, at(&format!("{index}.md"), index));
        }
        assert_eq!(positions.len(), KEPT_POSITIONS);
        assert!(!positions.iter().any(|kept| kept.path == "b.md"));
    }

    #[test]
    fn positions_follow_notes_and_folders_that_move() {
        let mut positions = vec![at("a.md", 1), at("Old/b.md", 2), at("Older/c.md", 3)];
        moved(&mut positions, "a.md", "z.md");
        moved(&mut positions, "Old", "New");
        assert_eq!(
            positions,
            [at("z.md", 1), at("New/b.md", 2), at("Older/c.md", 3)]
        );
    }
}
