//! Changing another note's text for the user, as renaming a note or
//! linking a mention does: in its editor when it's open, so undo and
//! unsaved edits keep working, else on disk.

use std::io;
use std::ops::Range;
use std::path::Path;

use gpui::{Context, Entity};

use super::build::relative;
use crate::editor::EditorView;
use crate::workspace::Workspace;
use crate::workspace::files::atomic_write;

/// The editor showing the note at `path`, if a tab has it open.
pub fn open_editor(
    workspace: &Workspace,
    path: &Path,
    cx: &gpui::App,
) -> Option<Entity<EditorView>> {
    workspace.panes().into_iter().find_map(|pane| {
        pane.read(cx).tabs().iter().find_map(|tab| {
            let note = tab.note()?;
            (note.doc.read(cx).path() == path).then(|| note.editor.clone())
        })
    })
}

/// Runs `change` on the note at `path` and keeps what it returns: in the
/// open editor, or written to the file. Returns the new text, or `None`
/// when `change` left it alone.
pub fn edit_note(
    workspace: &Workspace,
    path: &Path,
    change: impl FnOnce(&str) -> Option<String>,
    cx: &mut Context<Workspace>,
) -> io::Result<Option<String>> {
    if let Some(editor) = open_editor(workspace, path, cx) {
        let old = editor.read(cx).text();
        let Some(new) = change(&old) else {
            return Ok(None);
        };
        editor.update(cx, |editor, cx| {
            replace_keeping_cursor(editor, &old, &new, cx)
        });
        note_indexed(workspace, path, &new, cx);
        return Ok(Some(new));
    }
    let old = std::fs::read_to_string(path)?;
    let Some(new) = change(&old) else {
        return Ok(None);
    };
    atomic_write(path, &new)?;
    note_indexed(workspace, path, &new, cx);
    Ok(Some(new))
}

fn note_indexed(workspace: &Workspace, path: &Path, text: &str, cx: &mut Context<Workspace>) {
    let Some(relative) = relative(workspace.vault(), path) else {
        return;
    };
    workspace.vault_index().update(cx, |index, cx| {
        index.note_text_changed(&relative, text);
        cx.notify();
    });
}

/// Replaces only the part of the text that changed, as one undoable
/// edit, and leaves the selection where it was relative to the text.
fn replace_keeping_cursor(
    editor: &mut EditorView,
    old: &str,
    new: &str,
    cx: &mut Context<EditorView>,
) {
    let (old_range, new_range) = changed_ranges(old, new);
    let (anchor, head) = (editor.anchor(), editor.cursor());
    editor.replace(old_range.clone(), &new[new_range.clone()], cx);
    let shift = |at: usize| -> usize {
        if at <= old_range.start {
            at
        } else if at >= old_range.end {
            at + new_range.len() - old_range.len()
        } else {
            new_range.end
        }
    };
    editor.select(shift(anchor), shift(head), cx);
}

/// The ranges of `old` and `new` that differ, after their shared start
/// and end, cut at character boundaries.
pub fn changed_ranges(old: &str, new: &str) -> (Range<usize>, Range<usize>) {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let max_suffix = old.len().min(new.len()) - start;
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    (start..old.len() - suffix, start..new.len() - suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_changed_middle_is_replaced() {
        let old = "See [[Old]] and [[Old]].";
        let new = "See [[New name]] and [[New name]].";
        let (a, b) = changed_ranges(old, new);
        assert_eq!(&old[a], "Old]] and [[Old");
        assert_eq!(&new[b], "New name]] and [[New name");
        let (a, b) = changed_ranges("same", "same");
        assert!(a.is_empty() && b.is_empty());
        let (a, b) = changed_ranges("aé", "aè");
        assert_eq!((a, b), (1..3, 1..3));
    }
}
