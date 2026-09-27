//! Heights of every line, for scrolling and hit testing without laying out
//! the whole note.
//!
//! Lines wrap, so a line's true height needs shaping. Lines that have been
//! laid out keep their measured height; the rest carry an estimate from
//! their length and heading level. Visible lines are laid out every frame,
//! so estimates are corrected as they scroll into view.

use std::ops::Range;

use gpui::{Pixels, px};

use crate::preview::source::Source;
use crate::theme::Theme;

/// An average character's width as a share of the font size.
const AVERAGE_CHARACTER_WIDTH: f32 = 0.5;

/// Row heights for every line of a document.
#[derive(Clone, Debug, Default)]
pub struct LineMetrics {
    heights: Vec<Pixels>,
}

/// What an estimate depends on.
#[derive(Clone, Copy, Debug)]
pub struct Estimator<'a> {
    pub theme: &'a Theme,
    pub column_width: Pixels,
}

impl Estimator<'_> {
    /// A guess at a line's height before it has been laid out.
    pub fn estimate(&self, text: &str) -> Pixels {
        let theme = self.theme;
        let level = heading_level(text);
        let font_size = theme.font_size(level);
        let (line_height, space_above) = match level {
            0 => (theme.line_height(font_size), px(0.)),
            _ => (
                font_size * theme.heading_line_height_factor,
                font_size * theme.heading_space_above,
            ),
        };
        let text_width = font_size * AVERAGE_CHARACTER_WIDTH * text.len() as f32;
        let rows = (text_width / self.column_width.max(px(1.))).ceil().max(1.);
        line_height * rows + space_above
    }
}

/// Heading level from the line's `#` prefix.
pub fn heading_level(text: &str) -> u8 {
    let hashes = text.bytes().take_while(|byte| *byte == b'#').count();
    let spaced = matches!(text.as_bytes().get(hashes), Some(b' ') | None);
    if (1..=6).contains(&hashes) && spaced {
        hashes as u8
    } else {
        0
    }
}

impl LineMetrics {
    /// Estimates for every line.
    pub fn build(source: &Source, estimator: &Estimator<'_>) -> Self {
        let heights = (0..source.line_count())
            .map(|line| estimator.estimate(source.line_text(line)))
            .collect();
        Self { heights }
    }

    pub fn line_count(&self) -> usize {
        self.heights.len()
    }

    pub fn height(&self, line: usize) -> Pixels {
        self.heights.get(line).copied().unwrap_or_default()
    }

    /// Records a line's measured height.
    pub fn set(&mut self, line: usize, height: Pixels) {
        if let Some(slot) = self.heights.get_mut(line) {
            *slot = height;
        }
    }

    /// Replaces the rows for `old_lines` with estimates for `new_lines` of
    /// the edited source.
    pub fn splice(
        &mut self,
        old_lines: Range<usize>,
        new_lines: Range<usize>,
        source: &Source,
        estimator: &Estimator<'_>,
    ) {
        let len = self.heights.len();
        let old_lines = old_lines.start.min(len)..old_lines.end.min(len);
        let fresh = new_lines.map(|line| estimator.estimate(source.line_text(line)));
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

    /// The line whose rows contain `y` (document coordinates), and the
    /// line's top, clamped to the first and last lines. Collapsed lines
    /// are skipped.
    pub fn line_at_y(&self, y: Pixels) -> (usize, Pixels) {
        let mut top = px(0.);
        for (line, height) in self.heights.iter().enumerate() {
            if y < top + *height {
                return (line, top);
            }
            top += *height;
        }
        let last = self.heights.len().saturating_sub(1);
        (last, top - self.height(last))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(text: &str, width: f32) -> (LineMetrics, Theme) {
        let theme = Theme::default();
        let estimator = Estimator {
            theme: &theme,
            column_width: px(width),
        };
        let metrics = LineMetrics::build(&Source::new(text), &estimator);
        (metrics, theme.clone())
    }

    #[test]
    fn headings_and_long_lines_are_estimated_taller() {
        let long = "word ".repeat(100);
        let (metrics, theme) = metrics(&format!("# Title\nbody\n{long}"), 600.);
        let body = theme.body_line_height();
        assert!(metrics.height(0) > body);
        assert_eq!(metrics.height(1), body);
        assert!(metrics.height(2) >= body * 3.);
    }

    #[test]
    fn finds_lines_by_y() {
        let (metrics, theme) = metrics("a\nb\nc", 600.);
        let row = theme.body_line_height();
        assert_eq!(metrics.line_at_y(px(0.)), (0, px(0.)));
        assert_eq!(metrics.line_at_y(row * 1.5), (1, row));
        assert_eq!(metrics.line_at_y(row * 10.).0, 2);
        assert_eq!(metrics.top_of(2), row * 2.);
        assert_eq!(metrics.total_height(), row * 3.);
    }

    #[test]
    fn measured_heights_replace_estimates() {
        let (mut metrics, theme) = metrics("a\nb\nc", 600.);
        metrics.set(0, px(0.));
        assert_eq!(metrics.line_at_y(px(1.)), (1, px(0.)));
        assert_eq!(metrics.total_height(), theme.body_line_height() * 2.);
    }

    #[test]
    fn splice_updates_edited_rows() {
        let theme = Theme::default();
        let estimator = Estimator {
            theme: &theme,
            column_width: px(600.),
        };
        let mut metrics = LineMetrics::build(&Source::new("a\nb"), &estimator);
        let source = Source::new("a\n# b\nc");
        metrics.splice(1..2, 1..3, &source, &estimator);
        assert_eq!(metrics.line_count(), 3);
        assert!(metrics.height(1) > metrics.height(2));
    }

    #[test]
    fn heading_levels_need_a_space() {
        assert_eq!(heading_level("## a"), 2);
        assert_eq!(heading_level("#tag"), 0);
        assert_eq!(heading_level("####### seven"), 0);
    }
}
