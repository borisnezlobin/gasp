//! `edit.indent` and `edit.outdent` for Tab and Shift+Tab.

use std::ops::Range;

use crate::document::{Document, Selection};
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

pub const INDENT: &str = "edit.indent";
pub const OUTDENT: &str = "edit.outdent";

/// Spaces that count as one level when outdenting space-indented lines.
const SPACES_PER_LEVEL: usize = 4;

/// Tab indents every selected line when the selection spans lines or the
/// caret is in a list item, and otherwise types a tab character.
pub fn indent(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let lines = selected_lines(doc, selection);
    let edits = if indents_lines(doc, selection) {
        lines
            .map(|line| TextEdit::insert(doc.line_start(line), "\t"))
            .collect()
    } else {
        selection
            .ranges()
            .iter()
            .map(|range| TextEdit::new(range.range(), "\t"))
            .collect()
    };
    lines_transaction(edits, INDENT, timestamp_ms)
}

/// Shift+Tab removes one level (a tab, or up to four spaces) from every
/// selected line.
pub fn outdent(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let edits = selected_lines(doc, selection)
        .filter_map(|line| leading_level(doc, line))
        .map(TextEdit::delete)
        .collect();
    lines_transaction(edits, OUTDENT, timestamp_ms)
}

fn lines_transaction(edits: Vec<TextEdit>, command: &str, timestamp_ms: u64) -> Transaction {
    // Edits are one per line or one per selected range, in order, so they
    // never overlap.
    let changes = ChangeSet::new(edits).unwrap_or_default();
    Transaction::new(changes, Origin::command(command), timestamp_ms)
}

fn indents_lines(doc: &Document, selection: &Selection) -> bool {
    selection.ranges().iter().any(|range| {
        let first = doc.line_of_offset(range.from());
        first != doc.line_of_offset(range.to()) || is_list_item(&doc.line_text(first))
    })
}

/// Every line a selection touches, once each, in order.
fn selected_lines(doc: &Document, selection: &Selection) -> impl Iterator<Item = usize> {
    let mut lines: Vec<usize> = selection
        .ranges()
        .iter()
        .flat_map(|range| doc.line_of_offset(range.from())..=doc.line_of_offset(range.to()))
        .collect();
    lines.dedup();
    lines.into_iter()
}

fn leading_level(doc: &Document, line: usize) -> Option<Range<usize>> {
    let text = doc.line_text(line);
    let start = doc.line_start(line);
    if text.starts_with('\t') {
        return Some(start..start + 1);
    }
    let spaces = text
        .bytes()
        .take(SPACES_PER_LEVEL)
        .take_while(|byte| *byte == b' ')
        .count();
    (spaces > 0).then(|| start..start + spaces)
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    if ["- ", "* ", "+ "]
        .iter()
        .any(|marker| trimmed.starts_with(marker))
    {
        return true;
    }
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0
        && [". ", ") "]
            .iter()
            .any(|end| trimmed[digits..].starts_with(end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::SelectionRange;
    use crate::history::EditorState;

    fn run(
        text: &str,
        range: Range<usize>,
        command: fn(&Document, &Selection, u64) -> Transaction,
    ) -> String {
        let doc = Document::from(text);
        let selection = Selection::single(SelectionRange::new(range.start, range.end));
        let transaction = command(&doc, &selection, 0);
        let mut state = EditorState::new(doc);
        state.apply(transaction).unwrap();
        state.doc().slice(0..state.doc().len())
    }

    #[test]
    fn tab_in_plain_text_types_a_tab() {
        assert_eq!(run("ab", 1..1, indent), "a\tb");
    }

    #[test]
    fn tab_in_a_list_item_indents_the_item() {
        assert_eq!(run("- one\n- two", 9..9, indent), "- one\n\t- two");
        assert_eq!(run("1. one", 4..4, indent), "\t1. one");
    }

    #[test]
    fn tab_over_several_lines_indents_each() {
        assert_eq!(run("a\nb\nc", 0..3, indent), "\ta\n\tb\nc");
    }

    #[test]
    fn shift_tab_removes_one_level() {
        assert_eq!(run("\t- one\n      two", 0..10, outdent), "- one\n  two");
        assert_eq!(run("plain", 2..2, outdent), "plain");
    }
}
