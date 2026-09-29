//! Following a link from a note: web links open in the browser, note
//! links open (or create) the note and jump to the heading.

use gpui::{Context, Entity, Window};

pub use gasp_vault::knowledge::links::{WEB_SCHEMES, resolve_note, split_target};

use super::{OpenIn, Workspace};
use crate::editor::EditorView;
use crate::outline::headings;

/// The offset of the heading titled `heading` in `text`, ignoring case.
fn heading_offset(text: &str, heading: &str) -> Option<usize> {
    headings(text)
        .into_iter()
        .find(|found| found.title.eq_ignore_ascii_case(heading.trim()))
        .map(|found| found.offset)
}

impl Workspace {
    pub(crate) fn follow_link(
        &mut self,
        target: &str,
        from: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if WEB_SCHEMES.iter().any(|scheme| target.starts_with(scheme)) {
            crate::sandbox::open_url(target, cx);
            return;
        }
        let (note, heading) = split_target(target);
        if note.is_empty() {
            if let Some(offset) = heading.and_then(|h| heading_offset(&from.read(cx).text(), h)) {
                from.update(cx, |editor, cx| editor.select(offset, offset, cx));
            }
            return;
        }
        let from_path = self.active_path(cx);
        let path = resolve_note(self.vault(), from_path.as_deref(), note)
            .unwrap_or_else(|| self.vault().join(format!("{note}.md")));
        if !path.exists() && std::fs::write(&path, "").is_err() {
            return;
        }
        if self
            .open_path(&path, OpenIn::ActiveTab, window, cx)
            .is_err()
        {
            return;
        }
        self.jump_to_heading(heading, cx);
    }

    fn jump_to_heading(&mut self, heading: Option<&str>, cx: &mut Context<Self>) {
        let (Some(heading), Some(editor)) = (heading, self.active_editor(cx)) else {
            return;
        };
        let offset = heading_offset(&editor.read(cx).text(), heading);
        if let Some(offset) = offset {
            editor.update(cx, |editor, cx| editor.select(offset, offset, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_match_ignoring_case() {
        let text = "# One\ntext\n## Two words\n";
        assert_eq!(heading_offset(text, "two WORDS"), Some(11));
        assert_eq!(heading_offset(text, "three"), None);
    }
}
