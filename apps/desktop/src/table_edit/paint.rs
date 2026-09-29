//! What the table editor draws over the text: the header's fill and the
//! rules under rows, selected cells, the ring on the cell being edited,
//! the handles, and a row or column being dragged with the line where it
//! would drop.

use gpui::{Bounds, Hsla, Pixels, point, px, size};

use crate::editor::EditorView;
use crate::frame::{FrameLayout, PlacedLine};
use crate::icons::IconName;
use crate::table_edit::handles::{Drag, Picked, ScreenTable, column_handle, row_handle};
use crate::theme::Theme;

/// A handle to draw: where, which icon, whether it's under the pointer or
/// held, and how far it has faded in.
#[derive(Clone, Debug, PartialEq)]
pub struct HandleMark {
    pub bounds: Bounds<Pixels>,
    pub icon: IconName,
    pub hot: bool,
    pub opacity: f32,
}

/// A row or column being dragged: its lifted copy and where it'd drop.
#[derive(Clone, Debug)]
pub struct DragMark {
    /// The raised card under the copy.
    pub card: Bounds<Pixels>,
    /// The row's or column's own place, veiled while it's away, when
    /// it's on screen.
    pub source: Option<Bounds<Pixels>>,
    /// The row's line, or the column's cells, moved with the pointer.
    pub lines: Vec<PlacedLine>,
    /// The line between rows or columns where it would drop.
    pub indicator: Option<Bounds<Pixels>>,
}

/// Everything the table editor draws in a frame.
#[derive(Clone, Debug, Default)]
pub struct TableMarks {
    /// Header fills and rules, drawn under the text.
    pub fills: Vec<(Bounds<Pixels>, Hsla)>,
    /// Selected cells, drawn instead of the selection's text rectangles.
    pub block: Option<Vec<Bounds<Pixels>>>,
    /// The ring around the cell the caret is in.
    pub ring: Option<Bounds<Pixels>>,
    pub handles: Vec<HandleMark>,
    pub drag: Option<DragMark>,
    /// Whether a handle is still fading in, so another frame is due.
    pub animating: bool,
}

impl EditorView {
    /// What the table editor draws over `frame`.
    pub(crate) fn table_marks(&self, frame: &FrameLayout, focused: bool) -> TableMarks {
        let mut marks = TableMarks {
            fills: grid_fills(frame, &self.theme),
            ..TableMarks::default()
        };
        if marks.fills.is_empty() {
            return marks;
        }
        let block = self
            .table_edit
            .block_selected
            .then(|| self.cell_block())
            .flatten();
        marks.block = block
            .as_ref()
            .map(|block| self.cell_block_rects(frame, block));
        // A drag hides the cell being edited, so nothing of it shows
        // around the lifted card.
        let dragging = self.table_edit.drag.is_some();
        if block.is_none() && focused && !dragging {
            marks.ring = self.caret_cell_rect(frame);
        }
        let tables = self.tables_on_screen();
        self.handle_marks(&tables, &mut marks);
        marks.drag = self.drag_mark(frame, &tables);
        marks
    }

    /// The caret's cell, when the caret is in a grid table on screen,
    /// taking in the rules on all four sides so its ring covers them.
    fn caret_cell_rect(&self, frame: &FrameLayout) -> Option<Bounds<Pixels>> {
        let placed = frame.line_containing(self.cursor())?;
        let grid = placed.visual.grid.as_ref()?;
        let cell = grid.cell_for_offset(self.cursor() - placed.visual.start)?;
        let look = &self.theme.table;
        let left = frame.text_left + cell.x;
        let table_right = frame.text_left + grid.left + grid.width;
        let right = (left + cell.width + look.rule_width).min(table_right);
        let rule_above = match grid.index {
            0 => px(0.),
            1 => look.header_rule_width,
            _ => look.rule_width,
        };
        Some(Bounds::from_corners(
            point(left, placed.top - rule_above),
            point(right, placed.bottom()),
        ))
    }

    /// The handles of the row and column under the pointer, fading in,
    /// and the one being held.
    fn handle_marks(&self, tables: &[ScreenTable], marks: &mut TableMarks) {
        let look = &self.theme.table;
        let (side, gap) = (look.handle_size, look.handle_gap);
        let held = self
            .table_edit
            .drag
            .as_ref()
            .map(|drag| (drag.table, drag.picked));
        let shown = self.table_edit.hover.as_ref().map(|hover| {
            let fade = look.handle_fade.as_secs_f32().max(f32::EPSILON);
            let opacity = hover.since.elapsed().as_secs_f32() / fade;
            (hover, opacity.min(1.))
        });
        for table in tables {
            let mut wanted: Vec<(Picked, f32)> = Vec::new();
            if let Some((hover, opacity)) = shown.filter(|(hover, _)| hover.table == table.start) {
                wanted.extend(hover.row.map(|row| (Picked::Row(row), opacity)));
                wanted.extend(hover.column.map(|column| (Picked::Column(column), opacity)));
                marks.animating |= opacity < 1.;
            }
            if let Some((_, picked)) = held.filter(|(start, _)| *start == table.start) {
                wanted.retain(|(other, _)| *other != picked);
                wanted.push((picked, 1.));
            }
            let on = shown.and_then(|(hover, _)| hover.on);
            for (picked, opacity) in wanted {
                let (bounds, icon) = match picked {
                    Picked::Row(row) => {
                        (row_handle(table, row, side, gap), IconName::DotsSixVertical)
                    }
                    Picked::Column(column) => {
                        (column_handle(table, column, side, gap), IconName::DotsSix)
                    }
                };
                let hot = on == Some(picked) || held.is_some_and(|held| held.1 == picked);
                marks.handles.extend(bounds.map(|bounds| HandleMark {
                    bounds,
                    icon,
                    hot,
                    opacity,
                }));
            }
        }
    }

    /// The dragged row or column, lifted under the pointer, and where it
    /// would drop: a line only where dropping would move it.
    fn drag_mark(&self, frame: &FrameLayout, tables: &[ScreenTable]) -> Option<DragMark> {
        let drag = self.table_edit.drag.as_ref()?;
        let to = drag.to?;
        let table = tables.iter().find(|table| table.start == drag.table)?;
        let look = &self.theme.table;
        let line = DropLine {
            width: look.drop_indicator_width,
            overhang: look.handle_size + look.handle_gap,
        };
        let mut mark = match drag.picked {
            Picked::Row(row) => row_drag(drag, table, row)?,
            Picked::Column(column) => column_drag(frame, drag, table, column)?,
        };
        mark.indicator = match drag.picked {
            _ if !drag.moves => None,
            Picked::Row(row) => row_indicator(table, row, to, line),
            Picked::Column(column) => column_indicator(table, column, to, line),
        };
        Some(mark)
    }
}

/// A dragged row: its line as pressed, on a card under the pointer.
fn row_drag(drag: &Drag, table: &ScreenTable, row: usize) -> Option<DragMark> {
    let pressed = drag.lifted.as_ref()?;
    let mut lifted = pressed.clone();
    lifted.top = drag.lifted_top(pressed.top);
    let height = pressed.visual.height;
    let source = table
        .row(row)
        .map(|(top, _)| Bounds::new(point(table.left, top), size(table.width, height)));
    Some(DragMark {
        card: Bounds::new(point(table.left, lifted.top), size(table.width, height)),
        source,
        lines: vec![lifted],
        indicator: None,
    })
}

/// A dragged column: its cells in the rows on screen, moved sideways
/// with the pointer.
fn column_drag(
    frame: &FrameLayout,
    drag: &Drag,
    table: &ScreenTable,
    column: usize,
) -> Option<DragMark> {
    let dx = drag.at.x - drag.from.x;
    let (x, column_width) = *table.columns.get(column)?;
    let lines = frame
        .lines
        .iter()
        .filter(|placed| {
            placed.visual.grid.is_some() && table.rows.iter().any(|row| row.1 == placed.top)
        })
        .filter_map(|placed| column_copy(placed, column, dx))
        .collect();
    let source = Bounds::from_corners(
        point(x, table.top()),
        point(x + column_width, table.bottom()),
    );
    Some(DragMark {
        card: Bounds {
            origin: source.origin + point(dx, Pixels::ZERO),
            ..source
        },
        source: Some(source),
        lines,
        indicator: None,
    })
}

/// The header's fill and the rules of every grid table on screen: one
/// under each row (heavier under the header), one above the header, and
/// one at each column's edges. All are square, so they meet cleanly.
fn grid_fills(frame: &FrameLayout, theme: &Theme) -> Vec<(Bounds<Pixels>, Hsla)> {
    let look = &theme.table;
    let mut fills = Vec::new();
    for placed in &frame.lines {
        let Some(grid) = placed.visual.grid.as_ref() else {
            continue;
        };
        let row = RowBox {
            left: frame.text_left + grid.left,
            top: placed.top,
            width: grid.width,
            height: placed.visual.height,
        };
        if grid.index == 0 {
            fills.push((row.band(row.top, row.height), look.header_fill));
            fills.push((row.band(row.top, look.rule_width), look.rule));
        }
        let edges = grid
            .cells
            .iter()
            .map(|cell| frame.text_left + cell.x)
            .chain(std::iter::once(row.left + row.width - look.rule_width));
        fills.extend(edges.map(|x| (row.column_rule(x, look.rule_width), look.rule)));
        let (color, width) = match grid.index {
            0 => (look.header_rule, look.header_rule_width),
            _ => (look.rule, look.rule_width),
        };
        fills.push((row.band(row.top + row.height - width, width), color));
    }
    fills
}

/// Where a grid row is on screen.
struct RowBox {
    left: Pixels,
    top: Pixels,
    width: Pixels,
    height: Pixels,
}

impl RowBox {
    /// A band across the row, `height` tall from `top`.
    fn band(&self, top: Pixels, height: Pixels) -> Bounds<Pixels> {
        Bounds::new(point(self.left, top), size(self.width, height))
    }

    /// A rule down the row, `width` wide from `x`.
    fn column_rule(&self, x: Pixels, width: Pixels) -> Bounds<Pixels> {
        Bounds::new(point(x, self.top), size(width, self.height))
    }
}

/// A copy of a column's cells in `placed`, moved `dx` right.
fn column_copy(placed: &PlacedLine, column: usize, dx: Pixels) -> Option<PlacedLine> {
    let grid = placed.visual.grid.as_ref()?;
    let cell = grid.cells.get(column)?;
    let mut copy = placed.clone();
    copy.visual.rows = placed.visual.rows[cell.rows.clone()]
        .iter()
        .cloned()
        .map(|mut row| {
            row.left += dx;
            for piece in &mut row.pieces {
                piece.x += dx;
            }
            row
        })
        .collect();
    copy.visual.grid = None;
    copy.visual.decor = Default::default();
    copy.visual.overlays = Vec::new();
    Some(copy)
}

/// How the line where a row or column would drop is drawn: its width,
/// and how far it runs past the table's edges. The card covers the table
/// and the line is under the card, so the ends out in the handles' gutter
/// keep it in sight wherever the card is.
#[derive(Clone, Copy)]
struct DropLine {
    width: Pixels,
    overhang: Pixels,
}

/// The line where a row dropped at `to` would go: above `to` when it
/// moves up, below it when it moves down.
fn row_indicator(
    table: &ScreenTable,
    from: usize,
    to: usize,
    line: DropLine,
) -> Option<Bounds<Pixels>> {
    let (top, bottom) = table.row(to)?;
    let y = if to < from { top } else { bottom };
    Some(Bounds::from_corners(
        point(table.left - line.overhang, y - line.width / 2.),
        point(table.right() + line.overhang, y + line.width / 2.),
    ))
}

/// The line where a column dropped at `to` would go.
fn column_indicator(
    table: &ScreenTable,
    from: usize,
    to: usize,
    line: DropLine,
) -> Option<Bounds<Pixels>> {
    let (x, column_width) = *table.columns.get(to)?;
    let edge = if to < from { x } else { x + column_width };
    let bottom = table.bottom().max(table.top() + px(1.));
    Some(Bounds::from_corners(
        point(edge - line.width / 2., table.top() - line.overhang),
        point(edge + line.width / 2., bottom + line.overhang),
    ))
}
