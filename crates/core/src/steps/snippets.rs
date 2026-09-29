use std::ops::Range;
use std::sync::Arc;

use gasp_snippets::{Request, SnippetEdit, SnippetEngine, TriggerKey};

use crate::document::{Selection, SelectionRange};
use crate::pipeline::{
    EditRequest, PipelineStep, RangePlan, StepContext, StepOutcome, enlarge_in, plan_transaction,
};
use crate::transaction::{Assoc, ChangeSet, TextEdit, Transaction};

use super::caret::{Key, LineAround, snippet_context};

/// Stops in document offsets, one list of ranges per stop.
type Stops = Vec<Vec<Range<usize>>>;

/// Expands snippets on a typed character or Tab, and leaves their tab
/// stops for Tab to visit.
pub struct SnippetStep {
    engine: Arc<SnippetEngine>,
    /// Whether brackets around a `\sum` or `\frac` become `\left(` and
    /// `\right)` once a snippet in math expands.
    enlarge_brackets: bool,
}

impl SnippetStep {
    pub fn new(engine: Arc<SnippetEngine>) -> Self {
        Self {
            engine,
            enlarge_brackets: true,
        }
    }

    pub fn with_enlarged_brackets(mut self, on: bool) -> Self {
        self.enlarge_brackets = on;
        self
    }

    fn expand(&self, key: Key, cx: &StepContext<'_>) -> Option<StepOutcome> {
        let line = LineAround::read(cx, key)?;
        let edit = self.engine.expand(&Request {
            before: &line.before,
            selection: &line.selected,
            after: &line.after,
            context: snippet_context(cx.context),
            block_math: cx.math.is_some_and(|math| math.block),
            key: trigger_key(key),
        })?;
        let plan = plan_for(&line, &edit);
        let start = plan.edit.range.start;
        let stops: Stops = edit
            .stops
            .iter()
            .map(|stop| {
                let ranges = stop.ranges.iter();
                ranges.map(|r| start + r.start..start + r.end).collect()
            })
            .collect();
        let expanded = plan_transaction(cx, vec![plan.clone()]);
        let (mut transaction, stops) = match self.enlarge(cx, &plan) {
            Some(extra) => enlarged(expanded, stops, &extra),
            None => (expanded, stops),
        };
        // A stop that appears more than once is typed into everywhere.
        if let Some(first) = stops.first().filter(|ranges| ranges.len() > 1) {
            let ranges = first
                .iter()
                .map(|range| SelectionRange::new(range.start, range.end))
                .collect();
            transaction.selection = Some(Selection::new(ranges, 0));
        }
        Some(StepOutcome::EmitWithStops(transaction, stops))
    }

    /// Enlarges brackets in the math around the expansion, as a change to
    /// the document after it.
    fn enlarge(&self, cx: &StepContext<'_>, plan: &RangePlan) -> Option<ChangeSet> {
        let math = cx.math.filter(|_| self.enlarge_brackets)?;
        let range = &plan.edit.range;
        if range.start < math.inner.start || range.end > math.inner.end {
            return None;
        }
        let mut region = cx.doc.slice(math.inner.clone());
        let at = range.start - math.inner.start..range.end - math.inner.start;
        region.replace_range(at, &plan.edit.insert);
        enlarge_in(&region, math.inner.start)
    }
}

/// The expansion followed by `extra`, as one undo step, with the
/// selection and stops moved to match. `extra` only adds `\left` and
/// `\right` beside brackets, which never belong inside a stop, so
/// everything keeps to the text before an insertion.
fn enlarged(expanded: Transaction, stops: Stops, extra: &ChangeSet) -> (Transaction, Stops) {
    let map = |offset: usize| extra.map_offset(offset, Assoc::Before);
    let stops = stops
        .iter()
        .map(|stop| stop.iter().map(|r| map(r.start)..map(r.end)).collect())
        .collect();
    let selection = expanded.selection.map(|selection| {
        let ranges = selection
            .ranges()
            .iter()
            .map(|range| SelectionRange::new(map(range.anchor), map(range.head)))
            .collect();
        Selection::new(ranges, selection.primary_index())
    });
    let transaction = Transaction {
        changes: expanded.changes.compose(extra),
        selection,
        meta: expanded.meta,
    };
    (transaction, stops)
}

impl PipelineStep for SnippetStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        match Key::of(&request).and_then(|key| self.expand(key, cx)) {
            Some(outcome) => outcome,
            None => StepOutcome::Continue(request),
        }
    }
}

fn trigger_key(key: Key) -> TriggerKey {
    match key {
        Key::Char(typed) => TriggerKey::Char(typed),
        Key::Tab => TriggerKey::Tab,
    }
}

/// Replaces the trigger and selects the first tab stop, or puts the caret
/// after the expansion when it has none.
fn plan_for(line: &LineAround, edit: &SnippetEdit) -> RangePlan {
    let start = line.doc_offset(edit.replace.start);
    let end = line.doc_offset(edit.replace.end);
    let first_stop = edit.stops.first().and_then(|stop| stop.ranges.first());
    let (anchor, head) = first_stop.map_or((edit.text.len(), edit.text.len()), |range| {
        (range.start, range.end)
    });
    RangePlan {
        edit: TextEdit::new(start..end, edit.text.clone()),
        anchor,
        head,
    }
}
