//! Look up's text and placement, which every platform works out though
//! only macOS shows the popover: the word under a force click or at the
//! caret, or the selection, and where its baseline is drawn.

use gasp_desktop::EditorView;
use gasp_desktop::actions::bind_keys;
use gasp_desktop::look_up::LookUp;
use gpui::{
    Bounds, Entity, Focusable, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};

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

fn select(view: &Entity<EditorView>, cx: &mut VisualTestContext, range: std::ops::Range<usize>) {
    view.update(cx, |view, cx| view.select(range.start, range.end, cx));
    cx.run_until_parked();
}

/// The caret's box at `offset`, as drawn.
fn caret(view: &Entity<EditorView>, cx: &mut VisualTestContext, offset: usize) -> Bounds<Pixels> {
    view.read_with(cx, |view, _| {
        view.frame()
            .unwrap()
            .caret_bounds(offset, view.theme())
            .unwrap()
    })
}

/// A point inside the character at `offset`.
fn over(view: &Entity<EditorView>, cx: &mut VisualTestContext, offset: usize) -> Point<Pixels> {
    let start = caret(view, cx, offset);
    let end = caret(view, cx, offset + 1);
    point(
        (start.left() + end.left()) / 2.,
        start.top() + start.size.height / 2.,
    )
}

fn at_point(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    position: Point<Pixels>,
) -> Option<LookUp> {
    view.read_with(cx, |view, cx| view.look_up_at_point(position, cx))
}

/// What a force click on the character at `offset` looks up.
fn at_char(view: &Entity<EditorView>, cx: &mut VisualTestContext, offset: usize) -> Option<LookUp> {
    let position = over(view, cx, offset);
    at_point(view, cx, position)
}

fn at_cursor(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Option<LookUp> {
    view.read_with(cx, |view, cx| view.look_up_at_cursor(cx))
}

#[gpui::test]
fn a_force_click_looks_up_the_word_under_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "hello world\n\nnext");
    let position = over(&view, cx, 8);
    let found = at_point(&view, cx, position).expect("a word is under the pointer");
    assert_eq!(found.range, 6..11);
    assert_eq!(found.text, "world");
    let caret = caret(&view, cx, 6);
    assert_eq!(
        found.baseline.x,
        caret.left(),
        "drawn from the word's left edge"
    );
    assert!(
        // The test font has no descent, so the baseline can sit on the
        // caret's bottom edge, give or take rounding.
        caret.top() < found.baseline.y && found.baseline.y <= caret.bottom() + px(0.01),
        "the baseline {:?} is on the word's row {caret:?}",
        found.baseline.y
    );
    assert!(found.font_size > px(0.));
    // The test platform shapes with a font it never names, so the family
    // stays unknown here.
}

#[gpui::test]
fn each_line_has_its_own_baseline(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "first\n\nsecond");
    let first = at_char(&view, cx, 1).unwrap();
    let second = at_char(&view, cx, 9).unwrap();
    assert_eq!(second.text, "second");
    assert!(second.baseline.y > first.baseline.y);
}

#[gpui::test]
fn hidden_markup_is_left_out(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "plain\n\n**bold** text");
    let found = at_char(&view, cx, 10).unwrap();
    assert_eq!(found.text, "bold");
    assert_eq!(found.baseline.x, caret(&view, cx, 9).left());
}

#[gpui::test]
fn nothing_is_looked_up_off_the_text(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "hello world, again\n\nnext");
    let comma = over(&view, cx, 11);
    assert_eq!(at_point(&view, cx, comma), None);
    let past_the_end = point(caret(&view, cx, 18).left() + px(200.), comma.y);
    assert_eq!(at_point(&view, cx, past_the_end), None);
}

#[gpui::test]
fn inside_the_selection_the_selection_is_looked_up(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "the Rosetta Stone here");
    select(&view, cx, 4..17);
    let found = at_char(&view, cx, 6).unwrap();
    assert_eq!(found.text, "Rosetta Stone");
    let outside = at_char(&view, cx, 19).unwrap();
    assert_eq!(outside.text, "here");
}

#[gpui::test]
fn the_command_looks_up_the_selection_or_the_word_at_the_caret(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "look up words");
    select(&view, cx, 7..7);
    assert_eq!(at_cursor(&view, cx).unwrap().text, "up");
    select(&view, cx, 5..13);
    let found = at_cursor(&view, cx).unwrap();
    assert_eq!(found.text, "up words");
    assert_eq!(found.baseline.x, caret(&view, cx, 5).left());
    select(&view, cx, 4..4);
    assert_eq!(at_cursor(&view, cx).unwrap().text, "look");
}
