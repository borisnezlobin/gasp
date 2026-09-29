//! Moving the cursor by character, word, visual row, page and note. Up,
//! down, Home and End follow soft-wrapped rows, not source lines. In a
//! table drawn as a grid, Up and Down move through a cell's rows, then to
//! the cell above or below in the same column.

use gasp_core::motion;
use gpui::{Context, Pixels, Window, px};

use crate::editor::EditorView;
use crate::line_layout::VisualLine;

/// Where a cursor command moves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

/// A visual row: a line and the index of one of its caret rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RowPosition {
    line: usize,
    row: usize,
}

impl EditorView {
    /// Moves the cursor, or extends the selection when `extend` is set.
    pub fn apply_motion(
        &mut self,
        motion: Motion,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match motion {
            Motion::Up => self.move_rows(-1, extend, window, cx),
            Motion::Down => self.move_rows(1, extend, window, cx),
            Motion::PageUp => self.move_rows(-self.page_rows(), extend, window, cx),
            Motion::PageDown => self.move_rows(self.page_rows(), extend, window, cx),
            Motion::LineStart | Motion::LineEnd => {
                let target = self.row_edge(motion == Motion::LineEnd, window);
                self.goal_x = None;
                self.move_to(target, extend, cx);
            }
            _ => {
                let target = self.horizontal_target(motion, extend);
                self.goal_x = None;
                self.move_to(target, extend, cx);
            }
        }
    }

    /// Left and Right without Shift collapse a selection to its edge
    /// instead of moving past it.
    fn horizontal_target(&self, motion: Motion, extend: bool) -> usize {
        let selection = self.selected_range();
        let collapses = !extend && !selection.is_empty();
        let doc = self.doc();
        let at = self.cursor();
        match motion {
            Motion::Left if collapses => selection.start,
            Motion::Right if collapses => selection.end,
            Motion::Left => doc.prev_char_boundary(at),
            Motion::Right => doc.next_char_boundary(at),
            Motion::WordLeft => motion::word_left(doc, at),
            Motion::WordRight => motion::word_right(doc, at),
            Motion::DocStart => 0,
            _ => doc.len(),
        }
    }

    /// The start or end of the cursor's visual row.
    fn row_edge(&mut self, end: bool, window: &mut Window) -> usize {
        let position = self.cursor_row(window);
        let visual = self.visual_line(position.line, window);
        let Some(row) = visual.rows.get(position.row) else {
            let at = self.cursor();
            return if end {
                motion::line_end(self.doc(), at)
            } else {
                motion::line_start(self.doc(), at)
            };
        };
        let edge = if end { row.soft_end } else { row.range.start };
        visual.start + edge.min(visual.len)
    }

    /// Rows in one screenful, less one so a row of context stays.
    fn page_rows(&self) -> isize {
        let row_height = self.theme.body_line_height();
        let viewport = self
            .frame
            .as_ref()
            .map_or(px(0.), |frame| frame.bounds.size.height);
        let rows = (viewport / row_height).floor() as isize;
        (rows - 1).max(1)
    }

    /// The line and caret row the cursor is in.
    fn cursor_row(&mut self, window: &mut Window) -> RowPosition {
        let line = self.source.line_of(self.cursor());
        let visual = self.visual_line(line, window);
        let row = visual
            .row_for_offset(self.cursor() - visual.start)
            .unwrap_or(0);
        RowPosition { line, row }
    }

    /// Moves `delta` visual rows up or down, keeping the x the cursor had
    /// before the first vertical move.
    fn move_rows(
        &mut self,
        delta: isize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let goal_x = self.goal_column(window);
        let mut position = self.cursor_row(window);
        for _ in 0..delta.unsigned_abs() {
            match self.adjacent_row(position, delta > 0, goal_x, window) {
                Some(next) => position = next,
                None => {
                    let edge = if delta < 0 { 0 } else { self.doc().len() };
                    return self.move_to(edge, extend, cx);
                }
            }
        }
        let visual = self.visual_line(position.line, window);
        let offset = visual.start
            + visual
                .rows
                .get(position.row)
                .map_or(0, |row| row.offset_for_x(goal_x));
        self.move_to(offset.min(visual.end()), extend, cx);
        self.goal_x = Some(goal_x);
    }

    /// The caret row above or below, skipping collapsed lines. Going
    /// into a table row, it's a row of the cell under `goal_x`.
    fn adjacent_row(
        &mut self,
        from: RowPosition,
        down: bool,
        goal_x: Pixels,
        window: &mut Window,
    ) -> Option<RowPosition> {
        let visual = self.visual_line(from.line, window);
        if let Some(row) = row_beside(&visual, from.row, down) {
            return Some(RowPosition {
                line: from.line,
                row,
            });
        }
        let mut line = from.line;
        loop {
            line = if down {
                Some(line + 1).filter(|next| *next < self.source.line_count())?
            } else {
                line.checked_sub(1)?
            };
            let visual = self.visual_line(line, window);
            if let Some(row) = entry_row(&visual, down, goal_x) {
                return Some(RowPosition { line, row });
            }
        }
    }

    fn goal_column(&mut self, window: &mut Window) -> Pixels {
        if let Some(goal) = self.goal_x {
            return goal;
        }
        let line = self.source.line_of(self.cursor());
        let visual = self.visual_line(line, window);
        visual.x_for_offset(self.cursor() - visual.start)
    }
}

/// The caret rows a move within a line goes through: a grid row's
/// cell's own rows, or all of them.
fn caret_row_indices(visual: &VisualLine, row: usize) -> Vec<usize> {
    let cell = visual
        .grid
        .as_ref()
        .and_then(|grid| grid.cells.iter().find(|cell| cell.rows.contains(&row)));
    match cell {
        Some(cell) => cell.rows.clone().collect(),
        None => visual.caret_rows().map(|(index, _)| index).collect(),
    }
}

/// The caret row above or below `row` in its line, if there's one.
fn row_beside(visual: &VisualLine, row: usize, down: bool) -> Option<usize> {
    let rows = caret_row_indices(visual, row);
    let at = rows.iter().position(|&candidate| candidate == row)?;
    match down {
        true => rows.get(at + 1).copied(),
        false => at
            .checked_sub(1)
            .and_then(|previous| rows.get(previous).copied()),
    }
}

/// The row a move from above (or below) lands on in a line: its first
/// (or last) caret row, or in a grid row the first (or last) row of the
/// cell under `goal_x`.
fn entry_row(visual: &VisualLine, down: bool, goal_x: Pixels) -> Option<usize> {
    let rows: Vec<usize> = match &visual.grid {
        Some(grid) => grid
            .cell_at_x(goal_x)
            .filter(|cell| cell.range.is_some())
            .or_else(|| grid.cells.iter().find(|cell| cell.range.is_some()))?
            .rows
            .clone()
            .collect(),
        None => visual.caret_rows().map(|(index, _)| index).collect(),
    };
    match down {
        true => rows.first().copied(),
        false => rows.last().copied(),
    }
}
