//! Lays out one planned line: its frame, then its items into soft-wrapped
//! rows, then rows for widgets below it, then the ranges each row covers.

use std::ops::Range;

use gasp_core::render::{LinePlan, LineStyle, RevealSettings, StyleKey, WidgetKind};
use gasp_core::syntax::Alignment;
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
use crate::preview::table::TableStore;
use crate::preview::wrap::{Chunk, Extent, RowBuilder, Shaper};
use crate::styling::{LineTone, fill_padding, run_baseline_shift, run_font_size, text_run};
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
    /// What the lines were planned with, for planning a table's other
    /// rows to measure its columns.
    pub reveal: &'a RevealSettings,
    pub selections: &'a [Range<usize>],
}

/// Caches and the shaper a layout draws on.
pub struct LayoutResources<'a> {
    pub text_system: &'a WindowTextSystem,
    pub images: &'a mut ImageStore,
    pub math: &'a mut MathStore,
    pub code: &'a mut CodeHighlighter,
    pub tables: &'a mut TableStore,
}

/// Lays out a planned line.
pub fn layout_line(
    plan: &LinePlan,
    context: &LayoutContext<'_>,
    resources: &mut LayoutResources<'_>,
) -> VisualLine {
    let frame = frame_for(plan, context);
    let code_spans = spans_for_line(plan, context.source, resources.code);
    layout_framed(plan, frame, code_spans, context, resources)
}

/// A planned line's frame: its insets, padding and what's behind it.
pub fn frame_for(plan: &LinePlan, context: &LayoutContext<'_>) -> LineFrame {
    line_frame(
        plan,
        context.source,
        context.theme,
        context.column_width,
        context.code_line_numbers,
    )
}

/// Lays out a planned line whose frame and code colours are known.
pub fn layout_framed(
    plan: &LinePlan,
    frame: LineFrame,
    code_spans: Option<LineSpans>,
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
        grid: None,
    };
    if plan.collapsed {
        return line;
    }
    let mut layouter = LineLayouter::new(plan, frame, code_spans, context, resources);
    let items = line_items(plan);
    let rows = match &plan.table_row {
        Some(row) => {
            let (rows, grid) = layouter.table_row(row, &items);
            line.grid = Some(grid);
            rows
        }
        None => layouter.flow(&items),
    };
    let bottom = match &line.grid {
        Some(grid) => grid.height,
        None => rows.last().map_or(px(0.), VisualRow::bottom),
    };
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
    /// The last text chunk left out its end's padding, as the next one
    /// carries on its fill past a hidden byte.
    glued: bool,
}

impl<'a, 'b> LineLayouter<'a, 'b> {
    pub(super) fn new(
        plan: &'a LinePlan,
        frame: LineFrame,
        code_spans: Option<LineSpans>,
        context: &'a LayoutContext<'a>,
        resources: &'a mut LayoutResources<'b>,
    ) -> Self {
        Self {
            plan,
            context,
            resources,
            text: &context.source.text()[plan.range.clone()],
            tone: line_tone(plan),
            frame,
            code_spans,
            glued: false,
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

    /// Sets the line's items in rows that wrap at the column's edge, with
    /// the rows of widgets below it after them.
    fn flow(&mut self, items: &LineItems) -> Vec<VisualRow> {
        let mut builder = self.row_builder();
        self.place_items(items, &mut builder);
        self.place_below(items, &mut builder);
        let mut rows = builder.finish();
        if let Some(alignment) = line_alignment(self.plan) {
            let limit = self.context.column_width - self.frame.right;
            align_rows(&mut rows, alignment, limit);
        }
        assign_ranges(&mut rows, 0..self.text.len(), self.text);
        rows
    }

    pub(super) fn line_font(&self) -> Font {
        let run = text_run(1, &[], &self.tone, false, self.theme());
        run.font
    }

    /// How far the font itself reaches above and below the baseline.
    pub(super) fn glyph_extent(&self, font: &Font, font_size: Pixels) -> Extent {
        let text_system = self.resources.text_system;
        let font_id = text_system.resolve_font(font);
        Extent {
            ascent: text_system.ascent(font_id, font_size),
            descent: text_system.descent(font_id, font_size).abs(),
        }
    }

    fn row_builder(&self) -> RowBuilder {
        let font = self.line_font();
        let strut = self.strut(&font, self.font_size(), self.line_height());
        let caret = self.glyph_extent(&font, self.font_size());
        let limit = self.context.column_width - self.frame.right;
        let top = self.frame.pad_top + self.frame.decor.margin_top;
        RowBuilder::new(self.frame.left, limit, top, strut).with_caret(caret)
    }

    fn place_items(&mut self, line: &LineItems, builder: &mut RowBuilder) {
        let items = &line.items;
        let property = property_row(self.plan);
        if property == Some(false) {
            self.to_value_column(builder);
        }
        // A property's value sits in its column, however it's indented.
        let mut index = match property {
            Some(_) => 0,
            None => self.place_indent(items, builder),
        };
        if property == Some(true) && index == 0 && !items.is_empty() {
            index = self.place_item(items, 0, builder);
            self.to_value_column(builder);
        }
        let shown_marker = self.shown_marker();
        let mut in_marker = true;
        while index < items.len() {
            let is_marker = matches!(
                items[index],
                Item::Inline {
                    kind: WidgetKind::ListBullet { .. } | WidgetKind::Checkbox { .. },
                    ..
                }
            );
            let start = items[index].start();
            if in_marker && !is_marker && index > 0 {
                builder.hang_here();
            }
            if let Some(marker) = &shown_marker {
                self.place_around_shown_marker(marker, start, builder);
            }
            in_marker &= is_marker;
            index = self.place_item(items, index, builder);
        }
    }

    /// A list or task marker shown as source, relative to the line.
    fn shown_marker(&self) -> Option<Range<usize>> {
        let marker = self.plan.shown_marker.as_ref()?;
        let line = &self.plan.range;
        Some(marker.start - line.start..marker.end - line.start)
    }

    /// Sets a shown marker in the room its bullet takes, flush against
    /// the text, so the text starts where it does beside a bullet and its
    /// wrapped rows hang there too. A marker wider than the room pushes
    /// the text along.
    fn place_around_shown_marker(
        &self,
        marker: &Range<usize>,
        at: usize,
        builder: &mut RowBuilder,
    ) {
        if at == marker.start {
            let width = self.shown_marker_width(marker);
            builder.advance((self.marker_slot() - width).max(px(0.)));
        }
        if at == marker.end {
            builder.hang_here();
        }
    }

    fn shown_marker_width(&self, marker: &Range<usize>) -> Pixels {
        let text = self.text[marker.clone()].replace('\t', " ");
        let run = text_run(
            text.len(),
            &[StyleKey::MarkupDimmed],
            &self.tone,
            false,
            self.theme(),
        );
        self.shaper().shape(&text, self.font_size(), &[run]).width
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

    pub(super) fn place_item(
        &mut self,
        items: &[Item],
        index: usize,
        builder: &mut RowBuilder,
    ) -> usize {
        match &items[index] {
            Item::Text { .. } => return self.place_text(items, index, builder),
            Item::Inline { range, kind } => self.place_inline(range, kind, builder),
            Item::Break { .. } => builder.break_row(),
            Item::Block { range, kind } => {
                let row = self.block_row(range, kind, builder);
                builder.push_row(row);
            }
        }
        index + 1
    }

    /// Places the chunk of text starting at item `index`, with room at its
    /// ends for its fill, and answers the item after it. Two stretches of
    /// one style either side of a hidden byte, such as inline code around
    /// an escaped pipe's backslash, read as one: no room between them.
    fn place_text(&mut self, items: &[Item], index: usize, builder: &mut RowBuilder) -> usize {
        let (chunk, next) = self.collect_chunk(items, index);
        if chunk.text.is_empty() {
            return next;
        }
        let padding = self.chunk_padding(&chunk);
        if !std::mem::take(&mut self.glued) {
            builder.advance(padding);
        }
        builder.push_chunk(&chunk, &self.shaper());
        let carries_on = match (&items[next - 1], items.get(next)) {
            (
                Item::Text { styles, .. },
                Some(Item::Text {
                    range,
                    styles: after,
                }),
            ) => range.start == chunk.range.end + 1 && styles == after,
            _ => false,
        };
        match carries_on && padding > px(0.) {
            true => self.glued = true,
            false => builder.advance(padding + self.chip_gap(&items[index])),
        }
        next
    }

    /// Joins adjacent text items at one size into a chunk.
    fn collect_chunk(&self, items: &[Item], first: usize) -> (Chunk, usize) {
        let Item::Text { range, styles } = &items[first] else {
            unreachable!("chunks start at text");
        };
        let font_size = run_font_size(styles, &self.tone, self.theme());
        let baseline_shift = run_baseline_shift(styles, &self.tone, self.theme());
        let indent_end = indent_columns(self.text, self.theme().tab_columns).1;
        let start = range.start.max(indent_end).min(range.end);
        let mut end = start;
        let mut runs = Vec::new();
        let mut backgrounds = Vec::new();
        let mut next = first;
        while let Some(Item::Text { range, styles }) = items.get(next) {
            let same_size = run_font_size(styles, &self.tone, self.theme()) == font_size
                && run_baseline_shift(styles, &self.tone, self.theme()) == baseline_shift;
            let after_marker = next > first && self.ends_shown_marker(range.start);
            if range.start > end.max(start) || !same_size && next > first || after_marker {
                break;
            }
            let part = range.start.max(start)..range.end;
            let at = part.start.saturating_sub(start);
            let before = runs.len();
            self.push_runs(&mut runs, &part, styles);
            let padding = fill_padding(styles, self.theme());
            take_backgrounds(&mut runs[before..], at, padding, &mut backgrounds);
            end = part.end.max(end);
            next += 1;
        }
        let chunk = Chunk {
            range: start..end,
            text: self.text[start..end].replace('\t', " "),
            font_size,
            line_height: font_size * self.tone.line_height_factor(self.theme()),
            baseline_shift,
            runs,
            backgrounds,
        };
        (chunk, next)
    }

    /// Whether a shown list marker ends at `at`, where its text starts.
    fn ends_shown_marker(&self, at: usize) -> bool {
        self.shown_marker().is_some_and(|marker| marker.end == at)
    }

    /// Space after a list property's item, so the next one's fill stands
    /// apart from it.
    fn chip_gap(&self, item: &Item) -> Pixels {
        match item {
            Item::Text { styles, .. } if styles.contains(&StyleKey::PropertyChip) => {
                self.theme().property_chip_gap
            }
            _ => px(0.),
        }
    }

    /// Room on each side of inline code, for its fill to reach into.
    fn chunk_padding(&self, chunk: &Chunk) -> Pixels {
        chunk_padding(&chunk.backgrounds)
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
    padding: Pixels,
    backgrounds: &mut Vec<Background>,
) {
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
                padding,
            }),
        }
    }
}

/// The widest room any fill of a chunk wants at its ends.
pub(super) fn chunk_padding(backgrounds: &[Background]) -> Pixels {
    backgrounds
        .iter()
        .map(|background| background.padding)
        .fold(px(0.), Pixels::max)
}

/// How an HTML block such as `<center>` aligns this line, if it does.
fn line_alignment(plan: &LinePlan) -> Option<Alignment> {
    plan.line_styles.iter().find_map(|style| match style {
        LineStyle::Align(alignment) => Some(*alignment),
        _ => None,
    })
}

/// Moves each text row's pieces right, so the row is centred or ends at
/// `limit`, the right edge of the text.
fn align_rows(rows: &mut [VisualRow], alignment: Alignment, limit: Pixels) {
    let share = match alignment {
        Alignment::Center => 0.5,
        Alignment::Right => 1.,
        Alignment::Left | Alignment::None => return,
    };
    for row in rows.iter_mut().filter(|row| row.kind == RowKind::Text) {
        let room = (limit - row.right()).max(px(0.));
        let shift = room * share;
        row.left += shift;
        for piece in &mut row.pieces {
            piece.x += shift;
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

/// Gives each caret row the offsets of `span` it covers: from where the
/// previous one ended to where the next one's first piece starts. A
/// click past a wrapped row's end lands before the space the wrap broke
/// at.
pub(super) fn assign_ranges(rows: &mut [VisualRow], span: Range<usize>, text: &str) {
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
    let mut start = span.start;
    for (position, &index) in caret_rows.iter().enumerate() {
        let next_start = first_starts[position + 1..]
            .iter()
            .flatten()
            .next()
            .copied();
        let end = next_start.unwrap_or(span.end).max(start);
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
