//! Heading folds on the desktop through GPUI's test platform: the chevron
//! in the margin, the count of hidden lines, the `fold.*` commands and
//! their keys, keeping the caret out of folded text, and folds kept with
//! the note's position.

use std::path::Path;

use gasp_config::{Platform, RuleSet};
use gasp_desktop::EditorView;
use gasp_desktop::actions::bind_keys;
use gasp_desktop::keymap::editor_bindings;
use gasp_desktop::line_layout::{Hit, PieceContent};
use gasp_desktop::workspace::state::save_device;
use gasp_desktop::workspace::{OpenIn, Workspace};
use gpui::{
    Bounds, Entity, Focusable, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    TestAppContext, VisualTestContext, point, px,
};

const NOTE: &str = "# One\nfirst\nsecond\n\n## Two\nthird\n# Three\nlast line\n";

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

fn key_for(command: &str) -> String {
    editor_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|(_, id)| id == command)
        .map(|(keystroke, _)| keystroke)
        .unwrap_or_else(|| panic!("{command} has no key on this platform"))
}

fn press(cx: &mut VisualTestContext, command: &str) {
    cx.simulate_keystrokes(&key_for(command));
    cx.run_until_parked();
}

fn run(view: &Entity<EditorView>, cx: &mut VisualTestContext, command: &str) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.run_command(command, window, cx))
        })
    });
    cx.run_until_parked();
}

fn place_cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext, offset: usize) {
    view.update(cx, |view, cx| view.move_to(offset, false, cx));
    cx.run_until_parked();
}

fn cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, _| view.cursor())
}

/// The lines drawn with no height.
fn collapsed(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<usize> {
    view.read_with(cx, |view, _| {
        view.frame()
            .unwrap()
            .lines
            .iter()
            .filter(|placed| placed.visual.is_collapsed())
            .map(|placed| placed.visual.line)
            .collect()
    })
}

/// The window bounds of the count after a folded heading on `line`, and
/// its text.
fn count_on(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    line: usize,
) -> Option<(Bounds<Pixels>, String)> {
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let placed = frame.line(line)?;
        placed.visual.rows.iter().find_map(|row| {
            let piece = row.pieces.iter().find(|piece| piece.hit == Hit::Unfold)?;
            let PieceContent::Text(text) = &piece.content else {
                return None;
            };
            let origin = point(frame.text_left + piece.x, placed.top + row.top + piece.top);
            let size = gpui::size(piece.width, piece.height);
            Some((Bounds::new(origin, size), text.shaped.text.to_string()))
        })
    })
}

/// Where each text piece of `line` is drawn, to show nothing moved.
fn text_positions(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    line: usize,
) -> Vec<(Pixels, Pixels)> {
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let placed = frame.line(line).unwrap();
        placed
            .visual
            .rows
            .iter()
            .flat_map(|row| row.pieces.iter().map(move |piece| (row, piece)))
            .filter(|(_, piece)| piece.is_text())
            .map(|(row, piece)| (frame.text_left + piece.x, placed.top + row.top + piece.top))
            .collect()
    })
}

fn hover(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, position: Point<Pixels>) {
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    cx.run_until_parked();
}

/// A point on `line`'s text, halfway down it.
fn on_line(view: &Entity<EditorView>, cx: &mut VisualTestContext, line: usize) -> Point<Pixels> {
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let placed = frame.line(line).unwrap();
        point(
            frame.text_left + px(20.),
            placed.top + placed.visual.height / 2.,
        )
    })
}

fn hover_line(view: &Entity<EditorView>, cx: &mut VisualTestContext, line: usize) {
    let at = on_line(view, cx, line);
    hover(cx, at);
}

fn chevrons(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<(usize, bool)> {
    view.read_with(cx, |view, _| {
        view.fold_chevrons()
            .iter()
            .map(|chevron| (chevron.line, chevron.folded))
            .collect()
    })
}

#[gpui::test]
fn the_key_folds_the_heading_and_counts_what_it_hides(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    place_cursor(&view, cx, 2);
    press(cx, "fold.toggle");
    assert_eq!(collapsed(&view, cx), [1, 2, 3, 4, 5]);
    let (_, label) = count_on(&view, cx, 0).expect("a count after the heading");
    assert_eq!(label, "5 lines");
    assert_eq!(cursor(&view, cx), 2, "the caret stays on the heading");
    press(cx, "fold.toggle");
    assert!(collapsed(&view, cx).is_empty());
    assert!(count_on(&view, cx, 0).is_none());
}

#[gpui::test]
fn folding_from_inside_a_section_puts_the_caret_on_its_heading(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let third = NOTE.find("third").unwrap();
    place_cursor(&view, cx, third + 2);
    run(&view, cx, "fold.toggle");
    assert_eq!(collapsed(&view, cx), [5], "the innermost section folds");
    assert_eq!(cursor(&view, cx), NOTE.find("\nthird").unwrap());
}

#[gpui::test]
fn the_chevron_shows_on_hover_without_moving_anything(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    place_cursor(&view, cx, NOTE.find("last").unwrap());
    assert!(chevrons(&view, cx).is_empty(), "no chevrons at rest");
    let heading = text_positions(&view, cx, 0);
    let below = text_positions(&view, cx, 1);
    hover_line(&view, cx, 0);
    assert_eq!(chevrons(&view, cx), [(0, false)]);
    assert_eq!(text_positions(&view, cx, 0), heading);
    assert_eq!(text_positions(&view, cx, 1), below);

    hover_line(&view, cx, 1);
    assert!(chevrons(&view, cx).is_empty(), "plain text has none");
}

#[gpui::test]
fn clicking_the_chevron_folds_and_the_count_unfolds(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    place_cursor(&view, cx, NOTE.find("last").unwrap());
    hover_line(&view, cx, 4);
    let chevron = view.read_with(cx, |view, _| view.fold_chevrons()[0].bounds);
    hover(cx, chevron.center());
    let hot = view.read_with(cx, |view, _| view.fold_chevrons()[0].hot);
    assert!(hot, "the chevron under the pointer is marked");
    click(cx, chevron.center());
    assert_eq!(collapsed(&view, cx), [5]);
    hover_line(&view, cx, 7);
    assert_eq!(chevrons(&view, cx), [(4, true)], "a folded chevron stays");
    assert_eq!(cursor(&view, cx), NOTE.find("last").unwrap());

    let (count, label) = count_on(&view, cx, 4).unwrap();
    assert_eq!(label, "1 line");
    click(cx, count.center());
    assert!(collapsed(&view, cx).is_empty());
    assert_eq!(cursor(&view, cx), NOTE.find("last").unwrap());
}

#[gpui::test]
fn the_caret_steps_over_a_fold(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    place_cursor(&view, cx, 0);
    press(cx, "fold.toggle");
    let heading_end = NOTE.find('\n').unwrap();
    let three = NOTE.find("# Three").unwrap();
    press(cx, "cursor.down");
    let on_three = three..three + "# Three".len();
    assert!(
        on_three.contains(&cursor(&view, cx)),
        "down goes to the next heading"
    );
    press(cx, "cursor.up");
    assert!(cursor(&view, cx) <= heading_end, "up comes back");
    place_cursor(&view, cx, three);
    press(cx, "cursor.left");
    assert_eq!(
        cursor(&view, cx),
        heading_end,
        "left comes back to the heading"
    );
    press(cx, "cursor.right");
    assert_eq!(cursor(&view, cx), three, "right goes past the fold");
    assert_eq!(collapsed(&view, cx), [1, 2, 3, 4, 5], "still folded");
}

#[gpui::test]
fn the_last_section_keeps_the_caret_on_its_heading(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let three = NOTE.find("# Three").unwrap();
    place_cursor(&view, cx, three);
    press(cx, "fold.toggle");
    press(cx, "cursor.doc-end");
    assert_eq!(cursor(&view, cx), three + "# Three".len());
    assert!(!collapsed(&view, cx).is_empty(), "still folded");
    run(&view, cx, "select.all");
    assert!(
        !collapsed(&view, cx).is_empty(),
        "select all leaves it folded"
    );
}

#[gpui::test]
fn jumping_into_a_fold_opens_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    place_cursor(&view, cx, 0);
    run(&view, cx, "fold.all");
    assert_eq!(collapsed(&view, cx), [1, 2, 3, 4, 5, 7, 8]);
    let second = NOTE.find("second").unwrap();
    view.update(cx, |view, cx| view.select(second, second + 3, cx));
    cx.run_until_parked();
    assert_eq!(
        collapsed(&view, cx),
        [5, 7, 8],
        "a match found inside opens its fold only"
    );
    run(&view, cx, "fold.all");
    run(&view, cx, "fold.unfold-all");
    assert!(collapsed(&view, cx).is_empty());
}

#[gpui::test]
fn fold_all_moves_the_caret_out(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    place_cursor(&view, cx, NOTE.find("third").unwrap());
    run(&view, cx, "fold.all");
    assert_eq!(cursor(&view, cx), NOTE.find('\n').unwrap());
    assert_eq!(collapsed(&view, cx), [1, 2, 3, 4, 5, 7, 8]);
}

#[gpui::test]
fn the_key_folds_the_callout_the_caret_is_in(cx: &mut TestAppContext) {
    let text = "Intro\n\n> [!tip]+ Tip\n> hidden\n\nAfter\n";
    let (view, cx) = open(cx, text);
    place_cursor(&view, cx, text.find("hidden").unwrap());
    assert!(collapsed(&view, cx).is_empty());
    press(cx, "fold.toggle");
    assert_eq!(collapsed(&view, cx), [3]);
    assert_eq!(cursor(&view, cx), text.find("\n\nAfter").unwrap() + 1);
}

#[gpui::test]
fn folds_are_kept_with_the_note(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("a.md"), NOTE).unwrap();
    std::fs::write(vault.path().join("b.md"), "other").unwrap();
    cx.update(bind_keys);
    let root = vault.path().to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| Workspace::new(&root, window, cx));
    cx.run_until_parked();
    let open_note = |workspace: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str| {
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace
                    .open_path(Path::new(name), OpenIn::ActiveTab, window, cx)
                    .unwrap()
            })
        });
        cx.run_until_parked();
        cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap())
    };
    let editor = open_note(&workspace, cx, "a.md");
    let four = NOTE.find("## Two").unwrap();
    editor.update(cx, |editor, cx| {
        editor.move_to(four, false, cx);
        editor.toggle_fold_at_cursor(cx);
    });
    open_note(&workspace, cx, "b.md");
    let again = open_note(&workspace, cx, "a.md");
    assert!(again.read_with(cx, |editor, _| editor.is_line_folded(4)));

    let device = cx.read(|cx| workspace.read(cx).device_state(cx));
    let kept = device.positions.iter().find(|kept| kept.path == "a.md");
    assert_eq!(kept.map(|kept| kept.folds.clone()), Some(vec![4]));
    save_device(vault.path(), &device).unwrap();
    let root = vault.path().to_path_buf();
    let (relaunched, cx) = cx.add_window_view(move |window, cx| Workspace::new(&root, window, cx));
    cx.run_until_parked();
    let reopened = open_note(&relaunched, cx, "a.md");
    assert!(reopened.read_with(cx, |editor, _| editor.is_line_folded(4)));
    assert!(!reopened.read_with(cx, |editor, _| editor.is_line_folded(0)));
}
