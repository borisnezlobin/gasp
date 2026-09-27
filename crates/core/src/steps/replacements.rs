use std::sync::Arc;

use editor_snippets::Replacements;

use crate::document::Selection;
use crate::pipeline::{EditRequest, PipelineStep, StepContext, StepOutcome};
use crate::transaction::{ChangeSet, Origin, Transaction};

use super::caret::{Key, LineAround, snippet_context};

/// The command a replacement is recorded as, apart from the typing before
/// it.
pub const REPLACE_COMMAND: &str = "replacements";

/// Applies typing replacements such as curly quotes, dashes and arrows.
/// The typed character goes in first and the replacement is its own undo
/// step, so undo right after gives back what was typed.
///
/// Curly quotes are left to the smart quotes step, which has its own
/// setting and also curls pasted text, so a migrated table's quote entries
/// don't fight it.
pub struct ReplacementStep {
    table: Arc<Replacements>,
}

impl ReplacementStep {
    pub fn new(table: Arc<Replacements>) -> Self {
        Self { table }
    }

    /// The replacement, as a change to the document after the typing.
    fn replace(&self, typed: char, cx: &StepContext<'_>) -> Option<Transaction> {
        if !cx.selection.primary().is_empty() {
            return None;
        }
        let line = LineAround::read(cx, Key::Char(typed))?;
        let edit = self
            .table
            .find(&line.before, typed, snippet_context(cx.context))
            .filter(|edit| self.table.entries[edit.entry].closing.is_none())?;
        let range = line.typed_offset(edit.replace.start)..line.typed_offset(edit.replace.end);
        let caret = line.caret_after_typing() - range.len() + edit.text.len();
        let changes = ChangeSet::replace(range, edit.text);
        Some(
            Transaction::new(changes, Origin::command(REPLACE_COMMAND), cx.timestamp_ms)
                .with_selection(Selection::cursor(caret)),
        )
    }
}

impl PipelineStep for ReplacementStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        let plan = match Key::of(&request) {
            Some(Key::Char(typed)) => self.replace(typed, cx),
            _ => None,
        };
        match plan {
            Some(replacement) => StepOutcome::EmitAfterTyping(replacement),
            None => StepOutcome::Continue(request),
        }
    }
}
