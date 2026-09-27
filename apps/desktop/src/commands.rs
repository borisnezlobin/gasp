//! The commands the editor view runs, looked up by id.

use editor_core::commands::{
    FootnoteCommand, Format, indent, insert_link, insert_or_jump_footnote, outdent, toggle_format,
};
use editor_core::footnotes::FootnoteSettings;
use editor_core::motion;
use gpui::{ClipboardItem, Context, Window};

use crate::editor::EditorView;
use crate::navigation::Motion;

type Handler = fn(&mut EditorView, &mut Window, &mut Context<EditorView>);

/// Cursor and selection motions: (move command, select command, motion).
const MOTIONS: [(&str, &str, Motion); 12] = [
    ("cursor.left", "select.left", Motion::Left),
    ("cursor.right", "select.right", Motion::Right),
    ("cursor.up", "select.up", Motion::Up),
    ("cursor.down", "select.down", Motion::Down),
    ("cursor.word-left", "select.word-left", Motion::WordLeft),
    ("cursor.word-right", "select.word-right", Motion::WordRight),
    ("cursor.line-start", "select.line-start", Motion::LineStart),
    ("cursor.line-end", "select.line-end", Motion::LineEnd),
    ("cursor.doc-start", "select.doc-start", Motion::DocStart),
    ("cursor.doc-end", "select.doc-end", Motion::DocEnd),
    ("cursor.page-up", "select.page-up", Motion::PageUp),
    ("cursor.page-down", "select.page-down", Motion::PageDown),
];

const HANDLERS: [(&str, Handler); 18] = [
    ("select.all", |view, _, cx| view.select_all(cx)),
    ("edit.delete-backward", |view, _, cx| {
        view.delete_or(|doc, at| doc.prev_char_boundary(at)..at, cx)
    }),
    ("edit.delete-forward", |view, _, cx| {
        view.delete_or(|doc, at| at..doc.next_char_boundary(at), cx)
    }),
    ("edit.delete-word-backward", |view, _, cx| {
        view.delete_or(|doc, at| motion::word_left(doc, at)..at, cx)
    }),
    ("edit.delete-word-forward", |view, _, cx| {
        view.delete_or(|doc, at| at..motion::word_right(doc, at), cx)
    }),
    ("edit.delete-to-line-start", |view, _, cx| {
        view.delete_or(motion::to_line_start, cx)
    }),
    ("edit.delete-to-line-end", |view, _, cx| {
        view.delete_or(motion::to_line_end, cx)
    }),
    ("edit.newline", |view, _, cx| view.newline(cx)),
    ("edit.indent", |view, _, cx| view.run_edit(indent, cx)),
    ("edit.outdent", |view, _, cx| view.run_edit(outdent, cx)),
    ("edit.undo", |view, _, cx| view.undo(cx)),
    ("edit.redo", |view, _, cx| view.redo(cx)),
    ("edit.copy", |view, _, cx| view.copy(cx)),
    ("edit.cut", |view, _, cx| view.cut(cx)),
    ("edit.paste", |view, _, cx| view.paste(cx)),
    ("edit.paste-plain", |view, _, cx| view.paste(cx)),
    ("format.link", |view, _, cx| view.run_edit(insert_link, cx)),
    ("footnote.insert-or-jump", |view, _, cx| view.footnote(cx)),
];

/// Whether the editor view can run `id`.
pub fn handles(id: &str) -> bool {
    find(id).is_some()
}

enum Found {
    Motion(Motion, bool),
    Format(Format),
    Handler(Handler),
}

fn find(id: &str) -> Option<Found> {
    if let Some((move_id, _, motion)) = MOTIONS
        .iter()
        .find(|(move_id, select_id, _)| *move_id == id || *select_id == id)
    {
        return Some(Found::Motion(*motion, *move_id != id));
    }
    if let Some(format) = Format::from_command_id(id) {
        return Some(Found::Format(format));
    }
    HANDLERS
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, handler)| Found::Handler(*handler))
}

impl EditorView {
    /// Runs a command by id. Returns false when the view doesn't know it.
    pub fn run_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match find(id) {
            Some(Found::Motion(motion, extend)) => self.apply_motion(motion, extend, window, cx),
            Some(Found::Format(format)) => {
                self.run_edit(|doc, sel, ts| toggle_format(doc, sel, format, ts), cx)
            }
            Some(Found::Handler(handler)) => handler(self, window, cx),
            None => return false,
        }
        true
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.select(0, self.doc().len(), cx);
    }

    fn selected_text(&self) -> Option<String> {
        let range = self.selected_range();
        (!range.is_empty()).then(|| self.doc().slice(range))
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.insert("", cx);
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert(&text, cx);
        }
    }

    fn footnote(&mut self, cx: &mut Context<Self>) {
        let settings = FootnoteSettings::default();
        let outcome =
            insert_or_jump_footnote(self.doc(), self.state.selection(), &settings, self.now_ms());
        match outcome {
            FootnoteCommand::Apply(transaction) => self.apply_transaction(transaction, cx),
            // Notices have no surface yet; the plugin showed this one as a toast.
            FootnoteCommand::Notice(message) => eprintln!("{message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use editor_config::commands::BUILTIN_COMMANDS;

    use super::*;

    /// Every text-editing command in the config has a handler here.
    #[test]
    fn every_editing_command_is_handled() {
        let editing = ["cursor.", "select.", "edit.", "format.", "footnote."];
        let missing: Vec<&str> = BUILTIN_COMMANDS
            .iter()
            .map(|spec| spec.id)
            .filter(|id| editing.iter().any(|prefix| id.starts_with(prefix)))
            .filter(|id| !handles(id))
            .collect();
        assert!(missing.is_empty(), "no handler for {missing:?}");
    }

    #[test]
    fn unknown_commands_are_not_handled() {
        assert!(!handles("tab.new"));
        assert!(!handles("markdown.cycle-symbols"));
    }
}
