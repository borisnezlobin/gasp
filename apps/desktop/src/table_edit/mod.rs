//! The table editor: editing a table drawn as a grid.
//!
//! The Markdown source stays the truth. Typing in a cell is ordinary
//! typing in its source, with a typed `|` escaped and a line break made a
//! space; structural edits (rows, columns, alignment, sorting) rewrite the
//! table padded, as one undo step; and a table typed in is padded once the
//! caret leaves it, as part of the last typing step. "Edit table as
//! Markdown" shows one table's source until the caret leaves it.
//!
//! Keys are in [`keys`], selecting several cells in [`cells`], the row and
//! column handles and their drags in [`handles`], and drawing it all in
//! [`paint`].

pub mod cells;
pub mod handles;
pub mod keys;
pub mod menu;
pub mod paint;

use gasp_core::render::RevealMode;
use gasp_core::syntax::SyntaxKind;
use gasp_core::table::{
    CellPos, Table, TableOp, cell_paste_text, cell_text, insert_table, table_node_at,
    table_transaction,
};
use gasp_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::{ClipboardItem, Context};

use crate::editor::EditorView;

pub use handles::{Drag, HandleHover, Picked};

/// What the table editor keeps between events.
#[derive(Default)]
pub struct TableEditing {
    /// Where the table edited as Markdown starts, while it is.
    source: Option<usize>,
    /// Where the table typed in since the caret went into it starts, to
    /// pad when the caret leaves.
    typed_in: Option<usize>,
    /// Whether the selection takes in several cells, which then show
    /// rendered rather than revealing their markup.
    pub(crate) block_selected: bool,
    /// The handle under the pointer.
    pub(crate) hover: Option<HandleHover>,
    /// A handle pressed, and the row or column dragged by it.
    pub(crate) drag: Option<Drag>,
}

impl TableEditing {
    /// The offset whose table shows its source, while one is edited as
    /// Markdown: the caret's.
    pub fn source_table(&self, cursor: usize) -> Option<usize> {
        self.source.map(|_| cursor)
    }
}

/// Commands the table editor runs besides the structural edits.
pub const COMMANDS: [&str; 4] = [
    "table.insert",
    "table.copy-markdown",
    "table.copy-tsv",
    "table.edit-as-markdown",
];

impl EditorView {
    /// The table around `offset` when it's drawn as a grid: not while it
    /// shows its source, as all symbols do when shown.
    pub(crate) fn grid_table(&self, offset: usize) -> Option<Table> {
        let shows_source = self.table_edit.source.is_some()
            || self.reveal.mode_for(SyntaxKind::Table) == RevealMode::AlwaysShown;
        if shows_source || self.read_only || self.source.is_plain() {
            return None;
        }
        Table::at(self.source.text(), self.source.tree(), offset)
    }

    /// The grid table the caret is in, and its cell.
    pub(crate) fn caret_cell(&self) -> Option<(Table, CellPos)> {
        let table = self.grid_table(self.cursor())?;
        let cell = table.cell_at(self.cursor())?;
        Some((table, cell))
    }

    /// Makes a structural edit on the table around `at`: the caret's
    /// cell, or the cell a menu or handle names.
    pub fn run_table_op_at(&mut self, op: TableOp, at: usize, cx: &mut Context<Self>) {
        let transaction =
            table_transaction(self.state.doc(), self.source.tree(), op, at, self.now_ms());
        if let Some(transaction) = transaction {
            self.apply_transaction(transaction, cx);
        }
    }

    /// Makes a structural edit on the cell the caret is in, or the block
    /// of cells selected.
    pub fn run_table_op(&mut self, op: TableOp, cx: &mut Context<Self>) {
        let at = match self.cell_block() {
            Some(block) => block.anchor_offset,
            None => self.cursor(),
        };
        self.run_table_op_at(op, at, cx);
    }

    /// Whether a structural edit does anything where the caret is.
    pub fn table_op_applies(&self, op: TableOp, at: usize) -> bool {
        self.grid_table(at)
            .and_then(|table| Some((table.cell_at(at)?, table)))
            .is_some_and(|(cell, table)| op.applies(&table, cell))
    }

    /// Runs `table.*` commands that aren't structural edits. Answers
    /// whether `id` was one.
    pub(crate) fn run_table_command(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if let Some(op) = TableOp::from_command_id(id) {
            self.run_table_op(op, cx);
            return true;
        }
        self.run_table_command_at(id, self.cursor(), cx)
    }

    /// Runs a `table.*` command that isn't a structural edit on the table
    /// holding `at`, as the menu opened on one of its cells does.
    pub fn run_table_command_at(&mut self, id: &str, at: usize, cx: &mut Context<Self>) -> bool {
        match id {
            "table.insert" => self.run_edit(insert_table, cx),
            "table.copy-markdown" => self.copy_table(at, false, cx),
            "table.copy-tsv" => self.copy_table(at, true, cx),
            "table.edit-as-markdown" => {
                if !self.has_table_at(self.cursor()) {
                    self.select(at, at, cx);
                }
                self.edit_table_as_markdown(cx);
            }
            _ => return false,
        }
        true
    }

    /// Copies the table around `at`, padded Markdown or tab-separated.
    pub fn copy_table(&self, at: usize, tsv: bool, cx: &mut Context<Self>) {
        let Some(table) = Table::at(self.source.text(), self.source.tree(), at) else {
            return;
        };
        let text = match tsv {
            true => table.tsv(),
            false => table.format().text,
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// Shows the caret's table as Markdown until the caret leaves it, or
    /// back as a grid when it's already shown so.
    pub fn edit_table_as_markdown(&mut self, cx: &mut Context<Self>) {
        let tree = self.source.tree();
        let table = table_node_at(tree, self.cursor()).map(|id| tree.node(id).range.start);
        self.table_edit.source = match self.table_edit.source {
            Some(_) => None,
            None => table,
        };
        self.tables.clear();
        self.line_cache.clear();
        cx.notify();
    }

    /// Whether there's a table at `at` for the table commands to act on.
    pub fn has_table_at(&self, at: usize) -> bool {
        table_node_at(self.source.tree(), at).is_some()
    }

    /// Text typed into a cell as the cell can hold it, or `None` when the
    /// caret isn't in a cell or the text needs no change. Typing over
    /// several selected cells empties them first.
    pub(crate) fn cell_typing(&mut self, text: &str, cx: &mut Context<Self>) -> Option<String> {
        self.text_for_cell(text, cell_text, cx)
    }

    /// Text pasted into a cell as the cell can hold it: on one line, even
    /// when it's several lines or a whole line copied with its break.
    pub(crate) fn cell_pasting(&mut self, text: &str, cx: &mut Context<Self>) -> Option<String> {
        self.text_for_cell(text, cell_paste_text, cx)
    }

    fn text_for_cell(
        &mut self,
        text: &str,
        convert: fn(&str) -> String,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.selected_range().is_empty() && self.cell_block().is_some() {
            self.clear_cell_block(cx);
        }
        if !text.contains(['|', '\n', '\r']) {
            return None;
        }
        self.grid_table(self.cursor())?;
        Some(convert(text))
    }

    /// Notes that the caret's table was typed in, so it's padded when the
    /// caret leaves it.
    pub(crate) fn typed_in_table(&mut self) {
        if self.table_edit.source.is_some() {
            return;
        }
        let tree = self.source.tree();
        if let Some(id) = table_node_at(tree, self.cursor()) {
            self.table_edit.typed_in = Some(tree.node(id).range.start);
        }
    }

    /// Undo and redo take text away from under the typing, so there's
    /// nothing of it to pad.
    pub(crate) fn forget_table_typing(&mut self) {
        self.table_edit.typed_in = None;
    }

    /// Follows the caret out of tables: the table edited as Markdown goes
    /// back to a grid, and the table typed in is padded.
    pub(crate) fn caret_moved_in_tables(&mut self, cx: &mut Context<Self>) {
        self.table_edit.block_selected = self.cell_block().is_some();
        let tree = self.source.tree();
        let here = table_node_at(tree, self.cursor()).map(|id| tree.node(id).range.start);
        if self.table_edit.source.is_some() && self.table_edit.source != here {
            self.table_edit.source = None;
            self.tables.clear();
            self.line_cache.clear();
        }
        if let Some(start) = self
            .table_edit
            .typed_in
            .filter(|&start| Some(start) != here)
        {
            self.table_edit.typed_in = None;
            self.pad_table(start, cx);
        }
    }

    /// Pads the columns of the table starting at `start` as part of the
    /// last undo step, so undoing the typing takes the padding with it.
    fn pad_table(&mut self, start: usize, cx: &mut Context<Self>) {
        let text = self.source.text();
        let Some(table) = Table::at(text, self.source.tree(), start) else {
            return;
        };
        let formatted = table.format().text;
        if text[table.range.clone()] == formatted {
            return;
        }
        let changes = ChangeSet::replace(table.range.clone(), formatted);
        let transaction = Transaction::new(changes, Origin::command("table.format"), self.now_ms());
        self.apply_tidy(transaction, cx);
    }
}
