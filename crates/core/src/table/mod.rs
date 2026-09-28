//! GFM tables as grids of cells: finding the table around an offset,
//! reading its rows, and the structural edits the table editor makes —
//! adding, deleting and moving rows and columns, aligning and sorting
//! columns — each written back as one transaction with the columns padded
//! so the source stays readable.
//!
//! Cells are split the way GFM splits them: at every pipe that isn't
//! escaped as `\|`, inside code spans too.

mod format;
mod ops;
#[cfg(test)]
mod tests;

use std::ops::Range;

use crate::syntax::{Alignment, NodeId, NodeKind, SyntaxTree};

pub use format::{Formatted, display_width};
pub use ops::{TableOp, insert_table, table_transaction};

/// A cell by its row (the header is row 0; the delimiter row isn't one)
/// and column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellPos {
    pub row: usize,
    pub column: usize,
}

impl CellPos {
    pub const fn new(row: usize, column: usize) -> Self {
        Self { row, column }
    }
}

/// A table read from its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// The table's whole lines, from the header line's start to the end of
    /// its last row, without the final line break.
    pub range: Range<usize>,
    /// What every line starts with before the table, such as `> ` in a
    /// quote, as the header line writes it.
    pub prefix: String,
    pub alignments: Vec<Alignment>,
    /// Each row's cells as written, trimmed, header first. Rows may be
    /// ragged.
    pub rows: Vec<Vec<String>>,
    /// Each cell's source between its pipes, in document offsets.
    pub cells: Vec<Vec<Range<usize>>>,
    /// Each row's line, header first, without its line break.
    pub row_lines: Vec<Range<usize>>,
}

impl Table {
    /// The table containing `offset`, ends included.
    pub fn at(text: &str, tree: &SyntaxTree, offset: usize) -> Option<Table> {
        let id = table_node_at(tree, offset)?;
        Table::from_node(text, tree, id)
    }

    /// The table a `Table` node stands for.
    pub fn from_node(text: &str, tree: &SyntaxTree, id: NodeId) -> Option<Table> {
        let node = tree.node(id);
        let NodeKind::Table { alignments } = &node.kind else {
            return None;
        };
        let lines = tree.lines();
        let first = lines.line_of(node.range.start);
        let last = lines.line_of(node.range.end);
        let quoted = tree.ancestors(id).any(|ancestor| {
            matches!(
                tree.node(ancestor).kind,
                NodeKind::BlockQuote | NodeKind::Callout(_)
            )
        });
        let line_ranges: Vec<Range<usize>> = (first..=last)
            .map(|line| lines.line_range(text, line))
            .collect();
        let mut table = Table::parse_lines(text, &line_ranges, quoted)?;
        // The parser's alignments win where it has them, as it drew them.
        for (column, alignment) in alignments.iter().enumerate() {
            if let Some(slot) = table.alignments.get_mut(column) {
                *slot = *alignment;
            }
        }
        Some(table)
    }

    /// Reads a table from its lines: the header, the delimiter row, then
    /// the body. `quoted` lets lines start with `>` markers.
    pub fn parse_lines(text: &str, lines: &[Range<usize>], quoted: bool) -> Option<Table> {
        let (header, rest) = lines.split_first()?;
        let (delimiter, body) = rest.split_first()?;
        let header_prefix = prefix_len(&text[header.clone()], quoted);
        let prefix = text[header.start..header.start + header_prefix].to_owned();
        let split = |line: &Range<usize>| -> Vec<Range<usize>> {
            let skip = prefix_len(&text[line.clone()], quoted);
            split_cells(&text[line.clone()], skip)
                .into_iter()
                .map(|cell| line.start + cell.start..line.start + cell.end)
                .collect()
        };
        let alignments = split(delimiter)
            .into_iter()
            .map(|cell| parse_alignment(&text[cell]))
            .collect();
        let row_lines: Vec<Range<usize>> = std::iter::once(header).chain(body).cloned().collect();
        let cells: Vec<Vec<Range<usize>>> = row_lines.iter().map(split).collect();
        let rows = cells
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| text[cell.clone()].trim().to_owned())
                    .collect()
            })
            .collect();
        Some(Table {
            range: header.start..lines.last().map_or(header.end, |line| line.end),
            prefix,
            alignments,
            rows,
            cells,
            row_lines,
        })
    }

    /// How many columns the table has: its widest row's, or its
    /// delimiter row's when that's wider.
    pub fn column_count(&self) -> usize {
        self.rows
            .iter()
            .map(Vec::len)
            .chain([self.alignments.len()])
            .max()
            .unwrap_or(0)
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The cell whose source holds `offset`, pipes and padding counting
    /// for the nearest cell on their row. On the delimiter row it's the
    /// header's.
    pub fn cell_at(&self, offset: usize) -> Option<CellPos> {
        let row = self
            .row_lines
            .iter()
            .rposition(|line| line.start <= offset)
            .unwrap_or(0);
        let cells = self.cells.get(row)?;
        let column = cells
            .iter()
            .position(|cell| offset <= cell.end)
            .unwrap_or(cells.len().saturating_sub(1));
        (!cells.is_empty()).then_some(CellPos::new(row, column))
    }

    /// Where a cell's text is in the document: its source without the
    /// padding around it. An empty cell's is a spot one space in.
    pub fn content(&self, text: &str, at: CellPos) -> Option<Range<usize>> {
        let cell = self.cells.get(at.row)?.get(at.column)?;
        Some(cell_content(text, cell))
    }

    /// The cell's text, `""` past a ragged row's end.
    pub fn text(&self, at: CellPos) -> &str {
        self.rows
            .get(at.row)
            .and_then(|row| row.get(at.column))
            .map_or("", String::as_str)
    }

    pub fn alignment(&self, column: usize) -> Alignment {
        self.alignments
            .get(column)
            .copied()
            .unwrap_or(Alignment::None)
    }
}

/// The id of the innermost table containing `offset`.
pub fn table_node_at(tree: &SyntaxTree, offset: usize) -> Option<NodeId> {
    tree.path_at(offset)
        .into_iter()
        .rev()
        .find(|&id| matches!(tree.node(id).kind, NodeKind::Table { .. }))
}

/// A cell's text in the document: `cell`, the source between its pipes,
/// without the spaces around it. An empty cell's text is an empty range
/// one space in, where typing reads as `| x |`.
pub fn cell_content(text: &str, cell: &Range<usize>) -> Range<usize> {
    let source = &text[cell.clone()];
    let start = cell.start + (source.len() - source.trim_start().len());
    let end = cell.start + source.trim_end().len();
    if end > start {
        return start..end;
    }
    let at = cell.start + usize::from(!cell.is_empty() && source.starts_with([' ', '\t']));
    at..at
}

/// How many bytes a table line starts with before its first cell: its
/// indentation, and in a quote its `>` markers.
fn prefix_len(line: &str, quoted: bool) -> usize {
    line.bytes()
        .take_while(|&byte| matches!(byte, b' ' | b'\t') || quoted && byte == b'>')
        .count()
}

/// Whether the byte at `at` is a pipe that splits cells.
fn is_pipe(bytes: &[u8], at: usize) -> bool {
    bytes[at] == b'|' && (at == 0 || bytes[at - 1] != b'\\')
}

/// The source between the pipes of a row, from `start`, as ranges of
/// `line`. Leading and trailing pipes are optional, as GFM has them.
pub(crate) fn split_cells(line: &str, start: usize) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let end = line.trim_end().len().max(start);
    let mut from = start;
    let first = line[start..end].find(|c: char| !c.is_whitespace());
    if let Some(first) = first.map(|at| start + at)
        && is_pipe(bytes, first)
    {
        from = first + 1;
    }
    let mut to = end;
    if to > from && is_pipe(bytes, to - 1) {
        to -= 1;
    }
    let mut cells = Vec::new();
    let mut cell_start = from;
    for at in from..to {
        if is_pipe(bytes, at) {
            cells.push(cell_start..at);
            cell_start = at + 1;
        }
    }
    cells.push(cell_start..to);
    cells
}

fn parse_alignment(cell: &str) -> Alignment {
    let cell = cell.trim();
    match (cell.starts_with(':'), cell.ends_with(':') && cell.len() > 1) {
        (true, true) => Alignment::Center,
        (true, false) => Alignment::Left,
        (false, true) => Alignment::Right,
        (false, false) => Alignment::None,
    }
}

/// Text pasted into a cell as one line of it: each line trimmed, blank
/// ones dropped and the rest joined by spaces, pipes escaped. A copied
/// line's own line break goes with it rather than becoming a space.
pub fn cell_paste_text(pasted: &str) -> String {
    let lines: Vec<&str> = pasted
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    cell_text(&lines.join(" "))
}

/// Text typed into a cell as the cell can hold it: a pipe is escaped so it
/// doesn't split the cell, and a line break, which would end the row,
/// becomes a space.
pub fn cell_text(typed: &str) -> String {
    let mut out = String::with_capacity(typed.len());
    let mut previous = None;
    for character in typed.chars() {
        match character {
            '|' if previous != Some('\\') => out.push_str("\\|"),
            '\r' => {}
            '\n' => out.push(' '),
            other => out.push(other),
        }
        previous = Some(character);
    }
    out
}
