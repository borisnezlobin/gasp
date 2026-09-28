//! Selecting several cells. A selection from one cell of a grid table to
//! another — a drag across cells, Shift and an arrow from a cell's edge,
//! or a row or column handle's click — takes in the whole block of cells
//! between them, drawn as whole cells. Delete and Backspace empty them,
//! Copy gives them as tab-separated text (what spreadsheets paste), and
//! typing empties them and types into the first.

use std::ops::RangeInclusive;

use editor_core::table::{CellPos, Table, TableOp};
use gpui::{Bounds, ClipboardItem, Context, Pixels, point};

use crate::editor::EditorView;
use crate::frame::FrameLayout;

/// A block of cells selected in a grid table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellBlock {
    pub table: Table,
    pub rows: RangeInclusive<usize>,
    pub columns: RangeInclusive<usize>,
    /// Offsets in the cells the selection starts and ends in.
    pub anchor_offset: usize,
    pub head_offset: usize,
}

impl EditorView {
    /// The block of cells the selection takes in, when it runs from one
    /// cell of a grid table to another.
    pub(crate) fn cell_block(&self) -> Option<CellBlock> {
        let selection = self.state.selection().primary();
        if selection.is_empty() {
            return None;
        }
        let table = self.grid_table(selection.anchor)?;
        let inside = |at: usize| table.range.start <= at && at <= table.range.end;
        if !inside(selection.head) {
            return None;
        }
        let anchor = table.cell_at(selection.anchor)?;
        let head = table.cell_at(selection.head)?;
        if anchor == head {
            return None;
        }
        Some(CellBlock {
            rows: anchor.row.min(head.row)..=anchor.row.max(head.row),
            columns: anchor.column.min(head.column)..=anchor.column.max(head.column),
            anchor_offset: selection.anchor,
            head_offset: selection.head,
            table,
        })
    }

    /// Empties the selected cells, as one undo step, leaving the caret in
    /// the first.
    pub(crate) fn clear_cell_block(&mut self, cx: &mut Context<Self>) {
        let Some(block) = self.cell_block() else {
            return;
        };
        let first = CellPos::new(*block.rows.start(), *block.columns.start());
        let text = self.source.text();
        let at = block
            .table
            .content(text, first)
            .map_or(block.anchor_offset, |c| c.start);
        let op = TableOp::Clear {
            rows: (*block.rows.start(), *block.rows.end()),
            columns: (*block.columns.start(), *block.columns.end()),
        };
        self.run_table_op_at(op, at, cx);
    }

    /// Copies the selected cells as tab-separated text.
    pub(crate) fn copy_cell_block(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(block) = self.cell_block() else {
            return false;
        };
        let rows = *block.rows.start()..*block.rows.end() + 1;
        let columns = *block.columns.start()..*block.columns.end() + 1;
        let text = block.table.slice(rows, columns).tsv();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    /// Where the selected cells are on screen, one rectangle each.
    pub(crate) fn cell_block_rects(
        &self,
        frame: &FrameLayout,
        block: &CellBlock,
    ) -> Vec<Bounds<Pixels>> {
        let mut rects = Vec::new();
        for placed in &frame.lines {
            let Some(grid) = placed.visual.grid.as_ref() else {
                continue;
            };
            let in_table = block
                .table
                .row_lines
                .get(grid.index)
                .is_some_and(|line| line.start == placed.visual.start);
            if !in_table || !block.rows.contains(&grid.index) {
                continue;
            }
            for column in block.columns.clone() {
                let Some(cell) = grid.cells.get(column) else {
                    continue;
                };
                let left = frame.text_left + cell.x;
                rects.push(Bounds::from_corners(
                    point(left, placed.top),
                    point(left + cell.width, placed.bottom()),
                ));
            }
        }
        rects
    }
}
