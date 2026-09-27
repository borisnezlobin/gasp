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
    /// Running sums of `heights` as a Fenwick tree (1-based: entry `i`
    /// sums the `i & -i` heights ending at line `i - 1`), so a line's top
    /// and the line at a height take log time rather than a walk over the
    /// note, which a long note would pay every frame.
    sums: Vec<f64>,
}

/// The Fenwick tree over `heights`, built in one pass.
fn build_sums(heights: &[Pixels]) -> Vec<f64> {
    let mut sums = vec![0.; heights.len() + 1];
    for (line, height) in heights.iter().enumerate() {
        let index = line + 1;
        sums[index] += f64::from(f32::from(*height));
        let parent = index + (index & index.wrapping_neg());
        if parent < sums.len() {
            sums[parent] += sums[index];
        }
    }
    sums
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
        let heights: Vec<Pixels> = (0..source.line_count())
            .map(|line| estimator.estimate(source.line_text(line)))
            .collect();
        let sums = build_sums(&heights);
        Self { heights, sums }
    }

    pub fn line_count(&self) -> usize {
        self.heights.len()
    }

    pub fn height(&self, line: usize) -> Pixels {
        self.heights.get(line).copied().unwrap_or_default()
    }

    /// Records a line's measured height.
    pub fn set(&mut self, line: usize, height: Pixels) {
        let Some(slot) = self.heights.get_mut(line) else {
            return;
        };
        let delta = f64::from(f32::from(height)) - f64::from(f32::from(*slot));
        *slot = height;
        if delta != 0. {
            self.add(line, delta);
        }
    }

    /// Adds `delta` to line `line`'s running sums.
    fn add(&mut self, line: usize, delta: f64) {
        let mut index = line + 1;
        while index < self.sums.len() {
            self.sums[index] += delta;
            index += index & index.wrapping_neg();
        }
    }

    /// The height of the lines before `line`.
    fn prefix(&self, line: usize) -> f64 {
        let mut index = line.min(self.heights.len());
        let mut sum = 0.;
        while index > 0 {
            sum += self.sums[index];
            index &= index - 1;
        }
        sum
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
        // Typing within lines keeps the count: those lines update in
        // place, and only a change in the count rebuilds the sums.
        if old_lines.len() == new_lines.len() {
            for line in new_lines {
                self.set(line, estimator.estimate(source.line_text(line)));
            }
            return;
        }
        let fresh = new_lines.map(|line| estimator.estimate(source.line_text(line)));
        self.heights.splice(old_lines, fresh);
        self.sums = build_sums(&self.heights);
    }

    /// How many lines from `first` fit in `room` by their current
    /// heights, counting the one that crosses its bottom.
    pub fn lines_within(&self, first: usize, room: Pixels) -> usize {
        let mut used = px(0.);
        let rest = self.heights.get(first..).unwrap_or_default();
        let fitting = rest
            .iter()
            .take_while(|height| {
                used += **height;
                used < room
            })
            .count();
        (fitting + 1).min(rest.len())
    }

    pub fn total_height(&self) -> Pixels {
        px(self.prefix(self.heights.len()) as f32)
    }

    /// Top of a line's row in document coordinates.
    pub fn top_of(&self, line: usize) -> Pixels {
        px(self.prefix(line) as f32)
    }

    /// The line whose rows contain `y` (document coordinates), and the
    /// line's top, clamped to the first and last lines. Collapsed lines
    /// are skipped.
    pub fn line_at_y(&self, y: Pixels) -> (usize, Pixels) {
        let count = self.heights.len();
        if count == 0 {
            return (0, px(0.));
        }
        // Walks down the tree, passing every span that ends at or above
        // `y`; collapsed lines end where they start, so they're passed.
        let mut rest = f64::from(f32::from(y));
        let mut before = 0;
        let mut step = 1 << count.ilog2();
        while step > 0 {
            let next = before + step;
            if next <= count && self.sums[next] <= rest {
                before = next;
                rest -= self.sums[next];
            }
            step >>= 1;
        }
        let line = before.min(count - 1);
        (line, self.top_of(line))
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
    fn counts_the_lines_that_fit() {
        let (metrics, theme) = metrics("a\nb\nc\nd", 600.);
        let row = theme.body_line_height();
        assert_eq!(metrics.lines_within(0, row * 1.5), 2);
        assert_eq!(metrics.lines_within(1, row * 10.), 3);
        assert_eq!(metrics.lines_within(9, row), 0);
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
    fn running_sums_match_a_walk_over_the_lines() {
        let text: String = (0..300)
            .map(|line| match line % 7 {
                0 => "# Heading\n".to_string(),
                3 => format!("{}\n", "word ".repeat(line % 40)),
                _ => "line\n".to_string(),
            })
            .collect();
        let (mut metrics, _) = metrics(&text, 300.);
        for line in (0..metrics.line_count()).step_by(11) {
            metrics.set(line, px(3. + (line % 5) as f32));
        }
        let heights: Vec<Pixels> = (0..metrics.line_count())
            .map(|l| metrics.height(l))
            .collect();
        let walked = |line: usize| heights[..line].iter().fold(px(0.), |sum, h| sum + *h);
        let total = walked(heights.len());
        assert!((metrics.total_height() - total).abs() < px(0.01));
        for (line, height) in heights.iter().enumerate() {
            assert!((metrics.top_of(line) - walked(line)).abs() < px(0.01));
            let inside = walked(line) + *height / 2.;
            assert_eq!(metrics.line_at_y(inside).0, line);
        }
        assert_eq!(metrics.line_at_y(px(-5.)).0, 0);
        assert_eq!(metrics.line_at_y(total + px(100.)).0, heights.len() - 1);
    }

    #[test]
    fn an_edit_within_lines_keeps_the_sums_right() {
        let (mut metrics, theme) = metrics("a\nb\nc\nd", 600.);
        let estimator = Estimator {
            theme: &theme,
            column_width: px(600.),
        };
        let source = Source::new("a\n# b\nc\nd");
        metrics.splice(1..2, 1..2, &source, &estimator);
        let rebuilt = LineMetrics::build(&source, &estimator);
        assert_eq!(metrics.total_height(), rebuilt.total_height());
        assert_eq!(metrics.top_of(3), rebuilt.top_of(3));
    }

    #[test]
    fn heading_levels_need_a_space() {
        assert_eq!(heading_level("## a"), 2);
        assert_eq!(heading_level("#tag"), 0);
        assert_eq!(heading_level("####### seven"), 0);
    }
}
