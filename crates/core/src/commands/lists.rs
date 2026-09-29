//! Turning the selected lines into a bulleted or numbered list, and back.
//!
//! When every non-blank selected line is already that kind of list item,
//! the markers come off. Otherwise each non-blank line gets the marker in
//! place of any other list marker it had, so a bulleted list becomes a
//! numbered one in one step.

use crate::document::{Document, Selection};
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

use super::lines::selected_block;

pub const TOGGLE_BULLETS: &str = "format.bullet-list";
pub const TOGGLE_NUMBERS: &str = "format.numbered-list";

const BULLETS: [&str; 3] = ["- ", "* ", "+ "];
const TASK_BOXES: [&str; 3] = ["[ ] ", "[x] ", "[X] "];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListKind {
    Bullets,
    Numbers,
}

/// A line's list marker: where it starts, how long it is, and its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Marker {
    start: usize,
    len: usize,
    kind: ListKind,
}

/// A bullet, with a task's checkbox after it counted as part of it.
fn bullet_len(rest: &str) -> Option<usize> {
    let bullet = BULLETS.iter().find(|bullet| rest.starts_with(**bullet))?;
    let after = &rest[bullet.len()..];
    let task = TASK_BOXES.iter().find(|task| after.starts_with(**task));
    Some(bullet.len() + task.map_or(0, |task| task.len()))
}

fn number_len(rest: &str) -> Option<usize> {
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let after = rest.get(digits..)?;
    let punctuated = after.starts_with(". ") || after.starts_with(") ");
    (digits > 0 && punctuated).then_some(digits + 2)
}

fn marker_of(line: &str) -> Option<Marker> {
    let start = line.len() - line.trim_start().len();
    let rest = &line[start..];
    let (len, kind) = match bullet_len(rest) {
        Some(len) => (len, ListKind::Bullets),
        None => (number_len(rest)?, ListKind::Numbers),
    };
    Some(Marker { start, len, kind })
}

/// Toggles a bulleted list on the selected lines.
pub fn toggle_bullet_list(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    toggle_list(doc, selection, ListKind::Bullets, timestamp_ms)
}

/// Toggles a numbered list on the selected lines, numbering from 1.
pub fn toggle_numbered_list(
    doc: &Document,
    selection: &Selection,
    timestamp_ms: u64,
) -> Transaction {
    toggle_list(doc, selection, ListKind::Numbers, timestamp_ms)
}

/// The marker a line gets: its position among the non-blank lines
/// numbers it.
fn new_marker(kind: ListKind, position: usize) -> String {
    match kind {
        ListKind::Bullets => "- ".to_owned(),
        ListKind::Numbers => format!("{}. ", position + 1),
    }
}

fn toggle_list(
    doc: &Document,
    selection: &Selection,
    kind: ListKind,
    timestamp_ms: u64,
) -> Transaction {
    let lines: Vec<(usize, String)> = selected_block(doc, selection)
        .map(|line| (line, doc.line_text(line)))
        .filter(|(_, text)| !text.trim().is_empty())
        .collect();
    let already = !lines.is_empty()
        && lines
            .iter()
            .all(|(_, text)| marker_of(text).is_some_and(|marker| marker.kind == kind));
    let edits = lines
        .iter()
        .enumerate()
        .map(|(position, (line, text))| {
            let start = doc.line_start(*line);
            let indent = text.len() - text.trim_start().len();
            let len = marker_of(text).map_or(0, |marker| marker.len);
            let insert = if already {
                String::new()
            } else {
                new_marker(kind, position)
            };
            TextEdit::new(start + indent..start + indent + len, insert)
        })
        .collect();
    let id = match kind {
        ListKind::Bullets => TOGGLE_BULLETS,
        ListKind::Numbers => TOGGLE_NUMBERS,
    };
    let changes = ChangeSet::new(edits).unwrap_or_default();
    Transaction::new(changes, Origin::command(id), timestamp_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::SelectionRange;
    use crate::history::EditorState;

    fn run(text: &str, from: usize, to: usize, kind: ListKind) -> String {
        let doc = Document::from(text);
        let selection = Selection::single(SelectionRange::new(from, to));
        let mut state = EditorState::new(doc.clone());
        state
            .apply(toggle_list(&doc, &selection, kind, 1))
            .unwrap();
        state.doc().to_string()
    }

    #[test]
    fn lines_become_bullets_and_back() {
        let text = "one\n\n  two\nthree";
        let bulleted = run(text, 0, text.len(), ListKind::Bullets);
        assert_eq!(bulleted, "- one\n\n  - two\n- three");
        let plain = run(&bulleted, 0, bulleted.len(), ListKind::Bullets);
        assert_eq!(plain, text);
    }

    #[test]
    fn numbers_replace_bullets_and_count_from_one() {
        let text = "- a\n- [ ] b\nc";
        let numbered = run(text, 0, text.len(), ListKind::Numbers);
        assert_eq!(numbered, "1. a\n2. b\n3. c");
        assert_eq!(run(&numbered, 0, 0, ListKind::Numbers), "a\n2. b\n3. c");
    }

    #[test]
    fn a_blank_line_is_left_alone() {
        assert_eq!(run("", 0, 0, ListKind::Bullets), "");
    }
}
