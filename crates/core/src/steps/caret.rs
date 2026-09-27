//! What the typing steps read around a single caret.

use editor_snippets::InputContext as SnippetContext;

use crate::document::{Document, SelectionRange};
use crate::pipeline::{EditRequest, InputContext, StepContext};

/// The key that reached a typing step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Char(char),
    Tab,
}

impl Key {
    pub(super) fn of(request: &EditRequest) -> Option<Key> {
        match request {
            EditRequest::Tab => Some(Key::Tab),
            EditRequest::InsertText(text) => single_char(text).map(Key::Char),
            _ => None,
        }
    }
}

fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

/// The text of the caret's line split around the selection, with a typed
/// character already appended to `before` when nothing is selected.
///
/// Engine offsets index `before` followed by `selected`, and
/// [`LineAround::doc_offset`] maps them back into the document.
pub(super) struct LineAround {
    pub before: String,
    pub selected: String,
    pub after: String,
    line_start: usize,
    range: SelectionRange,
}

impl LineAround {
    /// `None` with several carets: typing steps only act on one.
    pub(super) fn read(cx: &StepContext<'_>, key: Key) -> Option<LineAround> {
        let [range] = cx.selection.ranges() else {
            return None;
        };
        let doc = cx.doc;
        let line = doc.line_of_offset(range.from());
        let line_start = doc.line_start(line);
        let mut before = doc.slice(line_start..range.from());
        if let (Key::Char(typed), true) = (key, range.is_empty()) {
            before.push(typed);
        }
        let after_end = doc.line_end(doc.line_of_offset(range.to())).max(range.to());
        Some(LineAround {
            before,
            selected: doc.slice(range.range()),
            after: doc.slice(range.to()..after_end),
            line_start,
            range: *range,
        })
    }

    /// Where an engine offset lands once the typed character is in the
    /// document, with nothing selected.
    pub(super) fn typed_offset(&self, engine_offset: usize) -> usize {
        self.line_start + engine_offset
    }

    /// The caret once the typed character is in the document.
    pub(super) fn caret_after_typing(&self) -> usize {
        self.line_start + self.before.len()
    }

    /// Engine offsets past the caret land on the typed character, which is not
    /// in the document yet, so they clamp to the end of the selection.
    pub(super) fn doc_offset(&self, engine_offset: usize) -> usize {
        (self.line_start + engine_offset).min(self.range.to())
    }
}

pub(super) fn snippet_context(context: InputContext) -> SnippetContext {
    match context {
        InputContext::Text => SnippetContext::Text,
        InputContext::Math => SnippetContext::Math,
        InputContext::Code => SnippetContext::Code,
        InputContext::Link => SnippetContext::Link,
        InputContext::Frontmatter => SnippetContext::Frontmatter,
        InputContext::Table => SnippetContext::Table,
        InputContext::Html => SnippetContext::Html,
        InputContext::Comment => SnippetContext::Comment,
    }
}

/// Whether math at `offset` is a `$$` block: an odd number of `$$` before it.
pub(super) fn in_block_math(doc: &Document, offset: usize) -> bool {
    let text = doc.slice(0..offset);
    text.matches("$$").count() % 2 == 1
}
