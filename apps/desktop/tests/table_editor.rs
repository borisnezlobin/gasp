//! The table editor through GPUI's test platform: clicking into a cell
//! and typing, moving between cells with Tab, Enter and the arrows, the
//! right-click menu's table items, and dragging a row or a column by its
//! handle. The test platform shapes every character as a fixed-width
//! glyph.

use editor_config::{Platform, RuleSet};
use editor_desktop::EditorView;
use editor_desktop::actions::bind_keys;
use editor_desktop::keymap::{RunCommand, editor_bindings};
use editor_desktop::table_edit::handles::{column_handle, row_handle};
use editor_desktop::table_edit::menu::table_items;
use editor_desktop::ui::{DropdownMenu, MenuItem};
use gpui::{
    AppContext, Entity, Focusable, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};

const NOTE: &str = "intro\n\n| name | n |\n| --- | --- |\n| pear | 10 |\n| fig | 2 |\n\nend";

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| EditorView::new(&text, Vec::new(), cx));
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    cx.run_until_parked();
    (view, cx)
}

fn text(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.text())
}

fn cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, _| view.cursor())
}

fn place_cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext, offset: usize) {
    view.update(cx, |view, cx| view.move_to(offset, false, cx));
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, command: &str) {
    let key = editor_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|(_, id)| id == command)
        .map(|(keystroke, _)| keystroke)
        .unwrap_or_else(|| panic!("{command} has no key"));
    cx.simulate_keystrokes(&key);
    cx.run_until_parked();
}

fn run(cx: &mut VisualTestContext, id: &str) {
    cx.dispatch_action(RunCommand {
        id: id.to_owned().into(),
    });
    cx.run_until_parked();
}

/// Where the text `needle` is drawn, at the middle of its first byte.
fn text_point(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    needle: &str,
) -> Point<Pixels> {
    let at = text(view, cx)
        .find(needle)
        .expect("the text is in the note");
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let placed = frame.line_containing(at).unwrap();
        let visual = &placed.visual;
        let row = &visual.rows[visual.row_for_offset(at - visual.start).unwrap()];
        let x = row.x_for(at - visual.start);
        point(
            frame.text_left + x + px(2.),
            placed.top + row.top + row.caret_top + row.caret_height / 2.,
        )
    })
}

fn click(cx: &mut VisualTestContext, position: Point<Pixels>) {
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn clicking_a_cell_puts_the_caret_in_its_text_and_typing_edits_the_source(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let pear = text_point(&view, cx, "pear");
    click(cx, pear);
    let at = NOTE.find("pear").unwrap();
    assert!(
        (at..=at + 1).contains(&cursor(&view, cx)),
        "the caret is in the cell"
    );
    let line = view.read_with(cx, |view, _| {
        view.frame()
            .unwrap()
            .line_containing(at)
            .unwrap()
            .visual
            .clone()
    });
    assert!(
        line.grid.is_some(),
        "the table stays a grid with the caret in it"
    );
    place_cursor(&view, cx, at + 4);
    cx.simulate_input("s|x");
    assert!(
        text(&view, cx).contains("| pears\\|x | 10 |"),
        "{}",
        text(&view, cx)
    );
    let still_grid = view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let placed = frame.line_containing(at).unwrap();
        placed.visual.grid.is_some()
            && !placed
                .visual
                .pieces()
                .any(|piece| piece.is_text() && piece.range.start == 0)
    });
    assert!(still_grid, "the pipes never show");
    // Leaving the table pads its columns, as part of the typing's undo step.
    place_cursor(&view, cx, 0);
    assert!(
        text(&view, cx).contains("| pears\\|x | 10  |\n| fig      | 2   |"),
        "{}",
        text(&view, cx)
    );
    run(cx, "edit.undo");
    assert_eq!(text(&view, cx), NOTE);
}

#[gpui::test]
fn a_selection_in_a_cell_is_drawn(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let at = NOTE.find("pear").unwrap();
    view.update(cx, |view, cx| view.select(at, at + 4, cx));
    cx.run_until_parked();
    let rects = view.read_with(cx, |view, _| {
        view.frame()
            .unwrap()
            .range_rects(&(at..at + 4), view.theme())
    });
    assert_eq!(rects.len(), 1);
    assert!(rects[0].size.width > px(10.));
}

#[gpui::test]
fn tab_moves_through_cells_and_adds_a_row_after_the_last(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let pear = NOTE.find("pear").unwrap();
    place_cursor(&view, cx, pear);
    press(cx, "edit.indent");
    let ten = NOTE.find("10").unwrap();
    assert_eq!(
        view.read_with(cx, |view, _| view.selected_range()),
        ten..ten + 2,
        "Tab selects the next cell's text"
    );
    press(cx, "edit.outdent");
    assert_eq!(
        view.read_with(cx, |view, _| view.selected_range()),
        pear..pear + 4
    );
    let two = NOTE.find("| 2 |").unwrap() + 2;
    place_cursor(&view, cx, two);
    press(cx, "edit.indent");
    let text = text(&view, cx);
    assert!(
        text.contains("| fig  | 2   |\n|      |     |\n\nend"),
        "{text}"
    );
    let new_row = text.find("|      |").unwrap();
    assert_eq!(
        cursor(&view, cx),
        new_row + 2,
        "the caret is in the new row's first cell"
    );
}

#[gpui::test]
fn enter_goes_down_a_column_and_adds_a_row_at_the_bottom(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let ten = NOTE.find("10").unwrap();
    place_cursor(&view, cx, ten);
    press(cx, "edit.newline");
    let two = NOTE.find("| 2 |").unwrap() + 2;
    assert_eq!(
        cursor(&view, cx),
        two + 1,
        "the caret goes to the end of the cell below"
    );
    press(cx, "edit.newline");
    let text = text(&view, cx);
    assert_eq!(
        text.matches('\n').count(),
        NOTE.matches('\n').count() + 1,
        "{text}"
    );
    assert!(text.contains("|      |     |\n\nend"), "{text}");
}

#[gpui::test]
fn arrows_cross_cells_at_their_edges_and_up_and_down_keep_the_column(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let pear = NOTE.find("pear").unwrap();
    place_cursor(&view, cx, pear + 4);
    press(cx, "cursor.right");
    assert_eq!(
        cursor(&view, cx),
        NOTE.find("10").unwrap(),
        "into the next cell"
    );
    press(cx, "cursor.left");
    assert_eq!(
        cursor(&view, cx),
        pear + 4,
        "back to the end of the cell before"
    );
    press(cx, "cursor.down");
    let fig = NOTE.find("fig").unwrap();
    assert!(
        (fig..=fig + 3).contains(&cursor(&view, cx)),
        "down stays in the column"
    );
    press(cx, "cursor.up");
    press(cx, "cursor.up");
    let name = NOTE.find("name").unwrap();
    assert!(
        (name..=name + 4).contains(&cursor(&view, cx)),
        "up reaches the header"
    );
    press(cx, "cursor.up");
    assert!(
        cursor(&view, cx) < NOTE.find('|').unwrap(),
        "and past it, out of the table"
    );
}

#[gpui::test]
fn backspace_stops_at_a_cells_start(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let ten = NOTE.find("10").unwrap();
    place_cursor(&view, cx, ten);
    press(cx, "edit.delete-backward");
    assert_eq!(text(&view, cx), NOTE, "the pipe stays");
}

#[gpui::test]
fn menu_items_act_on_the_clicked_cell_and_grey_out_where_they_dont_apply(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let name = NOTE.find("name").unwrap();
    let items = cx.update(|_, cx| table_items(&view, name, cx));
    let disabled = |label: &str| {
        items
            .iter()
            .find_map(|item| match item {
                MenuItem::Entry(entry) if entry.label == label => Some(entry.disabled),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {label} item"))
    };
    assert!(
        disabled("Insert row above"),
        "nothing goes above the header"
    );
    assert!(disabled("Delete row"), "the header can't be deleted");
    assert!(
        disabled("Move row down"),
        "the header can't move below the body"
    );
    assert!(disabled("Move column left"), "the first column is leftmost");
    assert!(!disabled("Move column right"));
    assert!(!disabled("Insert row below"));
    let fig = NOTE.find("fig").unwrap();
    let items = cx.update(|_, cx| table_items(&view, fig, cx));
    let menu = cx.update(|_, cx| cx.new(|cx| DropdownMenu::new(items, cx)));
    menu.update_in(cx, |menu, window, cx| {
        assert!(menu.choose("Move row up", window, cx));
    });
    cx.run_until_parked();
    assert!(
        text(&view, cx).contains("| fig  | 2   |\n| pear | 10  |"),
        "{}",
        text(&view, cx)
    );
}

#[gpui::test]
fn insert_table_makes_one_with_the_caret_in_its_header(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "text");
    place_cursor(&view, cx, 4);
    run(cx, "table.insert");
    let inserted = text(&view, cx);
    assert!(
        inserted.starts_with("text\n\n|     |     |     |\n| --- | --- | --- |"),
        "{inserted}"
    );
    assert_eq!(
        cursor(&view, cx),
        "text\n\n| ".len(),
        "the caret is in the first header cell"
    );
}

#[gpui::test]
fn commands_align_and_sort_columns_and_show_the_source(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let two = NOTE.find("| 2 |").unwrap() + 2;
    place_cursor(&view, cx, two);
    run(cx, "table.align-right");
    assert!(
        text(&view, cx).contains("| ---- | --: |"),
        "{}",
        text(&view, cx)
    );
    run(cx, "table.sort-ascending");
    assert!(
        text(&view, cx).contains("| fig  |   2 |\n| pear |  10 |"),
        "{}",
        text(&view, cx)
    );
    run(cx, "table.edit-as-markdown");
    let shows_source = view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        frame
            .lines
            .iter()
            .all(|placed| placed.visual.grid.is_none())
    });
    assert!(
        shows_source,
        "the table shows its Markdown while it's edited so"
    );
}

/// Presses at `from`, moves to `to` in steps, and lets go there.
fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_move(from, None, Modifiers::none());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    let mid = point((from.x + to.x) / 2., (from.y + to.y) / 2.);
    cx.simulate_mouse_move(mid, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn dragging_a_row_handle_moves_the_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let (handle, target) = view.read_with(cx, |view, _| {
        let table = view.tables_on_screen().pop().unwrap();
        let look = &view.theme().table;
        let handle = row_handle(&table, 2, look.handle_size, look.handle_gap).unwrap();
        let (top, _) = table.row(1).unwrap();
        (handle.center(), point(table.left + px(10.), top + px(2.)))
    });
    drag(cx, handle, target);
    assert!(
        text(&view, cx).contains("| ---- | --- |\n| fig  | 2   |\n| pear | 10  |"),
        "{}",
        text(&view, cx)
    );
    let fig = text(&view, cx).find("fig").unwrap();
    let selected = view.read_with(cx, |view, _| view.selected_range());
    assert_eq!(selected.start, fig, "the moved row stays selected");
}

#[gpui::test]
fn dragging_a_column_handle_moves_the_column(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let (handle, target) = view.read_with(cx, |view, _| {
        let table = view.tables_on_screen().pop().unwrap();
        let look = &view.theme().table;
        let handle = column_handle(&table, 1, look.handle_size, look.handle_gap).unwrap();
        (
            handle.center(),
            point(table.left + px(2.), table.top() + px(2.)),
        )
    });
    drag(cx, handle, target);
    assert!(
        text(&view, cx).contains("| n   | name |\n| --- | ---- |\n| 10  | pear |"),
        "{}",
        text(&view, cx)
    );
}

#[gpui::test]
fn clicking_a_row_handle_selects_the_row_and_delete_clears_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let handle = view.read_with(cx, |view, _| {
        let table = view.tables_on_screen().pop().unwrap();
        let look = &view.theme().table;
        row_handle(&table, 1, look.handle_size, look.handle_gap)
            .unwrap()
            .center()
    });
    cx.simulate_mouse_move(handle, None, Modifiers::none());
    click(cx, handle);
    let pear = NOTE.find("pear").unwrap();
    let ten = NOTE.find("10").unwrap();
    assert_eq!(
        view.read_with(cx, |view, _| view.selected_range()),
        pear..ten + 2
    );
    press(cx, "edit.delete-forward");
    assert!(
        text(&view, cx).contains("| name | n   |\n| ---- | --- |\n|      |     |\n| fig  | 2   |"),
        "{}",
        text(&view, cx)
    );
}
