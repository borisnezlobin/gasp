//! Undo and redo with typing groups, and the editor state that records them.

use std::collections::VecDeque;

use crate::document::{Document, Selection};
use crate::transaction::{ChangeError, ChangeSet, Origin, Transaction, TransactionMeta};

/// How history groups and keeps steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryConfig {
    /// Typing within this many milliseconds of the last keystroke joins the
    /// same undo step.
    pub group_window_ms: u64,
    /// The oldest steps are dropped beyond this many.
    pub max_steps: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            group_window_ms: 500,
            max_steps: 10_000,
        }
    }
}

/// One undo or redo step: the change that reverses an edit, applicable to the
/// current document, and the selection to restore with it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    changes: ChangeSet,
    selection: Selection,
    origin: Origin,
    timestamp_ms: u64,
}

impl Step {
    /// Rebases the step over `remote`, a change applied to the document this
    /// step applies to, and returns `remote` rebased over this step.
    fn rebase(&mut self, remote: &ChangeSet) -> ChangeSet {
        let rebased_remote = remote.map_through(&self.changes);
        self.changes = self.changes.map_through(remote);
        self.selection = self.selection.map(&rebased_remote);
        rebased_remote
    }
}

/// Undo and redo stacks.
#[derive(Clone, Debug, Default)]
pub struct History {
    config: HistoryConfig,
    undo: VecDeque<Step>,
    redo: Vec<Step>,
    group_open: bool,
}

impl History {
    pub fn new(config: HistoryConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    pub fn config(&self) -> HistoryConfig {
        self.config
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Ends the current typing group, so the next edit starts a new step.
    pub fn break_group(&mut self) {
        self.group_open = false;
    }

    /// Records an edit. `inverse` undoes it on the edited document and
    /// `selection_before` is the selection to restore on undo.
    pub fn record(
        &mut self,
        inverse: ChangeSet,
        selection_before: Selection,
        meta: &TransactionMeta,
    ) {
        self.redo.clear();
        if self.joins_group(meta)
            && let Some(top) = self.undo.back_mut()
        {
            top.changes = inverse.compose(&top.changes);
            top.timestamp_ms = meta.timestamp_ms;
            return;
        }
        self.undo.push_back(Step {
            changes: inverse,
            selection: selection_before,
            origin: meta.origin.clone(),
            timestamp_ms: meta.timestamp_ms,
        });
        if self.undo.len() > self.config.max_steps {
            self.undo.pop_front();
        }
        self.group_open = true;
    }

    fn joins_group(&self, meta: &TransactionMeta) -> bool {
        let Some(top) = self.undo.back().filter(|_| self.group_open) else {
            return false;
        };
        let elapsed = meta.timestamp_ms.checked_sub(top.timestamp_ms);
        meta.origin == Origin::Input
            && top.origin == meta.origin
            && elapsed.is_some_and(|elapsed| elapsed <= self.config.group_window_ms)
    }

    /// Rebases both stacks over a change that did not come from this history,
    /// such as a remote edit, which has just been applied to the document.
    pub fn rebase(&mut self, remote: &ChangeSet) {
        self.group_open = false;
        if remote.is_empty() {
            return;
        }
        let mut undo_remote = remote.clone();
        for step in self.undo.iter_mut().rev() {
            undo_remote = step.rebase(&undo_remote);
        }
        let mut redo_remote = remote.clone();
        for step in self.redo.iter_mut().rev() {
            redo_remote = step.rebase(&redo_remote);
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.group_open = false;
    }
}

/// A document, its selection and its history.
#[derive(Clone, Debug, Default)]
pub struct EditorState {
    doc: Document,
    selection: Selection,
    history: History,
}

impl EditorState {
    pub fn new(doc: Document) -> Self {
        Self::with_config(doc, HistoryConfig::default())
    }

    pub fn with_config(doc: Document, config: HistoryConfig) -> Self {
        Self {
            doc,
            selection: Selection::default(),
            history: History::new(config),
        }
    }

    pub fn doc(&self) -> &Document {
        &self.doc
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }

    /// Applies a transaction and records it. Remote transactions are not
    /// undoable; the history is rebased over them instead. A transaction that
    /// only moves the selection ends the current typing group.
    pub fn apply(&mut self, transaction: Transaction) -> Result<(), ChangeError> {
        let Transaction {
            changes,
            selection,
            meta,
        } = transaction;
        changes.validate(&self.doc)?;
        let inverse = changes.invert(&self.doc);
        changes.apply(&mut self.doc)?;
        let selection_before = self.set_selection_after(&changes, selection);
        if changes.is_empty() {
            self.history.break_group();
        } else if meta.origin == Origin::Remote {
            self.history.rebase(&changes);
        } else {
            self.history.record(inverse, selection_before, &meta);
        }
        Ok(())
    }

    /// Undoes the last step. Returns false when there is nothing to undo.
    pub fn undo(&mut self, timestamp_ms: u64) -> bool {
        let Some(step) = self.history.undo.pop_back() else {
            return false;
        };
        let redo = self.revert(step, Origin::Undo, timestamp_ms);
        self.history.redo.push(redo);
        self.history.break_group();
        true
    }

    /// Redoes the last undone step. Returns false when there is nothing to redo.
    pub fn redo(&mut self, timestamp_ms: u64) -> bool {
        let Some(step) = self.history.redo.pop() else {
            return false;
        };
        let undo = self.revert(step, Origin::Redo, timestamp_ms);
        self.history.undo.push_back(undo);
        self.history.break_group();
        true
    }

    /// Applies a stored step and returns the step that takes it back.
    fn revert(&mut self, step: Step, origin: Origin, timestamp_ms: u64) -> Step {
        let inverse = step.changes.invert(&self.doc);
        if step.changes.apply(&mut self.doc).is_err() {
            // A step that no longer fits the document is dropped rather than
            // corrupting the text; rebasing keeps this from happening.
            return Step {
                changes: ChangeSet::empty(),
                selection: self.selection.clone(),
                origin,
                timestamp_ms,
            };
        }
        let selection = std::mem::replace(&mut self.selection, step.selection);
        self.selection = self.selection.clamped(self.doc.len());
        Step {
            changes: inverse,
            selection,
            origin,
            timestamp_ms,
        }
    }

    fn set_selection_after(
        &mut self,
        changes: &ChangeSet,
        selection: Option<Selection>,
    ) -> Selection {
        let next = selection.unwrap_or_else(|| self.selection.map(changes));
        std::mem::replace(&mut self.selection, next.clamped(self.doc.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::SelectionRange;

    fn typed(state: &EditorState, text: &str, timestamp_ms: u64) -> Transaction {
        let at = state.selection().primary().head;
        Transaction::new(ChangeSet::insert(at, text), Origin::Input, timestamp_ms)
    }

    fn type_chars(state: &mut EditorState, text: &str, start_ms: u64, step_ms: u64) {
        for (index, ch) in text.chars().enumerate() {
            let tr = typed(state, &ch.to_string(), start_ms + index as u64 * step_ms);
            state.apply(tr).unwrap();
        }
    }

    #[test]
    fn fast_typing_is_one_undo_step() {
        let mut state = EditorState::new(Document::new());
        type_chars(&mut state, "hello", 0, 100);
        assert_eq!(state.history().undo_depth(), 1);
        assert!(state.undo(1000));
        assert_eq!(state.doc().to_string(), "");
        assert_eq!(state.selection(), &Selection::cursor(0));
    }

    #[test]
    fn pause_longer_than_window_splits_steps() {
        let mut state = EditorState::new(Document::new());
        type_chars(&mut state, "ab", 0, 100);
        type_chars(&mut state, "cd", 800, 100);
        assert_eq!(state.history().undo_depth(), 2);
        state.undo(2000);
        assert_eq!(state.doc().to_string(), "ab");
        assert_eq!(state.selection(), &Selection::cursor(2));
    }

    #[test]
    fn custom_window_is_respected() {
        let config = HistoryConfig {
            group_window_ms: 50,
            ..HistoryConfig::default()
        };
        let mut state = EditorState::with_config(Document::new(), config);
        type_chars(&mut state, "abc", 0, 100);
        assert_eq!(state.history().undo_depth(), 3);
    }

    #[test]
    fn selection_change_breaks_group() {
        let mut state = EditorState::new(Document::from("xyz"));
        state
            .apply(Transaction::select(Selection::cursor(3), Origin::Input, 0))
            .unwrap();
        type_chars(&mut state, "ab", 10, 10);
        state
            .apply(Transaction::select(Selection::cursor(0), Origin::Input, 40))
            .unwrap();
        type_chars(&mut state, "c", 50, 10);
        assert_eq!(state.doc().to_string(), "cxyzab");
        assert_eq!(state.history().undo_depth(), 2);
        state.undo(100);
        assert_eq!(state.doc().to_string(), "xyzab");
        assert_eq!(state.selection(), &Selection::cursor(0));
    }

    #[test]
    fn different_origin_breaks_group() {
        let mut state = EditorState::new(Document::new());
        type_chars(&mut state, "ab", 0, 10);
        let bold = Transaction::new(
            ChangeSet::insert(2, "**"),
            Origin::command("format.bold"),
            30,
        );
        state.apply(bold).unwrap();
        type_chars(&mut state, "c", 40, 10);
        assert_eq!(state.history().undo_depth(), 3);
    }

    #[test]
    fn commands_never_merge() {
        let mut state = EditorState::new(Document::new());
        for timestamp in [0, 10] {
            let tr = Transaction::new(ChangeSet::insert(0, "x"), Origin::command("x"), timestamp);
            state.apply(tr).unwrap();
        }
        assert_eq!(state.history().undo_depth(), 2);
    }

    #[test]
    fn redo_restores_and_new_edit_clears_redo() {
        let mut state = EditorState::new(Document::new());
        type_chars(&mut state, "abc", 0, 10);
        state.undo(100);
        assert!(state.redo(200));
        assert_eq!(state.doc().to_string(), "abc");
        assert_eq!(state.selection(), &Selection::cursor(3));
        state.undo(300);
        assert!(state.history().can_redo());
        type_chars(&mut state, "z", 400, 10);
        assert!(!state.history().can_redo());
        assert!(!state.redo(500));
    }

    #[test]
    fn undo_restores_non_caret_selection() {
        let mut state = EditorState::new(Document::from("hello world"));
        let selected = Selection::single(SelectionRange::new(0, 5));
        state
            .apply(Transaction::select(selected.clone(), Origin::Input, 0))
            .unwrap();
        let replace = Transaction::new(ChangeSet::replace(0..5, "bye"), Origin::Input, 10);
        state.apply(replace).unwrap();
        assert_eq!(state.doc().to_string(), "bye world");
        state.undo(20);
        assert_eq!(state.doc().to_string(), "hello world");
        assert_eq!(state.selection(), &selected);
    }

    #[test]
    fn undo_on_empty_history_is_noop() {
        let mut state = EditorState::new(Document::from("a"));
        assert!(!state.undo(0));
        assert_eq!(state.doc().to_string(), "a");
    }

    #[test]
    fn remote_edits_are_not_undone() {
        let mut state = EditorState::new(Document::from("world"));
        state
            .apply(Transaction::select(Selection::cursor(5), Origin::Input, 0))
            .unwrap();
        type_chars(&mut state, "!", 10, 10);
        let remote = Transaction::new(ChangeSet::insert(0, "hello "), Origin::Remote, 20);
        state.apply(remote).unwrap();
        assert_eq!(state.doc().to_string(), "hello world!");
        assert_eq!(state.history().undo_depth(), 1);
        state.undo(30);
        assert_eq!(state.doc().to_string(), "hello world");
        assert_eq!(state.selection(), &Selection::cursor(11));
        state.redo(40);
        assert_eq!(state.doc().to_string(), "hello world!");
    }

    #[test]
    fn remote_edit_rebases_redo_stack() {
        let mut state = EditorState::new(Document::from("ab"));
        let tr = Transaction::new(ChangeSet::insert(2, "c"), Origin::Input, 0);
        state.apply(tr).unwrap();
        state.undo(10);
        let remote = Transaction::new(ChangeSet::insert(0, "zz"), Origin::Remote, 20);
        state.apply(remote).unwrap();
        state.redo(30);
        assert_eq!(state.doc().to_string(), "zzabc");
    }

    #[test]
    fn invalid_transaction_leaves_state_alone() {
        let mut state = EditorState::new(Document::from("abc"));
        let bad = Transaction::new(ChangeSet::delete(2..9), Origin::Input, 0);
        assert!(state.apply(bad).is_err());
        assert_eq!(state.doc().to_string(), "abc");
        assert!(!state.history().can_undo());
    }

    #[test]
    fn max_steps_drops_oldest() {
        let config = HistoryConfig {
            max_steps: 2,
            ..HistoryConfig::default()
        };
        let mut state = EditorState::with_config(Document::new(), config);
        type_chars(&mut state, "abc", 0, 1000);
        assert_eq!(state.history().undo_depth(), 2);
        while state.undo(5000) {}
        assert_eq!(state.doc().to_string(), "a");
    }

    #[test]
    fn many_undo_redo_cycles_round_trip() {
        let mut state = EditorState::new(Document::from("start"));
        let edits = [(0..0, "a"), (1..3, "XY"), (6..6, "!"), (2..4, "")];
        for (index, (range, text)) in edits.into_iter().enumerate() {
            let tr = Transaction::new(
                ChangeSet::replace(range, text),
                Origin::command("t"),
                index as u64,
            );
            state.apply(tr).unwrap();
        }
        let final_text = state.doc().to_string();
        while state.undo(100) {}
        assert_eq!(state.doc().to_string(), "start");
        while state.redo(200) {}
        assert_eq!(state.doc().to_string(), final_text);
    }
}
