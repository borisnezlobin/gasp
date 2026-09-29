//! The phone's table grid: a cell's text written back into the note, and
//! which of the table editor's structural edits apply to a cell, for its
//! menu. The edits themselves are the core's table commands.

use editor_core::table::{Table, TableOp, cell_text};

use crate::document::NoteDocument;
use crate::edits::{self, CommandInput, CommandOutcome};
use crate::offsets::TextRange;

const NO_TABLE: &str = "Put the cursor in a table first.";

#[uniffi::export]
impl NoteDocument {
    /// Writes `text` into the cell whose source holds `offset` (UTF-16),
    /// then pads the table so its columns line up, as one edit with the
    /// cursor after the cell's text. A typed `|` is escaped and a line
    /// break becomes a space, since a cell holds one line.
    pub fn set_table_cell(&self, offset: u32, text: String) -> CommandOutcome {
        let parsed = self.lock();
        let at = parsed.offsets.byte(offset);
        let Some(mut table) = Table::at(&parsed.text, &parsed.tree, at) else {
            return notice(NO_TABLE);
        };
        let Some(cell) = table.cell_at(at) else {
            return notice(NO_TABLE);
        };
        let row = &mut table.rows[cell.row];
        if row.len() <= cell.column {
            row.resize(cell.column + 1, String::new());
        }
        row[cell.column] = cell_text(&text);
        let formatted = table.format();
        let caret = table.range.start + formatted.content(cell).map_or(0, |content| content.end);
        let input = CommandInput {
            text: &parsed.text,
            tree: &parsed.tree,
            offsets: &parsed.offsets,
            selection: edits::selection(&parsed.offsets, TextRange::default()),
        };
        edits::edit_outcome(
            &input,
            vec![(table.range.clone(), formatted.text)],
            caret..caret,
        )
    }

    /// The table commands that do something to the cell at `offset`, in
    /// menu order: moving the header or deleting the only column don't.
    pub fn table_commands_at(&self, offset: u32) -> Vec<String> {
        let parsed = self.lock();
        let at = parsed.offsets.byte(offset);
        let Some(table) = Table::at(&parsed.text, &parsed.tree, at) else {
            return Vec::new();
        };
        let Some(cell) = table.cell_at(at) else {
            return Vec::new();
        };
        TableOp::commands()
            .filter(|(op, _)| op.applies(&table, cell))
            .map(|(_, id)| id.to_owned())
            .collect()
    }
}

fn notice(message: &str) -> CommandOutcome {
    CommandOutcome::Notice {
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "| a | b |\n| - | - |\n| 1 | 2 |\n";

    fn applied(text: &str, outcome: CommandOutcome) -> String {
        let CommandOutcome::Edit { replacements, .. } = outcome else {
            panic!("expected an edit, got {outcome:?}");
        };
        let mut units: Vec<u16> = text.encode_utf16().collect();
        for replacement in replacements.iter().rev() {
            let range = replacement.range.start as usize..replacement.range.end as usize;
            units.splice(range, replacement.text.encode_utf16());
        }
        String::from_utf16_lossy(&units)
    }

    #[test]
    fn a_cell_takes_new_text_and_the_table_is_padded() {
        let document = NoteDocument::new(TABLE.into());
        let in_second_cell = TABLE.find('2').unwrap() as u32;
        let outcome = document.set_table_cell(in_second_cell, "wide | cell".into());
        assert_eq!(
            applied(TABLE, outcome),
            "| a   | b            |\n| --- | ------------ |\n| 1   | wide \\| cell |\n"
        );
    }

    #[test]
    fn the_header_row_offers_no_row_deletion() {
        let document = NoteDocument::new(TABLE.into());
        let header = document.table_commands_at(2);
        assert!(!header.contains(&"table.delete-row".to_owned()));
        assert!(header.contains(&"table.insert-row-below".to_owned()));
        let body = document.table_commands_at(TABLE.find('1').unwrap() as u32);
        assert!(body.contains(&"table.delete-row".to_owned()));
        let with_paragraph = NoteDocument::new(format!("{TABLE}\nafter"));
        let after = TABLE.len() as u32 + 2;
        assert!(with_paragraph.table_commands_at(after).is_empty());
    }
}
