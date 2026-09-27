//! The terminal step, which turns a request into a transaction, and helpers
//! other steps use to build per-cursor transactions.

use crate::document::{Document, Selection, SelectionRange};
use crate::transaction::{ChangeSet, TextEdit, Transaction};

use super::{EditRequest, PipelineStep, StepContext, StepOutcome};

/// What to do at one selected range: an edit in original coordinates, and the
/// new range as offsets from the edit's start in the new document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangePlan {
    pub edit: TextEdit,
    pub anchor: usize,
    pub head: usize,
}

impl RangePlan {
    /// Replace `range` with `text` and put the caret after it.
    pub fn replace(range: std::ops::Range<usize>, text: &str) -> Self {
        Self::replace_with_caret(range, text, text.len())
    }

    /// Replace `range` with `text` and put the caret `caret` bytes into it.
    pub fn replace_with_caret(range: std::ops::Range<usize>, text: &str, caret: usize) -> Self {
        Self {
            edit: TextEdit::new(range, text),
            anchor: caret,
            head: caret,
        }
    }

    /// Leave the text alone and put the caret at `offset`.
    pub fn move_caret(offset: usize) -> Self {
        Self::replace(offset..offset, "")
    }
}

/// Builds one transaction from per-range plans. A plan whose edit overlaps an
/// earlier one (two carets deleting the same character) is dropped.
pub fn plan_transaction(cx: &StepContext<'_>, mut plans: Vec<RangePlan>) -> Transaction {
    plans.sort_by_key(|plan| (plan.edit.range.start, plan.edit.range.end));
    let primary = cx.selection.primary_index();
    let mut edits = Vec::with_capacity(plans.len());
    let mut ranges = Vec::with_capacity(plans.len());
    let mut delta: isize = 0;
    let mut last_end = 0;
    for plan in plans {
        if !edits.is_empty() && plan.edit.range.start < last_end {
            continue;
        }
        let start = plan.edit.range.start.saturating_add_signed(delta);
        ranges.push(SelectionRange::new(start + plan.anchor, start + plan.head));
        delta += plan.edit.insert.len() as isize - plan.edit.range.len() as isize;
        last_end = plan.edit.range.end;
        edits.push(plan.edit);
    }
    // Plans are built from sorted, non-overlapping ranges, so these edits
    // never overlap; the fallback only guards against a misbehaving step.
    let changes = ChangeSet::new(edits).unwrap_or_default();
    let primary = primary.min(ranges.len().saturating_sub(1));
    cx.transaction(changes, Some(Selection::new(ranges, primary)))
}

/// Applies the same plan function to every selected range.
pub fn plan_each(cx: &StepContext<'_>, plan: impl Fn(&SelectionRange) -> RangePlan) -> Transaction {
    let plans = cx.selection.ranges().iter().map(plan).collect();
    plan_transaction(cx, plans)
}

/// Deletes the selection, or the character before a caret (`\r\n` counts as
/// one character).
pub fn backspace_plan(doc: &Document, range: &SelectionRange) -> RangePlan {
    if !range.is_empty() {
        return RangePlan::replace(range.range(), "");
    }
    let end = range.head;
    let mut start = doc.prev_char_boundary(end);
    if doc.char_before(end) == Some('\n') && doc.char_before(start) == Some('\r') {
        start -= 1;
    }
    RangePlan::replace(start..end, "")
}

/// Deletes the selection, or the character after a caret.
pub fn delete_forward_plan(doc: &Document, range: &SelectionRange) -> RangePlan {
    if !range.is_empty() {
        return RangePlan::replace(range.range(), "");
    }
    let start = range.head;
    let mut end = doc.next_char_boundary(start);
    if doc.char_after(start) == Some('\r') && doc.char_after(end) == Some('\n') {
        end += 1;
    }
    RangePlan::replace(start..end, "")
}

fn insert_each(cx: &StepContext<'_>, text: &str) -> Transaction {
    plan_each(cx, |range| RangePlan::replace(range.range(), text))
}

/// `request` as the apply step would make it, ignoring every other step.
pub(super) fn as_typed(request: EditRequest, cx: &StepContext<'_>) -> Transaction {
    match ApplyStep.run(request, cx) {
        StepOutcome::Emit(transaction) => transaction,
        _ => unreachable!("the apply step always emits"),
    }
}

/// Turns whatever request reaches it into a transaction.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApplyStep;

impl PipelineStep for ApplyStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        let transaction = match request {
            EditRequest::InsertText(text) => insert_each(cx, &text),
            EditRequest::Newline => insert_each(cx, "\n"),
            EditRequest::Tab => insert_each(cx, "\t"),
            EditRequest::DeleteBackward => plan_each(cx, |range| backspace_plan(cx.doc, range)),
            EditRequest::DeleteForward => plan_each(cx, |range| delete_forward_plan(cx.doc, range)),
            EditRequest::Replace { changes, selection } => cx.transaction(changes, selection),
        };
        StepOutcome::Emit(transaction)
    }
}
