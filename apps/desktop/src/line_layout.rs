//! A laid-out source line: soft-wrapped rows of text pieces and widgets,
//! block rows, and the decorations behind them, plus the geometry that
//! maps between byte offsets and positions within it.
//!
//! Offsets here are relative to the line's start, and x is measured from
//! the left edge of the text column. [`crate::preview::layout`] builds
//! these from the render planner's line plans.

use std::ops::Range;
use std::sync::Arc;

use gpui::{Hsla, Pixels, RenderImage, ShapedLine, SharedString, px};

pub use crate::preview::layout::{LayoutContext, LayoutResources, layout_line};

/// Shaped text and the line height it is centred in. A chunk of text is
/// shaped once; each soft-wrapped row shows a slice of it, so an edit
/// reshapes a long paragraph once rather than once per row.
#[derive(Clone, Debug)]
pub struct TextPiece {
    pub shaped: ShapedLine,
    pub line_height: Pixels,
    /// The bytes of `shaped` this piece shows.
    pub slice: Range<usize>,
    /// Where the slice starts in `shaped`.
    pub slice_x: Pixels,
    /// Fills behind parts of `shaped`, such as inline code.
    pub backgrounds: Vec<Background>,
}

/// A rounded fill behind some of a text piece's bytes. GPUI would fill
/// the whole line height with square ends; these hug the glyphs and are
/// painted under the selection.
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    /// Bytes of the shaped text.
    pub range: Range<usize>,
    pub color: Hsla,
    /// Room inside the fill at each end, which layout leaves clear:
    /// inline code and property chips have some, other fills none.
    pub padding: Pixels,
}

impl TextPiece {
    /// A piece showing all of `shaped`.
    pub fn whole(shaped: ShapedLine, line_height: Pixels) -> Self {
        Self {
            slice: 0..shaped.len(),
            shaped,
            line_height,
            slice_x: px(0.),
            backgrounds: Vec::new(),
        }
    }

    pub fn with_backgrounds(self, backgrounds: Vec<Background>) -> Self {
        Self {
            backgrounds,
            ..self
        }
    }

    pub fn is_whole(&self) -> bool {
        self.slice.start == 0 && self.slice.end == self.shaped.len()
    }

    /// The x of a byte within the slice, from the slice's left edge.
    pub fn x_for_index(&self, index: usize) -> Pixels {
        self.shaped.x_for_index(self.slice.start + index) - self.slice_x
    }

    /// The byte within the slice closest to `x`.
    pub fn closest_index_for_x(&self, x: Pixels) -> usize {
        let index = self.shaped.closest_index_for_x(x + self.slice_x);
        index.clamp(self.slice.start, self.slice.end) - self.slice.start
    }
}

/// What a piece draws.
#[derive(Clone, Debug)]
pub enum PieceContent {
    Text(Box<TextPiece>),
    Image {
        image: Arc<RenderImage>,
        radius: Pixels,
    },
    Icon {
        path: SharedString,
        color: Hsla,
    },
    Quad {
        color: Hsla,
        radius: Pixels,
    },
    /// A task's box, drawn at the piece's left edge, as wide as it is
    /// tall.
    Checkbox {
        checked: bool,
    },
    /// The block an empty snippet tab stop waits in. The caret at its
    /// offset stands in its middle.
    TabStop,
}

/// What clicking a piece does besides placing the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    Text,
    /// A widget drawn instead of its source; the cursor snaps to its edges.
    Widget,
    /// Toggles the task whose `[ ]` marker is at these document offsets.
    Checkbox {
        marker: Range<usize>,
    },
    /// Folds or unfolds the callout whose header token starts here.
    Fold {
        header: usize,
        folded: bool,
    },
    /// A link card: a click puts the cursor in its source, Mod+click
    /// opens the page.
    Card {
        url: String,
    },
    /// Opens this address, as a link card's Open button does. Drawn only
    /// while the pointer is over its card.
    Link {
        url: String,
    },
}

impl Hit {
    /// Whether a click here acts rather than placing the cursor, so the
    /// pointer shows a hand.
    pub fn is_control(&self) -> bool {
        matches!(
            self,
            Hit::Checkbox { .. } | Hit::Fold { .. } | Hit::Link { .. }
        )
    }
}

/// A laid-out piece of a row.
#[derive(Clone, Debug)]
pub struct Piece {
    /// Source bytes the piece stands for, relative to the line.
    pub range: Range<usize>,
    pub x: Pixels,
    /// Top edge, relative to the row's top.
    pub top: Pixels,
    pub width: Pixels,
    pub height: Pixels,
    pub content: PieceContent,
    pub hit: Hit,
}

impl Piece {
    pub fn is_text(&self) -> bool {
        matches!(self.content, PieceContent::Text(_))
    }

    pub fn right(&self) -> Pixels {
        self.x + self.width
    }

    fn text(&self) -> Option<&TextPiece> {
        match &self.content {
            PieceContent::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The offset closest to `x` within this piece.
    fn offset_at(&self, x: Pixels) -> usize {
        match self.text() {
            Some(text) => self.range.start + text.closest_index_for_x(x - self.x),
            None if x < self.x + self.width / 2. => self.range.start,
            None => self.range.end,
        }
    }
}

/// Where a row sits in its line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// Text and inline widgets; soft wrapping makes several of these.
    Text,
    /// A widget that takes the whole row, such as a rendered math block.
    Block,
    /// A widget drawn below the source, such as an image being edited.
    Below,
}

/// One visual row of a line.
#[derive(Clone, Debug)]
pub struct VisualRow {
    pub kind: RowKind,
    /// Top edge, relative to the line's top.
    pub top: Pixels,
    pub height: Pixels,
    /// The offsets the row covers. Text and block rows split the line
    /// between them without gaps.
    pub range: Range<usize>,
    /// Where a click past the row's end puts the cursor: before the
    /// space a soft wrap broke at, so the cursor stays on this row.
    pub soft_end: usize,
    /// The caret's box, relative to the row's top.
    pub caret_top: Pixels,
    pub caret_height: Pixels,
    /// Where an empty row's caret goes.
    pub left: Pixels,
    pub pieces: Vec<Piece>,
}

impl VisualRow {
    pub fn bottom(&self) -> Pixels {
        self.top + self.height
    }

    pub fn right(&self) -> Pixels {
        self.pieces
            .iter()
            .map(Piece::right)
            .fold(self.left, Pixels::max)
    }

    pub fn is_caret_row(&self) -> bool {
        self.kind != RowKind::Below
    }

    /// The x of an offset in this row. Offsets hidden between pieces snap
    /// to the next visible piece; offsets inside a widget snap to its left
    /// edge.
    pub fn x_for(&self, offset: usize) -> Pixels {
        if let Some(slot) = self.pieces.iter().find(|piece| {
            matches!(piece.content, PieceContent::TabStop) && piece.range.start == offset
        }) {
            return slot.x + slot.width / 2.;
        }
        let mut x = self.left;
        let mut previous_end = None;
        for piece in &self.pieces {
            if offset < piece.range.start {
                return if previous_end == Some(offset) {
                    x
                } else {
                    piece.x
                };
            }
            if let Some(text) = piece.text()
                && offset <= piece.range.end
            {
                return piece.x + text.x_for_index(offset - piece.range.start);
            }
            if offset < piece.range.end {
                return piece.x;
            }
            x = piece.right();
            previous_end = Some(piece.range.end);
        }
        x
    }

    /// The offset closest to `x` in this row.
    pub fn offset_for_x(&self, x: Pixels) -> usize {
        if self.pieces.first().is_none_or(|first| x < first.x) {
            return self.pieces.first().map_or(self.range.start, |first| {
                first.range.start.max(self.range.start)
            });
        }
        self.pieces
            .iter()
            .find(|piece| x < piece.right())
            .map_or(self.soft_end, |piece| piece.offset_at(x))
    }

    /// The piece under `x`, if any.
    pub fn piece_at(&self, x: Pixels) -> Option<&Piece> {
        self.pieces
            .iter()
            .find(|piece| piece.x <= x && x < piece.right())
    }
}

/// A rendered equation shown above its source while the cursor is in it.
#[derive(Clone, Debug)]
pub struct Overlay {
    /// The offset the overlay points at, relative to the line.
    pub anchor: usize,
    pub image: Arc<RenderImage>,
    pub width: Pixels,
    pub height: Pixels,
}

/// A fill behind a line, such as a callout's tint. Consecutive lines with
/// the same `group` are painted as one rounded shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub group: usize,
    pub left: Pixels,
    pub width: Pixels,
    pub color: Hsla,
}

/// A vertical bar beside a quote line.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub x: Pixels,
    pub width: Pixels,
    pub color: Hsla,
}

/// What is drawn behind and beside a line's rows.
#[derive(Clone, Debug, Default)]
pub struct LineDecor {
    pub surfaces: Vec<Surface>,
    /// Flat bands drawn over the surfaces behind this line alone, such as
    /// a highlighted code line.
    pub bands: Vec<Surface>,
    pub bars: Vec<Bar>,
    /// Pieces in the line's margin, such as code line numbers. Their tops
    /// are relative to the line's top.
    pub gutter: Vec<Piece>,
    /// Space above the line where a block starts or ends, outside the
    /// surface of a block that starts here.
    pub margin_top: Pixels,
}

/// A table row laid out as a row of its table's grid: where each cell
/// is and which of the line's rows hold its text. The rows of different
/// cells sit side by side, so finding a point or an offset goes through
/// the cells first.
#[derive(Clone, Debug, PartialEq)]
pub struct GridLine {
    /// The row's place in the table: 0 is the header.
    pub index: usize,
    /// How many rows the table has, header included.
    pub count: usize,
    pub cells: Vec<GridCell>,
    /// The table's left edge and width, from the text column's left.
    pub left: Pixels,
    pub width: Pixels,
    /// How tall the row is, its tallest cell and the padding around it.
    pub height: Pixels,
}

/// One cell of a [`GridLine`].
#[derive(Clone, Debug, PartialEq)]
pub struct GridCell {
    /// Left edge and width, from the text column's left.
    pub x: Pixels,
    pub width: Pixels,
    /// The indices of the line's rows the cell's text is set in.
    pub rows: Range<usize>,
    /// The cell's text relative to the line; `None` for a cell a short
    /// row leaves out.
    pub range: Option<Range<usize>>,
}

impl GridLine {
    /// The cell whose text holds `offset`, ends included, else the one
    /// nearest it.
    pub fn cell_for_offset(&self, offset: usize) -> Option<&GridCell> {
        let with_text = || self.cells.iter().filter(|cell| cell.range.is_some());
        let distance = |cell: &&GridCell| {
            let range = cell.range.clone().unwrap_or_default();
            range.start.saturating_sub(offset) + offset.saturating_sub(range.end)
        };
        with_text().min_by_key(distance)
    }

    /// The cell under `x`, else the nearest to it.
    pub fn cell_at_x(&self, x: Pixels) -> Option<&GridCell> {
        self.cells
            .iter()
            .find(|cell| x < cell.x + cell.width)
            .or(self.cells.last())
    }

    /// The column under `x`, if the table reaches there.
    pub fn column_at_x(&self, x: Pixels) -> Option<usize> {
        self.cells
            .iter()
            .position(|cell| cell.x <= x && x < cell.x + cell.width)
    }
}

/// One source line, laid out.
#[derive(Clone, Debug)]
pub struct VisualLine {
    pub line: usize,
    /// Document offset of the line's first byte.
    pub start: usize,
    pub len: usize,
    /// The whole line's height; zero when the line is collapsed.
    pub height: Pixels,
    pub rows: Vec<VisualRow>,
    pub decor: LineDecor,
    pub overlays: Vec<Overlay>,
    /// Where the cells are, when the line is a row of a table's grid.
    pub grid: Option<GridLine>,
}

impl VisualLine {
    pub fn end(&self) -> usize {
        self.start + self.len
    }

    pub fn is_collapsed(&self) -> bool {
        self.rows.is_empty()
    }

    /// The right edge of the widest row.
    pub fn width(&self) -> Pixels {
        self.rows
            .iter()
            .map(VisualRow::right)
            .fold(px(0.), Pixels::max)
    }

    /// Every piece, row by row.
    pub fn pieces(&self) -> impl Iterator<Item = &Piece> {
        self.rows.iter().flat_map(|row| row.pieces.iter())
    }

    /// Rows the caret can be in, in order.
    pub fn caret_rows(&self) -> impl Iterator<Item = (usize, &VisualRow)> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.is_caret_row())
    }

    /// The row an offset's caret is drawn in. At a soft wrap the caret
    /// goes to the start of the next row; in a grid row, to a row of the
    /// cell holding the offset.
    pub fn row_for_offset(&self, offset: usize) -> Option<usize> {
        if let Some(grid) = &self.grid {
            let rows = grid.cell_for_offset(offset)?.rows.clone();
            let mut within = rows.clone().filter(|&index| {
                let range = &self.rows[index].range;
                range.start <= offset && offset < range.end
            });
            return within.next().or(rows.last());
        }
        let mut last = None;
        for (index, row) in self.caret_rows() {
            if row.range.start <= offset && offset < row.range.end {
                return Some(index);
            }
            last = Some(index);
        }
        last
    }

    /// The x of a line-relative offset.
    pub fn x_for_offset(&self, offset: usize) -> Pixels {
        self.row_for_offset(offset)
            .map_or(px(0.), |row| self.rows[row].x_for(offset))
    }

    /// The row at `y` (relative to the line's top), clamped to the first
    /// and last rows.
    pub fn row_at_y(&self, y: Pixels) -> Option<usize> {
        if self.rows.is_empty() {
            return None;
        }
        let index = self
            .rows
            .iter()
            .position(|row| y < row.bottom())
            .unwrap_or(self.rows.len() - 1);
        Some(index)
    }

    /// The row under a point in a grid row: the cell under `x`, then its
    /// row at `y`, clamped to its first and last.
    fn grid_row_at(&self, grid: &GridLine, x: Pixels, y: Pixels) -> Option<usize> {
        let cell = grid.cell_at_x(x).filter(|cell| cell.range.is_some());
        let cell = cell.or_else(|| grid.cells.iter().rev().find(|cell| cell.range.is_some()))?;
        cell.rows
            .clone()
            .find(|&index| y < self.rows[index].bottom())
            .or(cell.rows.clone().last())
    }

    /// The offset under a point relative to the line's top-left. Points on
    /// a row below the source map to the end of the row above it.
    pub fn offset_for_point(&self, x: Pixels, y: Pixels) -> usize {
        if let Some(grid) = &self.grid {
            return self
                .grid_row_at(grid, x, y)
                .map_or(0, |index| self.rows[index].offset_for_x(x));
        }
        let Some(index) = self.row_at_y(y) else {
            return 0;
        };
        let row = &self.rows[index];
        match row.kind {
            RowKind::Below => row.range.start,
            _ => row.offset_for_x(x),
        }
    }

    /// The piece under a point relative to the line's top-left. In a
    /// block, where pieces stack, the one drawn last wins.
    pub fn piece_at_point(&self, x: Pixels, y: Pixels) -> Option<&Piece> {
        let index = match &self.grid {
            Some(grid) => self.grid_row_at(grid, x, y)?,
            None => self.row_at_y(y)?,
        };
        let row = &self.rows[index];
        let inside = y >= row.top && y < row.bottom();
        if !inside {
            return None;
        }
        let y = y - row.top;
        let stacked = (row.kind == RowKind::Block).then(|| {
            row.pieces.iter().rev().find(|piece| {
                piece.x <= x && x < piece.right() && piece.top <= y && y < piece.top + piece.height
            })
        });
        stacked.flatten().or_else(|| row.piece_at(x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget(range: Range<usize>, x: f32, width: f32) -> Piece {
        Piece {
            range,
            x: px(x),
            top: px(0.),
            width: px(width),
            height: px(10.),
            content: PieceContent::Quad {
                color: gpui::black(),
                radius: px(0.),
            },
            hit: Hit::Widget,
        }
    }

    fn row(kind: RowKind, top: f32, range: Range<usize>, pieces: Vec<Piece>) -> VisualRow {
        VisualRow {
            kind,
            top: px(top),
            height: px(10.),
            soft_end: range.end,
            range,
            caret_top: px(0.),
            caret_height: px(10.),
            left: px(0.),
            pieces,
        }
    }

    fn line(rows: Vec<VisualRow>) -> VisualLine {
        VisualLine {
            line: 0,
            start: 100,
            len: 20,
            height: px(30.),
            rows,
            decor: LineDecor::default(),
            overlays: Vec::new(),
            grid: None,
        }
    }

    #[test]
    fn wrapped_offsets_go_to_the_next_row() {
        let line = line(vec![
            row(RowKind::Text, 0., 0..10, vec![widget(0..10, 0., 50.)]),
            row(RowKind::Text, 10., 10..20, vec![widget(10..20, 0., 50.)]),
            row(RowKind::Below, 20., 20..20, vec![widget(20..20, 0., 50.)]),
        ]);
        assert_eq!(line.row_for_offset(9), Some(0));
        assert_eq!(line.row_for_offset(10), Some(1));
        assert_eq!(line.row_for_offset(20), Some(1));
        assert_eq!(line.row_at_y(px(15.)), Some(1));
        assert_eq!(line.row_at_y(px(99.)), Some(2));
        assert_eq!(line.offset_for_point(px(5.), px(25.)), 20);
    }

    #[test]
    fn widgets_snap_offsets_to_their_edges() {
        let row = row(
            RowKind::Text,
            0.,
            0..12,
            vec![widget(2..6, 10., 20.), widget(8..12, 40., 10.)],
        );
        assert_eq!(row.x_for(0), px(10.));
        assert_eq!(row.x_for(4), px(10.));
        assert_eq!(row.x_for(6), px(30.));
        assert_eq!(row.x_for(7), px(40.));
        assert_eq!(row.x_for(12), px(50.));
        assert_eq!(row.offset_for_x(px(12.)), 2);
        assert_eq!(row.offset_for_x(px(28.)), 6);
        assert_eq!(row.offset_for_x(px(99.)), 12);
        assert_eq!(row.offset_for_x(px(1.)), 2);
        assert!(row.piece_at(px(35.)).is_none());
        assert_eq!(row.piece_at(px(45.)).unwrap().range, 8..12);
    }

    #[test]
    fn grid_rows_find_offsets_and_points_by_cell() {
        // Two cells side by side: 2..5 at x 10, and 8..9 at x 60, the
        // second one wrapped over two rows.
        let mut line = line(vec![
            row(RowKind::Text, 0., 2..5, vec![widget(2..5, 10., 30.)]),
            row(RowKind::Text, 0., 8..8, vec![]),
            row(RowKind::Text, 10., 8..9, vec![widget(8..9, 60., 10.)]),
        ]);
        let cell = |x: f32, rows: Range<usize>, range: Range<usize>| GridCell {
            x: px(x),
            width: px(50.),
            rows,
            range: Some(range),
        };
        line.grid = Some(GridLine {
            index: 1,
            count: 2,
            cells: vec![cell(0., 0..1, 2..5), cell(50., 1..3, 8..9)],
            left: px(0.),
            width: px(100.),
            height: px(20.),
        });
        assert_eq!(line.row_for_offset(5), Some(0), "a cell's end is its own");
        assert_eq!(line.row_for_offset(9), Some(2));
        assert_eq!(
            line.row_for_offset(6),
            Some(0),
            "between cells is the nearer"
        );
        assert_eq!(line.offset_for_point(px(99.), px(15.)), 9);
        assert_eq!(line.offset_for_point(px(1.), px(15.)), 2);
        assert_eq!(line.piece_at_point(px(65.), px(15.)).unwrap().range, 8..9);
        let grid = line.grid.as_ref().unwrap();
        assert_eq!(grid.column_at_x(px(55.)), Some(1));
        assert_eq!(grid.column_at_x(px(120.)), None);
    }
}
