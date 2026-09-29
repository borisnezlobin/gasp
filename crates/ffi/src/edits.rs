//! Editing commands run by id on one note: the core works out the edit,
//! and the phone applies it to the text view as one undo step.

use std::ops::Range;

use gasp_core::commands::{
    FootnoteCommand, Format, duplicate_lines, indent, insert_callout, insert_link,
    insert_or_jump_footnote, move_lines_down, move_lines_up, outdent, toggle_bullet_list,
    toggle_format, toggle_numbered_list, toggle_tasks,
};
use gasp_core::document::{Document, Selection, SelectionRange};
use gasp_core::footnotes::{
    FootnoteEdit, FootnoteSettings, apply_renumber, fix_inline_typos, fix_typos_message,
    tidy_message,
};
use gasp_core::motion;
use gasp_core::syntax::{NodeKind, SyntaxTree};
use gasp_core::table::{Table, TableOp, insert_table, table_transaction};
use gasp_core::transaction::Transaction;

use crate::offsets::{TextRange, Utf16Offsets};

/// One piece of an edit: the range of the text before it, and what goes
/// there.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TextReplacement {
    pub range: TextRange,
    pub text: String,
}

/// What running a command on a note comes to.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum CommandOutcome {
    /// Replace these ranges (sorted, not overlapping, in the text as it
    /// was), then select `selection` in the new text.
    Edit {
        replacements: Vec<TextReplacement>,
        selection: TextRange,
    },
    /// Only move the selection.
    Select { selection: TextRange },
    /// Put this on the clipboard.
    Copy { text: String },
    /// Tell the person this, and change nothing.
    Notice { message: String },
    /// The note draws differently now; plan it again.
    Redraw,
    /// The command isn't one that edits a note.
    NotANoteCommand,
}

/// The note a command runs on.
pub(crate) struct CommandInput<'a> {
    pub text: &'a str,
    pub tree: &'a SyntaxTree,
    pub offsets: &'a Utf16Offsets,
    pub selection: Selection,
}

impl CommandInput<'_> {
    fn head(&self) -> usize {
        self.selection.primary().head
    }
}

type TransactionCommand = fn(&Document, &Selection, u64) -> Transaction;

/// Commands that are a core transaction on the whole selection.
const TRANSACTIONS: &[(&str, TransactionCommand)] = &[
    ("format.link", insert_link),
    ("format.callout", insert_callout),
    ("edit.indent", indent),
    ("edit.outdent", outdent),
    ("edit.move-line-up", move_lines_up),
    ("edit.move-line-down", move_lines_down),
    ("edit.duplicate-line", duplicate_lines),
    ("edit.toggle-task", toggle_tasks),
    ("format.bullet-list", toggle_bullet_list),
    ("format.numbered-list", toggle_numbered_list),
    ("table.insert", insert_table),
];

type OtherCommand = fn(&CommandInput<'_>) -> CommandOutcome;

/// Commands that edit, copy or tell something in their own way.
const OTHERS: &[(&str, OtherCommand)] = &[
    ("edit.delete-to-line-start", |input| {
        delete(input, motion::to_line_start)
    }),
    ("edit.delete-to-line-end", |input| {
        delete(input, motion::to_line_end)
    }),
    ("footnote.insert-or-jump", footnote),
    ("footnote.tidy", tidy_footnotes),
    ("footnote.fix-typos", fix_footnote_typos),
    ("table.copy-markdown", |input| {
        copy_table(input, |table| table.format().text)
    }),
    ("table.copy-tsv", |input| copy_table(input, Table::tsv)),
    ("code.copy-block", copy_code_block),
];

/// Whether `id` is a command [`run`] handles.
pub(crate) fn is_note_command(id: &str) -> bool {
    Format::from_command_id(id).is_some()
        || TableOp::from_command_id(id).is_some()
        || TRANSACTIONS.iter().any(|(name, _)| *name == id)
        || OTHERS.iter().any(|(name, _)| *name == id)
}

/// Runs the note command `id`.
pub(crate) fn run(id: &str, input: &CommandInput<'_>) -> CommandOutcome {
    let doc = Document::from(input.text);
    if let Some(format) = Format::from_command_id(id) {
        let transaction = toggle_format(&doc, &input.selection, format, now_ms());
        return outcome(input, transaction);
    }
    if let Some(op) = TableOp::from_command_id(id) {
        return table_op(input, &doc, op);
    }
    if let Some((_, command)) = TRANSACTIONS.iter().find(|(name, _)| *name == id) {
        return outcome(input, command(&doc, &input.selection, now_ms()));
    }
    OTHERS
        .iter()
        .find(|(name, _)| *name == id)
        .map_or(CommandOutcome::NotANoteCommand, |(_, command)| {
            command(input)
        })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// The selection for a UTF-16 range, the anchor first.
pub(crate) fn selection(offsets: &Utf16Offsets, range: TextRange) -> Selection {
    let bytes = offsets.byte_range(range);
    Selection::single(SelectionRange::new(bytes.start, bytes.end))
}

fn outcome(input: &CommandInput<'_>, transaction: Transaction) -> CommandOutcome {
    let selection = transaction
        .selection
        .clone()
        .unwrap_or_else(|| input.selection.map(&transaction.changes));
    let edits: Vec<(Range<usize>, String)> = transaction
        .changes
        .edits()
        .iter()
        .map(|edit| (edit.range.clone(), edit.insert.clone()))
        .collect();
    edit_outcome(input, edits, selection.primary().range())
}

/// The outcome of replacing `edits` (in the current text) and selecting
/// `selected` (in the text after them).
pub(crate) fn edit_outcome(
    input: &CommandInput<'_>,
    mut edits: Vec<(Range<usize>, String)>,
    selected: Range<usize>,
) -> CommandOutcome {
    edits.sort_by_key(|(range, _)| range.start);
    let new_text = applied(input.text, &edits);
    let new_offsets = Utf16Offsets::new(&new_text);
    let selection = new_offsets.range(&selected);
    if edits.is_empty() {
        return CommandOutcome::Select { selection };
    }
    CommandOutcome::Edit {
        replacements: edits
            .into_iter()
            .map(|(range, text)| TextReplacement {
                range: input.offsets.range(&range),
                text,
            })
            .collect(),
        selection,
    }
}

fn applied(text: &str, sorted_edits: &[(Range<usize>, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (range, insert) in sorted_edits {
        out.push_str(&text[at..range.start]);
        out.push_str(insert);
        at = range.end;
    }
    out.push_str(&text[at..]);
    out
}

fn delete(input: &CommandInput<'_>, reach: fn(&Document, usize) -> Range<usize>) -> CommandOutcome {
    let primary = input.selection.primary();
    let range = if primary.is_empty() {
        reach(&Document::from(input.text), primary.head)
    } else {
        primary.range()
    };
    let start = range.start;
    edit_outcome(input, vec![(range, String::new())], start..start)
}

fn footnote(input: &CommandInput<'_>) -> CommandOutcome {
    let doc = Document::from(input.text);
    let settings = FootnoteSettings::default();
    match insert_or_jump_footnote(&doc, &input.selection, &settings, now_ms()) {
        FootnoteCommand::Apply(transaction) => outcome(input, transaction),
        FootnoteCommand::Notice(message) => CommandOutcome::Notice { message },
    }
}

fn footnote_edits(
    input: &CommandInput<'_>,
    edits: Vec<FootnoteEdit>,
    cursor: usize,
) -> CommandOutcome {
    let edits = edits
        .into_iter()
        .map(|edit| (edit.range, edit.insert))
        .collect();
    edit_outcome(input, edits, cursor..cursor)
}

fn tidy_footnotes(input: &CommandInput<'_>) -> CommandOutcome {
    let renumber = apply_renumber(input.text, input.head(), false);
    if !renumber.applied {
        return CommandOutcome::Notice {
            message: tidy_message(&renumber.result, false),
        };
    }
    footnote_edits(input, renumber.result.edits, renumber.cursor)
}

fn fix_footnote_typos(input: &CommandInput<'_>) -> CommandOutcome {
    match fix_inline_typos(input.text, input.head()) {
        Some(fix) => footnote_edits(input, fix.edits, fix.cursor),
        None => CommandOutcome::Notice {
            message: fix_typos_message(0),
        },
    }
}

const NO_TABLE: &str = "Put the cursor in a table first.";

fn table_op(input: &CommandInput<'_>, doc: &Document, op: TableOp) -> CommandOutcome {
    match table_transaction(doc, input.tree, op, input.head(), now_ms()) {
        Some(transaction) => outcome(input, transaction),
        None => CommandOutcome::Notice {
            message: NO_TABLE.to_owned(),
        },
    }
}

fn copy_table(input: &CommandInput<'_>, write: fn(&Table) -> String) -> CommandOutcome {
    match Table::at(input.text, input.tree, input.head()) {
        Some(table) => CommandOutcome::Copy {
            text: write(&table),
        },
        None => CommandOutcome::Notice {
            message: NO_TABLE.to_owned(),
        },
    }
}

/// The code between the fences of the block holding the cursor.
fn copy_code_block(input: &CommandInput<'_>) -> CommandOutcome {
    let block = input
        .tree
        .path_at(input.head())
        .into_iter()
        .map(|id| input.tree.node(id))
        .find(|node| matches!(node.kind, NodeKind::CodeBlock(_)));
    let Some(block) = block else {
        return CommandOutcome::Notice {
            message: "Put the cursor in a code block first.".to_owned(),
        };
    };
    CommandOutcome::Copy {
        text: code_between_fences(&input.text[block.range.clone()]),
    }
}

fn code_between_fences(block: &str) -> String {
    let mut lines: Vec<&str> = block.lines().collect();
    let is_fence = |line: &&str| {
        let line = line.trim_start();
        line.starts_with("```") || line.starts_with("~~~")
    };
    if lines.first().is_some_and(is_fence) {
        lines.remove(0);
    }
    if lines.last().is_some_and(is_fence) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::NoteDocument;

    fn run_on(text: &str, start: u32, end: u32, id: &str) -> CommandOutcome {
        NoteDocument::new(text.into()).run_command(id.into(), TextRange { start, end })
    }

    #[test]
    fn bold_wraps_the_selection_and_keeps_it_selected() {
        let CommandOutcome::Edit {
            replacements,
            selection,
        } = run_on("make this bold", 5, 9, "format.bold")
        else {
            panic!("expected an edit");
        };
        let mut text: Vec<u16> = "make this bold".encode_utf16().collect();
        for replacement in replacements.iter().rev() {
            let range = replacement.range.start as usize..replacement.range.end as usize;
            text.splice(range, replacement.text.encode_utf16());
        }
        assert_eq!(String::from_utf16_lossy(&text), "make **this** bold");
        assert_eq!(selection, TextRange { start: 7, end: 11 });
    }

    #[test]
    fn offsets_come_back_in_utf16() {
        let CommandOutcome::Edit { replacements, .. } = run_on("𝜋 word", 3, 7, "format.italic")
        else {
            panic!("expected an edit");
        };
        assert_eq!(replacements[0].range.start, 3);
    }

    #[test]
    fn a_table_command_outside_a_table_says_so() {
        assert!(matches!(
            run_on("plain", 0, 0, "table.delete-row"),
            CommandOutcome::Notice { .. }
        ));
    }

    #[test]
    fn code_blocks_copy_without_their_fences() {
        let text = "```rust\nfn main() {}\n```\n";
        assert_eq!(
            run_on(text, 10, 10, "code.copy-block"),
            CommandOutcome::Copy {
                text: "fn main() {}".into()
            }
        );
    }

    #[test]
    fn unknown_commands_are_left_to_the_app() {
        assert_eq!(
            run_on("x", 0, 0, "tab.new"),
            CommandOutcome::NotANoteCommand
        );
        assert!(is_note_command("table.sort-ascending"));
        assert!(!is_note_command("tab.new"));
    }
}
