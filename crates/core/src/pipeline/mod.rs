//! The input pipeline: every keystroke becomes an [`EditRequest`] that passes
//! through ordered, named steps before it becomes a [`Transaction`].
//!
//! ```text
//! snippets → replacements → emoji → footnotes → list-continuation → auto-pair → apply
//! ```
//!
//! A step can pass the request on unchanged, pass on a different request, or
//! consume it by emitting a transaction. Each step has a context filter, so
//! "never in math" is one line of config.

mod apply;
mod auto_pair;
mod context;
mod list;

use std::fmt;

pub use apply::{ApplyStep, RangePlan, backspace_plan, plan_transaction};
pub use auto_pair::AutoPairStep;
pub use context::{
    ContextFilter, ContextProvider, ContextSet, FixedContext, InputContext, UnknownContext,
};
pub use list::ListContinuationStep;

use crate::document::{Document, Selection};
use crate::transaction::{ChangeSet, Origin, Transaction};

/// Names of the built-in steps, in their default order.
pub mod step_names {
    pub const SNIPPETS: &str = "snippets";
    pub const REPLACEMENTS: &str = "replacements";
    pub const EMOJI: &str = "emoji";
    pub const FOOTNOTES: &str = "footnotes";
    pub const LIST_CONTINUATION: &str = "list-continuation";
    pub const AUTO_PAIR: &str = "auto-pair";
    pub const APPLY: &str = "apply";

    pub const DEFAULT_ORDER: [&str; 7] = [
        SNIPPETS,
        REPLACEMENTS,
        EMOJI,
        FOOTNOTES,
        LIST_CONTINUATION,
        AUTO_PAIR,
        APPLY,
    ];
}

/// A change the user asked for, before any step has looked at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditRequest {
    /// Typed or pasted text, replacing each selected range.
    InsertText(String),
    DeleteBackward,
    DeleteForward,
    Newline,
    Tab,
    /// An explicit change, usually produced by an earlier step.
    Replace {
        changes: ChangeSet,
        selection: Option<Selection>,
    },
}

/// What a step sees besides the request.
#[derive(Clone, Copy, Debug)]
pub struct StepContext<'a> {
    pub doc: &'a Document,
    pub selection: &'a Selection,
    /// The context at the primary cursor.
    pub context: InputContext,
    pub timestamp_ms: u64,
}

impl StepContext<'_> {
    /// Wraps a change in an input transaction stamped with this request's time.
    pub fn transaction(&self, changes: ChangeSet, selection: Option<Selection>) -> Transaction {
        let transaction = Transaction::new(changes, Origin::Input, self.timestamp_ms);
        match selection {
            Some(selection) => transaction.with_selection(selection),
            None => transaction,
        }
    }
}

/// What a step did with a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    /// Pass this request (the same one or a new one) to the next step.
    Continue(EditRequest),
    /// Consume the request; this transaction is the result.
    Emit(Transaction),
    /// Consume the request and do nothing.
    Cancel,
}

/// One stage of the pipeline.
pub trait PipelineStep: Send + Sync {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome;
}

/// A named position in the pipeline. A slot without a step is a placeholder
/// that a later wiring step fills; it passes requests through.
pub struct StepSlot {
    name: String,
    step: Option<Box<dyn PipelineStep>>,
    enabled: bool,
    contexts: ContextFilter,
}

impl StepSlot {
    pub fn new(
        name: impl Into<String>,
        step: Box<dyn PipelineStep>,
        contexts: ContextFilter,
    ) -> Self {
        Self {
            name: name.into(),
            step: Some(step),
            enabled: true,
            contexts,
        }
    }

    pub fn placeholder(name: impl Into<String>, contexts: ContextFilter) -> Self {
        Self {
            name: name.into(),
            step: None,
            enabled: true,
            contexts,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_placeholder(&self) -> bool {
        self.step.is_none()
    }

    pub fn contexts(&self) -> ContextFilter {
        self.contexts
    }

    fn runs_in(&self, context: InputContext) -> Option<&dyn PipelineStep> {
        let active = self.enabled && self.contexts.allows(context);
        self.step.as_deref().filter(|_| active)
    }
}

impl fmt::Debug for StepSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StepSlot")
            .field("name", &self.name)
            .field("placeholder", &self.is_placeholder())
            .field("enabled", &self.enabled)
            .field("contexts", &self.contexts)
            .finish()
    }
}

/// Why a pipeline edit failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineError {
    UnknownStep(String),
    DuplicateStep(String),
    /// `reorder` was not given every step exactly once.
    IncompleteOrder,
}

impl fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownStep(name) => write!(formatter, "no pipeline step named `{name}`"),
            Self::DuplicateStep(name) => write!(formatter, "pipeline step `{name}` already exists"),
            Self::IncompleteOrder => formatter.write_str("the new order must name every step once"),
        }
    }
}

impl std::error::Error for PipelineError {}

/// The ordered list of steps.
#[derive(Debug, Default)]
pub struct Pipeline {
    slots: Vec<StepSlot>,
}

impl Pipeline {
    /// A pipeline with no steps at all.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The default pipeline: placeholders for snippets, replacements, emoji
    /// and footnotes, then the built-in list continuation, auto-pair and apply.
    pub fn builtin() -> Self {
        use InputContext::{Code, Frontmatter, Link, Math, Table};
        use step_names::*;
        let slots = vec![
            StepSlot::placeholder(SNIPPETS, ContextFilter::any()),
            StepSlot::placeholder(REPLACEMENTS, ContextFilter::except(&[Code, Math])),
            StepSlot::placeholder(EMOJI, ContextFilter::except(&[Code, Math, Link])),
            StepSlot::placeholder(FOOTNOTES, ContextFilter::except(&[Code, Math])),
            StepSlot::new(
                LIST_CONTINUATION,
                Box::new(ListContinuationStep),
                ContextFilter::except(&[Code, Math, Frontmatter, Table]),
            ),
            StepSlot::new(
                AUTO_PAIR,
                Box::new(AutoPairStep::default()),
                ContextFilter::except(&[Code]),
            ),
            StepSlot::new(APPLY, Box::new(ApplyStep), ContextFilter::any()),
        ];
        Self { slots }
    }

    pub fn step_names(&self) -> Vec<&str> {
        self.slots.iter().map(StepSlot::name).collect()
    }

    pub fn slot(&self, name: &str) -> Option<&StepSlot> {
        self.slots.iter().find(|slot| slot.name == name)
    }

    fn index_of(&self, name: &str) -> Result<usize, PipelineError> {
        self.slots
            .iter()
            .position(|slot| slot.name == name)
            .ok_or_else(|| PipelineError::UnknownStep(name.to_owned()))
    }

    fn slot_mut(&mut self, name: &str) -> Result<&mut StepSlot, PipelineError> {
        let index = self.index_of(name)?;
        Ok(&mut self.slots[index])
    }

    fn check_new_name(&self, name: &str) -> Result<(), PipelineError> {
        match self.slot(name) {
            Some(_) => Err(PipelineError::DuplicateStep(name.to_owned())),
            None => Ok(()),
        }
    }

    /// Adds a step at the end.
    pub fn push(&mut self, slot: StepSlot) -> Result<(), PipelineError> {
        self.check_new_name(&slot.name)?;
        self.slots.push(slot);
        Ok(())
    }

    /// Adds a step just before `anchor`.
    pub fn insert_before(&mut self, anchor: &str, slot: StepSlot) -> Result<(), PipelineError> {
        self.check_new_name(&slot.name)?;
        let index = self.index_of(anchor)?;
        self.slots.insert(index, slot);
        Ok(())
    }

    /// Adds a step just after `anchor`.
    pub fn insert_after(&mut self, anchor: &str, slot: StepSlot) -> Result<(), PipelineError> {
        self.check_new_name(&slot.name)?;
        let index = self.index_of(anchor)?;
        self.slots.insert(index + 1, slot);
        Ok(())
    }

    /// Puts a new implementation in a named slot, filling a placeholder or
    /// replacing a built-in step. The slot keeps its place and settings.
    pub fn replace(
        &mut self,
        name: &str,
        step: Box<dyn PipelineStep>,
    ) -> Result<(), PipelineError> {
        self.slot_mut(name)?.step = Some(step);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<StepSlot, PipelineError> {
        let index = self.index_of(name)?;
        Ok(self.slots.remove(index))
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<(), PipelineError> {
        self.slot_mut(name)?.enabled = enabled;
        Ok(())
    }

    pub fn set_contexts(
        &mut self,
        name: &str,
        contexts: ContextFilter,
    ) -> Result<(), PipelineError> {
        self.slot_mut(name)?.contexts = contexts;
        Ok(())
    }

    /// Moves `name` to just before `anchor`.
    pub fn move_before(&mut self, name: &str, anchor: &str) -> Result<(), PipelineError> {
        self.index_of(anchor)?;
        let slot = self.remove(name)?;
        let index = self.index_of(anchor)?;
        self.slots.insert(index, slot);
        Ok(())
    }

    /// Moves `name` to just after `anchor`.
    pub fn move_after(&mut self, name: &str, anchor: &str) -> Result<(), PipelineError> {
        self.index_of(anchor)?;
        let slot = self.remove(name)?;
        let index = self.index_of(anchor)?;
        self.slots.insert(index + 1, slot);
        Ok(())
    }

    /// Puts the steps in the given order, which must name each step once.
    pub fn reorder(&mut self, names: &[&str]) -> Result<(), PipelineError> {
        if names.len() != self.slots.len() {
            return Err(PipelineError::IncompleteOrder);
        }
        let mut indices = Vec::with_capacity(names.len());
        for name in names {
            let index = self
                .index_of(name)
                .map_err(|_| PipelineError::IncompleteOrder)?;
            if indices.contains(&index) {
                return Err(PipelineError::IncompleteOrder);
            }
            indices.push(index);
        }
        let mut slots: Vec<Option<StepSlot>> = self.slots.drain(..).map(Some).collect();
        self.slots = indices
            .into_iter()
            .filter_map(|index| slots[index].take())
            .collect();
        Ok(())
    }

    /// Runs a request through the steps. Returns the transaction to apply, or
    /// `None` when a step cancelled it or no step produced one.
    pub fn run(
        &self,
        request: EditRequest,
        doc: &Document,
        selection: &Selection,
        contexts: &dyn ContextProvider,
        timestamp_ms: u64,
    ) -> Option<Transaction> {
        let cx = StepContext {
            doc,
            selection,
            context: contexts.context_at(doc, selection.primary().head),
            timestamp_ms,
        };
        let mut request = request;
        for step in self
            .slots
            .iter()
            .filter_map(|slot| slot.runs_in(cx.context))
        {
            match step.run(request, &cx) {
                StepOutcome::Continue(next) => request = next,
                StepOutcome::Emit(transaction) => return Some(transaction),
                StepOutcome::Cancel => return None,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
