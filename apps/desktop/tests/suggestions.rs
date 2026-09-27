//! Suggestions while typing: note names after `[[`, headings after `#`
//! inside a link, and tags, driven through GPUI's test platform.

use editor_desktop::EditorView;
use editor_desktop::actions::bind_keys;
use editor_desktop::vault_index::{NoteScan, VaultIndex};
use gpui::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext};

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let end = text.len();
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| EditorView::new(&text, Vec::new(), cx));
    let index = cx.new(|_| {
        let mut index = VaultIndex::default();
        let notes: [(&str, &[&str]); 4] = [
            ("Wave Packets.md", &["physics", "waves"]),
            ("Physics/Waves.md", &["physics"]),
            ("Reading.md", &["books"]),
            ("a/Topic.md", &[]),
        ];
        for (path, tags) in notes {
            index.upsert(NoteScan {
                path: path.into(),
                tags: tags.iter().map(|tag| tag.to_string()).collect(),
            });
        }
        index
    });
    view.update(cx, |view, cx| view.set_vault_index(index, cx));
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    view.update(cx, |view, cx| view.move_to(end, false, cx));
    cx.run_until_parked();
    (view, cx)
}

fn text(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.text())
}

fn labels(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        view.suggestions().map_or_else(Vec::new, |open| {
            open.items
                .iter()
                .map(|item| item.row.label.to_string())
                .collect()
        })
    })
}

fn highlighted(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, _| view.suggestions().unwrap().highlighted)
}

#[gpui::test]
fn double_brackets_suggest_notes_and_enter_links_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "See ");
    cx.simulate_input("[[wav");
    assert_eq!(labels(&view, cx)[..2], ["Wave Packets", "Waves"]);
    cx.simulate_keystrokes("down");
    assert_eq!(highlighted(&view, cx), 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&view, cx), "See [[Waves]]");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 13);
    assert!(labels(&view, cx).is_empty());
}

#[gpui::test]
fn tab_accepts_and_undo_takes_it_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("[[read");
    cx.simulate_keystrokes("tab");
    assert_eq!(text(&view, cx), "[[Reading]]");
    view.update(cx, |view, cx| view.undo(cx));
    assert_eq!(text(&view, cx), "[[read]]", "typing [[ paired its brackets");
}

#[gpui::test]
fn escape_hides_the_list_until_the_cursor_leaves(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("[[w");
    assert!(!labels(&view, cx).is_empty());
    cx.simulate_keystrokes("escape");
    assert!(labels(&view, cx).is_empty());
    cx.simulate_input("a");
    assert!(labels(&view, cx).is_empty(), "still dismissed");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&view, cx), "[[wa\n]]", "Enter types a newline again");
    cx.simulate_input("[[");
    assert!(!labels(&view, cx).is_empty(), "a new link suggests again");
}

#[gpui::test]
fn up_and_down_move_the_cursor_when_nothing_is_suggested(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "one\ntwo");
    cx.simulate_keystrokes("up");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 3);
}

#[gpui::test]
fn headings_of_the_current_note_after_a_hash(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "# Intro\n## Group velocity\n\n");
    cx.simulate_input("[[#gro");
    assert_eq!(labels(&view, cx), ["Group velocity"]);
    cx.simulate_keystrokes("enter");
    assert!(text(&view, cx).ends_with("[[#Group velocity]]"));
}

#[gpui::test]
fn editing_a_closed_link_keeps_its_alias(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "[[Old|shown]]");
    view.update(cx, |view, cx| view.move_to(5, false, cx));
    view.update(cx, |view, cx| view.replace(2..5, "rea", cx));
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&view, cx), "[[Reading|shown]]");
    assert_eq!(view.read_with(cx, |view, _| view.cursor()), 17);
}

#[gpui::test]
fn tags_complete_from_the_vault(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "Filed under ");
    cx.simulate_input("#ph");
    assert_eq!(labels(&view, cx), ["#physics"]);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&view, cx), "Filed under #physics ");
}

#[gpui::test]
fn no_suggestions_in_code(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "```\n\n```");
    view.update(cx, |view, cx| view.move_to(4, false, cx));
    cx.simulate_input("#ph [[w");
    assert!(labels(&view, cx).is_empty());
}
