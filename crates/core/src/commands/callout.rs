//! `format.callout`: turn the selected lines into a callout, or start an
//! empty one, as Obsidian's "Insert callout" does.

use crate::document::{Document, Selection};
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

pub const CALLOUT: &str = "format.callout";

const HEADER: &str = "> [!note]";
const QUOTE: &str = "> ";

/// On a blank line, writes an empty note callout with the caret on its
/// first body line. Otherwise quotes every selected line under a note
/// callout header, keeping the selection on the same text.
pub fn insert_callout(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let primary = selection.primary();
    let first = doc.line_of_offset(primary.from());
    let last = doc.line_of_offset(primary.to());
    let origin = Origin::command(CALLOUT);
    if first == last && doc.line_text(first).trim().is_empty() {
        let line = doc.line_range(first);
        let text = format!("{HEADER}\n{QUOTE}");
        let caret = line.start + text.len();
        let changes = ChangeSet::replace(line, text);
        return Transaction::new(changes, origin, timestamp_ms)
            .with_selection(Selection::cursor(caret));
    }
    let mut edits = vec![TextEdit::insert(
        doc.line_start(first),
        format!("{HEADER}\n{QUOTE}"),
    )];
    edits.extend((first + 1..=last).map(|line| TextEdit::insert(doc.line_start(line), QUOTE)));
    let changes = ChangeSet::new(edits).unwrap_or_default();
    Transaction::new(changes, origin, timestamp_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, from: usize, to: usize) -> (String, Selection) {
        let doc = Document::from(text);
        let selection = Selection::single(crate::document::SelectionRange::new(from, to));
        let transaction = insert_callout(&doc, &selection, 0);
        let after = transaction.changes.apply_to_string(text).unwrap();
        let selection = transaction
            .selection
            .unwrap_or_else(|| selection.map(&transaction.changes));
        (after, selection)
    }

    #[test]
    fn a_blank_line_gets_an_empty_callout() {
        let (text, selection) = run("One\n\nTwo", 4, 4);
        assert_eq!(text, "One\n> [!note]\n> \nTwo");
        assert_eq!(selection.primary().to(), "One\n> [!note]\n> ".len());
    }

    #[test]
    fn selected_lines_go_inside_the_callout() {
        let (text, _) = run("First\nSecond\nThird", 2, 8);
        assert_eq!(text, "> [!note]\n> First\n> Second\nThird");
    }
}
