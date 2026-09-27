//! Drives the editor view through GPUI's test platform: typing, selection,
//! mouse input, IME composition and the laid-out geometry. The test
//! platform shapes text with fixed-width fake glyphs.

use editor_desktop::EditorView;
use editor_desktop::actions::bind_keys;
use editor_desktop::line_layout::PieceContent;
use gpui::{
    Entity, EntityInputHandler, Focusable, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
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

#[gpui::test]
fn headings_are_taller_and_images_grow_their_row(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    view.read_with(cx, |view, _| {
        let frame = view.frame().unwrap();
        let heading = &frame.line(0).unwrap().visual;
        let body = &frame.line(1).unwrap().visual;
        let image = &frame.line(2).unwrap().visual;
        assert!(heading.text_height > body.text_height);
        assert!(image.height > body.height);
        let kinds: Vec<bool> = image
            .pieces
            .iter()
            .map(|piece| matches!(piece.content, PieceContent::Image(_)))
            .collect();
        assert_eq!(kinds, vec![false, true, false]);
        assert!(image.pieces[2].x > image.pieces[1].x);
    });
}

#[gpui::test]
fn the_cursor_reveals_image_source(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, NOTE);
    let inside_image = view.read_with(cx, |view, _| view.doc().line_start(2) + 9);
    place_cursor(&view, cx, inside_image);
    view.read_with(cx, |view, _| {
        let image = &view.frame().unwrap().line(2).unwrap().visual;
        assert_eq!(image.pieces[0].range, 0..18);
        assert!(matches!(image.pieces[1].content, PieceContent::Image(_)));
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
