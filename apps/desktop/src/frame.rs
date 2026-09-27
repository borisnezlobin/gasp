//! The lines laid out for the last frame, in window coordinates, and the
//! geometry the view needs from them: the caret, selection rectangles and
//! the bounds an input method places its candidate window at.

use std::ops::Range;

use gpui::{Bounds, Pixels, point, px, size};

use crate::line_layout::VisualLine;
use crate::theme::Theme;

/// A laid-out line and the window y of its row's top.
#[derive(Clone, Debug)]
pub struct PlacedLine {
    pub top: Pixels,
    pub visual: VisualLine,
}

impl PlacedLine {
    /// Top of the text, which sits at the bottom of a row grown by an image.
    pub fn text_top(&self) -> Pixels {
        self.top + self.visual.height - self.visual.text_height
    }

    pub fn contains_offset(&self, offset: usize) -> bool {
        self.visual.start <= offset && offset <= self.visual.end()
    }
}

/// Everything laid out for one frame.
#[derive(Clone, Debug)]
pub struct FrameLayout {
    pub bounds: Bounds<Pixels>,
    /// Window x where line text starts.
    pub text_left: Pixels,
    pub lines: Vec<PlacedLine>,
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
        let x = self.text_left + placed.visual.x_for_offset(offset - placed.visual.start);
        Some(Bounds::new(
            point(x, placed.text_top()),
            size(theme.cursor_width, placed.visual.text_height),
        ))
    }

    /// Bounds of a range on the line where it starts; used for the IME
    /// candidate window.
    pub fn range_bounds(&self, range: &Range<usize>) -> Option<Bounds<Pixels>> {
        let placed = self.line_containing(range.start)?;
        let visual = &placed.visual;
        let end = range.end.min(visual.end()).max(range.start);
        let left = self.text_left + visual.x_for_offset(range.start - visual.start);
        let right = self.text_left + visual.x_for_offset(end - visual.start);
        Some(Bounds::from_corners(
            point(left, placed.text_top()),
            point(right, placed.top + visual.height),
        ))
    }

    /// One rectangle per visible line the selection touches.
    pub fn selection_rects(&self, range: &Range<usize>, theme: &Theme) -> Vec<Bounds<Pixels>> {
        if range.is_empty() {
            return Vec::new();
        }
        self.lines
            .iter()
            .filter_map(|placed| self.selection_rect(placed, range, theme))
            .collect()
    }

    fn selection_rect(
        &self,
        placed: &PlacedLine,
        range: &Range<usize>,
        theme: &Theme,
    ) -> Option<Bounds<Pixels>> {
        let visual = &placed.visual;
        if range.end < visual.start || range.start > visual.end() {
            return None;
        }
        let from = range.start.max(visual.start) - visual.start;
        let to = range.end.min(visual.end()) - visual.start;
        let includes_newline = range.end > visual.end();
        let newline_width = if includes_newline {
            theme.newline_selection_width
        } else {
            px(0.)
        };
        let left = visual.x_for_offset(from);
        let right = visual.x_for_offset(to) + newline_width;
        (right > left).then(|| {
            Bounds::from_corners(
                point(self.text_left + left, placed.top),
                point(self.text_left + right, placed.top + visual.height),
            )
        })
    }

    /// The visible line whose row contains window y, clamped to the first
    /// and last visible lines.
    pub fn line_at_y(&self, y: Pixels) -> Option<&PlacedLine> {
        let first = self.lines.first()?;
        if y < first.top {
            return Some(first);
        }
        self.lines
            .iter()
            .find(|placed| y < placed.top + placed.visual.height)
            .or(self.lines.last())
    }
}
