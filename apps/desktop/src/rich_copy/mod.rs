//! `edit.copy-rich-text`: the selection, or the whole note when nothing is
//! selected, on the clipboard as formatted text (the HTML export's
//! article) with its Markdown as the plain text, for pasting into mail,
//! documents and chat.

mod macos;

use std::path::Path;

use gasp_export::html::{HtmlOptions, export_html};
use gpui::{Context, Window};

use crate::notices::{self, Notice};
use crate::workspace::Workspace;

pub const COMMAND: &str = "edit.copy-rich-text";

pub fn install(workspace: &mut Workspace) {
    workspace.on_command(COMMAND, copy_rich_text);
}

/// The HTML for `markdown`, its images inlined so the paste carries them.
pub fn rich_html(markdown: &str, note: Option<&Path>, vault: &Path) -> String {
    let options = HtmlOptions {
        include_title: false,
        inline_images: true,
    };
    export_html(markdown, note, Some(vault), &options).html
}

fn copy_rich_text(workspace: &mut Workspace, _: &mut Window, cx: &mut Context<Workspace>) {
    let Some(editor) = workspace.active_editor(cx) else {
        return;
    };
    let (markdown, whole) = {
        let editor = editor.read(cx);
        let range = editor.selected_range();
        let text = editor.text();
        if range.is_empty() {
            (text, true)
        } else {
            (text[range].to_owned(), false)
        }
    };
    let html = rich_html(
        &markdown,
        workspace.active_path(cx).as_deref(),
        workspace.vault(),
    );
    match macos::copy_html_and_text(&html, &markdown) {
        Ok(()) => {
            let what = if whole { "the note" } else { "the selection" };
            notices::show(Notice::done(format!("Copied {what} as rich text.")), cx);
        }
        Err(error) => {
            notices::problem(format!("Couldn’t copy as rich text: {error}"), cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rich_text_is_the_notes_html() {
        let vault = tempfile::tempdir().unwrap();
        let html = rich_html("Some **bold** and a [[Link]].", None, vault.path());
        assert!(html.contains("<strong>bold</strong>"), "{html}");
        assert!(!html.contains("**"), "{html}");
    }
}
