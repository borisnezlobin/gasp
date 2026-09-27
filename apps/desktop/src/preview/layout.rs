//! Lays out one planned line: its frame, then its items into soft-wrapped
//! rows, then rows for widgets below it, then the ranges each row covers.

use std::ops::Range;

use editor_core::render::{LinePlan, LineStyle, StyleKey, WidgetKind};
use gpui::{Font, Pixels, TextRun, WindowTextSystem, px};

use crate::images::ImageStore;
use crate::line_layout::{
    Background, Hit, Piece, PieceContent, RowKind, TextPiece, VisualLine, VisualRow,
};
use crate::preview::code_highlight::{CodeHighlighter, LineSpans, spans_for_line};
use crate::preview::decor::{LineFrame, line_frame};
use crate::preview::items::{Item, LineItems, line_items, line_tone};
use crate::preview::math::MathStore;
use crate::preview::source::Source;
use crate::preview::wrap::{Chunk, Extent, RowBuilder, Shaper};
use crate::styling::{LineTone, run_font_size, text_run};
use crate::theme::Theme;

/// What every line of a frame is laid out against.
pub struct LayoutContext<'a> {
    pub source: &'a Source,
    pub theme: &'a Theme,
    /// Width of the text column.
    pub column_width: Pixels,
    /// The view's zoom, for sizes written in the note such as `|300`.
    pub zoom: f32,
    /// Device pixels per logical pixel, for rasterising math.
    pub scale_factor: f32,
    /// The IME composition, in document offsets.
    pub marked: Option<Range<usize>>,
    /// Whether code blocks number their lines unless a block says.
    pub code_line_numbers: bool,
}

/// Caches and the shaper a layout draws on.
pub struct LayoutResources<'a> {
    pub text_system: &'a WindowTextSystem,
    pub images: &'a mut ImageStore,
    pub math: &'a mut MathStore,
    pub code: &'a mut CodeHighlighter,
}

/// Lays out a planned line.
pub fn layout_line(
    plan: &LinePlan,
    context: &LayoutContext<'_>,
    resources: &mut LayoutResources<'_>,
) -> VisualLine {
    let mut line = VisualLine {
        line: plan.line,
        start: plan.range.start,
        len: plan.range.len(),
        height: px(0.),
        rows: Vec::new(),
        decor: Default::default(),
        overlays: Vec::new(),
    };
    if plan.collapsed {
        return line;
    }
    let mut layouter = LineLayouter::new(plan, context, resources);
    let items = line_items(plan);
    let mut builder = layouter.row_builder();
    layouter.place_items(&items, &mut builder);
    layouter.place_below(&items, &mut builder);
    let mut rows = builder.finish();
    assign_ranges(&mut rows, layouter.text);
    let bottom = rows.last().map_or(px(0.), VisualRow::bottom);
    layouter.number_line(&rows);
    line.overlays = layouter.overlays(&items);
    line.height = bottom + layouter.frame.pad_bottom;
    line.rows = rows;
    line.decor = std::mem::take(&mut layouter.frame.decor);
    line
}

/// Lays out one line; widget builders live in [`super::widgets`].
pub(super) struct LineLayouter<'a, 'b> {
    pub plan: &'a LinePlan,
    pub context: &'a LayoutContext<'a>,
    pub resources: &'a mut LayoutResources<'b>,
    pub text: &'a str,
    pub tone: LineTone,
    pub frame: LineFrame,
    /// Syntax colours when the line is code in a fenced block.
    pub code_spans: Option<LineSpans>,
}

impl<'a, 'b> LineLayouter<'a, 'b> {
    fn new(
        plan: &'a LinePlan,
        context: &'a LayoutContext<'a>,
        resources: &'a mut LayoutResources<'b>,
    ) -> Self {
        let frame = line_frame(
            plan,
            context.source,
            context.theme,
            context.column_width,
            context.code_line_numbers,
        );
        let code_spans = spans_for_line(plan, context.source, resources.code);
        Self {
            plan,
            context,
            resources,
            text: &context.source.text()[plan.range.clone()],
            tone: line_tone(plan),
            frame,
            code_spans,
        }
    }

    pub fn theme(&self) -> &'a Theme {
        self.context.theme
    }

    pub fn shaper(&self) -> Shaper<'b> {
        Shaper {
            text_system: self.resources.text_system,
        }
    }

    pub fn font_size(&self) -> Pixels {
        self.tone.font_size(self.theme())
    }

    pub fn line_height(&self) -> Pixels {
        self.font_size() * self.tone.line_height_factor(self.theme())
    }

    /// The extent of a line of text in `font` at `font_size`.
    pub fn strut(&self, font: &Font, font_size: Pixels, line_height: Pixels) -> Extent {
        let text_system = self.resources.text_system;
        let font_id = text_system.resolve_font(font);
        let ascent = text_system.ascent(font_id, font_size);
        let descent = text_system.descent(font_id, font_size).abs();
        let ascent = (line_height - ascent - descent) / 2. + ascent;
        Extent {
            ascent,
            descent: line_height - ascent,
        }
    }

    fn line_font(&self) -> Font {
        let run = text_run(1, &[], &self.tone, false, self.theme());
        run.font
    }

    fn row_builder(&self) -> RowBuilder {
        let strut = self.strut(&self.line_font(), self.font_size(), self.line_height());
        let limit = self.context.column_width - self.frame.right;
        let top = self.frame.pad_top + self.frame.decor.margin_top;
        RowBuilder::new(self.frame.left, limit, top, strut)
    }

    fn place_items(&mut self, line: &LineItems, builder: &mut RowBuilder) {
        let items = &line.items;
        let property = property_row(self.plan);
        if property == Some(false) {
            self.to_value_column(builder);
        }
        let mut index = self.place_indent(items, builder);
        if property == Some(true) && index == 0 && !items.is_empty() {
            index = self.place_item(items, 0, builder);
            self.to_value_column(builder);
        }
        let mut in_marker = true;
        while index < items.len() {
            let is_marker = matches!(
                items[index],
                Item::Inline {
                    kind: WidgetKind::ListBullet { .. } | WidgetKind::Checkbox { .. },
                    ..
                }
            );
            if in_marker && !is_marker && index > 0 {
                builder.hang_here();
            }
            in_marker &= is_marker;
            index = self.place_item(items, index, builder);
        }
    }

    /// Moves to where a property's value starts, leaving at least a gap
    /// after a long name.
    fn to_value_column(&self, builder: &mut RowBuilder) {
        let theme = self.theme();
        let column = self.frame.left + theme.property_key_width;
        builder.advance((column - builder.x()).max(theme.space_md));
    }

    /// Leading tabs and spaces become an indent, so tabs line up.
    fn place_indent(&mut self, items: &[Item], builder: &mut RowBuilder) -> usize {
        let columns = indent_columns(self.text, self.theme().tab_columns);
        if columns.0 == 0 {
            return 0;
        }
        let space = self.space_width();
        builder.advance(space * columns.0 as f32);
        builder.hang_here();
        let Some(Item::Text { range, .. }) = items.first() else {
            return 0;
        };
        if range.end <= columns.1 {
            return 1;
        }
        0
    }

    fn space_width(&self) -> Pixels {
        let run = text_run(1, &[], &self.tone, false, self.theme());
        self.shaper().shape(" ", self.font_size(), &[run]).width
    }

    fn place_item(&mut self, items: &[Item], index: usize, builder: &mut RowBuilder) -> usize {
        match &items[index] {
            Item::Text { .. } => {
                let (chunk, next) = self.collect_chunk(items, index);
                if !chunk.text.is_empty() {
                    let padding = self.chunk_padding(&chunk);
                    builder.advance(padding);
                    builder.push_chunk(&chunk, &self.shaper());
                    builder.advance(padding);
                }
                return next;
            }
            Item::Inline { range, kind } => self.place_inline(range, kind, builder),
            Item::Break { .. } => builder.break_row(),
            Item::Block { range, kind } => {
                let row = self.block_row(range, kind, builder);
                builder.push_row(row);
            }
        }
        index + 1
    }

    /// Joins adjacent text items at one size into a chunk.
    fn collect_chunk(&self, items: &[Item], first: usize) -> (Chunk, usize) {
        let Item::Text { range, styles } = &items[first] else {
            unreachable!("chunks start at text");
        };
        let font_size = run_font_size(styles, &self.tone, self.theme());
        let indent_end = indent_columns(self.text, self.theme().tab_columns).1;
        let start = range.start.max(indent_end).min(range.end);
        let mut end = start;
        let mut runs = Vec::new();
        let mut backgrounds = Vec::new();
        let mut next = first;
        while let Some(Item::Text { range, styles }) = items.get(next) {
            let same_size = run_font_size(styles, &self.tone, self.theme()) == font_size;
            if range.start > end.max(start) || !same_size && next > first {
                break;
            }
            let part = range.start.max(start)..range.end;
            let at = part.start.saturating_sub(start);
            let before = runs.len();
            self.push_runs(&mut runs, &part, styles);
            take_backgrounds(&mut runs[before..], at, styles, &mut backgrounds);
            end = part.end.max(end);
            next += 1;
        }
        let chunk = Chunk {
            range: start..end,
            text: self.text[start..end].replace('\t', " "),
            font_size,
            line_height: font_size * self.tone.line_height_factor(self.theme()),
            runs,
            backgrounds,
        };
        (chunk, next)
    }

    /// Room on each side of inline code, for its fill to reach into.
    fn chunk_padding(&self, chunk: &Chunk) -> Pixels {
        if chunk.backgrounds.iter().any(|background| background.padded) {
            self.theme().inline_code_padding
        } else {
            px(0.)
        }
    }

    /// Runs for `part`, split where the IME composition starts and ends
    /// and where code changes colour.
    fn push_runs(&self, runs: &mut Vec<TextRun>, part: &Range<usize>, styles: &[StyleKey]) {
        let marked = self.marked();
        let spans = self.code_spans.as_deref().unwrap_or_default();
        let mut cuts = vec![part.start, part.end];
        let edges = marked
            .iter()
            .chain(spans.iter().map(|(range, _)| range))
            .flat_map(|range| [range.start, range.end]);
        cuts.extend(edges.filter(|cut| part.contains(cut)));
        cuts.sort_unstable();
        cuts.dedup();
        for piece in cuts.windows(2) {
            let is_marked = marked
                .as_ref()
                .is_some_and(|marked| marked.start <= piece[0] && piece[1] <= marked.end);
            let len = piece[1] - piece[0];
            let mut run = text_run(len, styles, &self.tone, is_marked, self.theme());
            if let Some((_, kind)) = spans.iter().find(|(range, _)| range.contains(&piece[0])) {
                run.color = self.theme().code_color(*kind);
            }
            runs.push(run);
        }
    }

    /// The composition relative to the line.
    fn marked(&self) -> Option<Range<usize>> {
        let marked = self.context.marked.as_ref()?;
        let line = &self.plan.range;
        let start = marked.start.max(line.start);
        let end = marked.end.min(line.end);
        (start < end).then(|| start - line.start..end - line.start)
    }

    /// A text piece for `text`, shaped with one run.
    pub fn label(
        &self,
        text: &str,
        run: TextRun,
        font_size: Pixels,
        line_height: Pixels,
    ) -> (Piece, Extent) {
        let run = TextRun {
            len: text.len(),
            ..run
        };
        let shaped = self.shaper().shape(text, font_size, &[run]);
        let extent = Extent::of_text(&shaped, line_height);
        let piece = Piece {
            range: 0..0,
            x: px(0.),
            top: px(0.),
            width: shaped.width,
            height: line_height,
            content: PieceContent::Text(Box::new(TextPiece::whole(shaped, line_height))),
            hit: Hit::Widget,
        };
        (piece, extent)
    }

    fn number_line(&mut self, rows: &[VisualRow]) {
        let (Some(number), Some(first)) = (self.frame.line_number, rows.first()) else {
            return;
        };
        let theme = self.theme();
        let run = text_run(1, &[StyleKey::MarkupDimmed], &self.tone, false, theme);
        let (mut piece, extent) = self.label(
            &number.to_string(),
            run,
            self.font_size(),
            self.line_height(),
        );
        let baseline = first.top
            + first.caret_top
            + (first.caret_height - self.line_height()) / 2.
            + extent.ascent;
        piece.x = self.frame.left - theme.space_lg - piece.width;
        piece.top = baseline - extent.ascent;
        piece.range = 0..0;
        self.frame.decor.gutter.push(piece);
    }
}

/// Moves the fills of `runs`, which start `at` bytes into their chunk,
/// into `backgrounds`, joining one that continues the last.
pub(super) fn take_backgrounds(
    runs: &mut [TextRun],
    mut at: usize,
    styles: &[StyleKey],
    backgrounds: &mut Vec<Background>,
) {
    let padded = crate::styling::is_code(styles);
    for run in runs {
        let range = at..at + run.len;
        at = range.end;
        let Some(color) = run.background_color.take() else {
            continue;
        };
        match backgrounds.last_mut() {
            Some(last) if last.range.end == range.start && last.color == color => {
                last.range.end = range.end;
            }
            _ => backgrounds.push(Background {
                range,
                color,
                padded,
            }),
        }
    }
}

/// Whether the line is a property row, and whether it starts with a name.
fn property_row(plan: &LinePlan) -> Option<bool> {
    plan.line_styles.iter().find_map(|style| match style {
        LineStyle::Property { keyed } => Some(*keyed),
        _ => None,
    })
}

/// Columns of leading indentation, and the byte length they span.
fn indent_columns(text: &str, tab_columns: usize) -> (usize, usize) {
    let mut columns = 0;
    let mut bytes = 0;
    for character in text.chars() {
        match character {
            ' ' => columns += 1,
            '\t' => columns += tab_columns - columns % tab_columns,
            _ => break,
        }
        bytes += 1;
    }
    if bytes == text.len() {
        return (0, 0);
    }
    (columns, bytes)
}

/// Gives each caret row the offsets it covers: from where the previous one
/// ended to where the next one's first piece starts. A click past a
/// wrapped row's end lands before the space the wrap broke at.
fn assign_ranges(rows: &mut [VisualRow], text: &str) {
    let caret_rows: Vec<usize> = (0..rows.len())
        .filter(|&index| rows[index].is_caret_row())
        .collect();
    let first_starts: Vec<Option<usize>> = caret_rows
        .iter()
        .map(|&index| {
            rows[index]
                .pieces
                .iter()
                .map(|piece| piece.range.start)
                .min()
        })
        .collect();
    let mut start = 0;
    for (position, &index) in caret_rows.iter().enumerate() {
        let next_start = first_starts[position + 1..]
            .iter()
            .flatten()
            .next()
            .copied();
        let end = next_start.unwrap_or(text.len()).max(start);
        let wrapped = next_start.is_some() && rows[index].kind == RowKind::Text;
        let before_space = wrapped && text[..end].ends_with([' ', '\t']) && end > start;
        rows[index].range = start..end;
        rows[index].soft_end = if before_space { end - 1 } else { end };
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indentation_counts_tab_stops() {
        assert_eq!(indent_columns("\t- a", 4), (4, 1));
        assert_eq!(indent_columns("  \tb", 4), (4, 3));
        assert_eq!(indent_columns("plain", 4), (0, 0));
        assert_eq!(indent_columns("   ", 4), (0, 0));
    }
}
