//! Where the reader was in each note: the cursor and the line at the top
//! of the view, so a note opens again where it was left, in this session
//! or the next. Kept in the vault's `device.toml`, which never syncs,
//! for the notes shown most recently.

use std::path::Path;

use editor_config::device::NotePosition;
use gpui::{Context, Entity};

use super::Workspace;
use crate::editor::EditorView;

/// How many notes' positions are kept; the least recent go first.
pub const KEPT_POSITIONS: usize = 300;

/// Records `position`, most recent last, dropping the oldest past the cap.
pub fn remember(positions: &mut Vec<NotePosition>, position: NotePosition) {
    positions.retain(|kept| kept.path != position.path);
    positions.push(position);
    let over = positions.len().saturating_sub(KEPT_POSITIONS);
    positions.drain(..over);
}

/// Follows a note, or every note in a folder, that moved from `from` to
/// `to` (vault-relative).
pub fn moved(positions: &mut [NotePosition], from: &str, to: &str) {
    for position in positions {
        if position.path == from {
            position.path = to.to_string();
        } else if let Some(rest) = position.path.strip_prefix(&format!("{from}/")) {
            position.path = format!("{to}/{rest}");
        }
    }
}

impl Workspace {
    /// Where the reader is in `editor`'s note at `path`.
    pub(crate) fn position_of(
        &self,
        path: &Path,
        editor: &Entity<EditorView>,
        cx: &gpui::App,
    ) -> NotePosition {
        let (cursor, top) = editor.read(cx).position();
        NotePosition {
            path: self.relative_name(path),
            cursor,
            top,
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
        let (cursor, top) = (kept.cursor, kept.top);
        editor.update(cx, |editor, cx| editor.restore_position(cursor, top, cx));
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
    use super::*;

    fn at(path: &str, cursor: usize) -> NotePosition {
        NotePosition {
            path: path.into(),
            cursor,
            top: 0,
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
