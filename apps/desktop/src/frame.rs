//! The lines laid out for the last frame, in window coordinates, and the
//! geometry the view needs from them: the caret, selection and highlight
//! rectangles, hit testing, and the bounds an input method places its
//! candidate window at. All of it works row by row, so it follows soft
//! wraps.

use std::ops::Range;

use gpui::{Bounds, Pixels, Point, point, px, size};

use crate::editor::HighlightKind;
use crate::line_layout::{Piece, VisualLine, VisualRow};
use crate::theme::Theme;

/// A laid-out line and the window y of its top.
#[derive(Clone, Debug)]
pub struct PlacedLine {
    pub top: Pixels,
    pub visual: VisualLine,
}

impl PlacedLine {
    /// Top of the first row's text.
    pub fn text_top(&self) -> Pixels {
        self.visual
            .caret_rows()
            .next()
            .map_or(self.top, |(_, row)| self.top + row.top + row.caret_top)
    }

    pub fn bottom(&self) -> Pixels {
        self.top + self.visual.height
    }

    pub fn contains_offset(&self, offset: usize) -> bool {
        self.visual.start <= offset && offset <= self.visual.end()
    }

    fn row_bounds(
        &self,
        row: &VisualRow,
        text_left: Pixels,
        left: Pixels,
        right: Pixels,
    ) -> Bounds<Pixels> {
        Bounds::from_corners(
            point(text_left + left, self.top + row.top),
            point(text_left + right, self.top + row.bottom()),
        )
    }
}

/// Everything laid out for one frame.
#[derive(Clone, Debug)]
pub struct FrameLayout {
    pub bounds: Bounds<Pixels>,
    /// Window x of the text column's left edge.
    pub text_left: Pixels,
    pub column_width: Pixels,
    pub lines: Vec<PlacedLine>,
    /// Highlight rectangles painted behind the text, by kind.
    pub highlights: Vec<(HighlightKind, Bounds<Pixels>)>,
}

impl FrameLayout {
    pub fn line(&self, line: usize) -> Option<&PlacedLine> {
        self.lines.iter().find(|placed| placed.visual.line == line)
    }

    pub fn line_containing(&self, offset: usize) -> Option<&PlacedLine> {
        self.lines
            .iter()
            .find(|placed| placed.contains_offset(offset))
    }

    /// The caret at a document offset, if its line is on screen.
    pub fn caret_bounds(&self, offset: usize, theme: &Theme) -> Option<Bounds<Pixels>> {
        let placed = self.line_containing(offset)?;
        let visual = &placed.visual;
        let relative = offset - visual.start;
        let row = &visual.rows[visual.row_for_offset(relative)?];
        Some(Bounds::new(
            point(
                self.text_left + row.x_for(relative),
                placed.top + row.top + row.caret_top,
            ),
            size(theme.cursor_width, row.caret_height),
        ))
    }

    /// Bounds of a range on the row where it starts; used for the IME
    /// candidate window.
    pub fn range_bounds(&self, range: &Range<usize>) -> Option<Bounds<Pixels>> {
        let placed = self.line_containing(range.start)?;
        let visual = &placed.visual;
        let start = range.start - visual.start;
        let row = &visual.rows[visual.row_for_offset(start)?];
        let end =
            (range.end.min(visual.end()) - visual.start).clamp(start, row.range.end.max(start));
        let left = self.text_left + row.x_for(start);
        let right = self.text_left + row.x_for(end);
        Some(Bounds::from_corners(
            point(left, placed.top + row.top + row.caret_top),
            point(right.max(left), placed.top + row.bottom()),
        ))
    }

    /// Rectangles covering a range, one per row it touches. A range that
    /// runs past a line's end also covers a sliver for the line break.
    pub fn range_rects(&self, range: &Range<usize>, theme: &Theme) -> Vec<Bounds<Pixels>> {
        if range.is_empty() {
            return Vec::new();
        }
        let mut rects = Vec::new();
        for placed in &self.lines {
            let visual = &placed.visual;
            if range.end < visual.start || range.start > visual.end() {
                continue;
            }
            let from = range.start.max(visual.start) - visual.start;
            let to = range.end.min(visual.end()) - visual.start;
            let breaks_line = range.end > visual.end();
            let last_row = visual.caret_rows().last().map(|(index, _)| index);
            for (index, row) in visual.caret_rows() {
                if to < row.range.start || from > row.range.end {
                    continue;
                }
                let left = row.x_for(from.max(row.range.start));
                let mut right = row.x_for(to.min(row.range.end));
                if breaks_line && Some(index) == last_row {
                    right += theme.newline_selection_width;
                }
                if right > left {
                    rects.push(placed.row_bounds(row, self.text_left, left, right));
                }
            }
        }
        rects
    }

    /// The visible line under window y, clamped to the first and last
    /// visible lines. Collapsed lines are never hit.
    pub fn line_at_y(&self, y: Pixels) -> Option<&PlacedLine> {
        let mut shown = self
            .lines
            .iter()
            .filter(|placed| !placed.visual.is_collapsed());
        let first = shown.clone().next()?;
        if y < first.top {
            return Some(first);
        }
        shown
            .clone()
            .find(|placed| y < placed.bottom())
            .or_else(|| shown.next_back())
    }

    /// The document offset under a window position.
    pub fn offset_at(&self, position: Point<Pixels>) -> Option<usize> {
        let placed = self.line_at_y(position.y)?;
        let x = position.x - self.text_left;
        let y = position.y - placed.top;
        Some(placed.visual.start + placed.visual.offset_for_point(x, y))
    }

    /// The piece under a window position, and the line it is in.
    pub fn piece_at(&self, position: Point<Pixels>) -> Option<(&PlacedLine, &Piece)> {
        let placed = self
            .lines
            .iter()
            .find(|placed| placed.top <= position.y && position.y < placed.bottom())?;
        let x = position.x - self.text_left;
        let piece = placed.visual.piece_at_point(x, position.y - placed.top)?;
        Some((placed, piece))
    }

    /// Whether a window position is over a table drawn as a grid.
    pub fn grid_at(&self, position: Point<Pixels>) -> bool {
        self.lines.iter().any(|placed| {
            let over_line = placed.top <= position.y && position.y < placed.bottom();
            placed.visual.grid.as_ref().is_some_and(|grid| {
                let x = position.x - self.text_left;
                over_line && grid.left <= x && x < grid.left + grid.width
            })
        })
    }

    /// Where the text column ends.
    pub fn text_right(&self) -> Pixels {
        self.text_left + self.column_width.max(px(0.))
    }
}
