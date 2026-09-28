//! Keys in a grid table. Tab and Shift+Tab go to the next and previous
//! cell, selecting its text (Tab in the last cell adds a row); Enter goes
//! to the cell below (in the last row it adds one); Mod+Enter, off a
//! link, leaves the table onto a new line below it; Escape leaves it for
//! the line after it. Left and Right cross into the neighbouring cell at
//! a cell's edge and out of the table at its ends; Up and Down move by
//! rows (see [`crate::navigation`]). Backspace and Delete stop at a
//! cell's edge, so a pipe is never deleted.

use editor_core::document::Selection;
use editor_core::table::{CellPos, Table, TableOp};
use editor_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::Context;

use crate::editor::EditorView;

/// Which way a key moves through the cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Back,
    Forward,
}

impl EditorView {
    /// Runs a key's command the grid's way while the caret is in a cell.
    /// Answers false to let the command run as usual.
    pub(crate) fn run_table_key(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if self.read_only || self.caret_cell().is_none() {
            return false;
        }
        match id {
            "edit.indent" => self.tab_to_cell(Step::Forward, cx),
            "edit.outdent" => self.tab_to_cell(Step::Back, cx),
            "edit.newline" => self.enter_below(cx),
            "link.follow" => self.link_at(self.cursor()).is_none() && self.leave_below(cx),
            "cursor.left" | "select.left" => self.cross_cell(Step::Back, id == "select.left", cx),
            "cursor.right" | "select.right" => {
                self.cross_cell(Step::Forward, id == "select.right", cx)
            }
            "edit.delete-backward" | "edit.delete-forward" | "edit.cut" => {
                self.delete_in_cells(id, cx)
            }
            "edit.copy" => self.copy_cell_block(cx),
            _ => false,
        }
    }

    /// Tab and Shift+Tab: the next or previous cell with its text
    /// selected. Tab in the last cell adds a row and goes to its first.
    fn tab_to_cell(&mut self, step: Step, cx: &mut Context<Self>) -> bool {
        let Some((table, at)) = self.caret_cell() else {
            return false;
        };
        let text = self.source.text();
        match neighbour(&table, at, step) {
            Some(next) => {
                let content = table.content(text, next).unwrap_or(0..0);
                self.select(content.start, content.end, cx);
            }
            None if step == Step::Forward => {
                let first = CellPos::new(at.row, 0);
                let offset = table
                    .content(text, first)
                    .map_or(self.cursor(), |c| c.start);
                self.run_table_op_at(TableOp::InsertRowBelow, offset, cx);
            }
            None => {}
        }
        true
    }

    /// Enter: the cell below, caret at its text's end. In the last row a
    /// new row comes first.
    fn enter_below(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((table, at)) = self.caret_cell() else {
            return false;
        };
        let below = CellPos::new(at.row + 1, at.column);
        match table.content(self.source.text(), below) {
            Some(content) => self.select(content.end, content.end, cx),
            None => self.run_table_op(TableOp::InsertRowBelow, cx),
        }
        true
    }

    /// Mod+Enter: onto the line after the table, making one when the
    /// line after isn't blank.
    pub(crate) fn leave_below(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(table) = self.grid_table(self.cursor()) else {
            return false;
        };
        let doc = self.doc();
        let end = table.range.end;
        let next_line = doc.line_of_offset(end) + 1;
        let blank_after =
            next_line < doc.line_count() && doc.line_text(next_line).trim().is_empty();
        if blank_after {
            let at = doc.line_start(next_line);
            self.select(at, at, cx);
            return true;
        }
        let transaction = Transaction::new(
            ChangeSet::insert(end, "\n"),
            Origin::command("table.leave"),
            self.now_ms(),
        )
        .with_selection(Selection::cursor(end + 1));
        self.apply_transaction(transaction, cx);
        true
    }

    /// Escape: onto the line after the table, as Mod+Enter.
    pub(crate) fn escape_table(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(block) = self.cell_block() {
            let head = block.head_offset;
            self.select(head, head, cx);
            return true;
        }
        if self.caret_cell().is_none() {
            return false;
        }
        self.leave_below(cx)
    }

    /// Left and Right at a cell's edge: into the next cell's text or out
    /// of the table. Inside the text they move as anywhere.
    fn cross_cell(&mut self, step: Step, extend: bool, cx: &mut Context<Self>) -> bool {
        let collapsing = !extend && !self.selected_range().is_empty();
        let Some((table, at)) = self.caret_cell().filter(|_| !collapsing) else {
            return false;
        };
        let text = self.source.text();
        let head = self.cursor();
        let Some(content) = table.content(text, at) else {
            return false;
        };
        let at_edge = match step {
            Step::Back => head <= content.start,
            Step::Forward => head >= content.end,
        };
        if !at_edge {
            return false;
        }
        let target = match neighbour(&table, at, step) {
            Some(next) => table.content(text, next).map(|content| match step {
                Step::Back => content.end,
                Step::Forward => content.start,
            }),
            None => outside(&table, step, text.len()),
        };
        if let Some(target) = target {
            self.move_to(target, extend, cx);
        }
        true
    }

    /// Backspace and Delete stop at a cell's edge; over several selected
    /// cells they (and Cut) empty them.
    fn delete_in_cells(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if self.cell_block().is_some() {
            if id == "edit.cut" {
                self.copy_cell_block(cx);
            }
            self.clear_cell_block(cx);
            return true;
        }
        if id == "edit.cut" || !self.selected_range().is_empty() {
            return false;
        }
        let Some((table, at)) = self.caret_cell() else {
            return false;
        };
        let content = table.content(self.source.text(), at).unwrap_or(0..0);
        let head = self.cursor();
        match id {
            "edit.delete-backward" => head <= content.start,
            _ => head >= content.end,
        }
    }

    /// The deletion `around` the caret made to stay in its cell, as for
    /// the delete-word and delete-to-line commands.
    pub(crate) fn clamp_to_cell(&self, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        let Some((table, at)) = self.caret_cell() else {
            return range;
        };
        let Some(content) = table.content(self.source.text(), at) else {
            return range;
        };
        range.start.clamp(content.start, content.end)..range.end.clamp(content.start, content.end)
    }
}

/// The cell before or after `at` in reading order.
fn neighbour(table: &Table, at: CellPos, step: Step) -> Option<CellPos> {
    let columns = table.cells.get(at.row).map_or(0, Vec::len);
    match step {
        Step::Forward if at.column + 1 < columns => Some(CellPos::new(at.row, at.column + 1)),
        Step::Forward => (at.row + 1 < table.row_count()).then(|| CellPos::new(at.row + 1, 0)),
        Step::Back if at.column > 0 => Some(CellPos::new(at.row, at.column - 1)),
        Step::Back => {
            let row = at.row.checked_sub(1)?;
            let last = table.cells.get(row)?.len().checked_sub(1)?;
            Some(CellPos::new(row, last))
        }
    }
}

/// Just outside the table, going `step`: the end of the line before it
/// or the start of the line after it, when there's one.
fn outside(table: &Table, step: Step, len: usize) -> Option<usize> {
    match step {
        Step::Back => table.range.start.checked_sub(1),
        Step::Forward => Some(table.range.end + 1).filter(|&at| at <= len),
    }
}
