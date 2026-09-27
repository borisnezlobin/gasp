//! The insert-or-jump command (`Alt+0`):
//! - cursor on a definition line: jump up to its first reference
//! - cursor on a reference: jump down to its definition, creating it if missing
//! - otherwise: insert the next numbered footnote here

use super::FootnoteSettings;
use super::edits::{FootnoteEdit, apply_edits, compose, line_ending};
use super::mask::mask_code_and_math;
use super::parse::{
    FootnoteDef, def_head, find_def, first_ref, is_numeric, max_numeric_label, parse_footnotes,
};
use super::renumber::compute_renumber;

/// What insert-or-jump decided to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOrJump {
    /// Only move the cursor.
    Jump { cursor: usize },
    /// Apply the edits, then put the cursor at `cursor` (in the edited text).
    Edit {
        edits: Vec<FootnoteEdit>,
        cursor: usize,
    },
    /// The cursor is on a definition nothing references; show a notice.
    Unreferenced { label: String },
}

/// A planned footnote insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedInsert {
    pub new_text: String,
    /// Edits against the original text that produce `new_text`.
    pub edits: Vec<FootnoteEdit>,
    /// Cursor offset in `new_text`.
    pub cursor: usize,
}

/// The notice shown when jumping from a definition nothing references.
pub fn unreferenced_message(label: &str) -> String {
    format!("Footnote [^{label}] isn't referenced in the text.")
}

/// Where the cursor goes to type a definition: after `[^label]:` and one space.
pub fn definition_cursor(def: &FootnoteDef) -> usize {
    let leading_space = usize::from(def.body.starts_with(' '));
    def.head_range().end + leading_space
}

/// Where the cursor goes when jumping to a label's first reference.
pub fn reference_cursor(text: &str, label: &str) -> Option<usize> {
    let parsed = parse_footnotes(text);
    first_ref(&parsed.refs, label).map(|r| r.range.end)
}

/// Inserts an empty definition for `label` after the last definition, or at
/// the end of the document after a blank line.
pub fn plan_create_missing_definition(
    text: &str,
    defs: &[FootnoteDef],
    label: &str,
) -> FootnoteEdit {
    let (position, insert) = definition_insert(text, defs, label, false);
    FootnoteEdit::new(position..position, insert)
}

/// Where a new definition goes and its text. `after_marker` means a new
/// reference is being inserted at the very end of the text.
fn definition_insert(
    text: &str,
    defs: &[FootnoteDef],
    label: &str,
    after_marker: bool,
) -> (usize, String) {
    let newline = line_ending(text);
    if let Some(last) = defs.last() {
        return (last.end, format!("{newline}{}[^{label}]: ", last.indent));
    }
    let separator = if after_marker {
        format!("{newline}{newline}")
    } else if text.is_empty() || text.ends_with(&format!("{newline}{newline}")) {
        String::new()
    } else if text.ends_with('\n') {
        newline.to_string()
    } else {
        format!("{newline}{newline}")
    };
    (text.len(), format!("{separator}[^{label}]: "))
}

/// How many distinct numbered footnotes are first cited before `cursor`.
fn numbered_before(text: &str, cursor: usize) -> usize {
    let parsed = parse_footnotes(text);
    let mut seen: Vec<&str> = Vec::new();
    for reference in parsed.refs.iter().filter(|r| is_numeric(&r.label)) {
        if !seen.contains(&reference.label.as_str()) && reference.range.start < cursor {
            seen.push(&reference.label);
        }
        if reference.range.start >= cursor {
            break;
        }
    }
    seen.len()
}

/// Plans a new footnote at `cursor`: a reference there and an empty
/// definition in the definition block, numbered by reading order.
pub fn plan_new_footnote(text: &str, cursor: usize, jump_to_new_definition: bool) -> PlannedInsert {
    let parsed = parse_footnotes(text);
    let temp = (max_numeric_label(&parsed) + 1).to_string();
    let target = (numbered_before(text, cursor) + 1).to_string();

    let marker = format!("[^{temp}]");
    let (def_position, def_text) =
        definition_insert(text, &parsed.defs, &temp, cursor == text.len());
    let inserts = vec![
        FootnoteEdit::new(cursor..cursor, marker.clone()),
        FootnoteEdit::new(def_position..def_position, def_text),
    ];
    let interim = apply_edits(text, &inserts);
    let result = compute_renumber(&interim);
    let edits = compose(&inserts, &result.edits, &interim);
    // An inconsistent document is not renumbered, so the marker keeps its
    // temporary label.
    let label = if result.changed { target } else { temp };

    let placed = parse_footnotes(&result.new_text);
    let found = if jump_to_new_definition {
        find_def(&placed.defs, &label).map(definition_cursor)
    } else {
        first_ref(&placed.refs, &label).map(|r| r.range.end)
    };
    PlannedInsert {
        cursor: found.unwrap_or(cursor + marker.len()),
        new_text: result.new_text,
        edits,
    }
}

fn line_around(text: &str, cursor: usize) -> std::ops::Range<usize> {
    let start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i);
    start..end
}

fn jump_from_definition(text: &str, label: String) -> InsertOrJump {
    match reference_cursor(text, &label) {
        Some(cursor) => InsertOrJump::Jump { cursor },
        None => InsertOrJump::Unreferenced { label },
    }
}

fn jump_or_create_definition(text: &str, defs: &[FootnoteDef], label: &str) -> InsertOrJump {
    if let Some(def) = find_def(defs, label) {
        return InsertOrJump::Jump {
            cursor: definition_cursor(def),
        };
    }
    let edit = plan_create_missing_definition(text, defs, label);
    let created = apply_edits(text, std::slice::from_ref(&edit));
    let parsed = parse_footnotes(&created);
    let cursor = find_def(&parsed.defs, label).map_or(created.len(), definition_cursor);
    InsertOrJump::Edit {
        edits: vec![edit],
        cursor,
    }
}

/// The combined insert-or-jump command. `cursor` must be on a char boundary.
pub fn insert_or_jump(text: &str, cursor: usize, settings: &FootnoteSettings) -> InsertOrJump {
    let masked = mask_code_and_math(text);
    // On a definition line, always navigate: inserting there is what
    // produces footnotes inside footnotes.
    if let Some(head) = def_head(&masked[line_around(text, cursor)]) {
        return jump_from_definition(text, head.label);
    }

    let parsed = parse_footnotes(text);
    let on_ref = parsed
        .refs
        .iter()
        .find(|r| r.range.start <= cursor && cursor <= r.range.end);
    if let Some(reference) = on_ref {
        return jump_or_create_definition(text, &parsed.defs, &reference.label);
    }

    let planned = plan_new_footnote(text, cursor, settings.jump_to_new_definition);
    InsertOrJump::Edit {
        edits: planned.edits,
        cursor: planned.cursor,
    }
}
