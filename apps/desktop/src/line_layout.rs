//! Lays out one source line as text pieces and inline image widgets, and
//! maps between byte offsets and x positions within it.

use std::ops::Range;
use std::sync::Arc;

use gpui::{
    Font, Hsla, Pixels, RenderImage, ShapedLine, SharedString, TextRun, UnderlineStyle,
    WindowTextSystem, px,
};

use crate::images::{ImageStore, display_size};
use crate::styling::{LineStyle, SpanKind, style_line};
use crate::theme::Theme;

/// A stretch of a line before shaping. Ranges are bytes within the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Text(Range<usize>),
    /// An image drawn over `range`, which is hidden. The range is empty
    /// when the source is revealed and the image follows it.
    Image {
        range: Range<usize>,
        target: String,
    },
}

/// What a laid-out piece draws.
#[derive(Clone, Debug)]
pub enum PieceContent {
    Text(Box<ShapedLine>),
    Image(Arc<RenderImage>),
}

/// A laid-out piece of a line at `x` from the line's left edge.
#[derive(Clone, Debug)]
pub struct Piece {
    pub range: Range<usize>,
    pub x: Pixels,
    pub width: Pixels,
    pub height: Pixels,
    pub content: PieceContent,
}

impl Piece {
    fn offset_at(&self, dx: Pixels) -> usize {
        match &self.content {
            PieceContent::Text(shaped) => self.range.start + shaped.closest_index_for_x(dx),
            PieceContent::Image(_) if dx < self.width / 2. => self.range.start,
            PieceContent::Image(_) => self.range.end,
        }
    }

    fn x_for(&self, offset: usize) -> Option<Pixels> {
        match &self.content {
            PieceContent::Text(shaped)
                if self.range.contains(&offset) || self.range.end == offset =>
            {
                Some(self.x + shaped.x_for_index(offset - self.range.start))
            }
            PieceContent::Image(_) if self.range.start < offset && offset < self.range.end => {
                Some(self.x)
            }
            _ => None,
        }
    }
}

/// One source line, laid out.
#[derive(Clone, Debug)]
pub struct VisualLine {
    pub line: usize,
    /// Document offset of the line's first byte.
    pub start: usize,
    pub len: usize,
    /// Height of the whole row, images included.
    pub height: Pixels,
    /// Height of a line of this row's text.
    pub text_height: Pixels,
    pub pieces: Vec<Piece>,
}

impl VisualLine {
    pub fn width(&self) -> Pixels {
        self.pieces
            .last()
            .map_or(px(0.), |piece| piece.x + piece.width)
    }

    /// X of a line-relative offset. Offsets hidden under an image snap to
    /// its left edge.
    pub fn x_for_offset(&self, offset: usize) -> Pixels {
        self.pieces
            .iter()
            .find_map(|piece| piece.x_for(offset))
            .unwrap_or_else(|| self.width())
    }

    /// The line-relative offset closest to `x`.
    pub fn offset_for_x(&self, x: Pixels) -> usize {
        self.pieces
            .iter()
            .find(|piece| x < piece.x + piece.width)
            .map_or(self.len, |piece| piece.offset_at(x - piece.x))
    }

    pub fn end(&self) -> usize {
        self.start + self.len
    }
}

/// What to lay out.
#[derive(Clone, Debug)]
pub struct LineInput<'a> {
    pub text: &'a str,
    pub line: usize,
    pub start: usize,
    pub row_height: Pixels,
    /// The cursor, relative to the line, when it is on this line.
    pub cursor: Option<usize>,
    /// The IME composition, relative to the line, clipped to it.
    pub marked: Option<Range<usize>>,
}

/// Splits a line into text and image segments. An image's source is
/// hidden unless the cursor touches it.
pub fn segments(style: &LineStyle, len: usize, cursor: Option<usize>) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut text_start = 0;
    for span in style.spans.iter().filter(|span| span.is_image()) {
        let SpanKind::Image { target } = &span.kind else {
            continue;
        };
        let revealed = cursor.is_some_and(|at| span.range.start <= at && at <= span.range.end);
        let hidden = if revealed {
            span.range.end..span.range.end
        } else {
            span.range.clone()
        };
        push_text(&mut segments, text_start..hidden.start);
        segments.push(Segment::Image {
            range: hidden.clone(),
            target: target.clone(),
        });
        text_start = hidden.end;
    }
    push_text(&mut segments, text_start..len);
    segments
}

fn push_text(segments: &mut Vec<Segment>, range: Range<usize>) {
    if !range.is_empty() {
        segments.push(Segment::Text(range));
    }
}

/// Shapes a line with GPUI's text system.
pub fn layout_line(
    input: &LineInput<'_>,
    theme: &Theme,
    images: &mut ImageStore,
    text_system: &WindowTextSystem,
) -> VisualLine {
    let style = style_line(input.text);
    let font_size = theme.font_size(style.heading_level);
    let mut pieces = Vec::new();
    let mut x = px(0.);
    for segment in segments(&style, input.text.len(), input.cursor) {
        let piece = match segment {
            Segment::Text(range) => {
                let runs = text_runs(input.text, &range, &style, input.marked.as_ref(), theme);
                let text = SharedString::from(input.text[range.clone()].to_owned());
                let shaped = text_system.shape_line(text, font_size, &runs, None);
                text_piece(range, x, shaped, theme.line_height(font_size))
            }
            Segment::Image { range, target } => image_piece(range, x, images.image(&target), theme),
        };
        x = piece.x + piece.width;
        pieces.push(piece);
    }
    VisualLine {
        line: input.line,
        start: input.start,
        len: input.text.len(),
        height: input.row_height,
        text_height: theme.line_height(font_size),
        pieces,
    }
}

fn text_piece(range: Range<usize>, x: Pixels, shaped: ShapedLine, height: Pixels) -> Piece {
    Piece {
        range,
        x,
        width: shaped.width,
        height,
        content: PieceContent::Text(Box::new(shaped)),
    }
}

fn image_piece(range: Range<usize>, x: Pixels, image: Arc<RenderImage>, theme: &Theme) -> Piece {
    let shown = display_size(&image, theme);
    Piece {
        range,
        x: x + theme.image_gap,
        width: shown.width + theme.image_gap,
        height: shown.height,
        content: PieceContent::Image(image),
    }
}

/// Text runs for `range` of a line, split wherever the style or the
/// composition underline changes.
pub fn text_runs(
    text: &str,
    range: &Range<usize>,
    style: &LineStyle,
    marked: Option<&Range<usize>>,
    theme: &Theme,
) -> Vec<TextRun> {
    let cuts = cut_points(text, range, style, marked);
    cuts.windows(2)
        .map(|pair| run_for(pair[0]..pair[1], style, marked, theme))
        .collect()
}

fn cut_points(
    text: &str,
    range: &Range<usize>,
    style: &LineStyle,
    marked: Option<&Range<usize>>,
) -> Vec<usize> {
    let span_edges = style.spans.iter().flat_map(|span| {
        [
            span.range.start,
            span.content.start,
            span.content.end,
            span.range.end,
        ]
    });
    let marker_edges = style.heading_marker.iter().map(|marker| marker.end);
    let marked_edges = marked
        .into_iter()
        .flat_map(|marked| [marked.start, marked.end]);
    let mut cuts: Vec<usize> = [range.start, range.end]
        .into_iter()
        .chain(span_edges)
        .chain(marker_edges)
        .chain(marked_edges)
        .filter(|cut| range.start <= *cut && *cut <= range.end && text.is_char_boundary(*cut))
        .map(|cut| cut - range.start)
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts.into_iter().map(|cut| cut + range.start).collect()
}

fn run_for(
    segment: Range<usize>,
    style: &LineStyle,
    marked: Option<&Range<usize>>,
    theme: &Theme,
) -> TextRun {
    let span_kind = style.span_at(segment.start).map(|span| &span.kind);
    let is_code = matches!(span_kind, Some(SpanKind::Code));
    let in_code_content = is_code && !style.is_marker(segment.start);
    let is_marked =
        marked.is_some_and(|marked| marked.start <= segment.start && segment.end <= marked.end);
    TextRun {
        len: segment.len(),
        font: font_for(span_kind, style.heading_level, theme),
        color: color_for(style, segment.start, span_kind, theme),
        background_color: in_code_content.then_some(theme.code_background),
        underline: is_marked.then_some(UnderlineStyle {
            thickness: theme.composition_underline_thickness,
            color: Some(theme.composition_underline),
            wavy: false,
        }),
        strikethrough: None,
    }
}

fn font_for(span_kind: Option<&SpanKind>, heading_level: u8, theme: &Theme) -> Font {
    match span_kind {
        Some(SpanKind::Strong) => theme.strong_font(),
        Some(SpanKind::Emphasis) => theme.emphasis_font(),
        Some(SpanKind::Code) => theme.code_font(),
        _ if heading_level > 0 => theme.heading_font(),
        _ => theme.body_font(),
    }
}

fn color_for(
    style: &LineStyle,
    offset: usize,
    span_kind: Option<&SpanKind>,
    theme: &Theme,
) -> Hsla {
    if style.is_marker(offset) {
        return theme.markup_dimmed;
    }
    match span_kind {
        Some(SpanKind::Code) => theme.code_text,
        _ if style.heading_level > 0 => theme.heading_text,
        _ => theme.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(range: Range<usize>, target: &str) -> Segment {
        Segment::Image {
            range,
            target: target.to_owned(),
        }
    }

    #[test]
    fn hidden_images_replace_their_source() {
        let text = "a ![[p.png]] b";
        let style = style_line(text);
        assert_eq!(
            segments(&style, text.len(), None),
            vec![
                Segment::Text(0..2),
                image(2..12, "p.png"),
                Segment::Text(12..14)
            ]
        );
    }

    #[test]
    fn the_cursor_reveals_image_source() {
        let text = "a ![[p.png]] b";
        let style = style_line(text);
        assert_eq!(
            segments(&style, text.len(), Some(5)),
            vec![
                Segment::Text(0..12),
                image(12..12, "p.png"),
                Segment::Text(12..14)
            ]
        );
    }

    #[test]
    fn an_empty_line_has_no_segments() {
        assert!(segments(&LineStyle::default(), 0, Some(0)).is_empty());
    }

    #[test]
    fn runs_split_at_styles_and_composition() {
        let theme = Theme::default();
        let text = "ab **cd** `ef`";
        let style = style_line(text);
        let runs = text_runs(text, &(0..text.len()), &style, Some(&(1..2)), &theme);
        let lengths: Vec<usize> = runs.iter().map(|run| run.len).collect();
        assert_eq!(lengths, vec![1, 1, 1, 2, 2, 2, 1, 1, 2, 1]);
        assert_eq!(lengths.iter().sum::<usize>(), text.len());
        assert!(runs[1].underline.is_some());
        assert!(runs[0].underline.is_none());
        assert_eq!(runs[4].font.weight, theme.strong_font().weight);
        assert_eq!(runs[3].color, theme.markup_dimmed);
        assert_eq!(runs[8].font.family, theme.code_font().family);
        assert_eq!(runs[8].background_color, Some(theme.code_background));
    }

    #[test]
    fn heading_runs_use_the_heading_font() {
        let theme = Theme::default();
        let text = "# Title";
        let style = style_line(text);
        let runs = text_runs(text, &(0..text.len()), &style, None, &theme);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].color, theme.markup_dimmed);
        assert_eq!(runs[1].color, theme.heading_text);
        assert_eq!(runs[1].font.weight, theme.heading_font().weight);
    }

    #[test]
    fn runs_cover_only_the_requested_range() {
        let theme = Theme::default();
        let text = "x **bold** y";
        let style = style_line(text);
        let runs = text_runs(text, &(5..12), &style, None, &theme);
        assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), 7);
    }
}
