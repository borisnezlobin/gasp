//! Tidy (renumber) with cursor mapping, automatic renumbering with an undo
//! guard, and the `^[n]` to `[^n]` fix.

use super::FootnoteSettings;
use super::edits::{FootnoteEdit, apply_edits, compose, map_offset};
use super::lint::{FootnoteProblemKind, blocking_labels};
use super::parse::parse_footnotes;
use super::renumber::{RenumberResult, compute_renumber};

/// How long to wait after the last edit before renumbering automatically.
pub const AUTO_RENUMBER_DEBOUNCE_MS: u64 = 1200;

/// A renumber and whether it should be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedRenumber {
    pub result: RenumberResult,
    /// False when there was nothing to do, or when the cursor guard held it back.
    pub applied: bool,
    /// The cursor in the renumbered text (unchanged when not applied).
    pub cursor: usize,
}

/// Renumbers `text`, mapping `cursor` through the edits. With
/// `guard_cursor_in_block`, a cursor inside the definition block holds the
/// renumber back so the block isn't rearranged under an edit in progress.
pub fn apply_renumber(text: &str, cursor: usize, guard_cursor_in_block: bool) -> AppliedRenumber {
    let result = compute_renumber(text);
    let in_block = result
        .reordered_block
        .as_ref()
        .is_some_and(|block| block.start <= cursor && cursor <= block.end);
    if !result.changed || (guard_cursor_in_block && in_block) {
        return AppliedRenumber {
            result,
            applied: false,
            cursor,
        };
    }
    let cursor = map_offset(&result.edits, cursor);
    AppliedRenumber {
        result,
        applied: true,
        cursor,
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// The notice after the Tidy command.
pub fn tidy_message(result: &RenumberResult, applied: bool) -> String {
    let labels = blocking_labels(&result.problems);
    if !labels.is_empty() {
        return format!(
            "Couldn’t renumber. These footnotes don’t match up: {}. They’re underlined in the editor.",
            labels.join(", ")
        );
    }
    let mut message = if applied {
        "Footnotes renumbered.".to_string()
    } else {
        "Footnotes already in order.".to_string()
    };
    let typos = result
        .problems
        .iter()
        .filter(|p| p.kind == FootnoteProblemKind::InlineTypo)
        .count();
    if typos > 0 {
        message.push_str(&format!(
            " Also found {typos} inline typo{}. Run \"Convert inline footnote typos\".",
            plural(typos)
        ));
    }
    message
}

/// Runs automatic renumbering and remembers the text it replaced, so an
/// undo back to that text isn't immediately renumbered again.
#[derive(Debug, Clone, Default)]
pub struct AutoRenumber {
    undo_guard: Option<String>,
}

impl AutoRenumber {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call once edits have been idle for [`AUTO_RENUMBER_DEBOUNCE_MS`].
    /// Returns the edits to apply and the new cursor, if any.
    pub fn on_idle(
        &mut self,
        text: &str,
        cursor: usize,
        settings: &FootnoteSettings,
    ) -> Option<(Vec<FootnoteEdit>, usize)> {
        if !settings.auto_renumber_on_edit {
            return None;
        }
        if self.undo_guard.as_deref() == Some(text) {
            self.undo_guard = None;
            return None;
        }
        let outcome = apply_renumber(text, cursor, true);
        if !outcome.applied {
            self.undo_guard = None;
            return None;
        }
        self.undo_guard = Some(text.to_string());
        Some((outcome.result.edits, outcome.cursor))
    }
}

/// The result of converting `^[n]` typos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypoFix {
    /// Edits against the original text: the fixes, then a renumber.
    pub edits: Vec<FootnoteEdit>,
    pub cursor: usize,
    pub fixed: usize,
}

/// Converts every `^[n]` typo to `[^n]` and renumbers. `None` if there are none.
pub fn fix_inline_typos(text: &str, cursor: usize) -> Option<TypoFix> {
    let typos = parse_footnotes(text).inline_typos;
    if typos.is_empty() {
        return None;
    }
    let fixes: Vec<FootnoteEdit> = typos
        .iter()
        .map(|typo| FootnoteEdit::new(typo.range.clone(), format!("[^{}]", typo.label)))
        .collect();
    let fixed_text = apply_edits(text, &fixes);
    let renumbered = apply_renumber(&fixed_text, map_offset(&fixes, cursor), false);
    Some(TypoFix {
        edits: compose(&fixes, &renumbered.result.edits, &fixed_text),
        cursor: renumbered.cursor,
        fixed: typos.len(),
    })
}

/// The notice after fixing typos (`fixed` of 0 means none were found).
pub fn fix_typos_message(fixed: usize) -> String {
    if fixed == 0 {
        return "No inline footnote typos found.".to_string();
    }
    format!("Fixed {fixed} inline footnote{}.", plural(fixed))
}
