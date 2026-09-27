//! Editing commands the keymap binds: formatting toggles and footnotes.
//!
//! Each command reads the document and selection and returns a
//! [`Transaction`] tagged with the command's id, so it becomes its own undo
//! step. Returning `None` means there is nothing to do.

mod footnote;
mod format;

pub use footnote::{FootnoteCommand, insert_or_jump_footnote};
pub use format::{Format, toggle_format};

use crate::document::{Document, Selection};
use crate::pipeline::{InputContext, RangePlan, StepContext, plan_transaction};
use crate::transaction::{Origin, Transaction};

/// Builds a transaction from per-range plans and tags it with `command`.
fn command_transaction(
    doc: &Document,
    selection: &Selection,
    plans: Vec<RangePlan>,
    command: &str,
    timestamp_ms: u64,
) -> Transaction {
    let cx = StepContext {
        doc,
        selection,
        context: InputContext::Text,
        timestamp_ms,
    };
    let mut transaction = plan_transaction(&cx, plans);
    transaction.meta.origin = Origin::command(command);
    transaction
}
