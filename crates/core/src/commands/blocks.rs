//! `format.horizontal-rule`, `format.code-block` and `format.math-block`:
//! blocks that need lines of their own. Each gets a blank line between it
//! and any text above or below, since `---` right under a paragraph would
//! turn that paragraph into a heading.

use crate::document::{Document, Selection};
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

pub const HORIZONTAL_RULE: &str = "format.horizontal-rule";
pub const CODE_BLOCK: &str = "format.code-block";
pub const MATH_BLOCK: &str = "format.math-block";

const RULE: &str = "---";
const CODE_FENCE: &str = "```";
const MATH_FENCE: &str = "$$";

/// Puts a rule on the cursor's line when it's blank, or under it, with
/// the caret on a fresh line below the rule.
pub fn insert_horizontal_rule(
    doc: &Document,
    selection: &Selection,
    timestamp_ms: u64,
) -> Transaction {
    let line = doc.line_of_offset(selection.primary().head);
    let block = Block {
        lines: &[RULE, ""],
        caret_line: 1,
    };
    place_block(
        doc,
        line,
        block,
        Origin::command(HORIZONTAL_RULE),
        timestamp_ms,
    )
}

/// Starts an empty code block with the caret inside, or fences the
/// selected lines.
pub fn insert_code_block(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    fenced_block(doc, selection, CODE_FENCE, CODE_BLOCK, timestamp_ms)
}

/// Starts an empty math block with the caret inside, or fences the
/// selected lines.
pub fn insert_math_block(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    fenced_block(doc, selection, MATH_FENCE, MATH_BLOCK, timestamp_ms)
}

/// The lines a block is written as, and which of them gets the caret.
#[derive(Clone, Copy)]
struct Block<'a> {
    lines: &'a [&'a str],
    caret_line: usize,
}

fn fenced_block(
    doc: &Document,
    selection: &Selection,
    fence: &str,
    command: &str,
    timestamp_ms: u64,
) -> Transaction {
    let primary = selection.primary();
    let origin = Origin::command(command);
    if !primary.is_empty() {
        return fence_lines(doc, selection, fence, origin, timestamp_ms);
    }
    let line = doc.line_of_offset(primary.head);
    let block = Block {
        lines: &[fence, "", fence],
        caret_line: 1,
    };
    place_block(doc, line, block, origin, timestamp_ms)
}

/// Writes `block` over `line` when it's blank, or after it otherwise.
fn place_block(
    doc: &Document,
    line: usize,
    block: Block<'_>,
    origin: Origin,
    timestamp_ms: u64,
) -> Transaction {
    let (range, before) = if has_text(doc, line) {
        let end = doc.line_end(line);
        (end..end, "\n\n")
    } else {
        (doc.line_range(line), blank_line_above(doc, line))
    };
    let after = blank_line_below(doc, line);
    let caret_column: usize = block.lines[..block.caret_line]
        .iter()
        .map(|text| text.len() + 1)
        .sum();
    let caret = range.start + before.len() + caret_column;
    let text = format!("{before}{}{after}", block.lines.join("\n"));
    Transaction::new(ChangeSet::replace(range, text), origin, timestamp_ms)
        .with_selection(Selection::cursor(caret))
}

/// Puts `fence` above and below every selected line.
fn fence_lines(
    doc: &Document,
    selection: &Selection,
    fence: &str,
    origin: Origin,
    timestamp_ms: u64,
) -> Transaction {
    let primary = selection.primary();
    let first = doc.line_of_offset(primary.from());
    let mut last = doc.line_of_offset(primary.to());
    if last > first && primary.to() == doc.line_start(last) {
        last -= 1;
    }
    let edits = vec![
        TextEdit::insert(
            doc.line_start(first),
            format!("{}{fence}\n", blank_line_above(doc, first)),
        ),
        TextEdit::insert(
            doc.line_end(last),
            format!("\n{fence}{}", blank_line_below(doc, last)),
        ),
    ];
    let changes = ChangeSet::new(edits).unwrap_or_default();
    Transaction::new(changes, origin, timestamp_ms)
}

/// A line break to leave a blank line under the text above `line`.
fn blank_line_above(doc: &Document, line: usize) -> &'static str {
    match line.checked_sub(1) {
        Some(above) if has_text(doc, above) => "\n",
        _ => "",
    }
}

/// A line break to leave a blank line over the text below `line`.
fn blank_line_below(doc: &Document, line: usize) -> &'static str {
    if has_text(doc, line + 1) { "\n" } else { "" }
}

fn has_text(doc: &Document, line: usize) -> bool {
    line < doc.line_count() && !doc.line_text(line).trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::SelectionRange;

    type Command = fn(&Document, &Selection, u64) -> Transaction;

    /// The text after `command`, with `|` where the caret ends up.
    fn run(command: Command, text: &str, from: usize, to: usize) -> String {
        let doc = Document::from(text);
        let selection = Selection::single(SelectionRange::new(from, to));
        let transaction = command(&doc, &selection, 0);
        let mut after = transaction.changes.apply_to_string(text).unwrap();
        let selection = transaction
            .selection
            .unwrap_or_else(|| selection.map(&transaction.changes));
        if selection.primary().is_empty() {
            after.insert(selection.primary().head, '|');
        }
        after
    }

    #[test]
    fn a_rule_under_a_paragraph_leaves_a_blank_line_so_it_isnt_a_heading() {
        assert_eq!(
            run(insert_horizontal_rule, "Intro", 5, 5),
            "Intro\n\n---\n|"
        );
    }

    #[test]
    fn a_rule_on_a_blank_line_between_paragraphs_keeps_both_apart() {
        assert_eq!(
            run(insert_horizontal_rule, "One\n\nTwo", 4, 4),
            "One\n\n---\n|\n\nTwo"
        );
    }

    #[test]
    fn a_rule_in_an_empty_note_is_just_the_rule() {
        assert_eq!(run(insert_horizontal_rule, "", 0, 0), "---\n|");
    }

    #[test]
    fn an_empty_code_block_puts_the_caret_inside() {
        assert_eq!(
            run(insert_code_block, "One\n\nTwo", 4, 4),
            "One\n\n```\n|\n```\n\nTwo"
        );
    }

    #[test]
    fn a_code_block_after_a_line_of_text_starts_below_it() {
        assert_eq!(
            run(insert_code_block, "One\nTwo", 1, 1),
            "One\n\n```\n|\n```\n\nTwo"
        );
    }

    #[test]
    fn selected_lines_get_fenced() {
        assert_eq!(
            run(insert_math_block, "See\nx = 1\ny = 2\nDone", 4, 15),
            "See\n\n$$\nx = 1\ny = 2\n$$\n\nDone"
        );
    }

    #[test]
    fn a_selection_ending_at_the_next_line_start_leaves_that_line_out() {
        assert_eq!(
            run(insert_code_block, "a()\nb()\n", 0, 4),
            "```\na()\n```\n\nb()\n"
        );
    }
}
