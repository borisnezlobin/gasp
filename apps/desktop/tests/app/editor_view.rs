//! Drives the editor view through GPUI's test platform: typing, selection,
//! mouse input, IME composition and the laid-out geometry. The test
//! platform shapes text with fixed-width fake glyphs.

use gasp_config::{Platform, RuleSet};
use gasp_desktop::EditorView;
use gasp_desktop::actions::bind_keys;
use gasp_desktop::keymap::editor_bindings;
use gasp_desktop::line_layout::{Piece, PieceContent, RowKind};
use gpui::{
    Entity, EntityInputHandler, Focusable, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    Pixels, Point, TestAppContext, VisualTestContext, point, px,
};

const NOTE: &str = "# Heading\nbody **bold** text\nimage ![[pic.png]] here\nlast line";

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

/// A window point just past the end of line `line`.
fn point_after_line(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    line: usize,
) -> Point<Pixels> {
    let width = view.read_with(cx, |view, _| {
        view.frame().unwrap().line(line).unwrap().visual.width()
    });
    point_in_line(view, cx, line, width + px(20.))
}

fn text(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.text())
}

fn place_cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext, offset: usize) {
    view.update(cx, |view, cx| view.move_to(offset, false, cx));
    cx.run_until_parked();
}

/// A window point inside line `line`, `x` pixels into its text.
fn point_in_line(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    line: usize,
    x: Pixels,
) -> Point<Pixels> {
    view.read_with(cx, |view, _| {
        let frame = view.frame().expect("the view has been drawn");
        let placed = frame.line(line).expect("the line is visible");
        point(frame.text_left + x, placed.text_top() + px(2.))
    })
}

#[gpui::test]
fn typing_inserts_at_the_cursor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "ab");
    place_cursor(&view, cx, 1);
    cx.simulate_input("xyz");
    assert_eq!(text(&view, cx), "axyzb");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 4);
}

#[gpui::test]
fn enter_and_backspace_edit_lines(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "ab");
    place_cursor(&view, cx, 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&view, cx), "a\nb");
    cx.simulate_keystrokes("backspace backspace");
    assert_eq!(text(&view, cx), "b");
    cx.simulate_keystrokes("delete");
    assert_eq!(text(&view, cx), "");
}

#[gpui::test]
fn shift_arrows_extend_the_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "hello world");
    place_cursor(&view, cx, 5);
    cx.simulate_keystrokes("shift-left shift-left");
    assert_eq!(view.read_with(cx, |view, _| view.selected_range()), 3..5);
    cx.simulate_input("p");
    assert_eq!(text(&view, cx), "help world");
    cx.simulate_keystrokes("shift-right shift-right left");
    assert_eq!(view.read_with(cx, |view, _| view.selected_range()), 4..4);
}

#[gpui::test]
fn up_and_down_keep_the_column(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "abcdef\nab\nabcdef");
    place_cursor(&view, cx, 4);
    cx.simulate_keystrokes("down");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 9);
    cx.simulate_keystrokes("down");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 14);
    cx.simulate_keystrokes("shift-up");
    assert_eq!(view.read_with(cx, |view, _| view.selected_range()), 9..14);
}

#[gpui::test]
fn undo_and_redo_use_the_core_history(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "a");
    place_cursor(&view, cx, 1);
    cx.simulate_input("b");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(&view, cx), "a");
    cx.simulate_keystrokes("secondary-shift-z");
    assert_eq!(text(&view, cx), "ab");
}

#[gpui::test]
fn clicking_places_the_cursor_and_dragging_selects(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "first line\nsecond line");
    let line_start = view.read_with(cx, |view, _| view.doc().line_start(1));
    let near_start = point_in_line(&view, cx, 1, px(1.));
    cx.simulate_mouse_down(near_start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(near_start, MouseButton::Left, Modifiers::none());
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), line_start);

    let far_right = point_after_line(&view, cx, 1);
    cx.simulate_mouse_down(near_start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(far_right, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(far_right, MouseButton::Left, Modifiers::none());
    let end = view.read_with(cx, |view, _| view.doc().len());
    assert_eq!(
        view.read_with(cx, |view, _| view.selected_range()),
        line_start..end
    );
}

#[gpui::test]
fn a_drag_held_past_the_bottom_keeps_scrolling_and_selecting(cx: &mut TestAppContext) {
    let note = "A line of the note.\n".repeat(400);
    let (view, cx) = open(cx, &note);
    let start = point_in_line(&view, cx, 0, px(1.));
    let below = view.read_with(cx, |view, _| {
        let bounds = view.frame().unwrap().bounds;
        point(start.x, bounds.bottom() + px(40.))
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::none());
    let reach = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            (view.scroll_offset(), view.selected_range().end)
        })
    };
    let first = reach(cx);
    // Held still: no more moves, only time passing.
    for _ in 0..10 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.run_until_parked();
    }
    let held = reach(cx);
    assert!(held.0 > first.0, "the note scrolls while the drag is held");
    assert!(held.1 > first.1, "and the selection grows with it");
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::none());
    for _ in 0..5 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.run_until_parked();
    }
    assert_eq!(reach(cx), held.clone(), "letting go stops it");
}

#[gpui::test]
fn shift_click_extends_the_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "one\ntwo");
    place_cursor(&view, cx, 1);
    let end_of_two = point_after_line(&view, cx, 1);
    cx.simulate_mouse_down(end_of_two, MouseButton::Left, Modifiers::shift());
    cx.simulate_mouse_up(end_of_two, MouseButton::Left, Modifiers::shift());
    assert_eq!(view.read_with(cx, |view, _| view.selected_range()), 1..7);
}

#[gpui::test]
fn ime_composition_marks_then_commits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "ab");
    place_cursor(&view, cx, 1);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx);
            view.replace_and_mark_text_in_range(None, "にほ", Some(2..2), window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(text(&view, cx), "aにほb");
    assert_eq!(
        view.read_with(cx, |view, _| view.marked_range()),
        Some(1..7)
    );
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 7);

    let (marked_utf16, bounds) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let marked = view.marked_text_range(window, cx);
            let bounds = view.bounds_for_range(1..3, Default::default(), window, cx);
            (marked, bounds)
        })
    });
    assert_eq!(marked_utf16, Some(1..3));
    let bounds = bounds.expect("the composition is on screen");
    assert!(bounds.size.width > px(0.));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.replace_text_in_range(None, "日本", window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(text(&view, cx), "a日本b");
    assert_eq!(view.read_with(cx, |view, _| view.marked_range()), None);
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 7);
}

#[gpui::test]
fn ime_reads_text_and_selection_in_utf16(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "日本語abc");
    view.update(cx, |view, cx| view.select(3, 9, cx));
    let (selection, fragment) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let selection = view.selected_text_range(false, window, cx).unwrap();
            let mut adjusted = None;
            let fragment = view.text_for_range(2..4, &mut adjusted, window, cx);
            (selection, (fragment, adjusted))
        })
    });
    assert_eq!(selection.range, 1..3);
    assert!(!selection.reversed);
    assert_eq!(fragment, (Some("語a".to_owned()), Some(2..4)));
}

fn is_image(piece: &Piece) -> bool {
    matches!(piece.content, PieceContent::Image { .. })
}

#[gpui::test]
fn headings_are_taller_and_images_grow_their_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let heading = &frame.line(0).unwrap().visual;
        let body = &frame.line(1).unwrap().visual;
        let image = &frame.line(2).unwrap().visual;
        assert!(heading.rows[0].caret_height > body.rows[0].caret_height);
        assert!(image.height > body.height);
        let kinds: Vec<bool> = image.pieces().map(is_image).collect();
        assert_eq!(kinds, vec![false, true, false]);
        let pieces: Vec<&Piece> = image.pieces().collect();
        assert!(pieces[2].x > pieces[1].x);
    });
}

#[gpui::test]
fn the_cursor_reveals_image_source(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let inside_image = view.read_with(cx, |view, _| view.doc().line_start(2) + 9);
    place_cursor(&view, cx, inside_image);
    view.read_with(cx, |view, _| {
        let image = &view.frame().unwrap().line(2).unwrap().visual;
        let text: Vec<std::ops::Range<usize>> = image.rows[0]
            .pieces
            .iter()
            .map(|piece| piece.range.clone())
            .collect();
        assert_eq!(text.first().map(|range| range.start), Some(0));
        assert_eq!(text.last().map(|range| range.end), Some(23));
        assert!(!image.rows[0].pieces.iter().any(is_image));
        let below = image.rows.last().unwrap();
        assert_eq!(below.kind, RowKind::Below);
        assert!(below.pieces.iter().any(is_image));
    });
}

#[gpui::test]
fn frames_record_timings(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    cx.simulate_input("x");
    view.read_with(cx, |view, _| {
        let timings = view.timings();
        assert!(!timings.layout.is_empty());
        assert!(!timings.paint.is_empty());
        assert_eq!(timings.input_to_paint.len(), 1);
    });
}

#[gpui::test]
fn only_visible_lines_are_laid_out(cx: &mut TestAppContext) {
    let long_note = vec!["a line of text"; 5_000].join("\n");
    let (view, cx) = open(cx, &long_note);
    let laid_out = view.read_with(cx, |view, _| view.frame().unwrap().lines.len());
    assert!(laid_out > 0 && laid_out < 500, "laid out {laid_out} lines");
    let end = long_note.len();
    place_cursor(&view, cx, end);
    let last = view.read_with(cx, |view, _| {
        view.frame().unwrap().lines.last().unwrap().visual.line
    });
    assert_eq!(last, 4_999);
}

/// The keystroke this platform's default rules bind to `command`.
fn key_for(command: &str) -> String {
    editor_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|(_, id)| id == command)
        .map(|(keystroke, _)| keystroke)
        .unwrap_or_else(|| panic!("{command} has no key on this platform"))
}

fn press(cx: &mut VisualTestContext, command: &str) {
    cx.simulate_keystrokes(&key_for(command));
}

fn selection(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> std::ops::Range<usize> {
    view.read_with(cx, |view, _| view.selected_range())
}

#[gpui::test]
fn word_deletes_remove_whole_words(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "one two, three");
    place_cursor(&view, cx, 14);
    press(cx, "edit.delete-word-backward");
    assert_eq!(text(&view, cx), "one two, ");
    press(cx, "edit.delete-word-backward");
    assert_eq!(text(&view, cx), "one ");
    place_cursor(&view, cx, 0);
    press(cx, "edit.delete-word-forward");
    assert_eq!(text(&view, cx), " ");
}

#[gpui::test]
fn word_motions_move_and_select(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "alpha beta gamma");
    place_cursor(&view, cx, 0);
    press(cx, "cursor.word-right");
    assert_eq!(selection(&view, cx), 5..5);
    press(cx, "select.word-right");
    assert_eq!(selection(&view, cx), 5..10);
    press(cx, "cursor.doc-end");
    assert_eq!(selection(&view, cx), 16..16);
    press(cx, "select.word-left");
    assert_eq!(selection(&view, cx), 11..16);
}

#[gpui::test]
fn copy_and_paste_round_trip(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "copy me");
    view.update(cx, |view, cx| view.select(0, 4, cx));
    press(cx, "edit.copy");
    press(cx, "cursor.doc-end");
    press(cx, "edit.paste");
    assert_eq!(text(&view, cx), "copy mecopy");
    view.update(cx, |view, cx| view.select(0, 5, cx));
    press(cx, "edit.cut");
    assert_eq!(text(&view, cx), "mecopy");
    press(cx, "edit.paste");
    assert_eq!(text(&view, cx), "copy mecopy");
}

#[gpui::test]
fn formatting_and_footnote_keys_edit_the_note(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "a claim");
    view.update(cx, |view, cx| view.select(2, 7, cx));
    press(cx, "format.bold");
    assert_eq!(text(&view, cx), "a **claim**");
    press(cx, "cursor.doc-end");
    press(cx, "footnote.insert-or-jump");
    assert!(text(&view, cx).starts_with("a **claim**[^1]"));
    assert!(text(&view, cx).contains("\n[^1]: "));
}

#[gpui::test]
fn enter_continues_lists_and_tab_indents_them(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "- one");
    place_cursor(&view, cx, 5);
    press(cx, "edit.newline");
    assert_eq!(text(&view, cx), "- one\n- ");
    press(cx, "edit.indent");
    assert_eq!(text(&view, cx), "- one\n\t- ");
    press(cx, "edit.outdent");
    assert_eq!(text(&view, cx), "- one\n- ");
}

fn click(cx: &mut VisualTestContext, position: Point<Pixels>, count: usize) {
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: count,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: count,
    });
}

#[gpui::test]
fn double_click_selects_a_word_and_triple_click_a_line(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "hello there world\nnext");
    let glyph = view.read_with(cx, |view, _| {
        view.frame()
            .unwrap()
            .line(0)
            .unwrap()
            .visual
            .x_for_offset(7)
    });
    let inside_there = point_in_line(&view, cx, 0, glyph);
    click(cx, inside_there, 1);
    click(cx, inside_there, 2);
    assert_eq!(selection(&view, cx), 6..11);
    click(cx, inside_there, 3);
    assert_eq!(selection(&view, cx), 0..18);
}

#[gpui::test]
fn the_last_line_can_scroll_up_to_the_middle(cx: &mut TestAppContext) {
    let note: Vec<String> = (0..200).map(|line| format!("line {line}")).collect();
    let (view, cx) = open(cx, &note.join("\n"));
    view.update(cx, |view, cx| view.scroll_by(px(1_000_000.), cx));
    cx.run_until_parked();
    let (last_line_top, middle) = view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let last = frame.line(199).expect("the last line is on screen");
        (last.text_top(), frame.bounds.center().y)
    });
    assert!(
        (last_line_top - middle).abs() < px(40.),
        "the last line rests mid-view: {last_line_top:?} against {middle:?}"
    );
}

#[gpui::test]
fn select_all_keeps_the_scroll_position(cx: &mut TestAppContext) {
    let long: String = (0..400).map(|line| format!("line {line}\n")).collect();
    let (view, cx) = open(cx, &long);
    place_cursor(&view, cx, 0);
    let before = view.read_with(cx, |view, _| view.scroll_offset());
    press(cx, "select.all");
    cx.run_until_parked();
    assert_eq!(selection(&view, cx), 0..long.len());
    assert_eq!(view.read_with(cx, |view, _| view.scroll_offset()), before);
}

#[gpui::test]
fn the_caret_spans_the_letters_not_the_line_spacing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "some text");
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let row = &frame.line(0).unwrap().visual.rows[0];
        let caret = frame.caret_bounds(0, view.theme()).unwrap();
        assert!(
            caret.size.height < row.height,
            "the line's leading stays out of the caret: {:?} in a {:?} row",
            caret.size.height,
            row.height
        );
        assert!(caret.size.height >= view.theme().body_font_size);
    });
}
