//! A table drawn as an aligned grid while the cursor is outside it.
//!
//! Each cell is planned on its own with every symbol hidden, then set as
//! a row of fragments: text shaped at its own size (inline code is
//! smaller) and rendered equations, all sharing one baseline, as inline
//! math does in a paragraph.

use std::ops::Range;
use std::sync::Arc;

use editor_core::render::{
    LinePlan, RenderInput, RevealMode, RevealSettings, StyleKey, WidgetKind, plan_lines,
};
use editor_core::syntax::{Alignment, SyntaxKind};
use gpui::{Hsla, Pixels, TextRun, px};

use crate::line_layout::{Background, Hit, Piece, PieceContent, VisualRow};
use crate::preview::items::{Item, line_items};
use crate::preview::layout::{LineLayouter, take_backgrounds};
use crate::preview::math::{MathImage, MathState};
use crate::preview::wrap::{Chunk, Extent, RowBuilder, widest_word};
use crate::styling::{LineTone, run_font_size, text_run};

/// The planner's description of a table.
pub struct TableSpec<'a> {
    pub alignments: &'a [Alignment],
    /// Cell content ranges in document offsets, header row first.
    pub rows: &'a [Vec<Range<usize>>],
}

/// Something set in a cell: text that may wrap, or an equation that
/// never does.
enum Part {
    Text { chunk: Chunk, padding: Pixels },
    Math(Arc<MathImage>),
}

/// A cell's parts, how wide they are on one line, and the narrowest the
/// cell can get: its widest word or equation.
#[derive(Default)]
struct Cell {
    parts: Vec<Part>,
    width: Pixels,
    min_width: Pixels,
}

impl Cell {
    fn push(&mut self, part: Part, width: Pixels, min_width: Pixels) {
        self.parts.push(part);
        self.width += width;
        self.min_width = self.min_width.max(min_width);
    }
}

/// A cell set within its column: its rows, and how far its first row's
/// baseline is from its top, so cells in a table row share a baseline.
struct SetCell {
    rows: Vec<VisualRow>,
    ascent: Pixels,
    height: Pixels,
}

/// Text waiting to be shaped: consecutive runs at one size.
#[derive(Default)]
struct PendingText {
    text: String,
    runs: Vec<TextRun>,
    backgrounds: Vec<Background>,
    font_size: Option<Pixels>,
}

impl LineLayouter<'_, '_> {
    pub(super) fn table(
        &mut self,
        range: &Range<usize>,
        spec: &TableSpec<'_>,
        left: Pixels,
        width: Pixels,
    ) -> Vec<Piece> {
        let theme = self.theme();
        let pad_x = theme.space_md;
        let pad_y = theme.space_xs * 2.;
        let strut = self.strut(&self.body_font(), self.font_size(), self.line_height());
        let cells = self.cells(spec);
        let columns = column_widths(&cells, pad_x, width);
        let table_width = columns.iter().fold(px(0.), |sum, column| sum + *column);
        let mut pieces = Vec::new();
        let mut top = px(0.);
        for (row_index, row) in cells.into_iter().enumerate() {
            let set: Vec<SetCell> = row
                .into_iter()
                .enumerate()
                .map(|(column, cell)| {
                    let room = columns.get(column).copied().unwrap_or(px(0.)) - pad_x * 2.;
                    self.set_cell(cell, room, strut)
                })
                .collect();
            let ascent = set.iter().fold(px(0.), |most, cell| most.max(cell.ascent));
            let content_height = set.iter().fold(px(0.), |most, cell| {
                most.max(ascent - cell.ascent + cell.height)
            });
            let row_height = content_height + pad_y * 2.;
            if row_index == 0 {
                pieces.push(quad(
                    range,
                    left,
                    top,
                    table_width,
                    row_height,
                    theme.surface,
                ));
            }
            let mut x = left;
            for (column, cell) in set.into_iter().enumerate() {
                let column_width = columns.get(column).copied().unwrap_or(px(0.));
                let alignment = spec
                    .alignments
                    .get(column)
                    .copied()
                    .unwrap_or(Alignment::None);
                let cell_top = top + pad_y + ascent - cell.ascent;
                let room = column_width - pad_x * 2.;
                pieces.extend(cell_pieces(
                    range,
                    cell,
                    (x + pad_x, cell_top),
                    room,
                    alignment,
                ));
                x += column_width;
            }
            top += row_height;
            let rule = theme.rule_thickness;
            pieces.push(quad(
                range,
                left,
                top - rule,
                table_width,
                rule,
                theme.divider,
            ));
        }
        pieces
    }

    /// Sets a cell's parts in rows no wider than `room`, breaking text
    /// between words.
    fn set_cell(&self, cell: Cell, room: Pixels, strut: Extent) -> SetCell {
        let mut builder = RowBuilder::new(px(0.), room, px(0.), strut);
        for part in cell.parts {
            match part {
                Part::Text { chunk, padding } => {
                    builder.advance(padding);
                    builder.push_chunk(&chunk, &self.shaper());
                    builder.advance(padding);
                }
                Part::Math(image) => {
                    let extent = Extent {
                        ascent: image.baseline,
                        descent: image.height - image.baseline,
                    };
                    let piece = Piece {
                        range: 0..0,
                        x: px(0.),
                        top: px(0.),
                        width: image.width,
                        height: image.height,
                        content: PieceContent::Image {
                            image: image.image.clone(),
                            radius: px(0.),
                        },
                        hit: Hit::Widget,
                    };
                    builder.push_atomic(piece, extent);
                }
            }
        }
        let rows = builder.finish();
        let ascent = rows
            .first()
            .map_or(strut.ascent, |row| row.caret_top + strut.ascent);
        let height = rows.last().map_or(px(0.), VisualRow::bottom);
        SetCell {
            rows,
            ascent,
            height,
        }
    }

    fn body_font(&self) -> gpui::Font {
        text_run(1, &[], &self.tone, false, self.theme()).font
    }

    /// Every cell's contents, planned with inline markup hidden so the
    /// table's source shows nowhere.
    fn cells(&mut self, spec: &TableSpec<'_>) -> Vec<Vec<Cell>> {
        let plans = self.cell_plans(spec);
        let items: Vec<(Range<usize>, Vec<Item>)> = plans
            .iter()
            .map(|plan| (plan.range.clone(), line_items(plan).items))
            .collect();
        spec.rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                row.iter()
                    .map(|cell| {
                        let line = items.iter().find(|(line, _)| line.contains(&cell.start));
                        match line {
                            Some((line, items)) => self.cell(line.start, items, cell, index == 0),
                            None => Cell::default(),
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn cell_plans(&self, spec: &TableSpec<'_>) -> Vec<LinePlan> {
        let source = self.context.source;
        let cells = spec.rows.iter().flatten();
        let (Some(first), Some(last)) = (
            cells.clone().map(|cell| cell.start).min(),
            cells.map(|cell| cell.end).max(),
        ) else {
            return Vec::new();
        };
        let settings = RevealSettings::new(RevealMode::AlwaysHidden)
            .with_override(SyntaxKind::Table, RevealMode::AlwaysShown);
        plan_lines(
            &RenderInput {
                text: source.text(),
                tree: source.tree(),
                selections: &[],
                settings: &settings,
            },
            source.line_of(first)..source.line_of(last) + 1,
        )
        .lines
    }

    /// Lays out one cell from its line's items. `line_start` is the
    /// document offset the items are relative to.
    fn cell(
        &mut self,
        line_start: usize,
        items: &[Item],
        cell: &Range<usize>,
        header: bool,
    ) -> Cell {
        let text = self.context.source.text();
        let cell = cell.start - line_start..cell.end - line_start;
        let mut result = Cell::default();
        let mut pending = PendingText::default();
        for item in items {
            match item {
                Item::Text { range, styles } => {
                    let clipped = range.start.max(cell.start)..range.end.min(cell.end);
                    if clipped.is_empty() {
                        continue;
                    }
                    let mut styles = styles.clone();
                    if header {
                        styles.push(StyleKey::Strong);
                    }
                    let absolute = clipped.start + line_start..clipped.end + line_start;
                    self.push_text(&mut result, &mut pending, &text[absolute], &styles, None);
                }
                Item::Inline {
                    range,
                    kind: WidgetKind::InlineMath { tex, .. },
                } if cell.contains(&range.start) => {
                    self.flush(&mut result, &mut pending);
                    self.cell_math(&mut result, &mut pending, tex);
                }
                _ => {}
            }
        }
        self.flush(&mut result, &mut pending);
        result
    }

    /// Adds text to the pending fragment, shaping what came before first
    /// if the size or font changes. The main text shapes one font per
    /// chunk too: a line shaped with several fonts comes out in the first
    /// one on Linux.
    fn push_text(
        &self,
        cell: &mut Cell,
        pending: &mut PendingText,
        text: &str,
        styles: &[StyleKey],
        color: Option<Hsla>,
    ) {
        let font_size = run_font_size(styles, &LineTone::PLAIN, self.theme());
        let mut run = text_run(text.len(), styles, &LineTone::PLAIN, false, self.theme());
        if let Some(color) = color {
            run.color = color;
        }
        let same_font = pending.runs.last().is_none_or(|last| last.font == run.font);
        if pending.font_size.is_some_and(|size| size != font_size) || !same_font {
            self.flush(cell, pending);
        }
        pending.font_size = Some(font_size);
        let mut runs = [run];
        take_backgrounds(
            &mut runs,
            pending.text.len(),
            styles,
            &mut pending.backgrounds,
        );
        pending.text.push_str(text);
        pending.runs.extend(runs);
    }

    fn flush(&self, cell: &mut Cell, pending: &mut PendingText) {
        let pending = std::mem::take(pending);
        let Some(font_size) = pending.font_size.filter(|_| !pending.text.is_empty()) else {
            return;
        };
        let text = pending.text.replace('\t', " ");
        let shaped = self.shaper().shape(&text, font_size, &pending.runs);
        let padding = match pending
            .backgrounds
            .iter()
            .any(|background| background.padded)
        {
            true => self.theme().inline_code_padding,
            false => px(0.),
        };
        let width = shaped.width + padding * 2.;
        let min_width = widest_word(&shaped, &text) + padding * 2.;
        let chunk = Chunk {
            range: 0..text.len(),
            text,
            font_size,
            line_height: font_size * self.theme().line_height_factor,
            runs: pending.runs,
            backgrounds: pending.backgrounds,
        };
        cell.push(Part::Text { chunk, padding }, width, min_width);
    }

    /// An equation in a cell: rendered when ready, its source until then,
    /// in the error colour when it can't render.
    fn cell_math(&mut self, cell: &mut Cell, pending: &mut PendingText, tex: &str) {
        let source = |layouter: &Self, cell: &mut Cell, pending: &mut PendingText, color| {
            let styles = [StyleKey::MathSource];
            layouter.push_text(cell, pending, tex, &styles, color);
            layouter.flush(cell, pending);
        };
        match self.math(tex, false, self.font_size()) {
            MathState::Ready(image) => {
                let width = image.width;
                cell.push(Part::Math(image), width, width);
            }
            MathState::Pending => source(self, cell, pending, None),
            MathState::Failed(_) => source(self, cell, pending, Some(self.theme().error)),
        }
    }
}

fn quad(
    range: &Range<usize>,
    x: Pixels,
    top: Pixels,
    width: Pixels,
    height: Pixels,
    color: Hsla,
) -> Piece {
    Piece {
        range: range.clone(),
        x,
        top,
        width,
        height,
        content: PieceContent::Quad {
            color,
            radius: px(0.),
        },
        hit: Hit::Widget,
    }
}

/// Column widths: each column's widest cell when the table fits. When
/// it doesn't, every column keeps room for its longest word or equation
/// and the rest of the width goes to the columns that have more to wrap.
fn column_widths(rows: &[Vec<Cell>], padding: Pixels, available: Pixels) -> Vec<Pixels> {
    let count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widest = |column: usize, width: fn(&Cell) -> Pixels| {
        rows.iter()
            .filter_map(|row| row.get(column))
            .map(width)
            .fold(px(0.), Pixels::max)
            + padding * 2.
    };
    let natural: Vec<Pixels> = (0..count).map(|c| widest(c, |cell| cell.width)).collect();
    let least: Vec<Pixels> = (0..count)
        .map(|c| widest(c, |cell| cell.min_width))
        .collect();
    fit_widths(&natural, &least, available)
}

fn fit_widths(natural: &[Pixels], least: &[Pixels], available: Pixels) -> Vec<Pixels> {
    let sum = |widths: &[Pixels]| widths.iter().fold(px(0.), |sum, width| sum + *width);
    if sum(natural) <= available {
        return natural.to_vec();
    }
    if sum(least) >= available {
        return least.to_vec();
    }
    let (kept, room) = narrow_columns(natural, available);
    let open = |widths: &[Pixels]| {
        let widths = (0..widths.len()).filter(|&c| !kept[c]).map(|c| widths[c]);
        widths.fold(px(0.), |total, width| total + width)
    };
    let (total, floor) = (open(natural), open(least));
    let scale = ((room - floor) / (total - floor).max(px(1.))).clamp(0., 1.);
    (0..natural.len())
        .map(|c| match kept[c] {
            true => natural[c],
            false => least[c] + (natural[c] - least[c]) * scale,
        })
        .collect()
}

/// Columns narrower than an even share of the width left keep their
/// width, so a short name never wraps to make room for a long text.
/// Returns which columns keep theirs and the width left for the others.
fn narrow_columns(natural: &[Pixels], available: Pixels) -> (Vec<bool>, Pixels) {
    let mut kept = vec![false; natural.len()];
    let mut room = available;
    loop {
        let open: Vec<usize> = (0..natural.len()).filter(|&c| !kept[c]).collect();
        let share = room / open.len().max(1) as f32;
        let fitting: Vec<usize> = open.into_iter().filter(|&c| natural[c] <= share).collect();
        if fitting.is_empty() {
            return (kept, room);
        }
        for column in fitting {
            kept[column] = true;
            room -= natural[column];
        }
    }
}

fn aligned(alignment: Alignment, room: Pixels, width: Pixels) -> Pixels {
    let spare = (room - width).max(px(0.));
    match alignment {
        Alignment::Center => spare / 2.,
        Alignment::Right => spare,
        Alignment::Left | Alignment::None => px(0.),
    }
}

/// A set cell's pieces, each row aligned within `room` from its top
/// left corner. They all stand for the table's source.
fn cell_pieces(
    range: &Range<usize>,
    cell: SetCell,
    (x, top): (Pixels, Pixels),
    room: Pixels,
    alignment: Alignment,
) -> impl Iterator<Item = Piece> {
    let range = range.clone();
    cell.rows.into_iter().flat_map(move |row| {
        let offset = aligned(alignment, room, row.right());
        let range = range.clone();
        row.pieces.into_iter().map(move |mut piece| {
            piece.x += x + offset;
            piece.top += top + row.top;
            piece.range = range.clone();
            piece.hit = Hit::Widget;
            piece
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_places_cells_within_their_column() {
        assert_eq!(aligned(Alignment::Right, px(100.), px(40.)), px(60.));
        assert_eq!(aligned(Alignment::Center, px(100.), px(40.)), px(30.));
        assert_eq!(aligned(Alignment::Left, px(100.), px(40.)), px(0.));
        assert_eq!(aligned(Alignment::Right, px(10.), px(40.)), px(0.));
    }

    #[test]
    fn columns_keep_their_widest_word_and_share_the_rest() {
        let natural = [px(100.), px(300.)];
        let least = [px(60.), px(100.)];
        assert_eq!(fit_widths(&natural, &least, px(500.)), natural.to_vec());
        // The short column keeps its width; the long one wraps into the rest.
        assert_eq!(fit_widths(&natural, &least, px(250.)), [px(100.), px(150.)]);
        // Two long columns share what's over their longest words, 1:2.
        let natural = [px(400.), px(700.)];
        let least = [px(100.), px(100.)];
        assert_eq!(fit_widths(&natural, &least, px(500.)), [px(200.), px(300.)]);
        assert_eq!(fit_widths(&natural, &least, px(150.)), least.to_vec());
    }
}
