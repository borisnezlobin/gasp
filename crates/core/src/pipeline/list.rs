//! Enter inside a list item starts the next item; Enter on an empty item
//! ends the list.

use crate::document::{Document, Selection};
use crate::transaction::{ChangeSet, TextEdit};

use super::{EditRequest, PipelineStep, StepContext, StepOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    Bullet(char),
    Ordered { number: u64, delimiter: char },
}

/// A parsed list item line, with byte positions inside the line.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ListItem<'a> {
    /// Indentation and any `>` quote markers before the list marker.
    prefix: &'a str,
    marker: Marker,
    /// Byte range of the marker (digits and delimiter, or the bullet).
    marker_range: std::ops::Range<usize>,
    /// Whitespace between the marker and the content.
    spacing: &'a str,
    is_task: bool,
    content_start: usize,
}

impl ListItem<'_> {
    fn next_item_prefix(&self) -> String {
        let marker = match self.marker {
            Marker::Bullet(bullet) => bullet.to_string(),
            Marker::Ordered { number, delimiter } => format!("{}{delimiter}", number + 1),
        };
        let task = if self.is_task { "[ ] " } else { "" };
        format!("{}{marker}{}{task}", self.prefix, self.spacing)
    }
}

fn parse_list_item(line: &str) -> Option<ListItem<'_>> {
    let prefix_len = line.len() - line.trim_start_matches([' ', '\t', '>']).len();
    let rest = &line[prefix_len..];
    let (marker, marker_len) = parse_marker(rest)?;
    let after_marker = &rest[marker_len..];
    let spacing_len = after_marker.len() - after_marker.trim_start_matches([' ', '\t']).len();
    if spacing_len == 0 {
        return None;
    }
    let content_offset = prefix_len + marker_len + spacing_len;
    let task_len = task_box_len(&line[content_offset..]);
    Some(ListItem {
        prefix: &line[..prefix_len],
        marker,
        marker_range: prefix_len..prefix_len + marker_len,
        spacing: &after_marker[..spacing_len],
        is_task: task_len > 0,
        content_start: content_offset + task_len,
    })
}

fn parse_marker(text: &str) -> Option<(Marker, usize)> {
    let first = text.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        return Some((Marker::Bullet(first), 1));
    }
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 || digits > 9 {
        return None;
    }
    let delimiter = text[digits..]
        .chars()
        .next()
        .filter(|c| matches!(c, '.' | ')'))?;
    let number = text[..digits].parse().ok()?;
    Some((Marker::Ordered { number, delimiter }, digits + 1))
}

/// Length of a `[ ] `, `[x] ` or `[X] ` task box at the start of `text`.
fn task_box_len(text: &str) -> usize {
    let is_box = ["[ ]", "[x]", "[X]"]
        .iter()
        .any(|task| text.starts_with(task));
    if !is_box {
        return 0;
    }
    match text[3..].chars().next() {
        None => 3,
        Some(' ' | '\t') => 4,
        Some(_) => 0,
    }
}

/// Continues Markdown lists on Enter.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListContinuationStep;

impl PipelineStep for ListContinuationStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        let single_caret = cx.selection.ranges().len() == 1 && cx.selection.is_all_carets();
        if request != EditRequest::Newline || !single_caret {
            return StepOutcome::Continue(request);
        }
        match continue_list(cx) {
            Some(changes_and_caret) => {
                let (changes, caret) = changes_and_caret;
                StepOutcome::Emit(cx.transaction(changes, Some(Selection::cursor(caret))))
            }
            None => StepOutcome::Continue(request),
        }
    }
}

/// The change and new caret for Enter at the caret, if it is in a list item.
fn continue_list(cx: &StepContext<'_>) -> Option<(ChangeSet, usize)> {
    let doc = cx.doc;
    let caret = cx.selection.primary().head;
    let line = doc.line_of_offset(caret);
    let line_range = doc.line_range(line);
    let text = doc.line_text(line);
    let item = parse_list_item(&text)?;
    let column = caret - line_range.start;
    if column < item.content_start {
        return None;
    }
    if text[item.content_start..].trim().is_empty() {
        let changes = ChangeSet::delete(line_range.clone());
        return Some((changes, line_range.start));
    }
    let insert = format!("\n{}", item.next_item_prefix());
    let caret_after = caret + insert.len();
    let mut edits = vec![TextEdit::insert(caret, insert)];
    if let Marker::Ordered { number, .. } = item.marker {
        edits.extend(renumber_following(doc, line, &item, number + 2));
    }
    let changes = ChangeSet::new(edits).ok()?;
    Some((changes, caret_after))
}

/// Renumbers the ordered items after `line` at the same level that were
/// numbered in sequence, so inserting an item keeps the list consecutive.
fn renumber_following(
    doc: &Document,
    line: usize,
    item: &ListItem<'_>,
    first: u64,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let mut expected = first;
    for next_line in line + 1..doc.line_count() {
        let text = doc.line_text(next_line);
        let Some(next) = parse_list_item(&text) else {
            break;
        };
        if next.prefix.len() > item.prefix.len() && next.prefix.starts_with(item.prefix) {
            continue;
        }
        let Marker::Ordered { number, delimiter } = next.marker else {
            break;
        };
        if next.prefix != item.prefix || number + 1 != expected {
            break;
        }
        let start = doc.line_start(next_line);
        let range = start + next.marker_range.start..start + next.marker_range.end;
        edits.push(TextEdit::new(range, format!("{expected}{delimiter}")));
        expected += 1;
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markers() {
        let item = parse_list_item("  - [x] done").unwrap();
        assert_eq!(item.prefix, "  ");
        assert_eq!(item.marker, Marker::Bullet('-'));
        assert!(item.is_task);
        assert_eq!(item.content_start, 8);
        let ordered = parse_list_item("12) twelve").unwrap();
        assert_eq!(
            ordered.marker,
            Marker::Ordered {
                number: 12,
                delimiter: ')'
            }
        );
        assert_eq!(ordered.marker_range, 0..3);
        assert!(parse_list_item("-no space").is_none());
        assert!(parse_list_item("plain text").is_none());
        assert!(parse_list_item("1.5 apples").is_none());
        assert!(!parse_list_item("- [ ]x").unwrap().is_task);
    }

    #[test]
    fn next_prefix_keeps_spacing() {
        let item = parse_list_item("> 9.  nine").unwrap();
        assert_eq!(item.next_item_prefix(), "> 10.  ");
    }
}
