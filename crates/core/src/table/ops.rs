//! The table editor's structural edits. Each reads the table around the
//! cell it acts on, changes the grid, and writes the whole table back
//! padded, as one transaction tagged with the edit's command id, with the
//! caret in the cell the edit leaves you in.

use std::cmp::Ordering;

use crate::document::{Document, Selection};
use crate::syntax::{Alignment, SyntaxTree};
use crate::transaction::{ChangeSet, Origin, Transaction};

use super::{CellPos, Table};

/// A structural edit, named after the menu item that makes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TableOp {
    InsertRowAbove,
    InsertRowBelow,
    InsertColumnLeft,
    InsertColumnRight,
    DeleteRow,
    DeleteColumn,
    MoveRowUp,
    MoveRowDown,
    MoveColumnLeft,
    MoveColumnRight,
    Align(Alignment),
    SortAscending,
    SortDescending,
    DeleteTable,
    /// Pads the columns so they line up in the source; nothing else.
    Format,
    /// Empties the cells in a block, as Delete does to selected cells.
    Clear {
        rows: (usize, usize),
        columns: (usize, usize),
    },
    /// Moves a row to another row's place, as dragging its handle does.
    MoveRow {
        to: usize,
    },
    /// Moves a column to another column's place.
    MoveColumn {
        to: usize,
    },
}

/// (op, command id) for the edits a command runs.
const COMMANDS: [(TableOp, &str); 16] = [
    (TableOp::InsertRowAbove, "table.insert-row-above"),
    (TableOp::InsertRowBelow, "table.insert-row-below"),
    (TableOp::InsertColumnLeft, "table.insert-column-left"),
    (TableOp::InsertColumnRight, "table.insert-column-right"),
    (TableOp::DeleteRow, "table.delete-row"),
    (TableOp::DeleteColumn, "table.delete-column"),
    (TableOp::MoveRowUp, "table.move-row-up"),
    (TableOp::MoveRowDown, "table.move-row-down"),
    (TableOp::MoveColumnLeft, "table.move-column-left"),
    (TableOp::MoveColumnRight, "table.move-column-right"),
    (TableOp::Align(Alignment::Left), "table.align-left"),
    (TableOp::Align(Alignment::Center), "table.align-center"),
    (TableOp::Align(Alignment::Right), "table.align-right"),
    (TableOp::SortAscending, "table.sort-ascending"),
    (TableOp::SortDescending, "table.sort-descending"),
    (TableOp::DeleteTable, "table.delete"),
];

impl TableOp {
    /// Every edit a command runs, in menu order.
    pub fn commands() -> impl Iterator<Item = (TableOp, &'static str)> {
        COMMANDS.into_iter()
    }

    pub fn from_command_id(id: &str) -> Option<TableOp> {
        COMMANDS
            .iter()
            .find(|(_, name)| *name == id)
            .map(|(op, _)| *op)
    }

    /// The command id, such as `table.insert-row-below`. Edits no command
    /// runs, such as a drag, share `table.edit`.
    pub fn command_id(self) -> &'static str {
        COMMANDS
            .iter()
            .find(|(op, _)| *op == self)
            .map_or("table.edit", |(_, name)| name)
    }

    /// Whether the edit does anything to `table` at `at`: the header row
    /// can't be deleted or moved below the body, the first body row can't
    /// move up, the only column can't go, and so on.
    pub fn applies(self, table: &Table, at: CellPos) -> bool {
        let rows = table.row_count();
        let columns = table.column_count();
        let body = at.row > 0;
        match self {
            TableOp::InsertRowAbove | TableOp::DeleteRow => body,
            TableOp::MoveRowUp => at.row > 1,
            TableOp::MoveRowDown => body && at.row + 1 < rows,
            TableOp::DeleteColumn => columns > 1,
            TableOp::MoveColumnLeft => at.column > 0,
            TableOp::MoveColumnRight => at.column + 1 < columns,
            TableOp::Align(alignment) => table.alignment(at.column) != alignment,
            TableOp::SortAscending | TableOp::SortDescending => rows > 2,
            TableOp::MoveRow { to } => body && to > 0 && to < rows && to != at.row,
            TableOp::MoveColumn { to } => to < columns && to != at.column,
            _ => true,
        }
    }

    /// Makes the edit to the grid and answers the cell to leave the caret
    /// in, or `None` when it doesn't apply.
    fn apply(self, table: &mut Table, at: CellPos) -> Option<CellPos> {
        if !self.applies(table, at) {
            return None;
        }
        normalize(table);
        let moved = match self {
            TableOp::InsertRowAbove | TableOp::InsertRowBelow => insert_row(table, self, at),
            TableOp::InsertColumnLeft | TableOp::InsertColumnRight => {
                insert_column(table, self, at)
            }
            TableOp::DeleteRow => delete_row(table, at),
            TableOp::DeleteColumn => delete_column(table, at),
            TableOp::Align(alignment) => {
                table.alignments[at.column] = alignment;
                at
            }
            TableOp::SortAscending | TableOp::SortDescending => sort(table, self, at),
            TableOp::Clear { rows, columns } => clear(table, rows, columns, at),
            TableOp::DeleteTable | TableOp::Format => at,
            _ => self.move_line(table, at),
        };
        Some(moved)
    }

    /// Moving a row or a column, by one or to a place.
    fn move_line(self, table: &mut Table, at: CellPos) -> CellPos {
        match self {
            TableOp::MoveRowUp => move_row(table, at, at.row - 1),
            TableOp::MoveRowDown => move_row(table, at, at.row + 1),
            TableOp::MoveRow { to } => move_row(table, at, to),
            TableOp::MoveColumnLeft => move_column(table, at, at.column - 1),
            TableOp::MoveColumnRight => move_column(table, at, at.column + 1),
            TableOp::MoveColumn { to } => move_column(table, at, to),
            _ => at,
        }
    }
}

/// Every row as wide as the table, and an alignment for every column, so
/// edits can index freely. Extra cells in a long row stay, as columns.
fn normalize(table: &mut Table) {
    let columns = table.column_count();
    for row in &mut table.rows {
        row.resize(columns, String::new());
    }
    table.alignments.resize(columns, Alignment::None);
}

fn insert_row(table: &mut Table, op: TableOp, at: CellPos) -> CellPos {
    let row = match op {
        TableOp::InsertRowAbove => at.row,
        _ => at.row + 1,
    };
    let columns = table.column_count();
    table.rows.insert(row, vec![String::new(); columns]);
    CellPos::new(row, at.column)
}

fn insert_column(table: &mut Table, op: TableOp, at: CellPos) -> CellPos {
    let column = match op {
        TableOp::InsertColumnLeft => at.column,
        _ => at.column + 1,
    };
    for row in &mut table.rows {
        row.insert(column, String::new());
    }
    table.alignments.insert(column, Alignment::None);
    CellPos::new(at.row, column)
}

/// The caret goes to the row that takes the deleted one's place, or the
/// one above when it was the last.
fn delete_row(table: &mut Table, at: CellPos) -> CellPos {
    table.rows.remove(at.row);
    CellPos::new(at.row.min(table.rows.len() - 1), at.column)
}

fn delete_column(table: &mut Table, at: CellPos) -> CellPos {
    for row in &mut table.rows {
        row.remove(at.column);
    }
    table.alignments.remove(at.column);
    CellPos::new(at.row, at.column.min(table.column_count() - 1))
}

/// The caret moves with the row.
fn move_row(table: &mut Table, at: CellPos, to: usize) -> CellPos {
    let row = table.rows.remove(at.row);
    table.rows.insert(to, row);
    CellPos::new(to, at.column)
}

/// The caret and the alignment move with the column.
fn move_column(table: &mut Table, at: CellPos, to: usize) -> CellPos {
    for row in &mut table.rows {
        let cell = row.remove(at.column);
        row.insert(to, cell);
    }
    let alignment = table.alignments.remove(at.column);
    table.alignments.insert(to, alignment);
    CellPos::new(at.row, to)
}

/// Sorts the body by a column, keeping rows that compare equal in order.
/// The header stays; the caret stays with its row.
fn sort(table: &mut Table, op: TableOp, at: CellPos) -> CellPos {
    let column = at.column;
    let mut body: Vec<(usize, Vec<String>)> = table.rows.drain(1..).enumerate().collect();
    let descending = op == TableOp::SortDescending;
    body.sort_by(|(_, a), (_, b)| sort_order(&a[column], &b[column], descending));
    let row = body
        .iter()
        .position(|(index, _)| index + 1 == at.row)
        .map_or(at.row, |position| position + 1);
    table.rows.extend(body.into_iter().map(|(_, row)| row));
    CellPos::new(row, column)
}

/// Numbers compare as numbers, anything else by its text without regard
/// to case. Empty cells come last either way up, so a sort never buries
/// the rows with something in them.
fn sort_order(a: &str, b: &str, descending: bool) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    let number = |text: &str| text.trim().replace(',', "").parse::<f64>().ok();
    let order = match (number(a), number(b)) {
        (Some(a), Some(b)) => a.total_cmp(&b),
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    };
    match descending {
        true => order.reverse(),
        false => order,
    }
}

fn clear(table: &mut Table, rows: (usize, usize), columns: (usize, usize), at: CellPos) -> CellPos {
    for row in table.rows.iter_mut().take(rows.1 + 1).skip(rows.0) {
        for cell in row.iter_mut().take(columns.1 + 1).skip(columns.0) {
            cell.clear();
        }
    }
    at
}

/// The transaction making `op` on the table around `at_offset`, or
/// `None` when there's no table there or the edit doesn't apply. The
/// caret keeps its place in its cell's text where the cell survives.
pub fn table_transaction(
    doc: &Document,
    tree: &SyntaxTree,
    op: TableOp,
    at_offset: usize,
    timestamp_ms: u64,
) -> Option<Transaction> {
    let text = doc.to_string();
    let mut table = Table::at(&text, tree, at_offset)?;
    let at = table.cell_at(at_offset)?;
    let within = table
        .content(&text, at)
        .map_or(0, |content| at_offset.saturating_sub(content.start));
    let origin = Origin::command(op.command_id());
    if op == TableOp::DeleteTable {
        return Some(delete_table(&text, &table, origin, timestamp_ms));
    }
    let moved = op.apply(&mut table, at)?;
    let formatted = table.format();
    let content = formatted.content(moved).unwrap_or(0..0);
    let keeps_text = !matches!(
        op,
        TableOp::InsertRowAbove
            | TableOp::InsertRowBelow
            | TableOp::InsertColumnLeft
            | TableOp::InsertColumnRight
            | TableOp::DeleteRow
            | TableOp::DeleteColumn
    );
    let caret = table.range.start
        + content.start
        + if keeps_text {
            within.min(content.len())
        } else {
            0
        };
    let changes = ChangeSet::replace(table.range.clone(), formatted.text);
    Some(Transaction::new(changes, origin, timestamp_ms).with_selection(Selection::cursor(caret)))
}

/// Takes the table's lines out with the line break after them, and the
/// blank line after it too when one is before it, so no double gap is
/// left.
fn delete_table(text: &str, table: &Table, origin: Origin, timestamp_ms: u64) -> Transaction {
    let start = table.range.start;
    let mut end = table.range.end;
    for ending in ["\r\n", "\n"] {
        if text[end..].starts_with(ending) {
            end += ending.len();
            break;
        }
    }
    let blank_before = start == 0 || text[..start].ends_with("\n\n");
    if blank_before && text[end..].starts_with('\n') {
        end += 1;
    }
    let changes = ChangeSet::replace(start..end, "");
    Transaction::new(changes, origin, timestamp_ms).with_selection(Selection::cursor(start))
}

/// Columns and body rows in a table [`insert_table`] makes.
const NEW_COLUMNS: usize = 3;
const NEW_BODY_ROWS: usize = 2;

/// Inserts an empty table with a header row and two body rows, three
/// columns wide, on lines of its own below the caret's line (or on it,
/// when it's blank), with the caret in the first header cell.
pub fn insert_table(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    let at = selection.primary().to();
    let line = doc.line_of_offset(at);
    let line_text = doc.line_text(line);
    let blank = line_text.trim().is_empty();
    let table = Table {
        range: 0..0,
        prefix: String::new(),
        alignments: vec![Alignment::None; NEW_COLUMNS],
        rows: vec![vec![String::new(); NEW_COLUMNS]; NEW_BODY_ROWS + 1],
        cells: Vec::new(),
        row_lines: Vec::new(),
    };
    let formatted = table.format();
    let (insert_at, before) = match blank {
        true => (doc.line_start(line), ""),
        false => (doc.line_end(line), "\n\n"),
    };
    let replaced_end = match blank {
        true => doc.line_end(line),
        false => insert_at,
    };
    let next_blank = line + 1 >= doc.line_count() || doc.line_text(line + 1).trim().is_empty();
    let after = if next_blank { "" } else { "\n" };
    let text = format!("{before}{}{after}", formatted.text);
    let caret = insert_at + before.len() + formatted.cells[0][0].start;
    let changes = ChangeSet::replace(insert_at..replaced_end, text);
    Transaction::new(changes, Origin::command("table.insert"), timestamp_ms)
        .with_selection(Selection::cursor(caret))
}
