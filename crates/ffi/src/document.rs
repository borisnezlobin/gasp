//! One open note: its text and syntax tree, kept in step with the text view
//! so each keystroke reparses only the blocks it touched.

use std::ops::Range;
use std::sync::{Arc, Mutex, PoisonError};

use editor_core::render::{RenderInput, RevealSettings, plan};
use editor_core::syntax::{self, Edit, SyntaxTree};

use crate::offsets::{TextRange, Utf16Offsets};
use crate::plan::{NotePlan, note_plan};

#[derive(uniffi::Object)]
pub struct NoteDocument {
    parsed: Mutex<ParsedText>,
}

struct ParsedText {
    text: String,
    tree: SyntaxTree,
    offsets: Utf16Offsets,
}

impl ParsedText {
    fn new(text: String) -> Self {
        Self {
            tree: syntax::parse(&text),
            offsets: Utf16Offsets::new(&text),
            text,
        }
    }

    fn update(&mut self, text: String) {
        let Some(edit) = changed_span(&self.text, &text) else {
            return;
        };
        self.tree.edit(&text, &edit);
        self.offsets = Utf16Offsets::new(&text);
        self.text = text;
    }
}

#[uniffi::export]
impl NoteDocument {
    #[uniffi::constructor]
    pub fn new(text: String) -> Arc<Self> {
        Arc::new(Self {
            parsed: Mutex::new(ParsedText::new(text)),
        })
    }

    pub fn text(&self) -> String {
        self.lock().text.clone()
    }

    /// Takes the text view's whole text after any change (typing,
    /// autocorrect, dictation, paste or undo) and reparses what differs.
    pub fn update(&self, text: String) {
        self.lock().update(text);
    }

    /// What every line should look like with `selection` (in UTF-16
    /// offsets; an empty range is the cursor), as the desktop app plans it.
    pub fn plan(&self, selection: TextRange) -> NotePlan {
        let parsed = self.lock();
        let selections = [parsed.offsets.byte_range(selection)];
        let settings = RevealSettings::default();
        let plan = plan(&RenderInput {
            text: &parsed.text,
            tree: &parsed.tree,
            selections: &selections,
            settings: &settings,
        });
        note_plan(&plan, &parsed.offsets)
    }
}

impl NoteDocument {
    fn lock(&self) -> std::sync::MutexGuard<'_, ParsedText> {
        self.parsed.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The one edit that turns `old` into `new`: everything between their
/// common start and common end. `None` when they're the same.
fn changed_span(old: &str, new: &str) -> Option<Edit> {
    if old == new {
        return None;
    }
    let prefix = floor_boundary(old, common_prefix(old.as_bytes(), new.as_bytes()));
    let suffix_limit = old.len().min(new.len()) - prefix;
    let suffix = common_suffix(old.as_bytes(), new.as_bytes()).min(suffix_limit);
    let old_end = ceil_boundary(old, old.len() - suffix);
    let suffix = old.len() - old_end;
    Some(Edit {
        old: Range {
            start: prefix,
            end: old_end,
        },
        new_len: new.len() - suffix - prefix,
    })
}

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

fn common_suffix(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

fn floor_boundary(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn ceil_boundary(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(old: &str, edit: &Edit, new: &str) -> String {
        let inserted = &new[edit.old.start..edit.old.start + edit.new_len];
        format!(
            "{}{inserted}{}",
            &old[..edit.old.start],
            &old[edit.old.end..]
        )
    }

    #[test]
    fn the_changed_span_rebuilds_the_new_text() {
        let cases = [
            ("abc", "abXc"),
            ("aaa", "aaaa"),
            ("hello world", "hello"),
            ("é", "è"),
            ("x𝜋y", "x𝜎y"),
            ("", "new"),
            ("gone", ""),
        ];
        for (old, new) in cases {
            let edit = changed_span(old, new).unwrap();
            assert_eq!(apply(old, &edit, new), new, "{old:?} → {new:?}");
        }
    }

    #[test]
    fn the_same_text_is_no_edit() {
        assert_eq!(changed_span("same", "same"), None);
    }

    #[test]
    fn updating_parses_as_a_fresh_document_would() {
        let document = NoteDocument::new("# Title\n\nSome *text*\n".into());
        document.update("# Title\n\nSome *text* and **more**\n\n- item\n".into());
        let fresh = NoteDocument::new(document.text());
        let at_end = TextRange { start: 0, end: 0 };
        assert_eq!(document.plan(at_end), fresh.plan(at_end));
    }
}
