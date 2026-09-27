//! The `footnote.insert-or-jump` command.

use crate::document::{Document, Selection, SelectionRange};
use crate::footnotes::{FootnoteSettings, InsertOrJump, insert_or_jump};
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

pub const INSERT_OR_JUMP: &str = "footnote.insert-or-jump";

/// What the command did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FootnoteCommand {
    /// Apply this transaction; it moves the caret and may edit the text.
    Apply(Transaction),
    /// The caret is on a definition nothing references; show this notice.
    Notice(String),
}

/// Inserts the next numbered footnote, or jumps between a reference and its
/// definition, at the primary caret.
pub fn insert_or_jump_footnote(
    doc: &Document,
    selection: &Selection,
    settings: &FootnoteSettings,
    timestamp_ms: u64,
) -> FootnoteCommand {
    let text = doc.slice(0..doc.len());
    let caret = selection.primary().head;
    let origin = Origin::command(INSERT_OR_JUMP);
    match insert_or_jump(&text, caret, settings) {
        InsertOrJump::Jump { cursor } => FootnoteCommand::Apply(Transaction::select(
            Selection::cursor(cursor),
            origin,
            timestamp_ms,
        )),
        InsertOrJump::Edit { edits, cursor } => {
            let edits = edits
                .into_iter()
                .map(|edit| TextEdit::new(edit.range, edit.insert))
                .collect();
            let changes = ChangeSet::new(edits).unwrap_or_default();
            let caret = Selection::single(SelectionRange::cursor(cursor));
            FootnoteCommand::Apply(
                Transaction::new(changes, origin, timestamp_ms).with_selection(caret),
            )
        }
        InsertOrJump::Unreferenced { label } => {
            FootnoteCommand::Notice(crate::footnotes::unreferenced_message(&label))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::EditorState;

    #[test]
    fn inserts_then_jumps_back_and_forth() {
        let doc = Document::from("A claim.");
        let settings = FootnoteSettings::default();
        let mut state = EditorState::new(doc);
        let at_end = Transaction::select(Selection::cursor(8), Origin::Other("test".into()), 0);
        state.apply(at_end).unwrap();

        let FootnoteCommand::Apply(insert) =
            insert_or_jump_footnote(state.doc(), state.selection(), &settings, 1)
        else {
            panic!("expected an insert");
        };
        state.apply(insert).unwrap();
        let text = state.doc().slice(0..state.doc().len());
        assert!(text.starts_with("A claim.[^1]"), "{text}");
        assert!(text.contains("\n[^1]: "), "{text}");
        let in_definition = state.selection().primary().head;
        assert_eq!(in_definition, text.len());

        let FootnoteCommand::Apply(jump) =
            insert_or_jump_footnote(state.doc(), state.selection(), &settings, 2)
        else {
            panic!("expected a jump");
        };
        state.apply(jump).unwrap();
        let head = state.selection().primary().head;
        assert!((8..=12).contains(&head), "caret at {head}");
    }
}
