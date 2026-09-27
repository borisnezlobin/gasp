use editor_snippets::Replacements;

use crate::pipeline::{
    EditRequest, PipelineStep, RangePlan, StepContext, StepOutcome, plan_transaction,
};

use super::caret::{Key, LineAround, snippet_context};

/// Applies typing replacements such as curly quotes, dashes and arrows.
pub struct ReplacementStep {
    table: Replacements,
}

impl ReplacementStep {
    pub fn new(table: Replacements) -> Self {
        Self { table }
    }

    fn replace(&self, typed: char, cx: &StepContext<'_>) -> Option<RangePlan> {
        if !cx.selection.primary().is_empty() {
            return None;
        }
        let line = LineAround::read(cx, Key::Char(typed))?;
        let edit = self
            .table
            .find(&line.before, typed, snippet_context(cx.context))?;
        let range = line.doc_offset(edit.replace.start)..line.doc_offset(edit.replace.end);
        Some(RangePlan::replace(range, &edit.text))
    }
}

impl PipelineStep for ReplacementStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        let plan = match Key::of(&request) {
            Some(Key::Char(typed)) => self.replace(typed, cx),
            _ => None,
        };
        match plan {
            Some(plan) => StepOutcome::Emit(plan_transaction(cx, vec![plan])),
            None => StepOutcome::Continue(request),
        }
    }
}
