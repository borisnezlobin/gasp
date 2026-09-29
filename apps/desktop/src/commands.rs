//! The commands the editor view runs, looked up by id.

use gasp_core::commands::{
    FootnoteCommand, Format, duplicate_lines, insert_callout, insert_link, insert_or_jump_footnote,
    move_lines_down, move_lines_up, toggle_bullet_list, toggle_format, toggle_numbered_list,
    toggle_tasks,
};
use gasp_core::footnotes::FootnoteSettings;
use gasp_core::motion;
use gasp_core::table::TableOp;
use gpui::{Context, Window};

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

const HANDLERS: &[(&str, Handler)] = &[
    ("select.all", |view, _, cx| view.select_all(cx)),
    ("edit.delete-backward", |view, _, cx| {
        view.delete_backward(cx)
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
    ("edit.indent", |view, _, cx| view.tab(cx)),
    ("edit.outdent", |view, _, cx| view.back_tab(cx)),
    ("edit.move-line-up", |view, _, cx| {
        view.run_edit(move_lines_up, cx)
    }),
    ("edit.move-line-down", |view, _, cx| {
        view.run_edit(move_lines_down, cx)
    }),
    ("edit.duplicate-line", |view, _, cx| {
        view.run_edit(duplicate_lines, cx)
    }),
    ("edit.toggle-task", |view, _, cx| {
        view.run_edit(toggle_tasks, cx)
    }),
    ("format.bullet-list", |view, _, cx| {
        view.run_edit(toggle_bullet_list, cx)
    }),
    ("format.numbered-list", |view, _, cx| {
        view.run_edit(toggle_numbered_list, cx)
    }),
    ("edit.undo", |view, _, cx| view.undo(cx)),
    ("edit.redo", |view, _, cx| view.redo(cx)),
    ("edit.copy", |view, _, cx| view.copy_selection_or_line(cx)),
    ("edit.cut", |view, _, cx| view.cut_selection_or_line(cx)),
    ("edit.paste", |view, _, cx| view.smart_paste(cx)),
    ("edit.paste-plain", |view, _, cx| view.paste_plain(cx)),
    #[cfg(target_os = "macos")]
    ("edit.look-up", |view, window, cx| {
        view.look_up(None, window, cx);
    }),
    ("code.copy-block", |view, _, cx| {
        view.copy_code_block_at_cursor(cx)
    }),
    ("note.import-image", |view, window, cx| {
        view.import_image(window, cx)
    }),
    ("format.link", |view, _, cx| view.run_edit(insert_link, cx)),
    ("format.callout", |view, _, cx| {
        view.run_edit(insert_callout, cx)
    }),
    ("footnote.insert-or-jump", |view, _, cx| view.footnote(cx)),
    ("footnote.tidy", |view, _, cx| view.tidy_footnotes(cx)),
    ("footnote.fix-typos", |view, _, cx| {
        view.fix_footnote_typos(cx)
    }),
    ("markdown.cycle-symbols", |view, _, cx| {
        view.cycle_symbols(cx)
    }),
    ("link.follow", |view, _, cx| view.follow_link(cx)),
    ("link.make-card", |view, _, cx| view.make_card(cx)),
    ("view.zoom-in", |view, _, cx| view.zoom_in(cx)),
    ("view.zoom-out", |view, _, cx| view.zoom_out(cx)),
    ("view.zoom-reset", |view, _, cx| view.reset_zoom(cx)),
    ("view.toggle-readable-width", |view, _, cx| {
        view.toggle_readable_width(cx)
    }),
];

/// Whether the editor view can run `id`.
pub fn handles(id: &str) -> bool {
    find(id).is_some()
        || TableOp::from_command_id(id).is_some()
        || crate::table_edit::COMMANDS.contains(&id)
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
        if self.run_suggestion_key(id, cx) || self.run_table_key(id, cx) {
            return true;
        }
        if self.run_table_command(id, cx) {
            return true;
        }
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

    /// While suggestions show, Up and Down move through them and Enter or
    /// Tab accept one, whatever keys those commands are bound to.
    fn run_suggestion_key(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if self.suggestions().is_none() {
            return false;
        }
        match id {
            "cursor.up" => self.move_suggestion(-1, cx),
            "cursor.down" => self.move_suggestion(1, cx),
            "edit.newline" | "edit.indent" => return self.accept_suggestion(cx),
            _ => return false,
        }
        true
    }

    /// Selects everything without scrolling: the view stays where the
    /// reader is.
    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.select(0, self.doc().len(), cx);
        self.autoscroll = false;
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
    use gasp_config::commands::BUILTIN_COMMANDS;

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
        assert!(!handles("sidebar.files.toggle"));
    }

    #[test]
    fn live_preview_commands_are_handled() {
        for id in [
            "markdown.cycle-symbols",
            "link.follow",
            "view.zoom-in",
            "view.zoom-out",
            "view.zoom-reset",
            "view.toggle-readable-width",
        ] {
            assert!(handles(id), "{id}");
        }
    }
}
