//! What the table editor draws over the text: the header's fill and the
//! rules under rows, selected cells, the ring on the cell being edited,
//! the handles, and a row or column being dragged with the line where it
//! would drop.

use gpui::{Bounds, Hsla, Pixels, point, px, size};

use crate::editor::EditorView;
use crate::frame::{FrameLayout, PlacedLine};
use crate::icons::IconName;
use crate::table_edit::handles::{Picked, ScreenTable, column_handle, row_handle};
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
    /// The row's or column's own place, veiled while it's away.
    pub source: Bounds<Pixels>,
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
        if block.is_none() && focused {
            marks.ring = self.caret_cell_rect(frame);
        }
        let tables = self.tables_on_screen();
        self.handle_marks(&tables, &mut marks);
        marks.drag = self.drag_mark(frame, &tables);
        marks
    }

    /// The caret's cell, when the caret is in a grid table on screen.
    fn caret_cell_rect(&self, frame: &FrameLayout) -> Option<Bounds<Pixels>> {
        let placed = frame.line_containing(self.cursor())?;
        let grid = placed.visual.grid.as_ref()?;
        let cell = grid.cell_for_offset(self.cursor() - placed.visual.start)?;
        let left = frame.text_left + cell.x;
        Some(Bounds::from_corners(
            point(left, placed.top),
            point(left + cell.width, placed.bottom()),
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
    /// would drop.
    fn drag_mark(&self, frame: &FrameLayout, tables: &[ScreenTable]) -> Option<DragMark> {
        let drag = self.table_edit.drag.as_ref()?;
        let to = drag.to?;
        let table = tables.iter().find(|table| table.start == drag.table)?;
        let lines = frame.lines.iter().filter(|placed| {
            table.rows.iter().any(|row| row.1 == placed.top) && placed.visual.grid.is_some()
        });
        let width = self.theme.table.drop_indicator_width;
        let (source, lines, indicator) = match drag.picked {
            Picked::Row(row) => {
                let placed = lines
                    .into_iter()
                    .find(|placed| placed.visual.grid.as_ref().is_some_and(|g| g.index == row))?;
                let mut lifted = placed.clone();
                lifted.top += drag.at.y - drag.from.y;
                let source = Bounds::new(
                    point(table.left, placed.top),
                    size(table.width, placed.visual.height),
                );
                let indicator = (to != row).then(|| row_indicator(table, row, to, width));
                (source, vec![lifted], indicator.flatten())
            }
            Picked::Column(column) => {
                let dx = drag.at.x - drag.from.x;
                let (x, column_width) = *table.columns.get(column)?;
                let lifted = lines
                    .filter_map(|placed| column_copy(placed, column, dx))
                    .collect();
                let source = Bounds::from_corners(
                    point(x, table.top()),
                    point(x + column_width, table.bottom()),
                );
                let indicator = (to != column).then(|| column_indicator(table, column, to, width));
                (source, lifted, indicator.flatten())
            }
        };
        let offset = match drag.picked {
            Picked::Row(_) => point(Pixels::ZERO, drag.at.y - drag.from.y),
            Picked::Column(_) => point(drag.at.x - drag.from.x, Pixels::ZERO),
        };
        Some(DragMark {
            card: Bounds {
                origin: source.origin + offset,
                ..source
            },
            source,
            lines,
            indicator,
        })
    }
}

/// The header's fill and the rule under each row of every grid table.
fn grid_fills(frame: &FrameLayout, theme: &Theme) -> Vec<(Bounds<Pixels>, Hsla)> {
    let look = &theme.table;
    let mut fills = Vec::new();
    for placed in &frame.lines {
        let Some(grid) = placed.visual.grid.as_ref() else {
            continue;
        };
        let left = frame.text_left + grid.left;
        if grid.index == 0 {
            let header = Bounds::new(
                point(left, placed.top),
                size(grid.width, placed.visual.height),
            );
            fills.push((header, look.header_fill));
        }
        let rule = Bounds::new(
            point(left, placed.bottom() - look.rule_thickness),
            size(grid.width, look.rule_thickness),
        );
        fills.push((rule, look.rule));
    }
    fills
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

/// The line where a row dropped at `to` would go: above `to` when it
/// moves up, below it when it moves down.
fn row_indicator(
    table: &ScreenTable,
    from: usize,
    to: usize,
    width: Pixels,
) -> Option<Bounds<Pixels>> {
    let (top, bottom) = table.row(to)?;
    let y = if to < from { top } else { bottom };
    Some(Bounds::new(
        point(table.left, y - width / 2.),
        size(table.width, width),
    ))
}

/// The line where a column dropped at `to` would go.
fn column_indicator(
    table: &ScreenTable,
    from: usize,
    to: usize,
    width: Pixels,
) -> Option<Bounds<Pixels>> {
    let (x, column_width) = *table.columns.get(to)?;
    let edge = if to < from { x } else { x + column_width };
    Some(Bounds::from_corners(
        point(edge - width / 2., table.top()),
        point(edge + width / 2., table.bottom().max(table.top() + px(1.))),
    ))
}
