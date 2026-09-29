//! The text, selection, IME composition and undo history of a one-line
//! input, kept apart from GPUI so the editing rules can be tested directly.

use std::ops::Range;

use gasp_core::document::Document;
use gasp_core::motion;
use unicode_segmentation::UnicodeSegmentation;

/// Where a cursor motion, selection or delete goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Start,
    End,
}

/// What kind of edit changed the text. Consecutive typing, or consecutive
/// deleting, undoes as one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Deleting,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    text: String,
    anchor: usize,
    head: usize,
}

/// A one-line text with a selection (anchor and head, as byte offsets),
/// an optional IME composition and undo and redo stacks.
#[derive(Clone, Debug, Default)]
pub struct LineState {
    text: String,
    anchor: usize,
    head: usize,
    marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The kind of the last edit, while further edits of the same kind
    /// join its undo step.
    open_edit: Option<EditKind>,
}

impl LineState {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// Where the cursor is: the moving end of the selection.
    pub fn cursor(&self) -> usize {
        self.head
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    /// Whether the cursor is at the start of the selection.
    pub fn is_reversed(&self) -> bool {
        self.head < self.anchor
    }

    pub fn selected_text(&self) -> Option<&str> {
        let range = self.selected_range();
        (!range.is_empty()).then(|| &self.text[range])
    }

    /// The IME composition, if one is in progress.
    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn unmark(&mut self) {
        self.marked = None;
    }

    /// Replaces the whole text, as the owner setting a value rather than
    /// the user typing: the cursor goes to the end and history is cleared.
    pub fn set_text(&mut self, text: &str) {
        self.text = single_line(text);
        self.anchor = self.text.len();
        self.head = self.text.len();
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.open_edit = None;
    }

    /// Selects from `anchor` to `head`, clamped to character boundaries.
    pub fn select(&mut self, anchor: usize, head: usize) {
        self.anchor = self.floor(anchor);
        self.head = self.floor(head);
        self.open_edit = None;
    }

    pub fn select_all(&mut self) {
        self.select(0, self.text.len());
    }

    /// The run of word, space or punctuation characters around `offset`.
    pub fn word_at(&self, offset: usize) -> Range<usize> {
        motion::word_at(&self.doc(), self.floor(offset))
    }

    /// Moves the cursor, or extends the selection when `extend` is set.
    /// Moving left or right without extending collapses a selection to
    /// that edge.
    pub fn move_by(&mut self, motion: Motion, extend: bool) {
        let selection = self.selected_range();
        let head = match motion {
            Motion::Left if !extend && !selection.is_empty() => selection.start,
            Motion::Right if !extend && !selection.is_empty() => selection.end,
            _ => self.target(motion),
        };
        let anchor = if extend { self.anchor } else { head };
        self.select(anchor, head);
    }

    /// Deletes the selection, or from the cursor to where `motion` goes.
    /// Returns whether the text changed.
    pub fn delete(&mut self, motion: Motion) -> bool {
        let mut range = self.selected_range();
        if range.is_empty() {
            let target = self.target(motion);
            range = self.head.min(target)..self.head.max(target);
        }
        !range.is_empty() && self.edit(range, "", EditKind::Deleting).changed
    }

    /// Replaces `range` with `text` (on one line), leaves the cursor after
    /// it and ends any composition.
    pub fn edit(&mut self, range: Range<usize>, text: &str, kind: EditKind) -> Edited {
        let end = self.floor(range.end);
        let range = self.floor(range.start).min(end)..end;
        let text = single_line(text);
        let changed = self.text[range.clone()] != text;
        if changed {
            self.record(kind);
            self.text.replace_range(range.clone(), &text);
        }
        let inserted = range.start..range.start + text.len();
        self.anchor = inserted.end;
        self.head = inserted.end;
        self.marked = None;
        Edited { inserted, changed }
    }

    /// Replaces `range` with a composition and selects `selected` within
    /// it (relative byte offsets).
    pub fn compose(&mut self, range: Range<usize>, text: &str, selected: Option<Range<usize>>) {
        let composing = self.marked.is_some();
        let kind = if composing {
            self.open_edit.unwrap_or(EditKind::Typing)
        } else {
            EditKind::Typing
        };
        let inserted = self.edit(range, text, kind).inserted;
        self.marked = (!inserted.is_empty()).then(|| inserted.clone());
        if let Some(selected) = selected {
            let start = (inserted.start + selected.start).min(inserted.end);
            let end = (inserted.start + selected.end).min(inserted.end);
            self.anchor = self.floor(start);
            self.head = self.floor(end);
        }
    }

    /// Goes back one step. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        let current = self.snapshot();
        self.redo.push(current);
        self.restore(snapshot);
        true
    }

    /// Goes forward one undone step. Returns whether there was one.
    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        let current = self.snapshot();
        self.undo.push(current);
        self.restore(snapshot);
        true
    }

    /// Saves the text before an edit, unless the edit continues the last
    /// one (more typing, more deleting, or an IME composition).
    fn record(&mut self, kind: EditKind) {
        let continues =
            self.marked.is_some() || (kind != EditKind::Other && self.open_edit == Some(kind));
        if !continues {
            let snapshot = self.snapshot();
            self.undo.push(snapshot);
        }
        self.redo.clear();
        self.open_edit = (kind != EditKind::Other).then_some(kind);
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            head: self.head,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.anchor = snapshot.anchor;
        self.head = snapshot.head;
        self.marked = None;
        self.open_edit = None;
    }

    fn target(&self, motion: Motion) -> usize {
        let cursor = self.head;
        match motion {
            Motion::Left => previous_grapheme(&self.text, cursor),
            Motion::Right => next_grapheme(&self.text, cursor),
            Motion::WordLeft => motion::word_left(&self.doc(), cursor),
            Motion::WordRight => motion::word_right(&self.doc(), cursor),
            Motion::Start => 0,
            Motion::End => self.text.len(),
        }
    }

    fn doc(&self) -> Document {
        Document::from(self.text.as_str())
    }

    fn floor(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }
}

/// The result of [`LineState::edit`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edited {
    pub inserted: Range<usize>,
    pub changed: bool,
}

/// Inputs hold one line: line breaks and tabs become spaces, and other
/// control characters are dropped.
pub fn single_line(text: &str) -> String {
    text.replace("\r\n", " ")
        .chars()
        .filter_map(|ch| match ch {
            '\n' | '\r' | '\t' => Some(' '),
            ch if ch.is_control() => None,
            ch => Some(ch),
        })
        .collect()
}

fn previous_grapheme(text: &str, offset: usize) -> usize {
    text[..offset]
        .graphemes(true)
        .next_back()
        .map_or(0, |grapheme| offset - grapheme.len())
}

fn next_grapheme(text: &str, offset: usize) -> usize {
    text[offset..]
        .graphemes(true)
        .next()
        .map_or(text.len(), |grapheme| offset + grapheme.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(text: &str, anchor: usize, head: usize) -> LineState {
        let mut state = LineState::default();
        state.set_text(text);
        state.select(anchor, head);
        state
    }

    fn typed(state: &mut LineState, text: &str) {
        let at = state.selected_range();
        state.edit(at, text, EditKind::Typing);
    }

    #[test]
    fn line_breaks_and_tabs_become_spaces() {
        assert_eq!(single_line("a\nb\r\nc\td\u{7}"), "a b c d");
    }

    #[test]
    fn graphemes_move_over_whole_emoji_and_accents() {
        let text = "e\u{301}👍🏽";
        assert_eq!(next_grapheme(text, 0), 3);
        assert_eq!(next_grapheme(text, 3), text.len());
        assert_eq!(previous_grapheme(text, text.len()), 3);
        let mut line = state(text, text.len(), text.len());
        assert!(line.delete(Motion::Left));
        assert_eq!(line.text(), "e\u{301}");
    }

    #[test]
    fn word_motions_skip_separators_then_the_word() {
        let mut line = state("daily notes/2024-05", 19, 19);
        line.move_by(Motion::WordLeft, false);
        assert_eq!(line.cursor(), 17);
        line.move_by(Motion::WordLeft, true);
        assert_eq!(line.selected_range(), 12..17);
        assert!(line.is_reversed());
        line.move_by(Motion::Start, false);
        line.move_by(Motion::WordRight, false);
        assert_eq!(line.cursor(), 5);
        let mut accents = state("café crème", 0, 0);
        accents.move_by(Motion::WordRight, false);
        assert_eq!(accents.cursor(), 5);
    }

    #[test]
    fn moving_without_shift_collapses_the_selection() {
        let mut line = state("hello world", 2, 8);
        line.move_by(Motion::Left, false);
        assert_eq!(line.selected_range(), 2..2);
        let mut line = state("hello world", 8, 2);
        line.move_by(Motion::Right, false);
        assert_eq!(line.selected_range(), 8..8);
    }

    #[test]
    fn deletes_take_the_selection_first() {
        let mut line = state("one two three", 4, 7);
        assert!(line.delete(Motion::WordLeft));
        assert_eq!(line.text(), "one  three");
        line.move_by(Motion::End, false);
        assert!(line.delete(Motion::WordLeft));
        assert_eq!(line.text(), "one  ");
        assert!(line.delete(Motion::Start));
        assert_eq!(line.text(), "");
        assert!(!line.delete(Motion::Left));
    }

    #[test]
    fn typing_undoes_as_one_step_and_redoes() {
        let mut line = state("", 0, 0);
        typed(&mut line, "a");
        typed(&mut line, "b");
        line.delete(Motion::Left);
        assert_eq!(line.text(), "a");
        assert!(line.undo());
        assert_eq!(line.text(), "ab");
        assert!(line.undo());
        assert_eq!(line.text(), "");
        assert!(!line.undo());
        assert!(line.redo());
        assert_eq!(line.text(), "ab");
        assert_eq!(line.cursor(), 2);
    }

    #[test]
    fn a_new_edit_clears_redo_and_moving_starts_a_new_step() {
        let mut line = state("", 0, 0);
        typed(&mut line, "ab");
        line.move_by(Motion::Left, false);
        typed(&mut line, "x");
        assert_eq!(line.text(), "axb");
        line.undo();
        assert_eq!(line.text(), "ab");
        typed(&mut line, "y");
        assert!(!line.redo());
    }

    #[test]
    fn a_composition_is_one_undo_step() {
        let mut line = state("", 0, 0);
        line.compose(0..0, "に", Some(3..3));
        let marked = line.marked().unwrap();
        line.compose(marked, "にほ", Some(6..6));
        assert_eq!(line.marked(), Some(0..6));
        let marked = line.marked().unwrap();
        line.edit(marked, "日本", EditKind::Typing);
        assert_eq!(line.text(), "日本");
        assert_eq!(line.marked(), None);
        line.undo();
        assert_eq!(line.text(), "");
    }

    #[test]
    fn set_text_puts_the_cursor_at_the_end_and_clears_history() {
        let mut line = state("", 0, 0);
        typed(&mut line, "a");
        line.set_text("multi\nline");
        assert_eq!(line.text(), "multi line");
        assert_eq!(line.selected_range(), 10..10);
        assert!(!line.undo());
    }

    #[test]
    fn selections_clamp_to_character_boundaries() {
        let mut line = state("né", 0, 0);
        line.select(0, 2);
        assert_eq!(line.selected_range(), 0..1);
        line.select(0, 99);
        assert_eq!(line.selected_text(), Some("né"));
        assert_eq!(line.word_at(1), 0..3);
    }
}
