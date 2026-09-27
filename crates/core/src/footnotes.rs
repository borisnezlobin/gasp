//! Footnotes: parse references and definitions, insert the next numbered
//! footnote, jump between a reference and its definition, renumber numbered
//! footnotes in reading order, lint problems and fix the `^[1]` typo.
//!
//! Everything works on a `&str` with byte offsets and returns [`FootnoteEdit`]
//! lists (plus a cursor where relevant) for the caller to wrap in a
//! transaction. Footnote syntax inside code spans, fenced code and math is
//! ignored.

mod commands;
mod edits;
mod insert;
mod lint;
mod mask;
mod parse;
mod renumber;

#[cfg(test)]
mod tests_core;
#[cfg(test)]
mod tests_edge;
#[cfg(test)]
mod tests_sim;

pub use commands::{
    AUTO_RENUMBER_DEBOUNCE_MS, AppliedRenumber, AutoRenumber, TypoFix, apply_renumber,
    fix_inline_typos, fix_typos_message, tidy_message,
};
pub use edits::{FootnoteEdit, apply_edits, map_offset};
pub use insert::{
    InsertOrJump, PlannedInsert, definition_cursor, insert_or_jump, plan_create_missing_definition,
    plan_new_footnote, reference_cursor, unreferenced_message,
};
pub use lint::{
    FootnoteProblem, FootnoteProblemKind, blocking_labels, classify_problems, find_problems,
    has_blocking_problems, highlight_problems,
};
pub use parse::{
    FootnoteDef, FootnoteRef, InlineTypo, ParsedFootnotes, find_def, first_ref, is_numeric,
    max_numeric_label, parse_footnotes,
};
pub use renumber::{RenumberResult, compute_renumber};

/// User options, matching the Footnotes Plus plugin settings and defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FootnoteSettings {
    /// Keep numbered footnotes in order automatically after edits.
    pub auto_renumber_on_edit: bool,
    /// After inserting a footnote, put the cursor in its new definition.
    pub jump_to_new_definition: bool,
}

impl Default for FootnoteSettings {
    fn default() -> Self {
        Self {
            auto_renumber_on_edit: true,
            jump_to_new_definition: true,
        }
    }
}
