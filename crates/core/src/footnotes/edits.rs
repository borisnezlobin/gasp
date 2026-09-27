//! Text edits returned by every footnote command.

use std::ops::Range;

/// Replace `range` (byte offsets into the original text) with `insert`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteEdit {
    pub range: Range<usize>,
    pub insert: String,
}

impl FootnoteEdit {
    pub fn new(range: Range<usize>, insert: impl Into<String>) -> Self {
        Self {
            range,
            insert: insert.into(),
        }
    }

    fn delta(&self) -> isize {
        self.insert.len() as isize - self.range.len() as isize
    }
}

fn sorted(edits: &[FootnoteEdit]) -> Vec<&FootnoteEdit> {
    let mut sorted: Vec<&FootnoteEdit> = edits.iter().collect();
    sorted.sort_by_key(|edit| edit.range.start);
    sorted
}

/// Applies non-overlapping edits, all expressed against `text`.
pub fn apply_edits(text: &str, edits: &[FootnoteEdit]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for edit in sorted(edits) {
        out.push_str(&text[cursor..edit.range.start]);
        out.push_str(&edit.insert);
        cursor = edit.range.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Maps an offset in the original text to the edited text. An offset inside
/// a replaced range lands at the end of the replacement.
pub fn map_offset(edits: &[FootnoteEdit], offset: usize) -> usize {
    let mut delta = 0isize;
    for edit in sorted(edits) {
        if edit.range.end <= offset {
            delta += edit.delta();
        } else if edit.range.start < offset {
            return (edit.range.start as isize + delta) as usize + edit.insert.len();
        } else {
            break;
        }
    }
    (offset as isize + delta) as usize
}

/// The line break the document uses: `\r\n` if it has any, else `\n`.
pub(crate) fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Maps an offset in the text after `first` back to the original. `first`
/// must be sorted and never place `offset` strictly inside a replacement.
fn to_original(first: &[&FootnoteEdit], offset: usize, is_end: bool) -> usize {
    let mut delta = 0isize;
    for edit in first {
        let mid_start = (edit.range.start as isize + delta) as usize;
        let mid_end = mid_start + edit.insert.len();
        let past = if is_end {
            offset >= mid_end
        } else {
            offset > mid_start && offset >= mid_end
        };
        if !past {
            break;
        }
        delta += edit.delta();
    }
    (offset as isize - delta) as usize
}

/// Ranges in the intermediate text touched by either edit list, merged.
fn touched_spans(first: &[&FootnoteEdit], second: &[FootnoteEdit]) -> Vec<Range<usize>> {
    let mut delta = 0isize;
    let mut spans: Vec<Range<usize>> = Vec::new();
    for edit in first {
        let start = (edit.range.start as isize + delta) as usize;
        spans.push(start..start + edit.insert.len());
        delta += edit.delta();
    }
    spans.extend(second.iter().map(|edit| edit.range.clone()));
    spans.sort_by_key(|span| span.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
            _ => merged.push(span),
        }
    }
    merged
}

/// Combines `first` (against the original text) and `second` (against
/// `middle`, the text after `first`) into one list against the original.
pub(crate) fn compose(
    first: &[FootnoteEdit],
    second: &[FootnoteEdit],
    middle: &str,
) -> Vec<FootnoteEdit> {
    let first = sorted(first);
    touched_spans(&first, second)
        .into_iter()
        .map(|span| {
            let inner: Vec<FootnoteEdit> = second
                .iter()
                .filter(|edit| span.start <= edit.range.start && edit.range.end <= span.end)
                .map(|edit| {
                    let range = edit.range.start - span.start..edit.range.end - span.start;
                    FootnoteEdit::new(range, edit.insert.clone())
                })
                .collect();
            let insert = apply_edits(&middle[span.clone()], &inner);
            let start = to_original(&first, span.start, false);
            let end = to_original(&first, span.end, true);
            FootnoteEdit::new(start..end, insert)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_compose(text: &str, first: Vec<FootnoteEdit>, second: Vec<FootnoteEdit>) {
        let middle = apply_edits(text, &first);
        let expected = apply_edits(&middle, &second);
        let composed = compose(&first, &second, &middle);
        assert_eq!(apply_edits(text, &composed), expected);
    }

    #[test]
    fn compose_disjoint_and_overlapping_edits() {
        let text = "abcdefghij";
        check_compose(
            text,
            vec![FootnoteEdit::new(2..2, "XY"), FootnoteEdit::new(5..7, "")],
            vec![FootnoteEdit::new(3..4, "Q"), FootnoteEdit::new(8..9, "Z")],
        );
        check_compose(
            text,
            vec![
                FootnoteEdit::new(0..0, "12"),
                FootnoteEdit::new(10..10, "34"),
            ],
            vec![FootnoteEdit::new(0..14, "all")],
        );
        check_compose(
            text,
            vec![FootnoteEdit::new(4..4, "m"), FootnoteEdit::new(4..4, "n")],
            vec![FootnoteEdit::new(4..5, "M")],
        );
    }

    #[test]
    fn compose_keeps_edits_small() {
        let text = "aaaa bbbb cccc";
        let first = vec![FootnoteEdit::new(0..0, ">")];
        let second = vec![FootnoteEdit::new(11..12, "C")];
        let middle = apply_edits(text, &first);
        let composed = compose(&first, &second, &middle);
        assert_eq!(composed.len(), 2);
        assert_eq!(composed[1], FootnoteEdit::new(10..11, "C"));
    }

    #[test]
    fn line_ending_detects_crlf() {
        assert_eq!(line_ending("a\r\nb"), "\r\n");
        assert_eq!(line_ending("a\nb"), "\n");
    }
}
