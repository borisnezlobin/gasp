//! Writing a table back out: padded so its columns line up in the source,
//! or as tab-separated values for pasting into a spreadsheet.

use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use crate::syntax::Alignment;

use super::{CellPos, Table};

/// The fewest dashes a delimiter cell has, so `---` reads as a rule.
const MIN_WIDTH: usize = 3;

/// A table written out, and where each cell's text went in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    /// Each cell's text as [`super::cell_content`] finds it, relative to
    /// the start of `text`, header first.
    pub cells: Vec<Vec<Range<usize>>>,
}

impl Formatted {
    /// Where a cell's text starts, relative to the table.
    pub fn content(&self, at: CellPos) -> Option<Range<usize>> {
        self.cells.get(at.row)?.get(at.column).cloned()
    }
}

/// How many columns `text` takes in a monospace font.
pub fn display_width(text: &str) -> usize {
    text.width()
}

impl Table {
    /// Every row as wide as the widest, and every column as wide as its
    /// widest cell.
    fn widths(&self) -> Vec<usize> {
        (0..self.column_count())
            .map(|column| {
                self.rows
                    .iter()
                    .filter_map(|row| row.get(column))
                    .map(|cell| display_width(cell))
                    .fold(MIN_WIDTH, usize::max)
            })
            .collect()
    }

    /// The table as Markdown with its columns padded to line up, each
    /// cell's text placed by its column's alignment. Short rows gain empty
    /// cells.
    pub fn format(&self) -> Formatted {
        let widths = self.widths();
        let mut text = String::new();
        let mut cells = Vec::with_capacity(self.rows.len());
        for (index, row) in self.rows.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            let placed = self.write_row(&mut text, &widths, |column| {
                row.get(column).map_or("", String::as_str)
            });
            cells.push(placed);
            if index == 0 {
                text.push('\n');
                self.write_delimiter(&mut text, &widths);
            }
        }
        Formatted { text, cells }
    }

    /// Writes one row and answers where each cell's text went.
    fn write_row<'a>(
        &self,
        text: &mut String,
        widths: &[usize],
        cell: impl Fn(usize) -> &'a str,
    ) -> Vec<Range<usize>> {
        text.push_str(&self.prefix);
        text.push('|');
        let mut placed = Vec::with_capacity(widths.len());
        for (column, &width) in widths.iter().enumerate() {
            let content = cell(column);
            let spare = width.saturating_sub(display_width(content));
            let before = match self.alignment(column) {
                Alignment::Right => spare,
                Alignment::Center => spare / 2,
                Alignment::Left | Alignment::None => 0,
            };
            text.push(' ');
            text.push_str(&" ".repeat(before));
            let start = text.len();
            text.push_str(content);
            placed.push(match content.is_empty() {
                // An empty cell's caret waits one space in.
                true => start - before..start - before,
                false => start..text.len(),
            });
            text.push_str(&" ".repeat(spare - before));
            text.push_str(" |");
        }
        placed
    }

    fn write_delimiter(&self, text: &mut String, widths: &[usize]) {
        text.push_str(&self.prefix);
        text.push('|');
        for (column, &width) in widths.iter().enumerate() {
            text.push(' ');
            text.push_str(&delimiter_cell(self.alignment(column), width));
            text.push_str(" |");
        }
    }

    /// The table as tab-separated values: one line per row, pipes
    /// unescaped.
    pub fn tsv(&self) -> String {
        let count = self.column_count();
        self.rows
            .iter()
            .map(|row| {
                (0..count)
                    .map(|column| {
                        let cell = row.get(column).map_or("", String::as_str);
                        cell.replace("\\|", "|")
                    })
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The cells in `rows` × `columns` as a table of their own: the first
    /// of them is its header, and the columns keep their alignments.
    pub fn slice(&self, rows: Range<usize>, columns: Range<usize>) -> Table {
        let cells: Vec<Vec<String>> = self.rows
            [rows.start.min(self.rows.len())..rows.end.min(self.rows.len())]
            .iter()
            .map(|row| {
                columns
                    .clone()
                    .map(|column| row.get(column).cloned().unwrap_or_default())
                    .collect()
            })
            .collect();
        Table {
            range: 0..0,
            prefix: String::new(),
            alignments: columns.map(|column| self.alignment(column)).collect(),
            rows: cells,
            cells: Vec::new(),
            row_lines: Vec::new(),
        }
    }
}

/// A delimiter row cell: dashes as wide as the column, with colons for
/// its alignment.
fn delimiter_cell(alignment: Alignment, width: usize) -> String {
    let width = width.max(MIN_WIDTH);
    match alignment {
        Alignment::None => "-".repeat(width),
        Alignment::Left => format!(":{}", "-".repeat(width - 1)),
        Alignment::Right => format!("{}:", "-".repeat(width - 1)),
        Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
    }
}
