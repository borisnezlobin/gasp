//! Typing through the input pipeline: smart quotes and their undo, curled
//! pastes and the settings that turn them off, auto-pairing and wrapping a
//! selection.

use gasp_config::{Config, Platform, RuleSet};
use gasp_desktop::EditorView;
use gasp_desktop::actions::bind_keys;
use gasp_desktop::keymap::editor_bindings;
use gpui::{ClipboardItem, Entity, Focusable, TestAppContext, VisualTestContext};

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let end = text.len();
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| EditorView::new(&text, Vec::new(), cx));
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    view.update(cx, |view, cx| view.move_to(end, false, cx));
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

/// The built-in config with the smart quote settings changed.
fn with_settings(view: &Entity<EditorView>, cx: &mut VisualTestContext, smart: bool, paste: bool) {
    let mut config = Config::defaults();
    config.settings.editor.smart_quotes = smart;
    config.settings.editor.curl_pasted_quotes = paste;
    view.update(cx, |view, cx| view.apply_config(&config, cx));
}

#[gpui::test]
fn typed_quotes_curl_and_apostrophes_close(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("She said \"it's fine\" (\"really\").");
    assert_eq!(text(&view, cx), "She said “it’s fine” (“really”).");
}

#[gpui::test]
fn undo_right_after_a_curl_gives_the_straight_quote_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("say \"");
    assert_eq!(text(&view, cx), "say “");
    press(cx, "edit.undo");
    assert_eq!(text(&view, cx), "say \"");
    press(cx, "edit.redo");
    assert_eq!(text(&view, cx), "say “");
}

#[gpui::test]
fn quotes_stay_straight_in_code_and_math(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "`x` $y$\n```\n\n```");
    view.update(cx, |view, cx| view.move_to(2, false, cx));
    cx.simulate_input("\"");
    view.update(cx, |view, cx| view.move_to(7, false, cx));
    cx.simulate_input("'");
    view.update(cx, |view, cx| view.move_to(14, false, cx));
    cx.simulate_input("print(\"hi");
    assert_eq!(text(&view, cx), "`x\"` $y'$\n```\nprint(\"hi\n```");
}

#[gpui::test]
fn the_setting_turns_smart_quotes_off(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    with_settings(&view, cx, false, true);
    cx.simulate_input("it's");
    assert_eq!(text(&view, cx), "it's");
}

const PASTED: &str = "He said \"hi\" and `\"code\"`";

#[gpui::test]
fn pasted_text_is_curled_outside_code(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.write_to_clipboard(ClipboardItem::new_string(PASTED.to_owned()));
    press(cx, "edit.paste");
    assert_eq!(text(&view, cx), "He said “hi” and `\"code\"`");
}

#[gpui::test]
fn plain_paste_and_the_setting_keep_quotes_straight(cx: &mut TestAppContext) {
    let pasted = PASTED;
    let (view, cx) = open(cx, "");
    cx.write_to_clipboard(ClipboardItem::new_string(pasted.to_owned()));
    press(cx, "edit.paste-plain");
    assert_eq!(text(&view, cx), pasted);

    with_settings(&view, cx, true, false);
    view.update(cx, |view, cx| view.select(0, view.doc().len(), cx));
    press(cx, "edit.paste");
    assert_eq!(text(&view, cx), pasted);
}

#[gpui::test]
fn brackets_pair_and_markers_wrap_a_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "word");
    cx.simulate_input(" (");
    assert_eq!(text(&view, cx), "word ()");
    press(cx, "edit.delete-backward");
    assert_eq!(text(&view, cx), "word ", "backspace removes an empty pair");
    view.update(cx, |view, cx| view.select(0, 4, cx));
    cx.simulate_input("**");
    assert_eq!(text(&view, cx), "**word** ");
    assert_eq!(view.read_with(cx, |view, _| view.selected_range()), 2..6);
}

#[gpui::test]
fn lines_move_and_duplicate_from_their_keys(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "one\ntwo\nthree");
    view.update(cx, |view, cx| view.move_to(5, false, cx));
    press(cx, "edit.move-line-up");
    assert_eq!(text(&view, cx), "two\none\nthree");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 1);
    press(cx, "edit.move-line-down");
    press(cx, "edit.move-line-down");
    assert_eq!(text(&view, cx), "one\nthree\ntwo");
    press(cx, "edit.duplicate-line");
    assert_eq!(text(&view, cx), "one\nthree\ntwo\ntwo");
    press(cx, "edit.undo");
    assert_eq!(text(&view, cx), "one\nthree\ntwo");
}

#[gpui::test]
fn toggle_task_checks_the_line(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "- [ ] milk");
    view.update(cx, |view, cx| {
        view.run_edit(gasp_core::commands::toggle_tasks, cx)
    });
    assert_eq!(text(&view, cx), "- [x] milk");
}

#[gpui::test]
fn typing_a_code_span_leaves_no_stray_backtick(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("in `code \"x\"` they stay.");
    assert_eq!(text(&view, cx), "in `code \"x\"` they stay.");
}

#[gpui::test]
fn the_setting_turns_auto_pairing_off(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    let mut config = Config::defaults();
    config.settings.editor.auto_pair = false;
    view.update(cx, |view, cx| view.apply_config(&config, cx));
    cx.simulate_input("f(x");
    assert_eq!(text(&view, cx), "f(x");
}
