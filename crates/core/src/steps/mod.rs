//! Pipeline steps backed by other crates: snippets and typing replacements.
//!
//! [`install_typing_steps`] fills the `snippets` and `replacements` slots of a
//! [`Pipeline`]. Both engines decide per entry which contexts they fire in, so
//! the slots are opened to every context. The tables are shared: every
//! editor on a vault types with the same compiled snippets.

mod caret;
mod replacements;
mod snippets;

pub use replacements::ReplacementStep;
pub use snippets::SnippetStep;

use std::sync::Arc;

use editor_snippets::{Replacements, SnippetEngine};

use crate::pipeline::{ContextFilter, Pipeline, PipelineError, step_names};

/// Puts the snippet engine and the replacements table into their slots.
pub fn install_typing_steps(
    pipeline: &mut Pipeline,
    snippets: Arc<SnippetEngine>,
    replacements: Arc<Replacements>,
) -> Result<(), PipelineError> {
    install_snippets(pipeline, SnippetStep::new(snippets))?;
    pipeline.replace(
        step_names::REPLACEMENTS,
        Box::new(ReplacementStep::new(replacements)),
    )?;
    pipeline.set_contexts(step_names::REPLACEMENTS, ContextFilter::any())
}

/// Puts a snippet step, set up as the caller likes, into its slot.
pub fn install_snippets(pipeline: &mut Pipeline, step: SnippetStep) -> Result<(), PipelineError> {
    pipeline.replace(step_names::SNIPPETS, Box::new(step))?;
    pipeline.set_contexts(step_names::SNIPPETS, ContextFilter::any())
}

#[cfg(test)]
mod tests;
