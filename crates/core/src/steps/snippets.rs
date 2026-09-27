use editor_snippets::{Request, SnippetEdit, SnippetEngine, TriggerKey};

use crate::pipeline::{
    EditRequest, InputContext, PipelineStep, RangePlan, StepContext, StepOutcome, plan_transaction,
};
use crate::transaction::TextEdit;

use super::caret::{Key, LineAround, in_block_math, snippet_context};

/// Expands snippets on a typed character or Tab.
pub struct SnippetStep {
    engine: SnippetEngine,
}

impl SnippetStep {
    pub fn new(engine: SnippetEngine) -> Self {
        Self { engine }
    }

    fn expand(&self, key: Key, cx: &StepContext<'_>) -> Option<RangePlan> {
        let line = LineAround::read(cx, key)?;
        let block_math = cx.context == InputContext::Math
            && in_block_math(cx.doc, cx.selection.primary().from());
        let edit = self.engine.expand(&Request {
            before: &line.before,
            selection: &line.selected,
            after: &line.after,
            context: snippet_context(cx.context),
            block_math,
            key: trigger_key(key),
        })?;
        Some(plan_for(&line, &edit))
    }
}

impl PipelineStep for SnippetStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        let plan = Key::of(&request).and_then(|key| self.expand(key, cx));
        match plan {
            Some(plan) => StepOutcome::Emit(plan_transaction(cx, vec![plan])),
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
