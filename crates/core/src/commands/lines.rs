//! Whole-line commands: moving the selected lines up or down, duplicating
//! them, and checking or unchecking checklist items.

use std::ops::RangeInclusive;

use crate::document::{Document, Selection, SelectionRange};
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

pub const MOVE_UP: &str = "edit.move-line-up";
pub const MOVE_DOWN: &str = "edit.move-line-down";
pub const DUPLICATE: &str = "edit.duplicate-line";
pub const CHECK_CHECKBOXES: &str = "edit.check-checkboxes";

/// The lines the selection touches, as one block. A selection ending at
/// the very start of a line leaves that line out, as it looks unselected.
pub(super) fn selected_block(doc: &Document, selection: &Selection) -> RangeInclusive<usize> {
    let from = selection
        .ranges()
        .iter()
        .map(|r| r.from())
        .min()
        .unwrap_or(0);
    let to = selection.ranges().iter().map(|r| r.to()).max().unwrap_or(0);
    let first = doc.line_of_offset(from);
    let mut last = doc.line_of_offset(to);
    if last > first && doc.line_start(last) == to {
        last -= 1;
    }
    first..=last
}

/// The text of lines `first..=last` without the final line break.
fn block_text(doc: &Document, lines: &RangeInclusive<usize>) -> String {
    doc.slice(doc.line_start(*lines.start())..doc.line_end(*lines.end()))
}

/// The line break after `line`, or `\n` for the last line.
fn break_after(doc: &Document, line: usize) -> String {
    let text = doc.slice(doc.line_end(line)..doc.line_start(line + 1));
    if text.is_empty() {
        "\n".to_owned()
    } else {
        text
    }
}

/// Every range of the selection moved by `delta` bytes.
fn shifted(selection: &Selection, delta: isize) -> Selection {
    let ranges = selection
        .ranges()
        .iter()
        .map(|range| {
            SelectionRange::new(
                range.anchor.saturating_add_signed(delta),
                range.head.saturating_add_signed(delta),
            )
        })
        .collect();
    Selection::new(ranges, selection.primary_index())
}

fn command(edit: TextEdit, selection: Selection, id: &str, timestamp_ms: u64) -> Transaction {
    let changes = ChangeSet::new(vec![edit]).unwrap_or_default();
    Transaction::new(changes, Origin::command(id), timestamp_ms).with_selection(selection)
}

/// Swaps the selected lines with the line above them. Nothing happens on
/// the first line.
pub fn move_lines_up(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let lines = selected_block(doc, selection);
    let Some(above) = lines.start().checked_sub(1) else {
        return Transaction::new(ChangeSet::empty(), Origin::command(MOVE_UP), timestamp_ms);
    };
    let block = block_text(doc, &lines);
    let above_text = doc.line_text(above);
    let separator = break_after(doc, above);
    let start = doc.line_start(above);
    let end = doc.line_end(*lines.end());
    let moved = format!("{block}{separator}{above_text}");
    let delta = -((above_text.len() + separator.len()) as isize);
    command(
        TextEdit::new(start..end, moved),
        shifted(selection, delta),
        MOVE_UP,
        timestamp_ms,
    )
}

/// Swaps the selected lines with the line below them. Nothing happens on
/// the last line.
pub fn move_lines_down(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let lines = selected_block(doc, selection);
    let below = lines.end() + 1;
    if below >= doc.line_count() {
        return Transaction::new(ChangeSet::empty(), Origin::command(MOVE_DOWN), timestamp_ms);
    }
    let block = block_text(doc, &lines);
    let below_text = doc.line_text(below);
    let separator = break_after(doc, *lines.end());
    let start = doc.line_start(*lines.start());
    let end = doc.line_end(below);
    let moved = format!("{below_text}{separator}{block}");
    let delta = (below_text.len() + separator.len()) as isize;
    command(
        TextEdit::new(start..end, moved),
        shifted(selection, delta),
        MOVE_DOWN,
        timestamp_ms,
    )
}

/// Copies the selected lines below themselves and selects the copy, so
/// the cursor sits on the same spot one copy down.
pub fn duplicate_lines(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let lines = selected_block(doc, selection);
    let block = block_text(doc, &lines);
    let separator = break_after(doc, *lines.end());
    let end = doc.line_end(*lines.end());
    let delta = (block.len() + separator.len()) as isize;
    command(
        TextEdit::insert(end, format!("{separator}{block}")),
        shifted(selection, delta),
        DUPLICATE,
        timestamp_ms,
    )
}

/// Where a checklist item's checkmark goes (the byte inside `[ ]`), and
/// whether it's checked. `None` for a line that isn't a checklist item.
fn checkmark_of(line: &str) -> Option<(usize, bool)> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    let bullet = ["- ", "* ", "+ "]
        .iter()
        .find(|bullet| rest.starts_with(**bullet))?;
    let at = indent + bullet.len();
    match line.get(at..at + 4)? {
        "[ ] " => Some((at + 1, false)),
        "[x] " | "[X] " => Some((at + 1, true)),
        _ => None,
    }
}

/// Checks every checklist item among the selected lines, or unchecks them
/// all when every one is checked already. Other lines are left alone.
pub fn check_checkboxes(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let marks: Vec<(usize, bool)> = selected_block(doc, selection)
        .filter_map(|line| {
            let (at, checked) = checkmark_of(&doc.line_text(line))?;
            Some((doc.line_start(line) + at, checked))
        })
        .collect();
    let mark = if marks.iter().all(|(_, checked)| *checked) {
        " "
    } else {
        "x"
    };
    let edits: Vec<TextEdit> = marks
        .iter()
        .map(|(at, _)| TextEdit::new(*at..*at + 1, mark))
        .collect();
    let changes = ChangeSet::new(edits).unwrap_or_default();
    Transaction::new(changes, Origin::command(CHECK_CHECKBOXES), timestamp_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::EditorState;

    /// Runs `command` on `marked`, where `|` is the caret or `«`/`»` wrap a
    /// selection, and returns the result marked the same way.
    fn run(marked: &str, command: fn(&Document, &Selection, u64) -> Transaction) -> String {
        let (text, selection) = parse(marked);
        let doc = Document::from(text.as_str());
        let mut state = EditorState::new(doc.clone());
        state
            .apply(Transaction::select(selection.clone(), Origin::Input, 0))
            .unwrap();
        state.apply(command(&doc, &selection, 1)).unwrap();
        render(&state)
    }

    fn parse(marked: &str) -> (String, Selection) {
        let mut text = String::new();
        let (mut anchor, mut head) = (0, 0);
        for ch in marked.chars() {
            match ch {
                '|' => (anchor, head) = (text.len(), text.len()),
                '«' => anchor = text.len(),
                '»' => head = text.len(),
                _ => text.push(ch),
            }
        }
        (text, Selection::single(SelectionRange::new(anchor, head)))
    }

    fn render(state: &EditorState) -> String {
        let mut text = state.doc().to_string();
        let range = state.selection().primary();
        if range.is_empty() {
            text.insert(range.head, '|');
        } else {
            text.insert(range.to(), '»');
            text.insert(range.from(), '«');
        }
        text
    }

    #[test]
    fn a_line_moves_up_and_down_with_its_cursor() {
        assert_eq!(run("one\ntw|o\nthree", move_lines_up), "tw|o\none\nthree");
        assert_eq!(run("one\ntw|o\nthree", move_lines_down), "one\nthree\ntw|o");
        assert_eq!(run("o|ne\ntwo", move_lines_up), "o|ne\ntwo");
        assert_eq!(run("one\ntw|o", move_lines_down), "one\ntw|o");
    }

    #[test]
    fn a_selection_moves_its_whole_lines() {
        assert_eq!(run("a\n«b\nc»\nd", move_lines_up), "«b\nc»\na\nd");
        assert_eq!(run("a\n«b\n»c", move_lines_down), "a\nc\n«b»");
    }

    #[test]
    fn the_last_line_without_a_break_moves_up() {
        assert_eq!(run("a\nb|", move_lines_up), "b|\na");
        assert_eq!(run("a|\r\nb", move_lines_down), "b\r\na|");
    }

    #[test]
    fn duplicating_copies_below_and_follows_the_copy() {
        assert_eq!(run("a|b\nc", duplicate_lines), "ab\na|b\nc");
        assert_eq!(run("«a\nb»", duplicate_lines), "a\nb\n«a\nb»");
    }

    #[test]
    fn checking_checks_and_unchecks_checklist_items() {
        assert_eq!(run("- [ ] buy |milk", check_checkboxes), "- [x] buy |milk");
        assert_eq!(run("  * [x] do|ne", check_checkboxes), "  * [ ] do|ne");
        assert_eq!(run("- ite|m", check_checkboxes), "- ite|m");
        assert_eq!(run("plai|n", check_checkboxes), "plai|n");
        assert_eq!(run("|", check_checkboxes), "|");
    }

    #[test]
    fn a_mixed_selection_is_all_checked() {
        assert_eq!(
            run("«- [x] a\n- [ ] b»", check_checkboxes),
            "«- [x] a\n- [x] b»"
        );
        assert_eq!(
            run("«- [x] a\n- [X] b»", check_checkboxes),
            "«- [ ] a\n- [ ] b»"
        );
    }
}
