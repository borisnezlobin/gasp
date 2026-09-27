//! `format.link`: wrap the selection in a Markdown link.

use crate::document::{Document, Selection, SelectionRange};
use crate::pipeline::RangePlan;
use crate::transaction::{TextEdit, Transaction};

use super::command_transaction;

pub const LINK: &str = "format.link";

/// Wraps each selection in `[text]()` with the caret between the
/// parentheses, ready for the URL. A selected URL becomes the link target
/// instead, with the caret in the brackets. An empty selection inserts
/// `[]()` with the caret in the brackets.
pub fn insert_link(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let plans = selection
        .ranges()
        .iter()
        .map(|range| plan_link(doc, range))
        .collect();
    command_transaction(doc, selection, plans, LINK, timestamp_ms)
}

fn is_url(text: &str) -> bool {
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| text.starts_with(scheme))
        && !text.contains(char::is_whitespace)
}

fn plan_link(doc: &Document, range: &SelectionRange) -> RangePlan {
    let selected = doc.slice(range.range());
    let (text, caret) = if is_url(&selected) {
        (format!("[]({selected})"), 1)
    } else if selected.is_empty() {
        ("[]()".to_owned(), 1)
    } else {
        let text = format!("[{selected}]()");
        let caret = text.len() - 1;
        (text, caret)
    };
    RangePlan {
        edit: TextEdit::new(range.range(), text),
        anchor: caret,
        head: caret,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::EditorState;

    fn link(text: &str, range: std::ops::Range<usize>) -> (String, usize) {
        let doc = Document::from(text);
        let selection = Selection::single(SelectionRange::new(range.start, range.end));
        let transaction = insert_link(&doc, &selection, 0);
        let mut state = EditorState::new(doc);
        state.apply(transaction).unwrap();
        (
            state.doc().slice(0..state.doc().len()),
            state.selection().primary().head,
        )
    }

    #[test]
    fn wraps_text_and_waits_for_the_url() {
        assert_eq!(link("see docs", 4..8), ("see [docs]()".into(), 11));
    }

    #[test]
    fn a_selected_url_becomes_the_target() {
        let (text, caret) = link("https://example.com", 0..19);
        assert_eq!(text, "[](https://example.com)");
        assert_eq!(caret, 1);
    }

    #[test]
    fn an_empty_selection_inserts_an_empty_link() {
        assert_eq!(link("a  b", 2..2), ("a []() b".into(), 3));
    }
}
