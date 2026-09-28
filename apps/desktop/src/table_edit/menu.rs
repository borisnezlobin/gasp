//! The table's part of the note's right-click menu: on a cell or a row
//! or column handle, the table editor's edits for that cell, each greyed
//! out where it doesn't apply, above the usual editing items. Only the
//! insertions sit at the top level; the rest go in Row, Column and Table
//! submenus.

use editor_core::syntax::Alignment;
use editor_core::table::TableOp;
use gpui::{App, Entity};

use crate::editor::EditorView;
use crate::icons::IconName;
use crate::ui::MenuItem;

type Item = (&'static str, &'static str, IconName);

const INSERT: [Item; 4] = [
    (
        "table.insert-row-above",
        "Insert row above",
        IconName::RowsPlusTop,
    ),
    (
        "table.insert-row-below",
        "Insert row below",
        IconName::RowsPlusBottom,
    ),
    (
        "table.insert-column-left",
        "Insert column left",
        IconName::ColumnsPlusLeft,
    ),
    (
        "table.insert-column-right",
        "Insert column right",
        IconName::ColumnsPlusRight,
    ),
];

const ROW: [Item; 3] = [
    ("table.move-row-up", "Move up", IconName::ArrowUp),
    ("table.move-row-down", "Move down", IconName::ArrowDown),
    ("table.delete-row", "Delete row", IconName::Trash),
];

const MOVE_COLUMN: [Item; 2] = [
    ("table.move-column-left", "Move left", IconName::ArrowLeft),
    (
        "table.move-column-right",
        "Move right",
        IconName::ArrowRight,
    ),
];

const ALIGN: [Item; 3] = [
    ("table.align-left", "Left", IconName::TextAlignLeft),
    ("table.align-center", "Center", IconName::TextAlignCenter),
    ("table.align-right", "Right", IconName::TextAlignRight),
];

const SORT: [Item; 2] = [
    ("table.sort-ascending", "A→Z", IconName::SortAscending),
    ("table.sort-descending", "Z→A", IconName::SortDescending),
];

const DELETE_COLUMN: Item = ("table.delete-column", "Delete column", IconName::Trash);

const TABLE: [Item; 4] = [
    ("table.copy-markdown", "Copy as Markdown", IconName::Copy),
    (
        "table.copy-tsv",
        "Copy as tab-separated text",
        IconName::Copy,
    ),
    ("table.edit-as-markdown", "Edit as Markdown", IconName::Code),
    ("table.delete", "Delete table", IconName::Trash),
];

/// The table items for a menu opened on the cell holding `at`: the four
/// insertions, then what acts on its row, its column and the table, each
/// in a submenu so the menu stays short.
pub fn table_items(editor: &Entity<EditorView>, at: usize, cx: &App) -> Vec<MenuItem> {
    let item = |(id, label, icon): Item| {
        table_item(editor, at, id, cx)
            .with_label(label)
            .with_icon(icon)
    };
    let mut items: Vec<MenuItem> = INSERT.into_iter().map(item).collect();
    items.push(MenuItem::Separator);
    let row = ROW.into_iter().map(item).collect();
    items.push(MenuItem::submenu("Row", row).with_icon(IconName::Rows));
    let column = column_items(editor, at, &item, cx);
    items.push(MenuItem::submenu("Column", column).with_icon(IconName::Columns));
    let table = TABLE.into_iter().map(item).collect();
    items.push(MenuItem::submenu("Table", table).with_icon(IconName::Table));
    items.push(MenuItem::Separator);
    items
}

/// The column's submenu: moving it, its alignment with the current one
/// checked, sorting by it, and deleting it.
fn column_items(
    editor: &Entity<EditorView>,
    at: usize,
    item: &dyn Fn(Item) -> MenuItem,
    cx: &App,
) -> Vec<MenuItem> {
    let current = editor
        .read(cx)
        .grid_table(at)
        .and_then(|table| Some(table.alignment(table.cell_at(at)?.column)));
    let align = ALIGN
        .into_iter()
        .map(|entry| {
            let alignment = TableOp::from_command_id(entry.0).map(|op| match op {
                TableOp::Align(alignment) => alignment,
                _ => Alignment::None,
            });
            item(entry).checked(alignment == current)
        })
        .collect();
    let sort = SORT.into_iter().map(item).collect();
    let mut items: Vec<MenuItem> = MOVE_COLUMN.into_iter().map(item).collect();
    items.push(MenuItem::Separator);
    items.push(MenuItem::submenu("Align", align).with_icon(IconName::TextAlignLeft));
    items.push(MenuItem::submenu("Sort", sort).with_icon(IconName::SortAscending));
    items.push(MenuItem::Separator);
    items.push(item(DELETE_COLUMN));
    items
}

/// An item running table command `id` on the cell holding `at`, greyed
/// out where it doesn't apply there.
fn table_item(editor: &Entity<EditorView>, at: usize, id: &'static str, cx: &App) -> MenuItem {
    let op = TableOp::from_command_id(id);
    let applies = op.is_none_or(|op| editor.read(cx).table_op_applies(op, at));
    let editor = editor.downgrade();
    MenuItem::command(id, cx)
        .with_handler(move |_, cx| {
            editor
                .update(cx, |editor, cx| match op {
                    Some(op) => editor.run_table_op_at(op, at, cx),
                    None => {
                        editor.run_table_command_at(id, at, cx);
                    }
                })
                .ok();
        })
        .disabled(!applies)
}

/// Every command id the table menu runs, for checking they're all
/// registered.
pub fn menu_commands() -> impl Iterator<Item = &'static str> {
    INSERT
        .iter()
        .chain(&ROW)
        .chain(&MOVE_COLUMN)
        .chain(&ALIGN)
        .chain(&SORT)
        .chain(std::iter::once(&DELETE_COLUMN))
        .chain(&TABLE)
        .map(|(id, ..)| *id)
}
