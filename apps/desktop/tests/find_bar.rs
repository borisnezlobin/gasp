//! Drives the find bar over an editor: seeding, live matching, keyboard
//! navigation, toggles, replace and closing. A small workspace view hosts
//! both and forwards `find.next` and `find.previous`, as the real one does.

use std::cell::RefCell;
use std::rc::Rc;

use gasp_config::{Platform, RuleSet};
use gasp_desktop::actions::bind_keys;
use gasp_desktop::find::{self, FindBar, FindBarEvent};
use gasp_desktop::keymap::{RunCommand, WORKSPACE_CONTEXT, all_bindings};
use gasp_desktop::{EditorView, HighlightKind};
use gpui::{
    ClipboardItem, Context, Entity, Focusable, IntoElement, Render, TestAppContext,
    VisualTestContext, Window, div, prelude::*,
};

struct Workspace {
    editor: Entity<EditorView>,
    bar: Entity<FindBar>,
}

impl Workspace {
    fn on_run_command(&mut self, action: &RunCommand, _: &mut Window, cx: &mut Context<Self>) {
        match action.id.as_ref() {
            "find.next" => self.bar.update(cx, |bar, cx| bar.next(cx)),
            "find.previous" => self.bar.update(cx, |bar, cx| bar.previous(cx)),
            _ => {}
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(WORKSPACE_CONTEXT)
            .on_action(cx.listener(Self::on_run_command))
            .size_full()
            .flex()
            .flex_col()
            .child(self.bar.clone())
            .child(self.editor.clone())
    }
}

struct Harness<'a> {
    editor: Entity<EditorView>,
    bar: Entity<FindBar>,
    events: Rc<RefCell<Vec<FindBarEvent>>>,
    cx: &'a mut VisualTestContext,
}

/// Opens an editor on `text` with `selection` selected, then the bar.
fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    selection: std::ops::Range<usize>,
) -> Harness<'a> {
    cx.update(|cx| {
        bind_keys(cx);
        find::bind_keys(cx);
    });
    let text = text.to_owned();
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let editor = cx.new(|cx| EditorView::new(&text, Vec::new(), cx));
        editor.update(cx, |editor, cx| {
            editor.select(selection.start, selection.end, cx)
        });
        let bar = cx.new(|cx| FindBar::new(editor.clone(), window, cx));
        Workspace { editor, bar }
    });
    let (editor, bar) = workspace.read_with(cx, |workspace, _| {
        (workspace.editor.clone(), workspace.bar.clone())
    });
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&bar, move |_, event: &FindBarEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });
    cx.run_until_parked();
    Harness {
        editor,
        bar,
        events,
        cx,
    }
}

impl Harness<'_> {
    fn matches(&mut self) -> Vec<std::ops::Range<usize>> {
        self.bar.read_with(self.cx, |bar, _| bar.matches().to_vec())
    }

    fn label(&mut self) -> String {
        self.bar.read_with(self.cx, |bar, cx| bar.label(cx))
    }

    fn selection(&mut self) -> std::ops::Range<usize> {
        self.editor
            .read_with(self.cx, |editor, _| editor.selected_range())
    }

    fn text(&mut self) -> String {
        self.editor.read_with(self.cx, |editor, _| editor.text())
    }

    fn highlights(&mut self, kind: HighlightKind) -> Vec<std::ops::Range<usize>> {
        self.editor
            .read_with(self.cx, |editor, _| editor.highlights(kind).to_vec())
    }

    fn keys(&mut self, keys: &str) {
        self.cx.simulate_keystrokes(keys);
        self.cx.run_until_parked();
    }

    fn type_text(&mut self, text: &str) {
        self.cx.simulate_input(text);
        self.cx.run_until_parked();
    }

    fn editor_focused(&mut self) -> bool {
        let editor = self.editor.clone();
        self.cx
            .update(|window, cx| editor.focus_handle(cx).is_focused(window))
    }
}

/// The keystroke this platform's default rules bind to `command`.
fn key_for(command: &str) -> String {
    all_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|binding| binding.command == command)
        .map(|binding| binding.keystroke)
        .unwrap_or_else(|| panic!("{command} has no key on this platform"))
}

const NOTE: &str = "one two one\ntwo one two";

#[gpui::test]
fn opening_seeds_the_query_from_the_selection(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 4..7);
    assert_eq!(h.bar.read_with(h.cx, |bar, cx| bar.query_text(cx)), "two");
    assert_eq!(h.matches(), vec![4..7, 12..15, 20..23]);
    assert_eq!(h.label(), "1 of 3");
    assert_eq!(h.highlights(HighlightKind::SearchMatch).len(), 3);
    assert_eq!(h.highlights(HighlightKind::ActiveSearchMatch), vec![4..7]);
}

#[gpui::test]
fn typing_finds_as_you_type(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 0..0);
    assert_eq!(h.label(), "");
    h.type_text("on");
    assert_eq!(h.matches(), vec![0..2, 8..10, 16..18]);
    assert_eq!(h.selection(), 0..2);
    h.type_text("ex");
    assert_eq!(h.label(), "No results");
    assert!(h.highlights(HighlightKind::SearchMatch).is_empty());
    assert_eq!(h.selection(), 0..0, "the last match isn't left selected");
}

#[gpui::test]
fn enter_and_shift_enter_step_through_matches(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 0..3);
    h.keys("enter");
    assert_eq!(h.label(), "2 of 3");
    assert_eq!(h.selection(), 8..11);
    h.keys("enter enter");
    assert_eq!(h.label(), "1 of 3");
    h.keys("shift-enter");
    assert_eq!(h.label(), "3 of 3");
    assert_eq!(h.highlights(HighlightKind::ActiveSearchMatch), vec![16..19]);
}

#[gpui::test]
fn next_and_previous_work_from_the_editor(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 0..3);
    let editor = h.editor.clone();
    h.cx.update(|window, cx| window.focus(&editor.focus_handle(cx)));
    h.keys(&key_for("find.next"));
    assert_eq!(h.selection(), 8..11);
    h.keys(&key_for("find.previous"));
    assert_eq!(h.selection(), 0..3);
    // Moving the cursor makes the next search start from it.
    h.editor
        .update(h.cx, |editor, cx| editor.select(12, 12, cx));
    h.keys(&key_for("find.next"));
    assert_eq!(h.selection(), 16..19);
}

#[gpui::test]
fn matches_follow_edits(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 0..3);
    h.editor.update(h.cx, |editor, cx| {
        editor.replace(0..0, "one ", cx);
    });
    h.cx.run_until_parked();
    assert_eq!(h.matches(), vec![0..3, 4..7, 12..15, 20..23]);
    assert_eq!(h.highlights(HighlightKind::SearchMatch).len(), 4);
    h.editor.update(h.cx, |editor, cx| editor.undo(cx));
    h.cx.run_until_parked();
    assert_eq!(h.matches().len(), 3);
}

#[gpui::test]
fn toggles_change_matching(cx: &mut TestAppContext) {
    let mut h = open(cx, "Word word words", 0..0);
    h.type_text("word");
    assert_eq!(h.matches().len(), 3);
    h.keys("alt-c");
    assert_eq!(h.matches(), vec![5..9, 10..14]);
    h.keys("alt-w");
    assert_eq!(h.matches(), vec![5..9]);
    h.keys("alt-c alt-w alt-r");
    h.bar.update(h.cx, |bar, cx| bar.set_query("w(o", cx));
    h.cx.run_until_parked();
    assert!(h.matches().is_empty());
    h.bar
        .update(h.cx, |bar, cx| bar.set_query(r"w(o)rds?\b", cx));
    h.cx.run_until_parked();
    assert_eq!(h.matches().len(), 3);
}

#[gpui::test]
fn replace_one_then_all_undoes_in_one_step(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 0..3);
    h.keys(&key_for("find.replace"));
    assert!(h.bar.read_with(h.cx, |bar, _| bar.is_replace_visible()));
    h.type_text("1");
    h.keys("enter");
    assert_eq!(h.text(), "1 two one\ntwo one two");
    assert_eq!(h.label(), "1 of 2");
    h.keys("secondary-alt-enter");
    assert_eq!(h.text(), "1 two 1\ntwo 1 two");
    assert_eq!(h.label(), "No results");
    h.editor.update(h.cx, |editor, cx| editor.undo(cx));
    assert_eq!(h.text(), "1 two one\ntwo one two");
}

#[gpui::test]
fn regex_replace_expands_groups(cx: &mut TestAppContext) {
    let mut h = open(cx, "a=1, b=2", 0..0);
    h.bar.update(h.cx, |bar, cx| {
        bar.set_options(
            gasp_core::find::FindOptions {
                regex: true,
                ..Default::default()
            },
            cx,
        );
        bar.set_query(r"(\w)=(\d)", cx);
        bar.set_replacement("$2=$1", cx);
        assert_eq!(bar.replace_all(cx), 2);
    });
    assert_eq!(h.text(), "1=a, 2=b");
}

#[gpui::test]
fn escape_closes_and_selects_the_match(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 0..3);
    h.keys("enter");
    h.keys("escape");
    assert!(h.editor_focused());
    assert_eq!(h.selection(), 8..11);
    assert!(h.highlights(HighlightKind::SearchMatch).is_empty());
    assert_eq!(*h.events.borrow(), vec![FindBarEvent::Dismissed]);
}

#[gpui::test]
fn a_closed_bar_leaves_edits_unmarked(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 0..3);
    h.keys("escape");
    h.editor.update(h.cx, |editor, cx| {
        editor.replace(0..0, "one ", cx);
    });
    h.cx.run_until_parked();
    assert!(h.highlights(HighlightKind::SearchMatch).is_empty());
    assert!(h.highlights(HighlightKind::ActiveSearchMatch).is_empty());
}

#[gpui::test]
fn the_query_field_edits_like_a_text_field(cx: &mut TestAppContext) {
    let mut h = open(cx, NOTE, 0..0);
    h.cx.write_to_clipboard(ClipboardItem::new_string("two\n".into()));
    h.keys(&key_for("edit.paste"));
    assert_eq!(h.bar.read_with(h.cx, |bar, cx| bar.query_text(cx)), "two ");
    h.keys(&key_for("edit.delete-backward"));
    assert_eq!(h.matches().len(), 3);
    h.keys(&key_for("select.all"));
    h.type_text("one");
    assert_eq!(h.bar.read_with(h.cx, |bar, cx| bar.query_text(cx)), "one");
    h.keys(&key_for("cursor.left"));
    h.keys(&key_for("edit.delete-backward"));
    assert_eq!(h.bar.read_with(h.cx, |bar, cx| bar.query_text(cx)), "oe");
}
