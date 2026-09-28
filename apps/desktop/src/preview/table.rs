//! Tables drawn as grids. Each row of a table is its own source line laid
//! out as a row of cells, so the caret, selection and hit testing work in
//! a cell as they do in a paragraph; the delimiter row's line takes no
//! space.
//!
//! Every row of a table sets its cells in the same columns, sized from
//! all its rows' cells. [`TableStore`] keeps each row's measured cells
//! until the row changes, so typing in a cell measures only its row, and
//! the columns once a frame.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use editor_core::render::{
    LinePlan, RenderInput, RevealSettings, StyleKey, TableRowPlan, plan_lines,
};
use editor_core::syntax::{Alignment, NodeKind};
use editor_core::table::table_node_at;
use gpui::{Pixels, px};

use crate::line_layout::{GridCell, GridLine, PieceContent, VisualRow};
use crate::preview::decor::LineFrame;
use crate::preview::items::{Item, LineItems, line_items};
use crate::preview::layout::{LayoutContext, LayoutResources, LineLayouter, assign_ranges};
use crate::preview::wrap::{RowBuilder, widest_word};

/// Rows measured before the store starts over; far more than a screen.
const ROW_LIMIT: usize = 4096;

/// Where a table's columns go.
#[derive(Clone, Debug, PartialEq)]
pub struct Columns {
    /// Each column's left edge, from the table's.
    pub lefts: Vec<Pixels>,
    pub widths: Vec<Pixels>,
}

impl Columns {
    fn from_widths(widths: Vec<Pixels>) -> Self {
        let mut left = px(0.);
        let lefts = widths
            .iter()
            .map(|width| {
                let at = left;
                left += *width;
                at
            })
            .collect();
        Self { lefts, widths }
    }

    pub fn width(&self) -> Pixels {
        self.widths.iter().fold(px(0.), |sum, width| sum + *width)
    }

    /// Hashes the columns, for the line cache: a row laid out against
    /// other columns is laid out again.
    pub fn hash_into(&self, hasher: &mut DefaultHasher) {
        for width in &self.widths {
            f32::from(*width).to_bits().hash(hasher);
        }
    }
}

/// How wide a row's cells want to be: on one line, and the narrowest
/// each can wrap to (its widest word or equation). Padding not included.
#[derive(Clone, Debug, Default)]
struct RowWidths {
    cells: Vec<(Pixels, Pixels)>,
}

/// What every measured row was measured against.
#[derive(Clone, Debug, Default, PartialEq)]
struct Measured {
    zoom: f32,
    scale_factor: f32,
    reveal: Option<RevealSettings>,
}

/// Measured rows and this frame's columns.
#[derive(Default)]
pub struct TableStore {
    measured: Measured,
    rows: HashMap<u64, Arc<RowWidths>>,
    /// Columns worked out since the text or selection last changed, by
    /// where their table starts.
    columns: HashMap<usize, Arc<Columns>>,
}

impl TableStore {
    /// Forgets everything, as when the theme changes.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.columns.clear();
    }

    /// Forgets the columns worked out, as the text or the selection
    /// changed. Measured rows stay: they know what they depend on.
    pub fn forget_columns(&mut self) {
        self.columns.clear();
    }

    /// The columns worked out for the table starting at `start`, if they
    /// still hold.
    pub fn columns(&self, start: usize) -> Option<Arc<Columns>> {
        self.columns.get(&start).cloned()
    }

    /// Starts over when what rows are measured against changed.
    fn check(&mut self, context: &LayoutContext<'_>) {
        let stale = self.measured.zoom != context.zoom
            || self.measured.scale_factor != context.scale_factor
            || self.measured.reveal.as_ref() != Some(context.reveal);
        if stale {
            self.clear();
            self.measured = Measured {
                zoom: context.zoom,
                scale_factor: context.scale_factor,
                reveal: Some(context.reveal.clone()),
            };
        }
    }
}

/// The columns of the table `plan` is a row of, for the line cache's key:
/// worked out now if this frame hasn't yet.
pub fn table_columns(
    plan: &LinePlan,
    frame: &LineFrame,
    context: &LayoutContext<'_>,
    resources: &mut LayoutResources<'_>,
) -> Option<Arc<Columns>> {
    let row = plan.table_row.as_ref()?;
    let mut layouter = LineLayouter::new(plan, frame.clone(), None, context, resources);
    Some(layouter.columns(row))
}

/// A cell's rows before they're placed in its row: how far the first
/// one's baseline is from its top, so the cells of a row share one.
struct SetCell {
    rows: Vec<VisualRow>,
    ascent: Pixels,
}

impl LineLayouter<'_, '_> {
    /// Lays out a table row: each cell's text set in its column, wrapping
    /// within it, the cells sharing the first line's baseline.
    pub(super) fn table_row(
        &mut self,
        row: &TableRowPlan,
        items: &LineItems,
    ) -> (Vec<VisualRow>, GridLine) {
        let columns = self.columns(row);
        let look = &self.theme().table;
        let (pad_x, pad_y) = (look.cell_padding_x, look.cell_padding_y);
        let top = self.frame.pad_top + self.frame.decor.margin_top;
        let left = self.frame.left;
        let line_start = self.plan.range.start;
        let set: Vec<(Option<Range<usize>>, SetCell)> = (0..columns.widths.len())
            .map(|column| {
                let range = row
                    .cells
                    .get(column)
                    .map(|cell| cell.start - line_start..cell.end - line_start);
                let x = left + columns.lefts[column] + pad_x;
                let room = columns.widths[column] - pad_x * 2.;
                let alignment = row.alignments.get(column).copied();
                let header = row.index == 0;
                let cell = self.set_cell(items, range.clone(), (x, room), alignment, header);
                (range, cell)
            })
            .collect();
        let ascent = set
            .iter()
            .fold(px(0.), |most, (_, cell)| most.max(cell.ascent));
        let mut rows = Vec::new();
        let mut cells = Vec::with_capacity(set.len());
        let mut bottom = top + pad_y + self.line_height();
        for (column, (range, cell)) in set.into_iter().enumerate() {
            let shift = top + pad_y + ascent - cell.ascent;
            let first = rows.len();
            for mut visual in cell.rows {
                visual.top += shift;
                bottom = bottom.max(visual.bottom());
                rows.push(visual);
            }
            cells.push(GridCell {
                x: left + columns.lefts[column],
                width: columns.widths[column],
                rows: first..rows.len(),
                range,
            });
        }
        let grid = GridLine {
            index: row.index,
            count: row.count,
            cells,
            left,
            width: columns.width(),
            height: bottom + pad_y,
        };
        (rows, grid)
    }

    /// Sets a cell's text in rows `room` wide from `x`, aligned in them.
    /// A cell a short row leaves out has no rows.
    fn set_cell(
        &mut self,
        items: &LineItems,
        range: Option<Range<usize>>,
        (x, room): (Pixels, Pixels),
        alignment: Option<Alignment>,
        header: bool,
    ) -> SetCell {
        let font = self.line_font();
        let strut = self.strut(&font, self.font_size(), self.line_height());
        let caret = self.glyph_extent(&font, self.font_size());
        let Some(range) = range else {
            return SetCell {
                rows: Vec::new(),
                ascent: strut.ascent,
            };
        };
        let mut builder = RowBuilder::new(x, x + room, px(0.), strut).with_caret(caret);
        self.place_cell(&items.items, &range, header, &mut builder);
        let mut rows = builder.finish();
        assign_ranges(&mut rows, range, self.text);
        for row in &mut rows {
            let shift = aligned(alignment, room, row.right() - x);
            row.left += shift;
            for piece in &mut row.pieces {
                piece.x += shift;
            }
        }
        let ascent = rows.first().map_or(strut.ascent, |row| {
            row.caret_top + caret.ascent.min(strut.ascent)
        });
        SetCell { rows, ascent }
    }

    /// Places the items inside a cell's text, bold in the header.
    fn place_cell(
        &mut self,
        items: &[Item],
        cell: &Range<usize>,
        header: bool,
        builder: &mut RowBuilder,
    ) {
        let items = cell_items(items, cell, header);
        let mut index = 0;
        while index < items.len() {
            index = self.place_item(&items, index, builder);
        }
    }

    /// The table's columns: from this frame's, or worked out from every
    /// row's measured cells.
    pub(super) fn columns(&mut self, row: &TableRowPlan) -> Arc<Columns> {
        self.resources.tables.check(self.context);
        if let Some(columns) = self.resources.tables.columns(row.table_start) {
            return columns;
        }
        let tree = self.context.source.tree();
        let lines: Vec<usize> = table_node_at(tree, row.table_start)
            .filter(|&id| matches!(tree.node(id).kind, NodeKind::Table { .. }))
            .map(|id| {
                let rows = &tree.node(id).children;
                rows.iter()
                    .map(|&row| tree.lines().line_of(tree.node(row).range.start))
                    .collect()
            })
            .unwrap_or_default();
        let mut natural: Vec<Pixels> = Vec::new();
        let mut least: Vec<Pixels> = Vec::new();
        for (index, line) in lines.into_iter().enumerate() {
            let widths = self.row_widths(line, index == 0);
            for (column, (one_line, narrowest)) in widths.cells.iter().enumerate() {
                if natural.len() <= column {
                    natural.push(px(0.));
                    least.push(px(0.));
                }
                natural[column] = natural[column].max(*one_line);
                least[column] = least[column].max(*narrowest);
            }
        }
        let padding = self.theme().table.cell_padding_x * 2.;
        let padded = |widths: Vec<Pixels>| -> Vec<Pixels> {
            widths.into_iter().map(|width| width + padding).collect()
        };
        let available = self.context.column_width - self.frame.right - self.frame.left;
        let widths = fit_widths(&padded(natural), &padded(least), available);
        let columns = Arc::new(Columns::from_widths(widths));
        self.resources
            .tables
            .columns
            .insert(row.table_start, columns.clone());
        columns
    }

    /// A row's measured cells: kept from before when the row's text and
    /// what's selected in it are the same, else planned and measured now.
    fn row_widths(&mut self, line: usize, header: bool) -> Arc<RowWidths> {
        let source = self.context.source;
        let range = source.line_range(line);
        let key = row_key(&source.text()[range.clone()], header, &range, self.context);
        if let Some(widths) = self.resources.tables.rows.get(&key) {
            return widths.clone();
        }
        let input = RenderInput {
            text: source.text(),
            tree: source.tree(),
            selections: self.context.selections,
            settings: self.context.reveal,
        };
        let Some(plan) = plan_lines(&input, line..line + 1).lines.pop() else {
            return Arc::default();
        };
        let widths = Arc::new(self.measure_row(&plan, header));
        let settled = plan
            .widgets
            .iter()
            .all(|widget| crate::line_cache::is_settled(&widget.kind));
        let store = &mut self.resources.tables;
        if settled {
            if store.rows.len() >= ROW_LIMIT {
                store.rows.clear();
            }
            store.rows.insert(key, widths.clone());
        }
        widths
    }

    /// Measures each cell of a planned row, laying it out on one line.
    fn measure_row(&mut self, plan: &LinePlan, header: bool) -> RowWidths {
        let Some(row) = &plan.table_row else {
            return RowWidths::default();
        };
        let frame = self.frame.clone();
        let mut layouter = LineLayouter::new(plan, frame, None, self.context, self.resources);
        let items = line_items(plan);
        let cells = row
            .cells
            .iter()
            .map(|cell| {
                let relative = cell.start - plan.range.start..cell.end - plan.range.start;
                layouter.measure_cell(&items.items, &relative, header)
            })
            .collect();
        RowWidths { cells }
    }

    /// A cell's width on one line, and its widest word or equation.
    fn measure_cell(
        &mut self,
        items: &[Item],
        cell: &Range<usize>,
        header: bool,
    ) -> (Pixels, Pixels) {
        let font = self.line_font();
        let strut = self.strut(&font, self.font_size(), self.line_height());
        let mut builder = RowBuilder::new(px(0.), UNBOUNDED, px(0.), strut);
        self.place_cell(items, cell, header, &mut builder);
        let rows = builder.finish();
        let natural = rows.iter().map(VisualRow::right).fold(px(0.), Pixels::max);
        let least = rows
            .iter()
            .flat_map(|row| row.pieces.iter())
            .map(|piece| match &piece.content {
                PieceContent::Text(text) => widest_word(&text.shaped, &text.shaped.text),
                _ => piece.width,
            })
            .fold(px(0.), Pixels::max);
        (natural, least)
    }
}

/// Room for a cell being measured on one line.
const UNBOUNDED: Pixels = px(1.0e6);

/// What a row's measurement depends on besides the frame's: its text,
/// whether it's the header, and the selections and composition in it,
/// which reveal markup.
fn row_key(text: &str, header: bool, line: &Range<usize>, context: &LayoutContext<'_>) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    header.hash(&mut hasher);
    let relative = |range: &Range<usize>| {
        let touches = range.start <= line.end && range.end >= line.start;
        touches.then(|| {
            (
                range.start.saturating_sub(line.start),
                range.end.saturating_sub(line.start),
            )
        })
    };
    for selection in context.selections {
        relative(selection).hash(&mut hasher);
    }
    context.marked.as_ref().and_then(relative).hash(&mut hasher);
    hasher.finish()
}

/// The items inside a cell's text, clipped to it; the header's text is
/// bold.
fn cell_items(items: &[Item], cell: &Range<usize>, header: bool) -> Vec<Item> {
    let inside = |range: &Range<usize>| cell.start <= range.start && range.end <= cell.end;
    items
        .iter()
        .filter_map(|item| match item {
            Item::Text { range, styles } => {
                let clipped = range.start.max(cell.start)..range.end.min(cell.end);
                let mut styles = styles.clone();
                if header && !styles.contains(&StyleKey::Strong) {
                    styles.push(StyleKey::Strong);
                }
                (clipped.start < clipped.end).then_some(Item::Text {
                    range: clipped,
                    styles,
                })
            }
            Item::Inline { range, .. } | Item::Break { range } if inside(range) => {
                Some(item.clone())
            }
            _ => None,
        })
        .collect()
}

/// Column widths: each column's widest cell when the table fits. When
/// it doesn't, every column keeps room for its longest word or equation
/// and the rest of the width goes to the columns that have more to wrap.
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

/// How far a row `width` wide moves right to sit in `room` as its column
/// aligns.
fn aligned(alignment: Option<Alignment>, room: Pixels, width: Pixels) -> Pixels {
    let spare = (room - width).max(px(0.));
    match alignment {
        Some(Alignment::Center) => spare / 2.,
        Some(Alignment::Right) => spare,
        _ => px(0.),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_places_cells_within_their_column() {
        let at = |alignment| aligned(Some(alignment), px(100.), px(40.));
        assert_eq!(at(Alignment::Right), px(60.));
        assert_eq!(at(Alignment::Center), px(30.));
        assert_eq!(at(Alignment::Left), px(0.));
        assert_eq!(aligned(Some(Alignment::Right), px(10.), px(40.)), px(0.));
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

    #[test]
    fn columns_sit_side_by_side() {
        let columns = Columns::from_widths(vec![px(10.), px(20.), px(5.)]);
        assert_eq!(columns.lefts, [px(0.), px(10.), px(30.)]);
        assert_eq!(columns.width(), px(35.));
    }
}
