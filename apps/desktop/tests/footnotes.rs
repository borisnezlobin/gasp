//! Footnotes as you write: insert and jump, automatic renumbering, the
//! `^[1]` typo fix and problem underlines, driven through GPUI's test
//! platform with a fake clock.

use std::time::Duration;

use editor_desktop::actions::bind_keys;
use editor_desktop::footnotes::{LINT_DELAY, TIDY_DELAY};
use editor_desktop::{EditorView, HighlightKind};
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    cursor: usize,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| EditorView::new(&text, Vec::new(), cx));
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    view.update(cx, |view, cx| view.move_to(cursor, false, cx));
    cx.run_until_parked();
    (view, cx)
}

fn text(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.text())
}

fn wait(cx: &mut VisualTestContext, delay: Duration) {
    cx.executor()
        .advance_clock(delay + Duration::from_millis(50));
    cx.run_until_parked();
}

#[gpui::test]
fn alt_zero_inserts_the_next_footnote_then_jumps_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "One[^1].\n\n[^1]: First.\n", 8);
    cx.simulate_keystrokes("alt-0");
    let after = text(&view, cx);
    assert!(after.starts_with("One[^1].[^2]"), "{after}");
    assert!(after.contains("\n[^2]: "), "{after}");
    cx.simulate_keystrokes("alt-0");
    let cursor = view.read_with(cx, |view, _| view.cursor());
    assert_eq!(&after[cursor - 4..cursor], "[^2]", "back at the reference");
}

#[gpui::test]
fn footnotes_renumber_once_typing_pauses(cx: &mut TestAppContext) {
    let out_of_order = "B[^2]. A[^1]\n\n[^1]: one\n[^2]: two\n";
    let (view, cx) = open(cx, out_of_order, 6);
    view.update(cx, |view, cx| view.replace(6..6, "", cx));
    wait(cx, TIDY_DELAY / 2);
    assert_eq!(text(&view, cx), out_of_order, "not while typing");
    wait(cx, TIDY_DELAY);
    assert_eq!(text(&view, cx), "B[^1]. A[^2]\n\n[^1]: two\n[^2]: one\n");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 6);
    view.update(cx, |view, cx| view.undo(cx));
    assert_eq!(text(&view, cx), out_of_order);
    wait(cx, TIDY_DELAY);
    assert_eq!(
        text(&view, cx),
        out_of_order,
        "an undone renumber stays undone"
    );
}

#[gpui::test]
fn the_setting_leaves_footnotes_as_written(cx: &mut TestAppContext) {
    let out_of_order = "B[^2]. A[^1]\n\n[^1]: one\n[^2]: two\n";
    let (view, cx) = open(cx, out_of_order, 6);
    let mut config = editor_config::Config::defaults();
    config.settings.editor.renumber_footnotes = false;
    view.update(cx, |view, cx| view.apply_config(&config, cx));
    view.update(cx, |view, cx| view.replace(6..6, "", cx));
    wait(cx, TIDY_DELAY * 2);
    assert_eq!(text(&view, cx), out_of_order);
}

#[gpui::test]
fn inline_typos_convert_once_the_cursor_moves_on(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "Claim\n\n[^1]: Source.\n", 5);
    view.update(cx, |view, cx| view.replace(5..5, "^[1]", cx));
    wait(cx, TIDY_DELAY);
    assert_eq!(
        text(&view, cx),
        "Claim^[1]\n\n[^1]: Source.\n",
        "cursor still on it"
    );
    view.update(cx, |view, cx| view.replace(9..9, " more.", cx));
    wait(cx, TIDY_DELAY);
    assert_eq!(text(&view, cx), "Claim[^1] more.\n\n[^1]: Source.\n");
}

#[gpui::test]
fn problems_are_underlined_after_a_pause(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "A[^1] and B[^2].\n\n[^1]: one\n", 0);
    let ranges = |view: &Entity<EditorView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.highlights(HighlightKind::FootnoteProblem).to_vec()
        })
    };
    let found = ranges(&view, cx);
    assert_eq!(
        (found.len(), found.first()),
        (1, Some(&(11..15))),
        "found when the note opens"
    );
    view.update(cx, |view, cx| {
        let end = view.doc().len();
        view.replace(end..end, "[^2]: two\n", cx)
    });
    wait(cx, LINT_DELAY);
    assert!(ranges(&view, cx).is_empty());
    let messages: Vec<String> = view.read_with(cx, |view, _| {
        view.footnote_problems()
            .iter()
            .map(|p| p.message.clone())
            .collect()
    });
    assert!(messages.is_empty());
}

#[gpui::test]
fn the_tidy_and_typo_commands_run_on_demand(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "B[^2] A^[1]\n\n[^1]: one\n[^2]: two\n", 11);
    view.update_in(cx, |view, window, cx| {
        assert!(view.run_command("footnote.fix-typos", window, cx));
    });
    assert_eq!(text(&view, cx), "B[^1] A[^2]\n\n[^1]: two\n[^2]: one\n");
}

#[gpui::test]
fn the_tidy_command_renumbers_right_away(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "B[^2] A[^1]\n\n[^1]: one\n[^2]: two\n", 0);
    view.update_in(cx, |view, window, cx| {
        assert!(view.run_command("footnote.tidy", window, cx));
    });
    assert_eq!(text(&view, cx), "B[^1] A[^2]\n\n[^1]: two\n[^2]: one\n");
}
