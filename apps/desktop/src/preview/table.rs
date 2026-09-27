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
use gpui::{Hsla, Pixels, ShapedLine, TextRun, px};

use crate::line_layout::{Hit, Piece, PieceContent, TextPiece};
use crate::preview::items::{Item, line_items};
use crate::preview::layout::LineLayouter;
use crate::preview::math::{MathImage, MathState};
use crate::preview::wrap::Extent;
use crate::styling::{LineTone, run_font_size, text_run};

/// The planner's description of a table.
pub struct TableSpec<'a> {
    pub alignments: &'a [Alignment],
    /// Cell content ranges in document offsets, header row first.
    pub rows: &'a [Vec<Range<usize>>],
}

/// Something drawn in a cell.
enum FragmentContent {
    Text(Box<ShapedLine>),
    Math(Arc<MathImage>),
}

/// One piece of a cell, placed left to right from the cell's start.
struct Fragment {
    content: FragmentContent,
    x: Pixels,
    width: Pixels,
    extent: Extent,
}

/// A cell's fragments and the box they make together.
#[derive(Default)]
struct Cell {
    fragments: Vec<Fragment>,
    width: Pixels,
    extent: Extent,
}

impl Cell {
    fn push(&mut self, content: FragmentContent, width: Pixels, extent: Extent) {
        self.fragments.push(Fragment {
            content,
            x: self.width,
            width,
            extent,
        });
        self.width += width;
        self.extent = Extent {
            ascent: self.extent.ascent.max(extent.ascent),
            descent: self.extent.descent.max(extent.descent),
        };
    }
}

/// Text waiting to be shaped: consecutive runs at one size.
#[derive(Default)]
struct PendingText {
    text: String,
    runs: Vec<TextRun>,
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
            let extent = row.iter().fold(strut, |extent, cell| Extent {
                ascent: extent.ascent.max(cell.extent.ascent),
                descent: extent.descent.max(cell.extent.descent),
            });
            let row_height = extent.ascent + extent.descent + pad_y * 2.;
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
            let baseline = top + pad_y + extent.ascent;
            let mut x = left;
            for (column, cell) in row.into_iter().enumerate() {
                let column_width = columns.get(column).copied().unwrap_or(px(0.));
                let alignment = spec
                    .alignments
                    .get(column)
                    .copied()
                    .unwrap_or(Alignment::None);
                let offset = aligned(alignment, column_width - pad_x * 2., cell.width);
                let origin = x + pad_x + offset;
                pieces.extend(cell_pieces(range, cell, origin, baseline));
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
        pending.text.push_str(text);
        pending.runs.push(run);
    }

    fn flush(&self, cell: &mut Cell, pending: &mut PendingText) {
        let pending = std::mem::take(pending);
        let Some(font_size) = pending.font_size.filter(|_| !pending.text.is_empty()) else {
            return;
        };
        let text = pending.text.replace('\t', " ");
        let shaped = self.shaper().shape(&text, font_size, &pending.runs);
        let line_height = font_size * self.theme().line_height_factor;
        let extent = Extent::of_text(&shaped, line_height);
        let width = shaped.width;
        cell.push(FragmentContent::Text(Box::new(shaped)), width, extent);
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
                let extent = Extent {
                    ascent: image.baseline,
                    descent: image.height - image.baseline,
                };
                let width = image.width;
                cell.push(FragmentContent::Math(image), width, extent);
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

/// Column widths from the widest cell in each, shrunk to fit `available`.
fn column_widths(rows: &[Vec<Cell>], padding: Pixels, available: Pixels) -> Vec<Pixels> {
    let count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<Pixels> = (0..count)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.width)
                .fold(px(0.), Pixels::max)
                + padding * 2.
        })
        .collect();
    fit_widths(widths, available)
}

fn fit_widths(widths: Vec<Pixels>, available: Pixels) -> Vec<Pixels> {
    let total = widths.iter().fold(px(0.), |sum, width| sum + *width);
    if total <= available || total <= px(0.) {
        return widths;
    }
    let scale = available / total;
    widths.into_iter().map(|width| width * scale).collect()
}

fn aligned(alignment: Alignment, room: Pixels, width: Pixels) -> Pixels {
    let spare = (room - width).max(px(0.));
    match alignment {
        Alignment::Center => spare / 2.,
        Alignment::Right => spare,
        Alignment::Left | Alignment::None => px(0.),
    }
}

/// The pieces of a cell whose content starts at `x`, sitting on
/// `baseline`.
fn cell_pieces(
    range: &Range<usize>,
    cell: Cell,
    x: Pixels,
    baseline: Pixels,
) -> impl Iterator<Item = Piece> {
    let range = range.clone();
    cell.fragments.into_iter().map(move |fragment| {
        let top = baseline - fragment.extent.ascent;
        let (content, height) = match fragment.content {
            FragmentContent::Text(shaped) => {
                let height = fragment.extent.ascent + fragment.extent.descent;
                let text = TextPiece::whole(*shaped, height);
                (PieceContent::Text(Box::new(text)), height)
            }
            FragmentContent::Math(image) => (
                PieceContent::Image {
                    image: image.image.clone(),
                    radius: px(0.),
                },
                image.height,
            ),
        };
        Piece {
            range: range.clone(),
            x: x + fragment.x,
            top,
            width: fragment.width,
            height,
            content,
            hit: Hit::Widget,
        }
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
    fn columns_shrink_to_fit_in_proportion() {
        let fitted = fit_widths(vec![px(100.), px(300.)], px(200.));
        assert_eq!(fitted, vec![px(50.), px(150.)]);
        let roomy = fit_widths(vec![px(100.), px(300.)], px(500.));
        assert_eq!(roomy, vec![px(100.), px(300.)]);
    }

    #[test]
    fn cells_place_fragments_side_by_side_on_one_baseline() {
        let mut cell = Cell::default();
        let tall = Extent {
            ascent: px(12.),
            descent: px(4.),
        };
        let short = Extent {
            ascent: px(8.),
            descent: px(2.),
        };
        cell.push(FragmentContent::Text(Box::default()), px(10.), short);
        cell.push(FragmentContent::Text(Box::default()), px(5.), tall);
        assert_eq!(cell.width, px(15.));
        assert_eq!(cell.fragments[1].x, px(10.));
        assert_eq!(
            cell.extent,
            Extent {
                ascent: px(12.),
                descent: px(4.)
            }
        );
    }
}
