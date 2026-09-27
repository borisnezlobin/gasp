//! Smart paste through the editor's keys, with the test platform's
//! clipboard: images become attachments, URLs over a selection become
//! links, and copy or cut with nothing selected take the whole line.

use editor_config::{Platform, RuleSet};
use editor_desktop::EditorView;
use editor_desktop::actions::bind_keys;
use editor_desktop::keymap::editor_bindings;
use editor_desktop::paste::{PasteContext, set_paste_context};
use gpui::{
    ClipboardItem, Entity, Focusable, Image, ImageFormat, TestAppContext, VisualTestContext,
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

fn press(cx: &mut VisualTestContext, command: &str) {
    let keystroke = editor_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|(_, id)| id == command)
        .map(|(keystroke, _)| keystroke)
        .unwrap_or_else(|| panic!("{command} has no key on this platform"));
    cx.simulate_keystrokes(&keystroke);
    cx.run_until_parked();
}

fn text(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.text())
}

fn select(view: &Entity<EditorView>, cx: &mut VisualTestContext, range: std::ops::Range<usize>) {
    view.update(cx, |view, cx| view.select(range.start, range.end, cx));
}

fn clipboard_text(cx: &mut VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui::test]
fn pasted_images_are_saved_next_to_the_note(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    let note = vault.path().join("Lemma.md");
    let (view, cx) = open(cx, "see ");
    cx.update(|_, cx| {
        let context = PasteContext {
            note_path: Some(note.clone()),
            attachments: "./images".to_owned(),
        };
        set_paste_context(&view, context, cx);
    });
    select(&view, cx, 4..4);
    let image = Image::from_bytes(ImageFormat::Png, b"not really a png".to_vec());
    cx.write_to_clipboard(ClipboardItem::new_image(&image));
    press(cx, "edit.paste");
    press(cx, "edit.paste");
    assert_eq!(text(&view, cx), "see ![[Lemma-1.png]]![[Lemma-2.png]]");
    let saved = std::fs::read(vault.path().join("images/Lemma-2.png")).unwrap();
    assert_eq!(saved, b"not really a png");
    press(cx, "edit.undo");
    assert_eq!(text(&view, cx), "see ![[Lemma-1.png]]");
}

#[gpui::test]
fn images_need_a_saved_note(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "x");
    let image = Image::from_bytes(ImageFormat::Jpeg, vec![1, 2, 3]);
    cx.write_to_clipboard(ClipboardItem::new_image(&image));
    press(cx, "edit.paste");
    assert_eq!(text(&view, cx), "x");
}

#[gpui::test]
fn a_url_over_a_selection_becomes_a_link(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "read the docs\n");
    select(&view, cx, 9..13);
    cx.write_to_clipboard(ClipboardItem::new_string("https://example.com/docs".into()));
    press(cx, "edit.paste");
    assert_eq!(
        text(&view, cx),
        "read the [docs](https://example.com/docs)\n"
    );
    let end = view.read_with(cx, |view, _| view.doc().len());
    select(&view, cx, end..end);
    press(cx, "edit.paste");
    assert_eq!(
        text(&view, cx),
        "read the [docs](https://example.com/docs)\nhttps://example.com/docs"
    );
}

#[gpui::test]
fn paste_plain_never_links(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "word");
    select(&view, cx, 0..4);
    cx.write_to_clipboard(ClipboardItem::new_string("https://example.com".into()));
    press(cx, "edit.paste-plain");
    assert_eq!(text(&view, cx), "https://example.com");
}

#[gpui::test]
fn copy_without_a_selection_takes_the_line(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "first\nsecond\nthird");
    select(&view, cx, 8..8);
    press(cx, "edit.copy");
    assert_eq!(clipboard_text(cx).as_deref(), Some("second\n"));
    // A whole line pastes above the cursor's line, not at the cursor.
    select(&view, cx, 15..15);
    press(cx, "edit.paste");
    assert_eq!(text(&view, cx), "first\nsecond\nsecond\nthird");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 22);
}

#[gpui::test]
fn cut_without_a_selection_removes_the_line(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "first\nsecond\nthird");
    select(&view, cx, 2..2);
    press(cx, "edit.cut");
    assert_eq!(text(&view, cx), "second\nthird");
    assert_eq!(clipboard_text(cx).as_deref(), Some("first\n"));
    let end = view.read_with(cx, |view, _| view.doc().len());
    select(&view, cx, end..end);
    press(cx, "edit.cut");
    assert_eq!(text(&view, cx), "second");
    assert_eq!(clipboard_text(cx).as_deref(), Some("third\n"));
}

#[gpui::test]
fn a_selection_is_copied_as_is(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "alpha beta");
    select(&view, cx, 0..5);
    press(cx, "edit.copy");
    select(&view, cx, 10..10);
    press(cx, "edit.paste");
    assert_eq!(text(&view, cx), "alpha betaalpha");
}

#[gpui::test]
fn dropped_files_are_embedded_or_linked(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let picture = outside.path().join("photo.jpg");
    std::fs::write(&picture, b"jpeg").unwrap();
    let (view, cx) = open(cx, "ab");
    cx.update(|_, cx| {
        let context = PasteContext {
            note_path: Some(vault.path().join("Trip.md")),
            attachments: "assets".to_owned(),
        };
        set_paste_context(&view, context, cx);
    });
    view.update(cx, |view, cx| {
        view.insert_files(std::slice::from_ref(&picture), 1, cx)
    });
    assert_eq!(text(&view, cx), "a![[Trip-1.jpg]]b");
    assert!(vault.path().join("assets/Trip-1.jpg").is_file());
}
