//! Row and column handles. Hovering a row of a grid table shows a handle
//! at its left edge, and hovering a column one above its header; each
//! fades in. Clicking a handle selects its row or column; dragging it
//! moves the row or column, lifted under the pointer, to where a line
//! between rows or columns shows it would go, as one undo step.

use std::time::Instant;

use editor_core::table::{CellPos, Table, TableOp};
use gpui::{Bounds, Context, Pixels, Point, point, size};

use crate::editor::EditorView;
use crate::frame::{FrameLayout, PlacedLine};

/// A row of a table, by its place (the header is 0), or a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picked {
    Row(usize),
    Column(usize),
}

/// The row and column of a table under the pointer, whose handles show.
#[derive(Clone, Debug, PartialEq)]
pub struct HandleHover {
    /// Where the table starts.
    pub table: usize,
    pub row: Option<usize>,
    pub column: Option<usize>,
    /// The handle right under the pointer.
    pub on: Option<Picked>,
    /// When the handles last changed, to fade them in from.
    pub since: Instant,
}

/// A handle pressed, and where its row or column would drop once the
/// press became a drag.
#[derive(Clone, Debug)]
pub struct Drag {
    pub table: usize,
    pub picked: Picked,
    pub from: Point<Pixels>,
    pub at: Point<Pixels>,
    /// Where it drops: the place it takes. `None` until the pointer has
    /// moved far enough to make the press a drag.
    pub to: Option<usize>,
    /// Whether dropping at `to` would move it: not onto its own place,
    /// and not the header into the body.
    pub moves: bool,
    /// A dragged row's line as it was when pressed, to lift under the
    /// pointer even once the note has scrolled it away.
    pub lifted: Option<PlacedLine>,
}

impl Drag {
    /// Where the lifted row's top goes: under the pointer, as far above
    /// it as the row's top was above the press.
    pub fn lifted_top(&self, pressed_top: Pixels) -> Pixels {
        self.at.y - (self.from.y - pressed_top)
    }
}

/// A grid table's rows and columns on screen, in window coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenTable {
    /// Where the table starts in the note.
    pub start: usize,
    pub left: Pixels,
    pub width: Pixels,
    /// Each row on screen: its place in the table, top and bottom.
    pub rows: Vec<(usize, Pixels, Pixels)>,
    /// Each column's left edge and width.
    pub columns: Vec<(Pixels, Pixels)>,
    /// How many rows the table has.
    pub count: usize,
}

impl ScreenTable {
    pub fn right(&self) -> Pixels {
        self.left + self.width
    }

    pub fn top(&self) -> Pixels {
        self.rows.first().map_or(Pixels::ZERO, |row| row.1)
    }

    pub fn bottom(&self) -> Pixels {
        self.rows.last().map_or(Pixels::ZERO, |row| row.2)
    }

    /// The header's top, when the header is on screen.
    pub fn header_top(&self) -> Option<Pixels> {
        self.rows.first().filter(|row| row.0 == 0).map(|row| row.1)
    }

    pub fn row(&self, index: usize) -> Option<(Pixels, Pixels)> {
        self.rows
            .iter()
            .find(|row| row.0 == index)
            .map(|row| (row.1, row.2))
    }

    /// The row whose band holds `y`, clamped to those on screen.
    fn row_at_y(&self, y: Pixels) -> Option<usize> {
        let found = self.rows.iter().find(|row| y < row.2);
        found.or(self.rows.last()).map(|row| row.0)
    }

    /// The column under `x`, clamped to the first and last.
    fn column_at_x(&self, x: Pixels) -> usize {
        self.columns
            .iter()
            .position(|column| x < column.0 + column.1)
            .unwrap_or(self.columns.len().saturating_sub(1))
    }
}

/// The handle beside a row, centred on it, left of the table.
pub fn row_handle(
    table: &ScreenTable,
    row: usize,
    side: Pixels,
    gap: Pixels,
) -> Option<Bounds<Pixels>> {
    let (top, bottom) = table.row(row)?;
    let y = top + (bottom - top - side) / 2.;
    Some(Bounds::new(
        point(table.left - gap - side, y),
        size(side, side),
    ))
}

/// The handle above a column, centred on it, above the header.
pub fn column_handle(
    table: &ScreenTable,
    column: usize,
    side: Pixels,
    gap: Pixels,
) -> Option<Bounds<Pixels>> {
    let top = table.header_top()?;
    let (x, width) = *table.columns.get(column)?;
    Some(Bounds::new(
        point(x + (width - side) / 2., top - gap - side),
        size(side, side),
    ))
}

/// Whether `placed` is row `row` of the table spanning `table`.
fn row_line(placed: &PlacedLine, table: &std::ops::Range<usize>, row: usize) -> bool {
    let in_table = table.contains(&placed.visual.start);
    in_table
        && placed
            .visual
            .grid
            .as_ref()
            .is_some_and(|grid| grid.index == row)
}

/// The grid tables in a frame, each from its run of grid rows.
pub fn screen_tables(
    frame: &FrameLayout,
    starts: impl Fn(usize) -> Option<usize>,
) -> Vec<ScreenTable> {
    let mut tables: Vec<ScreenTable> = Vec::new();
    let mut previous: Option<usize> = None;
    for placed in &frame.lines {
        let visual = &placed.visual;
        let Some(grid) = visual.grid.as_ref() else {
            if !visual.is_collapsed() {
                previous = None;
            }
            continue;
        };
        let continues = previous.is_some_and(|index| index + 1 == grid.index);
        if !continues {
            let Some(start) = starts(visual.start) else {
                continue;
            };
            let left = frame.text_left + grid.left;
            tables.push(ScreenTable {
                start,
                left,
                width: grid.width,
                rows: Vec::new(),
                columns: grid
                    .cells
                    .iter()
                    .map(|cell| (frame.text_left + cell.x, cell.width))
                    .collect(),
                count: grid.count,
            });
        }
        previous = Some(grid.index);
        if let Some(table) = tables.last_mut() {
            table.rows.push((grid.index, placed.top, placed.bottom()));
        }
    }
    tables
}

impl EditorView {
    /// The grid tables on screen in the last frame.
    pub fn tables_on_screen(&self) -> Vec<ScreenTable> {
        let Some(frame) = self.frame.as_ref() else {
            return Vec::new();
        };
        let tree = self.source.tree();
        screen_tables(frame, |offset| {
            let id = editor_core::table::table_node_at(tree, offset)?;
            Some(tree.node(id).range.start)
        })
    }

    /// Which handles show for the pointer at `position`: its table's row
    /// and column, and the handle right under it.
    fn handles_under(&self, position: Point<Pixels>) -> Option<HandleHover> {
        let look = &self.theme.table;
        let reach = look.handle_size + look.handle_gap;
        self.tables_on_screen().into_iter().find_map(|table| {
            let top = table.header_top().map_or(table.top(), |top| top - reach);
            let across = table.left - reach <= position.x && position.x < table.right();
            let down = top <= position.y && position.y < table.bottom();
            if !across || !down {
                return None;
            }
            let row = (position.y >= table.top())
                .then(|| table.row_at_y(position.y))
                .flatten();
            let column = (position.x >= table.left).then(|| table.column_at_x(position.x));
            let hit =
                |bounds: Option<Bounds<Pixels>>| bounds.is_some_and(|b| b.contains(&position));
            let (side, gap) = (look.handle_size, look.handle_gap);
            let on_row = row.filter(|&row| hit(row_handle(&table, row, side, gap)));
            let on_column = column.filter(|&column| hit(column_handle(&table, column, side, gap)));
            // Over the gap above the header, the column handle's there to
            // be reached, so the column under the pointer keeps it.
            let column = column.or(on_column);
            Some(HandleHover {
                table: table.start,
                row,
                column,
                on: on_row.map(Picked::Row).or(on_column.map(Picked::Column)),
                since: Instant::now(),
            })
        })
    }

    /// Follows the pointer over tables, fading handles in as their row or
    /// column comes under it. Answers whether the pointer is on a handle.
    pub(crate) fn hover_table_handles(
        &mut self,
        position: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.read_only || self.table_edit.drag.is_some() {
            return self.table_edit.drag.is_some();
        }
        let mut next = position.and_then(|position| self.handles_under(position));
        let previous = self.table_edit.hover.as_ref();
        let same_handles = |a: &HandleHover, b: &HandleHover| {
            (a.table, a.row, a.column) == (b.table, b.row, b.column)
        };
        if let (Some(next), Some(previous)) = (next.as_mut(), previous)
            && same_handles(next, previous)
        {
            next.since = previous.since;
        }
        let on = next.as_ref().is_some_and(|hover| hover.on.is_some());
        if next != self.table_edit.hover {
            self.table_edit.hover = next;
            cx.notify();
        }
        on
    }

    /// A press on a handle starts what may become a drag. Answers whether
    /// the press was on one.
    pub(crate) fn press_table_handle(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(hover) = self
            .handles_under(position)
            .filter(|hover| hover.on.is_some())
        else {
            return false;
        };
        let Some(picked) = hover.on else {
            return false;
        };
        self.table_edit.drag = Some(Drag {
            table: hover.table,
            picked,
            from: position,
            at: position,
            to: None,
            moves: false,
            lifted: None,
        });
        self.pointer_cursor = gpui::CursorStyle::ClosedHand;
        cx.notify();
        true
    }

    /// The pointer moved with a handle held: past a few pixels the press
    /// is a drag, and the row or column would drop where it points.
    pub(crate) fn drag_table_handle(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let threshold = self.theme.table.drag_threshold;
        let screen = self.tables_on_screen();
        let Some(drag) = self.table_edit.drag.as_mut() else {
            return;
        };
        drag.at = position;
        let moved = (position.x - drag.from.x)
            .abs()
            .max((position.y - drag.from.y).abs());
        if drag.to.is_none() && moved < threshold {
            return;
        }
        let Some(table) = screen.iter().find(|table| table.start == drag.table) else {
            return;
        };
        let to = match drag.picked {
            // Nothing goes above the header.
            Picked::Row(_) => table.row_at_y(position.y).unwrap_or(1).max(1),
            Picked::Column(_) => table.column_at_x(position.x),
        };
        drag.to = Some(to);
        let (start, picked, lifted) = (drag.table, drag.picked, drag.lifted.is_some());
        let moves = self.drop_moves(start, picked, to);
        // The row's line from the last frame, drawn since the press with
        // nothing revealed in it.
        let lifted = match picked {
            Picked::Row(row) if !lifted => self.row_line_on_screen(start, row),
            _ => None,
        };
        if let Some(drag) = self.table_edit.drag.as_mut() {
            drag.moves = moves;
            drag.lifted = drag.lifted.take().or(lifted);
        }
        cx.notify();
    }

    /// Row `row` of the table starting at `start`, as the last frame laid
    /// it out.
    fn row_line_on_screen(&self, start: usize, row: usize) -> Option<PlacedLine> {
        let table = self.grid_table(start)?.range.clone();
        let frame = self.frame.as_ref()?;
        frame
            .lines
            .iter()
            .find(|placed| row_line(placed, &table, row))
            .cloned()
    }

    /// Where the row or column `picked` starts, and the move dropping it
    /// at `to` makes, when that moves it at all.
    fn drop_op(&self, start: usize, picked: Picked, to: usize) -> Option<(usize, TableOp)> {
        let table = self.grid_table(start)?;
        let first = match picked {
            Picked::Row(row) => CellPos::new(row, 0),
            Picked::Column(column) => CellPos::new(0, column),
        };
        let at = table.content(self.source.text(), first)?.start;
        let op = match picked {
            Picked::Row(_) => TableOp::MoveRow { to },
            Picked::Column(_) => TableOp::MoveColumn { to },
        };
        op.applies(&table, table.cell_at(at)?).then_some((at, op))
    }

    fn drop_moves(&self, start: usize, picked: Picked, to: usize) -> bool {
        self.drop_op(start, picked, to).is_some()
    }

    /// Whether a row or column is held by its handle, and if so whether
    /// dropping it where the pointer is would move it, which is when the
    /// line showing where it'd go is drawn.
    pub fn table_drag_moves(&self) -> Option<bool> {
        self.table_edit.drag.as_ref().map(|drag| drag.moves)
    }

    /// Escape during a drag: the row or column goes back where it was,
    /// with nothing changed. Answers whether a drag was held.
    pub(crate) fn cancel_table_drag(&mut self, cx: &mut Context<Self>) -> bool {
        if self.table_edit.drag.take().is_none() {
            return false;
        }
        self.table_edit.hover = None;
        self.pointer_cursor = gpui::CursorStyle::IBeam;
        cx.notify();
        true
    }

    /// Lets go of a handle: a drag moves its row or column, a click
    /// selects it.
    pub(crate) fn release_table_handle(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.table_edit.drag.take() else {
            return;
        };
        cx.notify();
        let Some(to) = drag.to else {
            if let Some(table) = self.grid_table(drag.table) {
                self.pick(&table, drag.picked, cx);
            }
            return;
        };
        // Dropped where it was, or where it can't go: nothing changes.
        let Some((at, op)) = self.drop_op(drag.table, drag.picked, to) else {
            return;
        };
        self.run_table_op_at(op, at, cx);
        // The moved row or column stays selected where it landed.
        let moved = match drag.picked {
            Picked::Row(_) => Picked::Row(to),
            Picked::Column(_) => Picked::Column(to),
        };
        if let Some(table) = self.grid_table(self.cursor()) {
            self.pick(&table, moved, cx);
        }
    }

    /// Selects a whole row or column, as its handle's click does.
    fn pick(&mut self, table: &Table, picked: Picked, cx: &mut Context<Self>) {
        let last_row = table.row_count().saturating_sub(1);
        let last_column = table.column_count().saturating_sub(1);
        let (first, last) = match picked {
            Picked::Row(row) => (CellPos::new(row, 0), CellPos::new(row, last_column)),
            Picked::Column(column) => (CellPos::new(0, column), CellPos::new(last_row, column)),
        };
        let text = self.source.text();
        let (Some(from), Some(to)) = (table.content(text, first), table.content(text, last)) else {
            return;
        };
        self.select(from.start, to.end, cx);
    }

    /// The offset a table menu opened at `position` acts on: in the first
    /// cell of the row or column whose handle is there, or in the cell
    /// under it.
    pub fn table_offset_at(
        &mut self,
        position: Point<Pixels>,
        window: &gpui::Window,
    ) -> Option<usize> {
        if let Some(hover) = self.handles_under(position)
            && let Some(picked) = hover.on
        {
            let table = self.grid_table(hover.table)?;
            let first = match picked {
                Picked::Row(row) => CellPos::new(row, 0),
                Picked::Column(column) => CellPos::new(0, column),
            };
            return table
                .content(self.source.text(), first)
                .map(|content| content.start);
        }
        let over_grid = self.frame.as_ref()?.grid_at(position);
        if !over_grid {
            return None;
        }
        let offset = self.offset_for_point(position, window);
        self.grid_table(offset).map(|_| offset)
    }
}
