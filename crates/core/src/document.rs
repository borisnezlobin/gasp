//! The text buffer and the selection.
//!
//! All offsets are byte offsets into the UTF-8 text. The rope keeps every
//! operation here at O(log n) in the document length.

use std::fmt;
use std::ops::Range;

use ropey::Rope;

use crate::transaction::{Assoc, ChangeSet};

/// A Markdown document backed by a rope.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Document {
    rope: Rope,
}

impl Document {
    /// Creates an empty document.
    pub fn new() -> Self {
        Self::default()
    }

    /// The underlying rope, for callers that need chunk-level access.
    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.rope.len_bytes()
    }

    /// True when the document holds no text.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Copies a byte range out as a `String`. Panics when the range is out of
    /// bounds or not on character boundaries.
    pub fn slice(&self, range: Range<usize>) -> String {
        self.rope.byte_slice(range).to_string()
    }

    /// Number of lines. A trailing newline starts a final empty line, so
    /// `"a\n"` has two lines and the empty document has one.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// The zero-based line that contains `offset`.
    pub fn line_of_offset(&self, offset: usize) -> usize {
        self.rope.byte_to_line(offset.min(self.len()))
    }

    /// Byte offset where `line` starts.
    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_byte(line.min(self.line_count()))
    }

    /// Byte offset where `line` ends, before its line break (`\n` or `\r\n`).
    pub fn line_end(&self, line: usize) -> usize {
        let Some(next_line) = line.checked_add(1).filter(|next| *next < self.line_count()) else {
            return self.len();
        };
        let next_start = self.rope.line_to_byte(next_line);
        let mut end = next_start;
        if end > 0 && self.byte_at(end - 1) == b'\n' {
            end -= 1;
            if end > 0 && self.byte_at(end - 1) == b'\r' {
                end -= 1;
            }
        }
        end
    }

    /// Text of `line` without its line break.
    pub fn line_text(&self, line: usize) -> String {
        self.slice(self.line_start(line)..self.line_end(line))
    }

    /// The byte range of `line` without its line break.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        self.line_start(line)..self.line_end(line)
    }

    /// True when `offset` lies on a character boundary (the ends count).
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        if offset > self.len() {
            return false;
        }
        let char_index = self.rope.byte_to_char(offset);
        self.rope.char_to_byte(char_index) == offset
    }

    /// The nearest character boundary at or before `offset`.
    pub fn floor_char_boundary(&self, offset: usize) -> usize {
        let offset = offset.min(self.len());
        self.rope.char_to_byte(self.rope.byte_to_char(offset))
    }

    /// The boundary of the character that starts before `offset`, or 0.
    pub fn prev_char_boundary(&self, offset: usize) -> usize {
        let char_index = self.rope.byte_to_char(offset.min(self.len()));
        self.rope.char_to_byte(char_index.saturating_sub(1))
    }

    /// The boundary after the character at `offset`, or the document length.
    pub fn next_char_boundary(&self, offset: usize) -> usize {
        if offset >= self.len() {
            return self.len();
        }
        let char_index = self.rope.byte_to_char(offset);
        self.rope.char_to_byte(char_index + 1)
    }

    /// The character that ends at `offset`.
    pub fn char_before(&self, offset: usize) -> Option<char> {
        if offset == 0 || offset > self.len() {
            return None;
        }
        let char_index = self.rope.byte_to_char(offset);
        char_index.checked_sub(1).map(|index| self.rope.char(index))
    }

    /// The character that starts at `offset`.
    pub fn char_after(&self, offset: usize) -> Option<char> {
        if offset >= self.len() {
            return None;
        }
        Some(self.rope.char(self.rope.byte_to_char(offset)))
    }

    /// Replaces `range` with `text`. The caller has checked the range.
    pub(crate) fn replace(&mut self, range: Range<usize>, text: &str) {
        let start = self.rope.byte_to_char(range.start);
        if range.end > range.start {
            let end = self.rope.byte_to_char(range.end);
            self.rope.remove(start..end);
        }
        if !text.is_empty() {
            self.rope.insert(start, text);
        }
    }

    fn byte_at(&self, offset: usize) -> u8 {
        self.rope.byte(offset)
    }
}

impl From<&str> for Document {
    fn from(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
        }
    }
}

impl fmt::Display for Document {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for chunk in self.rope.chunks() {
            formatter.write_str(chunk)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Document {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Document({:?})", self.to_string())
    }
}

/// One selected range. `anchor` stays put while `head` moves with the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SelectionRange {
    pub anchor: usize,
    pub head: usize,
}

impl SelectionRange {
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// A collapsed range (a caret).
    pub fn cursor(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    /// The lower end.
    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// The upper end.
    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn range(&self) -> Range<usize> {
        self.from()..self.to()
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Maps the range through a change. A caret sticks after inserted text;
    /// a non-empty range does not grow to take in text inserted at its edges.
    pub fn map(&self, changes: &ChangeSet) -> Self {
        if self.is_empty() {
            return Self::cursor(changes.map_offset(self.head, Assoc::After));
        }
        let (from_assoc, to_assoc) = (Assoc::After, Assoc::Before);
        let assoc_of = |offset: usize| {
            if offset == self.from() {
                from_assoc
            } else {
                to_assoc
            }
        };
        Self::new(
            changes.map_offset(self.anchor, assoc_of(self.anchor)),
            changes.map_offset(self.head, assoc_of(self.head)),
        )
    }

    fn overlaps_or_touches(&self, other: &Self) -> bool {
        let touching_carets = self.is_empty() || other.is_empty();
        other.from() < self.to() || (touching_carets && other.from() == self.to())
    }

    fn merged_with(&self, other: &Self) -> Self {
        let from = self.from().min(other.from());
        let to = self.to().max(other.to());
        if self.head < self.anchor {
            Self::new(to, from)
        } else {
            Self::new(from, to)
        }
    }
}

/// One or more selected ranges, sorted and non-overlapping, with one primary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    ranges: Vec<SelectionRange>,
    primary: usize,
}

impl Selection {
    /// A single caret.
    pub fn cursor(offset: usize) -> Self {
        Self::single(SelectionRange::cursor(offset))
    }

    pub fn single(range: SelectionRange) -> Self {
        Self {
            ranges: vec![range],
            primary: 0,
        }
    }

    /// Builds a selection, sorting and merging overlapping ranges. An empty
    /// list becomes a caret at 0. The primary index follows its range.
    pub fn new(ranges: Vec<SelectionRange>, primary: usize) -> Self {
        if ranges.is_empty() {
            return Self::cursor(0);
        }
        let primary_range = ranges[primary.min(ranges.len() - 1)];
        let mut sorted = ranges;
        sorted.sort_by_key(|range| (range.from(), range.to()));
        let mut merged: Vec<SelectionRange> = Vec::with_capacity(sorted.len());
        let mut primary_index = 0;
        for range in sorted {
            let is_primary = range == primary_range;
            match merged.last_mut() {
                Some(last) if last.overlaps_or_touches(&range) => *last = last.merged_with(&range),
                _ => merged.push(range),
            }
            if is_primary {
                primary_index = merged.len() - 1;
            }
        }
        Self {
            ranges: merged,
            primary: primary_index,
        }
    }

    pub fn ranges(&self) -> &[SelectionRange] {
        &self.ranges
    }

    pub fn primary(&self) -> SelectionRange {
        self.ranges[self.primary]
    }

    pub fn primary_index(&self) -> usize {
        self.primary
    }

    /// True when every range is a caret.
    pub fn is_all_carets(&self) -> bool {
        self.ranges.iter().all(SelectionRange::is_empty)
    }

    /// Maps every range through a change.
    pub fn map(&self, changes: &ChangeSet) -> Self {
        let ranges = self.ranges.iter().map(|range| range.map(changes)).collect();
        Self::new(ranges, self.primary)
    }

    /// Clamps every range to a document of `len` bytes.
    pub fn clamped(&self, len: usize) -> Self {
        let ranges = self
            .ranges
            .iter()
            .map(|range| SelectionRange::new(range.anchor.min(len), range.head.min(len)))
            .collect();
        Self::new(ranges, self.primary)
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::cursor(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::TextEdit;

    #[test]
    fn lines_and_offsets() {
        let doc = Document::from("ab\ncd\r\n\nlast");
        assert_eq!(doc.line_count(), 4);
        assert_eq!(doc.line_of_offset(0), 0);
        assert_eq!(doc.line_of_offset(3), 1);
        assert_eq!(doc.line_start(1), 3);
        assert_eq!(doc.line_end(1), 5);
        assert_eq!(doc.line_text(2), "");
        assert_eq!(doc.line_range(3), 8..12);
        assert_eq!(doc.line_end(3), doc.len());
    }

    #[test]
    fn trailing_newline_starts_empty_line() {
        let doc = Document::from("a\n");
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.line_range(1), 2..2);
        assert_eq!(Document::new().line_count(), 1);
    }

    #[test]
    fn char_boundaries_with_multibyte_text() {
        let doc = Document::from("aé😀b");
        assert!(doc.is_char_boundary(1));
        assert!(!doc.is_char_boundary(2));
        assert_eq!(doc.next_char_boundary(1), 3);
        assert_eq!(doc.next_char_boundary(3), 7);
        assert_eq!(doc.prev_char_boundary(7), 3);
        assert_eq!(doc.floor_char_boundary(5), 3);
        assert_eq!(doc.char_before(7), Some('😀'));
        assert_eq!(doc.char_after(7), Some('b'));
        assert_eq!(doc.char_after(8), None);
        assert_eq!(doc.char_before(0), None);
        assert!(!doc.is_char_boundary(99));
    }

    #[test]
    fn slice_and_display() {
        let doc = Document::from("hello world");
        assert_eq!(doc.slice(6..11), "world");
        assert_eq!(doc.to_string(), "hello world");
        assert_eq!(doc.len(), 11);
        assert!(!doc.is_empty());
    }

    #[test]
    fn replace_edits_the_rope() {
        let mut doc = Document::from("héllo");
        doc.replace(1..3, "e");
        assert_eq!(doc.to_string(), "hello");
        doc.replace(5..5, "!");
        assert_eq!(doc.to_string(), "hello!");
    }

    #[test]
    fn selection_sorts_and_merges() {
        let selection = Selection::new(
            vec![
                SelectionRange::new(8, 10),
                SelectionRange::new(0, 2),
                SelectionRange::new(9, 12),
            ],
            2,
        );
        assert_eq!(
            selection.ranges(),
            &[SelectionRange::new(0, 2), SelectionRange::new(8, 12)]
        );
        assert_eq!(selection.primary_index(), 1);
    }

    #[test]
    fn adjacent_non_empty_ranges_stay_apart() {
        let selection = Selection::new(
            vec![SelectionRange::new(0, 2), SelectionRange::new(2, 4)],
            0,
        );
        assert_eq!(selection.ranges().len(), 2);
    }

    #[test]
    fn selection_maps_through_changes() {
        let changes = ChangeSet::new(vec![TextEdit::insert(0, "xx")]).unwrap();
        let selection = Selection::new(
            vec![SelectionRange::cursor(0), SelectionRange::new(3, 1)],
            1,
        );
        let mapped = selection.map(&changes);
        assert_eq!(
            mapped.ranges(),
            &[SelectionRange::cursor(2), SelectionRange::new(5, 3)]
        );
        assert_eq!(mapped.primary(), SelectionRange::new(5, 3));
    }

    #[test]
    fn range_does_not_grow_at_edges() {
        let changes =
            ChangeSet::new(vec![TextEdit::insert(2, "a"), TextEdit::insert(5, "b")]).unwrap();
        let mapped = SelectionRange::new(2, 5).map(&changes);
        assert_eq!(mapped, SelectionRange::new(3, 6));
    }

    #[test]
    fn clamped_limits_ranges() {
        let selection = Selection::single(SelectionRange::new(2, 40)).clamped(10);
        assert_eq!(selection.primary(), SelectionRange::new(2, 10));
    }
}
