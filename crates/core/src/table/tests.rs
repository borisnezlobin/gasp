use super::*;
use crate::document::Document;
use crate::syntax::parse;

/// Makes `op` on the table in `text` with the caret at the `^` in it,
/// and answers the new text with `^` at the new caret.
fn edit(text: &str, op: TableOp) -> String {
    let at = text.find('^').expect("the text marks the caret");
    let text = text.replacen('^', "", 1);
    let doc = Document::from(text.as_str());
    let tree = parse(&text);
    let Some(transaction) = table_transaction(&doc, &tree, op, at, 0) else {
        return "(nothing)".into();
    };
    let caret = transaction.selection.as_ref().unwrap().primary().head;
    let mut edited = transaction.changes.apply_to_string(&text).unwrap();
    edited.insert(caret, '^');
    edited
}

fn table(text: &str) -> Table {
    Table::at(text, &parse(text), 0).expect("a table")
}

#[test]
fn rows_and_cells_are_read_from_the_source() {
    let text = "| a | b \\| c |\n|:--|--:|\n| 1 | `x` |\n";
    let table = table(text);
    assert_eq!(table.rows, [vec!["a", "b \\| c"], vec!["1", "`x`"]]);
    assert_eq!(table.alignments, [Alignment::Left, Alignment::Right]);
    assert_eq!(&text[table.cells[0][1].clone()], " b \\| c ");
    assert_eq!(table.range, 0..text.len() - 1);
    assert_eq!(
        table.cell_at(text.find('x').unwrap()),
        Some(CellPos::new(1, 1))
    );
    // The delimiter row counts as the header's; pipes as the nearest cell.
    assert_eq!(table.cell_at(17).map(|at| at.row), Some(0));
    assert_eq!(table.cell_at(0), Some(CellPos::new(0, 0)));
}

#[test]
fn rows_without_outer_pipes_split_the_same() {
    let text = "a | b\n--- | ---\n1 | 2";
    let table = table(text);
    assert_eq!(table.rows, [vec!["a", "b"], vec!["1", "2"]]);
}

#[test]
fn an_empty_cell_keeps_its_caret_one_space_in() {
    let text = "|     | b |\n| --- | --- |";
    let table = table(text);
    assert_eq!(table.content(text, CellPos::new(0, 0)), Some(2..2));
    assert_eq!(table.content(text, CellPos::new(0, 1)), Some(8..9));
}

#[test]
fn format_pads_columns_and_places_text_by_alignment() {
    let text = "| a | long header | c |\n|---|:-:|--:|\n| wide cell | x | 1 |";
    let formatted = table(text).format();
    assert_eq!(
        formatted.text,
        "| a         | long header |   c |\n\
         | --------- | :---------: | --: |\n\
         | wide cell |      x      |   1 |"
    );
    let content = formatted.content(CellPos::new(1, 1)).unwrap();
    assert_eq!(&formatted.text[content], "x");
}

#[test]
fn format_counts_wide_characters_by_their_columns() {
    let formatted = table("| 日本 | a |\n|---|---|\n| x | b |").format();
    assert_eq!(formatted.text.lines().nth(2), Some("| x    | b   |"));
}

#[test]
fn inserting_rows_puts_the_caret_in_the_new_row() {
    let text = "| a | b |\n|---|---|\n| 1 | ^2 |";
    assert_eq!(
        edit(text, TableOp::InsertRowBelow),
        "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n|     | ^    |"
    );
    assert_eq!(
        edit(text, TableOp::InsertRowAbove),
        "| a   | b   |\n| --- | --- |\n|     | ^    |\n| 1   | 2   |"
    );
}

#[test]
fn the_header_row_has_no_row_above_it_and_cant_be_deleted() {
    let text = "| ^a | b |\n|---|---|\n| 1 | 2 |";
    assert_eq!(edit(text, TableOp::InsertRowAbove), "(nothing)");
    assert_eq!(edit(text, TableOp::DeleteRow), "(nothing)");
    assert_eq!(edit(text, TableOp::MoveRowDown), "(nothing)");
    assert_eq!(edit(text, TableOp::MoveRowUp), "(nothing)");
}

#[test]
fn deleting_a_row_leaves_the_caret_in_the_next() {
    let text = "| a |\n|---|\n| ^1 |\n| 2 |\n| 3 |";
    assert_eq!(
        edit(text, TableOp::DeleteRow),
        "| a   |\n| --- |\n| ^2   |\n| 3   |"
    );
    let last = "| a |\n|---|\n| 1 |\n| ^2 |";
    assert_eq!(edit(last, TableOp::DeleteRow), "| a   |\n| --- |\n| ^1   |");
}

#[test]
fn moving_rows_keeps_the_caret_in_its_row() {
    let text = "| a |\n|---|\n| 1 |\n| t^wo |\n| 3 |";
    assert_eq!(
        edit(text, TableOp::MoveRowUp),
        "| a   |\n| --- |\n| t^wo |\n| 1   |\n| 3   |"
    );
    assert_eq!(
        edit(text, TableOp::MoveRowDown),
        "| a   |\n| --- |\n| 1   |\n| 3   |\n| t^wo |"
    );
    assert_eq!(
        edit(text, TableOp::MoveRow { to: 1 }),
        "| a   |\n| --- |\n| t^wo |\n| 1   |\n| 3   |"
    );
    let first = "| a |\n|---|\n| ^1 |\n| 2 |";
    assert_eq!(edit(first, TableOp::MoveRowUp), "(nothing)");
    let last = "| a |\n|---|\n| 1 |\n| ^2 |";
    assert_eq!(edit(last, TableOp::MoveRowDown), "(nothing)");
}

#[test]
fn inserting_and_deleting_columns() {
    let text = "| a | ^b |\n|---|---|\n| 1 | 2 |";
    assert_eq!(
        edit(text, TableOp::InsertColumnLeft),
        "| a   | ^    | b   |\n| --- | --- | --- |\n| 1   |     | 2   |"
    );
    assert_eq!(
        edit(text, TableOp::InsertColumnRight),
        "| a   | b   | ^    |\n| --- | --- | --- |\n| 1   | 2   |     |"
    );
    assert_eq!(
        edit(text, TableOp::DeleteColumn),
        "| ^a   |\n| --- |\n| 1   |"
    );
    assert_eq!(edit("| ^a |\n|---|", TableOp::DeleteColumn), "(nothing)");
}

#[test]
fn moving_a_column_takes_its_alignment_with_it() {
    let text = "| a | b^b | c |\n|:--|--:|:-:|\n| 1 | 2 | 3 |";
    assert_eq!(
        edit(text, TableOp::MoveColumnLeft),
        "|  b^b | a   |  c  |\n| --: | :-- | :-: |\n|   2 | 1   |  3  |"
    );
    assert_eq!(
        edit(text, TableOp::MoveColumn { to: 2 }),
        "| a   |  c  |  b^b |\n| :-- | :-: | --: |\n| 1   |  3  |   2 |"
    );
    let first = "| ^a | b |\n|---|---|";
    assert_eq!(edit(first, TableOp::MoveColumnLeft), "(nothing)");
    assert_eq!(
        edit(first, TableOp::MoveColumnRight).lines().next(),
        Some("| b   | ^a   |")
    );
}

#[test]
fn aligning_writes_colons_in_the_delimiter_row() {
    let text = "| a | ^b |\n|---|---|\n| 1 | 2 |";
    let delimiter = |op| edit(text, op).lines().nth(1).unwrap().to_owned();
    assert_eq!(delimiter(TableOp::Align(Alignment::Left)), "| --- | :-- |");
    assert_eq!(
        delimiter(TableOp::Align(Alignment::Center)),
        "| --- | :-: |"
    );
    assert_eq!(delimiter(TableOp::Align(Alignment::Right)), "| --- | --: |");
    let right = "| a |\n|--:|\n| ^1 |";
    assert_eq!(edit(right, TableOp::Align(Alignment::Right)), "(nothing)");
}

#[test]
fn sorting_keeps_the_header_and_follows_the_caret_row() {
    let text = "| name | n |\n|---|---|\n| pear | 10 |\n| ^Apple | 9 |\n|  | 1 |\n| fig | 100 |";
    assert_eq!(
        edit(text, TableOp::SortAscending),
        "| name  | n   |\n| ----- | --- |\n| ^Apple | 9   |\n| fig   | 100 |\n| pear  | 10  |\n|       | 1   |"
    );
    let numbers = "| name | ^n |\n|---|---|\n| pear | 10 |\n| Apple | 9 |\n| fig | 100 |";
    let sorted = edit(numbers, TableOp::SortDescending);
    let order: Vec<&str> = sorted
        .lines()
        .skip(2)
        .map(|line| line.split('|').nth(1).unwrap().trim())
        .collect();
    assert_eq!(order, ["fig", "pear", "Apple"], "numbers sort as numbers");
}

#[test]
fn ragged_rows_gain_cells_and_long_rows_keep_theirs() {
    let text = "| ^a | b |\n|---|---|\n| 1 |\n| 1 | 2 | 3 |";
    assert_eq!(
        edit(text, TableOp::Format),
        "| ^a   | b   |     |\n| --- | --- | --- |\n| 1   |     |     |\n| 1   | 2   | 3   |"
    );
}

#[test]
fn escaped_pipes_and_code_with_pipes_stay_in_their_cell() {
    let text = "| ^a | `x \\| y` |\n|---|---|\n| b\\|c | d |";
    let table = table(&text.replace('^', ""));
    assert_eq!(table.rows[0], ["a", "`x \\| y`"]);
    assert_eq!(table.rows[1], ["b\\|c", "d"]);
    assert_eq!(
        edit(text, TableOp::InsertColumnRight),
        "| a    | ^    | `x \\| y` |\n| ---- | --- | -------- |\n| b\\|c |     | d        |"
    );
    assert_eq!(table.tsv(), "a\t`x | y`\nb|c\td");
}

#[test]
fn typed_pipes_are_escaped_and_line_breaks_become_spaces() {
    assert_eq!(cell_text("a|b"), "a\\|b");
    assert_eq!(cell_text("a\\|b"), "a\\|b");
    assert_eq!(cell_text("one\r\ntwo\nthree"), "one two three");
}

#[test]
fn pasted_lines_flatten_to_one() {
    // A line copied whole brings its line break, which goes.
    assert_eq!(cell_paste_text("| a | b |\n"), "\\| a \\| b \\|");
    assert_eq!(
        cell_paste_text("  first\r\n\n  second  \nthird\n\n"),
        "first second third"
    );
    assert_eq!(cell_paste_text("\n"), "");
}

#[test]
fn tables_in_quotes_keep_their_markers() {
    let text = "> | ^a | b |\n> |---|---|\n> | 1 | 2 |";
    assert_eq!(
        edit(text, TableOp::InsertRowBelow),
        "> | a   | b   |\n> | --- | --- |\n> | ^    |     |\n> | 1   | 2   |"
    );
}

#[test]
fn clearing_cells_empties_a_block() {
    let text = "| a | b | c |\n|---|---|---|\n| ^1 | 2 | 3 |\n| 4 | 5 | 6 |";
    let op = TableOp::Clear {
        rows: (1, 2),
        columns: (0, 1),
    };
    assert_eq!(
        edit(text, op),
        "| a   | b   | c   |\n| --- | --- | --- |\n| ^    |     | 3   |\n|     |     | 6   |"
    );
}

#[test]
fn deleting_the_table_leaves_one_gap() {
    let text = "before\n\n| ^a |\n|---|\n| 1 |\n\nafter";
    assert_eq!(edit(text, TableOp::DeleteTable), "before\n\n^after");
    let at_end = "x\n\n| ^a |\n|---|";
    assert_eq!(edit(at_end, TableOp::DeleteTable), "x\n\n^");
}

#[test]
fn an_inserted_table_is_a_table_with_the_caret_in_its_header() {
    for (text, at) in [("", 0), ("para", 4), ("para\n\nnext", 2)] {
        let doc = Document::from(text);
        let transaction = insert_table(&doc, &Selection::cursor(at), 0);
        let caret = transaction.selection.as_ref().unwrap().primary().head;
        let edited = transaction.changes.apply_to_string(text).unwrap();
        let table = Table::at(&edited, &parse(&edited), caret).expect("a table");
        assert_eq!(
            (table.row_count(), table.column_count()),
            (3, 3),
            "{edited:?}"
        );
        assert_eq!(table.cell_at(caret), Some(CellPos::new(0, 0)));
        assert_eq!(
            table.content(&edited, CellPos::new(0, 0)),
            Some(caret..caret)
        );
    }
}

#[test]
fn every_command_op_has_an_id() {
    for (op, id) in TableOp::commands() {
        assert_eq!(TableOp::from_command_id(id), Some(op));
        assert_eq!(op.command_id(), id);
    }
    assert_eq!(TableOp::MoveRow { to: 1 }.command_id(), "table.edit");
}

use crate::document::Selection;
