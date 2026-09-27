//! Drives a [`TextInput`] through GPUI's test platform with the default
//! key rules bound.

use std::cell::RefCell;
use std::rc::Rc;

use editor_config::RuleSet;
use gpui::{
    ClipboardItem, Entity, EntityInputHandler, Focusable, KeyBinding, Modifiers, TestAppContext,
    VisualTestContext, actions, div, point, prelude::*, px,
};

use super::*;

type Events = Rc<RefCell<Vec<TextInputEvent>>>;

fn input(cx: &mut TestAppContext) -> (Entity<TextInput>, &mut VisualTestContext, Events) {
    cx.update(|cx| bind_keys(&RuleSet::defaults(), cx));
    let (input, cx) =
        cx.add_window_view(|window, cx| TextInput::new(window, cx).with_placeholder("Name"));
    let events: Events = Rc::default();
    let seen = events.clone();
    cx.update(|window, cx| {
        window.focus(&input.focus_handle(cx));
        cx.subscribe(&input, move |_, event: &TextInputEvent, _| {
            seen.borrow_mut().push(event.clone());
        })
        .detach();
    });
    cx.run_until_parked();
    (input, cx, events)
}

fn text(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> String {
    input.read_with(cx, |input, _| input.text().to_owned())
}

#[gpui::test]
fn typing_and_editing_keys(cx: &mut TestAppContext) {
    let (input, cx, _) = input(cx);
    cx.simulate_input("héllo");
    assert_eq!(text(&input, cx), "héllo");
    cx.simulate_keystrokes("left left backspace");
    assert_eq!(text(&input, cx), "hélo");
    cx.simulate_keystrokes("home delete");
    assert_eq!(text(&input, cx), "élo");
    cx.simulate_keystrokes("shift-end");
    cx.simulate_input("x");
    assert_eq!(text(&input, cx), "x");
}

#[gpui::test]
fn word_deletes_and_select_all_come_from_the_rules(cx: &mut TestAppContext) {
    let (input, cx, _) = input(cx);
    cx.simulate_input("daily notes");
    cx.simulate_keystrokes("ctrl-backspace");
    assert_eq!(text(&input, cx), "daily ");
    input.update(cx, |input, cx| input.set_text("old name", cx));
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("new");
    assert_eq!(text(&input, cx), "new");
}

#[gpui::test]
fn undo_and_redo(cx: &mut TestAppContext) {
    let (input, cx, events) = input(cx);
    cx.simulate_input("plans");
    cx.simulate_keystrokes("backspace");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(&input, cx), "plans");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(&input, cx), "");
    cx.simulate_keystrokes("secondary-shift-z");
    assert_eq!(text(&input, cx), "plans");
    assert!(
        events
            .borrow()
            .iter()
            .all(|e| *e == TextInputEvent::Changed)
    );
}

#[gpui::test]
fn copy_cut_and_paste_keep_one_line(cx: &mut TestAppContext) {
    let (input, cx, _) = input(cx);
    cx.write_to_clipboard(ClipboardItem::new_string("two\nlines".into()));
    cx.simulate_keystrokes("secondary-v");
    assert_eq!(text(&input, cx), "two lines");
    cx.simulate_keystrokes("shift-home secondary-x");
    assert_eq!(text(&input, cx), "");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("two lines".to_owned())
    );
    cx.simulate_keystrokes("secondary-v secondary-a secondary-c right");
    cx.simulate_keystrokes("secondary-v");
    assert_eq!(text(&input, cx), "two linestwo lines");
}

#[gpui::test]
fn enter_escape_and_blur_are_reported(cx: &mut TestAppContext) {
    let (input, cx, events) = input(cx);
    cx.update(|window, _| window.activate_window());
    cx.simulate_input("a");
    cx.simulate_keystrokes("enter escape");
    cx.update(|window, cx| {
        window.blur();
        window.draw(cx).clear();
    });
    cx.run_until_parked();
    assert_eq!(
        *events.borrow(),
        [
            TextInputEvent::Changed,
            TextInputEvent::Submitted,
            TextInputEvent::Cancelled,
            TextInputEvent::Blurred,
        ]
    );
    assert_eq!(text(&input, cx), "a");
}

#[gpui::test]
fn set_text_is_not_an_edit(cx: &mut TestAppContext) {
    let (input, cx, events) = input(cx);
    input.update(cx, |input, cx| input.set_text("line\none", cx));
    assert_eq!(text(&input, cx), "line one");
    assert_eq!(input.read_with(cx, |input, _| input.cursor()), 8);
    assert!(events.borrow().is_empty());
}

#[gpui::test]
fn ime_composition_marks_then_commits(cx: &mut TestAppContext) {
    let (input, cx, _) = input(cx);
    input.update_in(cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx);
        input.replace_and_mark_text_in_range(None, "にほ", Some(2..2), window, cx);
    });
    let marked = input.update_in(cx, |input, window, cx| input.marked_text_range(window, cx));
    assert_eq!(marked, Some(0..2));
    assert_eq!(
        input.read_with(cx, |input, _| input.marked_range()),
        Some(0..6)
    );
    assert_eq!(input.read_with(cx, |input, _| input.cursor()), 6);
    input.update_in(cx, |input, window, cx| {
        input.replace_text_in_range(None, "日本", window, cx);
    });
    assert_eq!(text(&input, cx), "日本");
    assert_eq!(input.read_with(cx, |input, _| input.marked_range()), None);
    let selection = input.update_in(cx, |input, window, cx| {
        input.selected_text_range(false, window, cx)
    });
    assert_eq!(selection.map(|selection| selection.range), Some(2..2));
}

#[gpui::test]
fn the_candidate_window_sits_under_the_composition(cx: &mut TestAppContext) {
    let (input, cx, _) = input(cx);
    cx.simulate_input("ab");
    cx.run_until_parked();
    let element = gpui::Bounds::new(point(px(0.), px(10.)), gpui::size(px(200.), px(20.)));
    let bounds = input.update_in(cx, |input, window, cx| {
        input.bounds_for_range(1..2, element, window, cx)
    });
    let bounds = bounds.expect("the line was painted");
    assert!(bounds.size.width > px(0.));
    assert_eq!(bounds.top(), px(10.));
}

#[gpui::test]
fn clicks_place_the_cursor_and_double_clicks_select_a_word(cx: &mut TestAppContext) {
    let (input, cx, _) = input(cx);
    cx.simulate_input("one two");
    cx.run_until_parked();
    cx.simulate_click(point(px(1.), px(5.)), Modifiers::none());
    assert_eq!(input.read_with(cx, |input, _| input.cursor()), 0);
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: point(px(1.), px(5.)),
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    assert_eq!(input.read_with(cx, |input, _| input.selected_range()), 0..3);
}

#[gpui::test]
fn invalid_state_is_kept(cx: &mut TestAppContext) {
    let (input, cx, _) = input(cx);
    input.update(cx, |input, cx| input.set_invalid(true, cx));
    assert!(input.read_with(cx, |input, _| input.is_invalid()));
}

actions!(text_input_test, [OwnerEnter]);

/// Hosts an input the way a picker does: Enter is bound around it too.
struct Owner {
    input: Entity<TextInput>,
    confirmed: usize,
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Owner")
            .on_action(cx.listener(|owner, _: &OwnerEnter, _, _| owner.confirmed += 1))
            .child(self.input.clone())
    }
}

fn owner(bubble: bool, cx: &mut TestAppContext) -> (Entity<Owner>, &mut VisualTestContext) {
    cx.update(|cx| {
        bind_keys(&RuleSet::defaults(), cx);
        cx.bind_keys([KeyBinding::new("enter", OwnerEnter, Some("Owner"))]);
    });
    let (owner, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let input = TextInput::new(window, cx);
            if bubble {
                input.bubble_enter_and_escape()
            } else {
                input
            }
        });
        Owner {
            input,
            confirmed: 0,
        }
    });
    cx.update(|window, cx| {
        let input = owner.read(cx).input.clone();
        window.focus(&input.focus_handle(cx));
    });
    cx.run_until_parked();
    (owner, cx)
}

#[gpui::test]
fn enter_stays_in_the_input_unless_it_bubbles(cx: &mut TestAppContext) {
    let (owner, cx) = owner(false, cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(owner.read_with(cx, |owner, _| owner.confirmed), 0);
}

#[gpui::test]
fn enter_reaches_the_owner_when_it_bubbles(cx: &mut TestAppContext) {
    let (owner, cx) = owner(true, cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(owner.read_with(cx, |owner, _| owner.confirmed), 1);
}
