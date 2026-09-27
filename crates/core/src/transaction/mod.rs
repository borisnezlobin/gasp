//! Changes to a document and the transactions that carry them.
//!
//! A [`ChangeSet`] is a sorted list of non-overlapping [`TextEdit`]s, all in
//! the coordinates of the document before the change.

use std::fmt;
use std::ops::Range;
use std::str::FromStr;

use crate::document::{Document, Selection};

/// Replace `range` (in the original document) with `insert`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextEdit {
    pub range: Range<usize>,
    pub insert: String,
}

impl TextEdit {
    pub fn new(range: Range<usize>, insert: impl Into<String>) -> Self {
        Self {
            range,
            insert: insert.into(),
        }
    }

    pub fn insert(offset: usize, text: impl Into<String>) -> Self {
        Self::new(offset..offset, text)
    }

    pub fn delete(range: Range<usize>) -> Self {
        Self::new(range, "")
    }

    fn is_noop(&self) -> bool {
        self.range.is_empty() && self.insert.is_empty()
    }

    /// Bytes added minus bytes removed.
    fn delta(&self) -> isize {
        self.insert.len() as isize - self.range.len() as isize
    }
}

/// Which side of an insertion a mapped position sticks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    Before,
    After,
}

/// Why a change could not be built or applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeError {
    /// Two edits overlap in the original document.
    Overlap {
        first: Range<usize>,
        second: Range<usize>,
    },
    /// An edit's range is reversed.
    Reversed(Range<usize>),
    /// An edit reaches past the end of the document.
    OutOfBounds { range: Range<usize>, len: usize },
    /// An edit starts or ends inside a UTF-8 character.
    NotCharBoundary(usize),
}

impl fmt::Display for ChangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overlap { first, second } => {
                write!(formatter, "edits {first:?} and {second:?} overlap")
            }
            Self::Reversed(range) => write!(formatter, "edit range {range:?} is reversed"),
            Self::OutOfBounds { range, len } => {
                write!(formatter, "edit range {range:?} is past the end ({len})")
            }
            Self::NotCharBoundary(offset) => {
                write!(formatter, "offset {offset} is inside a character")
            }
        }
    }
}

impl std::error::Error for ChangeError {}

/// A set of non-overlapping edits against one document version.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet {
    edits: Vec<TextEdit>,
}

impl ChangeSet {
    /// The change that does nothing.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds a change from edits in any order. Edits that touch end to end
    /// are merged and no-op edits are dropped; overlapping edits are an error.
    pub fn new(mut edits: Vec<TextEdit>) -> Result<Self, ChangeError> {
        if let Some(reversed) = edits.iter().find(|edit| edit.range.start > edit.range.end) {
            return Err(ChangeError::Reversed(reversed.range.clone()));
        }
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let mut normalized: Vec<TextEdit> = Vec::with_capacity(edits.len());
        for edit in edits {
            if edit.is_noop() {
                continue;
            }
            let Some(last) = normalized.last_mut() else {
                normalized.push(edit);
                continue;
            };
            if edit.range.start < last.range.end {
                return Err(ChangeError::Overlap {
                    first: last.range.clone(),
                    second: edit.range,
                });
            }
            if edit.range.start == last.range.end {
                last.range.end = edit.range.end;
                last.insert.push_str(&edit.insert);
            } else {
                normalized.push(edit);
            }
        }
        Ok(Self { edits: normalized })
    }

    /// A single insertion.
    pub fn insert(offset: usize, text: impl Into<String>) -> Self {
        Self::replace(offset..offset, text)
    }

    /// A single deletion.
    pub fn delete(range: Range<usize>) -> Self {
        Self::replace(range, "")
    }

    /// A single replacement. Panics when the range is reversed.
    pub fn replace(range: Range<usize>, text: impl Into<String>) -> Self {
        assert!(range.start <= range.end, "reversed range {range:?}");
        let edit = TextEdit::new(range, text);
        let edits = if edit.is_noop() { vec![] } else { vec![edit] };
        Self { edits }
    }

    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Checks that every edit fits `doc` and sits on character boundaries.
    pub fn validate(&self, doc: &Document) -> Result<(), ChangeError> {
        let len = doc.len();
        for edit in &self.edits {
            if edit.range.end > len {
                return Err(ChangeError::OutOfBounds {
                    range: edit.range.clone(),
                    len,
                });
            }
            for offset in [edit.range.start, edit.range.end] {
                if !doc.is_char_boundary(offset) {
                    return Err(ChangeError::NotCharBoundary(offset));
                }
            }
        }
        Ok(())
    }

    /// Applies the change to `doc`, leaving it untouched on error.
    pub fn apply(&self, doc: &mut Document) -> Result<(), ChangeError> {
        self.validate(doc)?;
        for edit in self.edits.iter().rev() {
            doc.replace(edit.range.clone(), &edit.insert);
        }
        Ok(())
    }

    /// Applies the change to a string, for tests and small texts.
    pub fn apply_to_string(&self, text: &str) -> Result<String, ChangeError> {
        let mut doc = Document::from(text);
        self.apply(&mut doc)?;
        Ok(doc.to_string())
    }

    /// Length of the document after applying this change to one of `len`.
    pub fn new_len(&self, len: usize) -> usize {
        let delta: isize = self.edits.iter().map(TextEdit::delta).sum();
        len.saturating_add_signed(delta)
    }

    /// The change that undoes this one. `before` is the document this change
    /// applies to.
    pub fn invert(&self, before: &Document) -> Self {
        let mut delta: isize = 0;
        let edits = self
            .edits
            .iter()
            .map(|edit| {
                let start = edit.range.start.saturating_add_signed(delta);
                delta += edit.delta();
                TextEdit::new(
                    start..start + edit.insert.len(),
                    before.slice(edit.range.clone()),
                )
            })
            .collect();
        Self { edits }
    }

    /// Maps an offset in the old document to the new one. Offsets inside a
    /// replaced range collapse to its start (`Before`) or end (`After`).
    pub fn map_offset(&self, offset: usize, assoc: Assoc) -> usize {
        let mut delta: isize = 0;
        for edit in &self.edits {
            if offset < edit.range.start {
                break;
            }
            let start = edit.range.start.saturating_add_signed(delta);
            let is_inside = offset < edit.range.end || offset == edit.range.start;
            if is_inside {
                return match assoc {
                    Assoc::Before => start,
                    Assoc::After => start + edit.insert.len(),
                };
            }
            delta += edit.delta();
        }
        offset.saturating_add_signed(delta)
    }

    /// Composes `self` followed by `next` into one change against the
    /// document `self` applies to. `next` must apply to the result of `self`.
    pub fn compose(&self, next: &ChangeSet) -> ChangeSet {
        let first = ops_from_edits(&self.edits);
        let second = ops_from_edits(&next.edits);
        Self {
            edits: edits_from_ops(compose_ops(first, second)),
        }
    }

    /// Rebases this change over `other`, a concurrent change to the same
    /// document, so it applies after `other`. Where the two overlap, the
    /// rebased edit shrinks to the text `other` left in place.
    pub fn map_through(&self, other: &ChangeSet) -> ChangeSet {
        let edits = self
            .edits
            .iter()
            .map(|edit| TextEdit::new(rebase_range(&edit.range, other), edit.insert.clone()))
            .collect();
        Self::new(edits).unwrap_or_default()
    }
}

fn rebase_range(range: &Range<usize>, other: &ChangeSet) -> Range<usize> {
    if range.is_empty() {
        let offset = other.map_offset(range.start, Assoc::Before);
        return offset..offset;
    }
    let start = other.map_offset(range.start, Assoc::After);
    let end = other.map_offset(range.end, Assoc::Before).max(start);
    start..end
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Retain(usize),
    Delete(usize),
    Insert(String),
}

fn ops_from_edits(edits: &[TextEdit]) -> Vec<Op> {
    let mut ops = Vec::with_capacity(edits.len() * 3);
    let mut position = 0;
    for edit in edits {
        if edit.range.start > position {
            ops.push(Op::Retain(edit.range.start - position));
        }
        if !edit.range.is_empty() {
            ops.push(Op::Delete(edit.range.len()));
        }
        if !edit.insert.is_empty() {
            ops.push(Op::Insert(edit.insert.clone()));
        }
        position = edit.range.end;
    }
    ops
}

fn edits_from_ops(ops: Vec<Op>) -> Vec<TextEdit> {
    let mut edits: Vec<TextEdit> = Vec::new();
    let mut position = 0;
    let mut pending: Option<TextEdit> = None;
    for op in ops {
        match op {
            Op::Retain(count) => {
                edits.extend(pending.take());
                position += count;
            }
            Op::Delete(count) => {
                let edit = pending.get_or_insert_with(|| TextEdit::insert(position, ""));
                edit.range.end += count;
                position += count;
            }
            Op::Insert(text) => {
                let edit = pending.get_or_insert_with(|| TextEdit::insert(position, ""));
                edit.insert.push_str(&text);
            }
        }
    }
    edits.extend(pending);
    edits
}

/// Walks two op lists in step. Each list ends with an implicit endless retain.
struct OpCursor {
    ops: std::vec::IntoIter<Op>,
    current: Option<Op>,
}

impl OpCursor {
    fn new(ops: Vec<Op>) -> Self {
        let mut ops = ops.into_iter();
        let current = ops.next();
        Self { ops, current }
    }

    fn advance(&mut self) {
        self.current = self.ops.next();
    }

    /// Consumes `count` units of the current retain or delete, or bytes of the
    /// current insert, returning what was consumed.
    fn take(&mut self, count: usize) -> Op {
        let Some(current) = self.current.take() else {
            return Op::Retain(count);
        };
        let (taken, rest) = split_op(current, count);
        self.current = rest;
        if self.current.is_none() {
            self.advance();
        }
        taken
    }
}

fn split_op(op: Op, count: usize) -> (Op, Option<Op>) {
    let remainder = |left: usize| (left > 0).then_some(left);
    match op {
        Op::Retain(len) => (Op::Retain(count), remainder(len - count).map(Op::Retain)),
        Op::Delete(len) => (Op::Delete(count), remainder(len - count).map(Op::Delete)),
        Op::Insert(mut text) => {
            let rest = text.split_off(count);
            (
                Op::Insert(text),
                (!rest.is_empty()).then_some(Op::Insert(rest)),
            )
        }
    }
}

fn op_len(op: &Op) -> usize {
    match op {
        Op::Retain(len) | Op::Delete(len) => *len,
        Op::Insert(text) => text.len(),
    }
}

fn compose_ops(first: Vec<Op>, second: Vec<Op>) -> Vec<Op> {
    let mut first = OpCursor::new(first);
    let mut second = OpCursor::new(second);
    let mut output = Vec::new();
    loop {
        if let Some(Op::Delete(len)) = first.current {
            output.push(Op::Delete(len));
            first.advance();
            continue;
        }
        if let Some(Op::Insert(text)) = &second.current {
            output.push(Op::Insert(text.clone()));
            second.advance();
            continue;
        }
        let count = match (&first.current, &second.current) {
            (None, None) => break,
            (Some(op), None) | (None, Some(op)) => op_len(op),
            (Some(a), Some(b)) => op_len(a).min(op_len(b)),
        };
        let pair = (first.take(count), second.take(count));
        output.extend(compose_pair(pair));
    }
    output
}

/// Combines an op from the first change (retain or insert) with one of equal
/// length from the second change (retain or delete).
fn compose_pair(pair: (Op, Op)) -> Option<Op> {
    match pair {
        (Op::Retain(len), Op::Retain(_)) => Some(Op::Retain(len)),
        (Op::Retain(len), Op::Delete(_)) => Some(Op::Delete(len)),
        (Op::Insert(text), Op::Retain(_)) => Some(Op::Insert(text)),
        _ => None,
    }
}

/// Where a transaction came from. Written as `input`, `command:<id>`,
/// `undo`, `redo`, `remote`, or any other word.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    Input,
    Command(String),
    Undo,
    Redo,
    Remote,
    Other(String),
}

impl Origin {
    pub fn command(id: impl Into<String>) -> Self {
        Self::Command(id.into())
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input => formatter.write_str("input"),
            Self::Command(id) => write!(formatter, "command:{id}"),
            Self::Undo => formatter.write_str("undo"),
            Self::Redo => formatter.write_str("redo"),
            Self::Remote => formatter.write_str("remote"),
            Self::Other(name) => formatter.write_str(name),
        }
    }
}

impl FromStr for Origin {
    type Err = std::convert::Infallible;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Some(id) = text.strip_prefix("command:") {
            return Ok(Self::command(id));
        }
        Ok(match text {
            "input" => Self::Input,
            "undo" => Self::Undo,
            "redo" => Self::Redo,
            "remote" => Self::Remote,
            other => Self::Other(other.to_owned()),
        })
    }
}

/// Who made a transaction and when, in milliseconds on the caller's clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionMeta {
    pub origin: Origin,
    pub timestamp_ms: u64,
}

/// A change plus the selection that follows it. When `selection` is `None`
/// the current selection is mapped through the change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub changes: ChangeSet,
    pub selection: Option<Selection>,
    pub meta: TransactionMeta,
}

impl Transaction {
    pub fn new(changes: ChangeSet, origin: Origin, timestamp_ms: u64) -> Self {
        Self {
            changes,
            selection: None,
            meta: TransactionMeta {
                origin,
                timestamp_ms,
            },
        }
    }

    /// A transaction that only moves the selection.
    pub fn select(selection: Selection, origin: Origin, timestamp_ms: u64) -> Self {
        Self::new(ChangeSet::empty(), origin, timestamp_ms).with_selection(selection)
    }

    pub fn with_selection(mut self, selection: Selection) -> Self {
        self.selection = Some(selection);
        self
    }

    pub fn origin(&self) -> &Origin {
        &self.meta.origin
    }

    pub fn timestamp_ms(&self) -> u64 {
        self.meta.timestamp_ms
    }
}

#[cfg(test)]
mod props;

#[cfg(test)]
mod tests {
    use super::*;

    fn change(edits: Vec<TextEdit>) -> ChangeSet {
        ChangeSet::new(edits).unwrap()
    }

    #[test]
    fn new_sorts_merges_and_rejects_overlap() {
        let merged = change(vec![TextEdit::insert(3, "b"), TextEdit::new(1..3, "a")]);
        assert_eq!(merged.edits(), &[TextEdit::new(1..3, "ab")]);
        let overlap = ChangeSet::new(vec![TextEdit::delete(0..3), TextEdit::delete(2..4)]);
        assert!(matches!(overlap, Err(ChangeError::Overlap { .. })));
        assert!(change(vec![TextEdit::insert(2, "")]).is_empty());
        let backwards = Range { start: 3, end: 1 };
        let reversed = ChangeSet::new(vec![TextEdit::delete(backwards.clone())]);
        assert_eq!(reversed, Err(ChangeError::Reversed(backwards)));
    }

    #[test]
    fn apply_and_invert() {
        let doc = Document::from("hello world");
        let changes = change(vec![
            TextEdit::new(0..5, "goodbye"),
            TextEdit::insert(11, "!"),
        ]);
        let mut edited = doc.clone();
        changes.apply(&mut edited).unwrap();
        assert_eq!(edited.to_string(), "goodbye world!");
        changes.invert(&doc).apply(&mut edited).unwrap();
        assert_eq!(edited, doc);
    }

    #[test]
    fn apply_rejects_bad_ranges() {
        let mut doc = Document::from("é");
        let inside = ChangeSet::delete(1..2).apply(&mut doc);
        assert_eq!(inside, Err(ChangeError::NotCharBoundary(1)));
        let past = ChangeSet::delete(0..9).apply(&mut doc);
        assert!(matches!(past, Err(ChangeError::OutOfBounds { .. })));
        assert_eq!(doc.to_string(), "é");
    }

    #[test]
    fn map_offset_respects_assoc() {
        let changes = change(vec![TextEdit::insert(2, "xy"), TextEdit::new(5..8, "z")]);
        assert_eq!(changes.map_offset(1, Assoc::After), 1);
        assert_eq!(changes.map_offset(2, Assoc::Before), 2);
        assert_eq!(changes.map_offset(2, Assoc::After), 4);
        assert_eq!(changes.map_offset(5, Assoc::Before), 7);
        assert_eq!(changes.map_offset(6, Assoc::Before), 7);
        assert_eq!(changes.map_offset(6, Assoc::After), 8);
        assert_eq!(changes.map_offset(8, Assoc::Before), 8);
        assert_eq!(changes.map_offset(10, Assoc::Before), 10);
    }

    #[test]
    fn compose_simple() {
        let first = ChangeSet::insert(0, "abc");
        let second = ChangeSet::new(vec![TextEdit::delete(1..2), TextEdit::insert(5, "!")]);
        let composed = first.compose(&second.unwrap());
        assert_eq!(composed.apply_to_string("xy").unwrap(), "acxy!");
    }

    #[test]
    fn new_len_counts_delta() {
        let changes = change(vec![TextEdit::new(0..3, "a"), TextEdit::insert(5, "bcd")]);
        assert_eq!(changes.new_len(6), 7);
    }

    #[test]
    fn map_through_rebases_concurrent_edits() {
        let local = ChangeSet::insert(5, "!");
        let remote = ChangeSet::insert(0, ">> ");
        let rebased = local.map_through(&remote);
        let text = remote.apply_to_string("hello").unwrap();
        assert_eq!(rebased.apply_to_string(&text).unwrap(), ">> hello!");
    }

    #[test]
    fn origin_round_trips() {
        for text in [
            "input",
            "command:format.bold",
            "undo",
            "redo",
            "remote",
            "paste",
        ] {
            let origin: Origin = text.parse().unwrap();
            assert_eq!(origin.to_string(), text);
        }
        assert_eq!("command:x".parse::<Origin>().unwrap(), Origin::command("x"));
    }

    #[test]
    fn transaction_builders() {
        let tr = Transaction::select(Selection::cursor(3), Origin::Input, 7);
        assert!(tr.changes.is_empty());
        assert_eq!(tr.selection, Some(Selection::cursor(3)));
        assert_eq!(tr.timestamp_ms(), 7);
        assert_eq!(tr.origin(), &Origin::Input);
    }
}
