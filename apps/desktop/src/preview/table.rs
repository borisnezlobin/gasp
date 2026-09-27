//! A table drawn as an aligned grid while the cursor is outside it.

use std::ops::Range;

use editor_core::render::{
    LinePlan, RenderInput, RevealMode, RevealSettings, StyleKey, plan_lines,
};
use editor_core::syntax::{Alignment, SyntaxKind};
use gpui::{Pixels, ShapedLine, TextRun, px};

use crate::line_layout::{Hit, Piece, PieceContent, TextPiece};
use crate::preview::items::visible_parts;
use crate::preview::layout::LineLayouter;
use crate::styling::{LineTone, text_run};

/// The planner's description of a table.
pub struct TableSpec<'a> {
    pub alignments: &'a [Alignment],
    /// Cell content ranges in document offsets, header row first.
    pub rows: &'a [Vec<Range<usize>>],
}

/// A cell's visible text and its runs.
struct CellText {
    text: String,
    runs: Vec<TextRun>,
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
        let line_height = self.line_height();
        let shaped: Vec<Vec<ShapedLine>> = self
            .cell_texts(spec)
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| {
                        self.shaper()
                            .shape(&cell.text, self.font_size(), &cell.runs)
                    })
                    .collect()
            })
            .collect();
        let columns = column_widths(&shaped, pad_x, width);
        let row_height = line_height + pad_y * 2.;
        let table_width = columns.iter().fold(px(0.), |sum, column| sum + *column);
        let mut pieces =
            vec![self.quad(range, left, px(0.), table_width, row_height, theme.surface)];
        for (row_index, row) in shaped.into_iter().enumerate() {
            let top = row_height * row_index as f32;
            let mut x = left;
            for (column, cell) in row.into_iter().enumerate() {
                let column_width = columns.get(column).copied().unwrap_or(px(0.));
                let alignment = spec
                    .alignments
                    .get(column)
                    .copied()
                    .unwrap_or(Alignment::None);
                let offset = aligned(alignment, column_width - pad_x * 2., cell.width);
                pieces.push(cell_piece(
                    range,
                    cell,
                    x + pad_x + offset,
                    top + pad_y,
                    line_height,
                ));
                x += column_width;
            }
            let rule_top = top + row_height - theme.rule_thickness;
            pieces.push(self.quad(
                range,
                left,
                rule_top,
                table_width,
                theme.rule_thickness,
                theme.divider,
            ));
        }
        pieces
    }

    fn quad(
        &self,
        range: &Range<usize>,
        x: Pixels,
        top: Pixels,
        width: Pixels,
        height: Pixels,
        color: gpui::Hsla,
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

    /// Each cell's text with inline markup hidden, planned on its own so
    /// the table's source shows nowhere.
    fn cell_texts(&self, spec: &TableSpec<'_>) -> Vec<Vec<CellText>> {
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
        let plan = plan_lines(
            &RenderInput {
                text: source.text(),
                tree: source.tree(),
                selections: &[],
                settings: &settings,
            },
            source.line_of(first)..source.line_of(last) + 1,
        );
        spec.rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                row.iter()
                    .map(|cell| self.cell_text(&plan.lines, cell, index == 0))
                    .collect()
            })
            .collect()
    }

    fn cell_text(&self, lines: &[LinePlan], cell: &Range<usize>, header: bool) -> CellText {
        let text = self.context.source.text();
        let mut result = CellText {
            text: String::new(),
            runs: Vec::new(),
        };
        let Some(line) = lines.iter().find(|line| line.range.contains(&cell.start)) else {
            return result;
        };
        for run in &line.runs {
            let clipped = run.range.start.max(cell.start)..run.range.end.min(cell.end);
            if clipped.is_empty() {
                continue;
            }
            let mut styles = run.styles.clone();
            if header {
                styles.push(StyleKey::Strong);
            }
            for part in visible_parts(&clipped, &line.hidden) {
                result.text.push_str(&text[part.clone()]);
                let run = text_run(part.len(), &styles, &LineTone::PLAIN, false, self.theme());
                result.runs.push(run);
            }
        }
        result
    }
}

/// Column widths from the widest cell in each, shrunk to fit `available`.
fn column_widths(rows: &[Vec<ShapedLine>], padding: Pixels, available: Pixels) -> Vec<Pixels> {
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

fn cell_piece(
    range: &Range<usize>,
    shaped: ShapedLine,
    x: Pixels,
    top: Pixels,
    line_height: Pixels,
) -> Piece {
    Piece {
        range: range.clone(),
        x,
        top,
        width: shaped.width,
        height: line_height,
        content: PieceContent::Text(Box::new(TextPiece::whole(shaped, line_height))),
        hit: Hit::Widget,
    }
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
}
