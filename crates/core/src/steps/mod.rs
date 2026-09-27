//! Pipeline steps backed by other crates: snippets and typing replacements.
//!
//! [`install_typing_steps`] fills the `snippets` and `replacements` slots of a
//! [`Pipeline`]. Both engines decide per entry which contexts they fire in, so
//! the slots are opened to every context.

mod caret;
mod replacements;
mod snippets;

pub use replacements::ReplacementStep;
pub use snippets::SnippetStep;

use editor_snippets::{Replacements, SnippetEngine};

use crate::pipeline::{ContextFilter, Pipeline, PipelineError, step_names};

/// Puts the snippet engine and the replacements table into their slots.
pub fn install_typing_steps(
    pipeline: &mut Pipeline,
    snippets: SnippetEngine,
    replacements: Replacements,
) -> Result<(), PipelineError> {
    pipeline.replace(step_names::SNIPPETS, Box::new(SnippetStep::new(snippets)))?;
    pipeline.set_contexts(step_names::SNIPPETS, ContextFilter::any())?;
    pipeline.replace(
        step_names::REPLACEMENTS,
        Box::new(ReplacementStep::new(replacements)),
    )?;
    pipeline.set_contexts(step_names::REPLACEMENTS, ContextFilter::any())
}

#[cfg(test)]
mod tests;
