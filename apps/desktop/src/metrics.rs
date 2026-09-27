//! Row heights for every line. Lines don't soft-wrap in the spike, so a
//! row's height follows from its style alone and needs no shaping, which
//! keeps scrolling and hit testing cheap on long notes.

use std::ops::Range;

use editor_core::document::Document;
use gpui::{Pixels, px};

use crate::styling::{heading_level, line_has_image};
use crate::theme::Theme;

/// The first visible line and where its row starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisibleLines {
    pub first: usize,
    /// Top of the first line's row, relative to the viewport top.
    pub first_top: Pixels,
    /// One past the last visible line.
    pub end: usize,
}

/// Row heights for every line of a document.
#[derive(Clone, Debug, Default)]
pub struct LineMetrics {
    heights: Vec<Pixels>,
}

/// The row height for a line of text.
pub fn row_height(text: &str, theme: &Theme) -> Pixels {
    let text_height = theme.line_height(theme.font_size(heading_level(text)));
    if line_has_image(text) {
        return text_height.max(theme.image_height + theme.image_gap * 2.);
    }
    text_height
}

impl LineMetrics {
    pub fn build(doc: &Document, theme: &Theme) -> Self {
        let heights = (0..doc.line_count())
            .map(|line| row_height(&doc.line_text(line), theme))
            .collect();
        Self { heights }
    }

    pub fn line_count(&self) -> usize {
        self.heights.len()
    }

    pub fn height(&self, line: usize) -> Pixels {
        self.heights.get(line).copied().unwrap_or_default()
    }

    /// Replaces the rows for `old_lines` with freshly measured rows for
    /// `new_lines` of the edited document.
    pub fn splice(
        &mut self,
        old_lines: Range<usize>,
        new_lines: Range<usize>,
        doc: &Document,
        theme: &Theme,
    ) {
        let old_lines =
            old_lines.start.min(self.heights.len())..old_lines.end.min(self.heights.len());
        let fresh = new_lines.map(|line| row_height(&doc.line_text(line), theme));
        self.heights.splice(old_lines, fresh);
    }

    pub fn total_height(&self) -> Pixels {
        self.heights
            .iter()
            .fold(px(0.), |sum, height| sum + *height)
    }

    /// Top of a line's row in document coordinates.
    pub fn top_of(&self, line: usize) -> Pixels {
        self.heights
            .iter()
            .take(line)
            .fold(px(0.), |sum, height| sum + *height)
    }

    /// The line whose row contains `y` (document coordinates), clamped to
    /// the first and last lines.
    pub fn line_at_y(&self, y: Pixels) -> usize {
        let mut top = px(0.);
        for (line, height) in self.heights.iter().enumerate() {
            if y < top + *height {
                return line;
            }
            top += *height;
        }
        self.heights.len().saturating_sub(1)
    }

    /// The lines that show in a viewport scrolled to `scroll_y`.
    pub fn visible(&self, scroll_y: Pixels, viewport_height: Pixels) -> VisibleLines {
        let first = self.line_at_y(scroll_y);
        let first_top = self.top_of(first) - scroll_y;
        let mut end = first;
        let mut bottom = first_top;
        while end < self.heights.len() && bottom < viewport_height {
            bottom += self.heights[end];
            end += 1;
        }
        VisibleLines {
            first,
            first_top,
            end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(text: &str) -> (LineMetrics, Theme) {
        let theme = Theme::default();
        (LineMetrics::build(&Document::from(text), &theme), theme)
    }

    #[test]
    fn headings_and_images_make_taller_rows() {
        let (metrics, theme) = metrics("# Title\nbody\nsee ![[a.png]] here");
        let body = theme.line_height(theme.body_font_size);
        assert!(metrics.height(0) > body);
        assert_eq!(metrics.height(1), body);
        assert_eq!(metrics.height(2), theme.image_height + theme.image_gap * 2.);
    }

    #[test]
    fn finds_lines_by_y() {
        let (metrics, theme) = metrics("a\nb\nc");
        let row = theme.line_height(theme.body_font_size);
        assert_eq!(metrics.line_at_y(px(0.)), 0);
        assert_eq!(metrics.line_at_y(row * 1.5), 1);
        assert_eq!(metrics.line_at_y(row * 10.), 2);
        assert_eq!(metrics.top_of(2), row * 2.);
        assert_eq!(metrics.total_height(), row * 3.);
    }

    #[test]
    fn visible_lines_cover_the_viewport() {
        let text = vec!["line"; 100].join("\n");
        let (metrics, theme) = metrics(&text);
        let row = theme.line_height(theme.body_font_size);
        let visible = metrics.visible(row * 10.5, row * 5.);
        assert_eq!(visible.first, 10);
        assert_eq!(visible.first_top, -row * 0.5);
        assert_eq!(visible.end, 16);
    }

    #[test]
    fn splice_updates_edited_rows() {
        let theme = Theme::default();
        let mut doc = Document::from("a\nb");
        let mut metrics = LineMetrics::build(&doc, &theme);
        doc = Document::from("a\n# b\nc");
        metrics.splice(1..2, 1..3, &doc, &theme);
        assert_eq!(metrics.line_count(), 3);
        assert!(metrics.height(1) > metrics.height(2));
    }
}
