//! Drives the command palette, quick switcher and jump to heading through
//! GPUI's test platform: typing filters, keys move and confirm, Escape
//! dismisses, the mouse picks rows and the palette captures new shortcuts.

use std::cell::RefCell;
use std::rc::Rc;

use editor_config::{Platform, RuleSet};
use editor_desktop::keymap::{RunCommand, WORKSPACE_CONTEXT, bind_rules};
use editor_desktop::outline::{OutlineEvent, OutlinePicker};
use editor_desktop::palette::{CommandPalette, PaletteEvent};
use editor_desktop::picker;
use editor_desktop::switcher::{QuickSwitcher, SwitcherEvent};
use editor_desktop::text_input::input_bindings;
use editor_desktop::theme::{InputTheme, PickerTheme};
use gpui::{
    AppContext, ClipboardItem, Context, DismissEvent, Entity, EntityInputHandler, EventEmitter,
    Focusable, IntoElement, ManagedView, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, point, prelude::*, px,
};

/// Stands in for the workspace: sets its key context, hosts one modal and
/// records the commands that reach it and the events the modal emits.
struct Host<V: ManagedView> {
    modal: Entity<V>,
    commands: Vec<String>,
}

impl<V: ManagedView> Host<V> {
    fn on_command(&mut self, action: &RunCommand, _: &mut Window, _: &mut Context<Self>) {
        self.commands.push(action.id.to_string());
    }
}

impl<V: ManagedView> Render for Host<V> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(WORKSPACE_CONTEXT)
            .on_action(cx.listener(Self::on_command))
            .size_full()
            .child(self.modal.clone())
    }
}

type Log<E> = Rc<RefCell<Vec<E>>>;

struct Opened<'a, V: ManagedView, E> {
    host: Entity<Host<V>>,
    modal: Entity<V>,
    events: Log<E>,
    dismissed: Rc<RefCell<usize>>,
    cx: &'a mut VisualTestContext,
}

fn open<'a, V, E>(
    cx: &'a mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<V>) -> V + 'static,
) -> Opened<'a, V, E>
where
    V: ManagedView + EventEmitter<E>,
    E: Clone + 'static,
{
    cx.update(|cx| {
        bind_rules(&RuleSet::defaults(), cx);
        picker::bind_keys(cx);
    });
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let modal = cx.new(|cx| build(window, cx));
        Host {
            modal,
            commands: Vec::new(),
        }
    });
    let modal = host.read_with(cx, |host, _| host.modal.clone());
    let events: Log<E> = Rc::default();
    let dismissed = Rc::new(RefCell::new(0));
    cx.update(|window, cx| {
        let log = events.clone();
        cx.subscribe(&modal, move |_, event: &E, _| {
            log.borrow_mut().push(event.clone())
        })
        .detach();
        let count = dismissed.clone();
        cx.subscribe(&modal, move |_, _: &DismissEvent, _| {
            *count.borrow_mut() += 1
        })
        .detach();
        window.focus(&modal.focus_handle(cx));
    });
    cx.run_until_parked();
    Opened {
        host,
        modal,
        events,
        dismissed,
        cx,
    }
}

fn palette<'a>(
    cx: &'a mut TestAppContext,
    recent: &[&str],
) -> Opened<'a, CommandPalette, PaletteEvent> {
    let recent: Vec<String> = recent.iter().map(|id| id.to_string()).collect();
    open(cx, move |window, cx| {
        CommandPalette::new(&RuleSet::defaults(), recent, window, cx)
    })
}

fn palette_rows(opened: &mut Opened<'_, CommandPalette, PaletteEvent>) -> Vec<String> {
    let picker = opened
        .modal
        .read_with(opened.cx, |palette, _| palette.picker().clone());
    picker.read_with(opened.cx, |picker, _| {
        let delegate = picker.delegate();
        (0..picker::PickerDelegate::match_count(delegate))
            .map(|index| delegate.command_at(index).unwrap().id.clone())
            .collect()
    })
}

fn palette_selection(opened: &mut Opened<'_, CommandPalette, PaletteEvent>) -> usize {
    let picker = opened
        .modal
        .read_with(opened.cx, |palette, _| palette.picker().clone());
    picker.read_with(opened.cx, |picker, _| picker.selected_index())
}

fn palette_query(opened: &mut Opened<'_, CommandPalette, PaletteEvent>) -> String {
    let picker = opened
        .modal
        .read_with(opened.cx, |palette, _| palette.picker().clone());
    picker.read_with(opened.cx, |picker, cx| picker.query(cx))
}

/// The GPUI keystroke the default rules bind to `command` on this platform.
fn key_for(command: &str) -> String {
    input_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|(_, id)| id == command)
        .map(|(keystroke, _)| keystroke)
        .unwrap_or_else(|| panic!("no key for {command}"))
}

#[gpui::test]
fn palette_typing_filters_the_commands(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    let everything = palette_rows(&mut opened).len();
    opened.cx.simulate_input("bold");
    assert_eq!(palette_query(&mut opened), "bold");
    let rows = palette_rows(&mut opened);
    assert!(rows.len() < everything);
    assert_eq!(rows[0], "format.bold");
}

#[gpui::test]
fn palette_lists_recent_commands_first(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &["sync.now", "tab.new"]);
    assert_eq!(palette_rows(&mut opened)[..2], ["sync.now", "tab.new"]);
}

#[gpui::test]
fn arrows_and_ctrl_n_p_move_the_selection(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    let count = palette_rows(&mut opened).len();
    opened.cx.simulate_keystrokes("down down");
    assert_eq!(palette_selection(&mut opened), 2);
    opened.cx.simulate_keystrokes("up");
    assert_eq!(palette_selection(&mut opened), 1);
    opened.cx.simulate_keystrokes("ctrl-n ctrl-n ctrl-p");
    assert_eq!(palette_selection(&mut opened), 2);
    opened.cx.simulate_keystrokes("up up up");
    assert_eq!(palette_selection(&mut opened), count - 1);
    opened.cx.simulate_keystrokes("down");
    assert_eq!(palette_selection(&mut opened), 0);
    opened.cx.simulate_keystrokes("pagedown");
    let visible = PickerTheme::default().visible_rows;
    assert_eq!(palette_selection(&mut opened), visible - 1);
    let commands = opened
        .host
        .read_with(opened.cx, |host, _| host.commands.clone());
    assert!(commands.is_empty(), "{commands:?} leaked to the workspace");
}

#[gpui::test]
fn typing_resets_the_selection_to_the_best_match(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    opened.cx.simulate_keystrokes("down down");
    opened.cx.simulate_input("tab");
    assert_eq!(palette_selection(&mut opened), 0);
}

#[gpui::test]
fn enter_runs_the_selected_command_and_closes(cx: &mut TestAppContext) {
    let opened = palette(cx, &[]);
    opened.cx.simulate_input("new tab");
    opened.cx.simulate_keystrokes("enter");
    assert_eq!(
        *opened.events.borrow(),
        [PaletteEvent::Run("tab.new".into())]
    );
    assert_eq!(*opened.dismissed.borrow(), 1);
}

#[gpui::test]
fn escape_dismisses_without_running_anything(cx: &mut TestAppContext) {
    let opened = palette(cx, &[]);
    opened.cx.simulate_input("bold");
    opened.cx.simulate_keystrokes("escape");
    assert!(opened.events.borrow().is_empty());
    assert_eq!(*opened.dismissed.borrow(), 1);
}

#[gpui::test]
fn no_match_leaves_nothing_to_confirm(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    opened.cx.simulate_input("qqqzzz");
    assert!(palette_rows(&mut opened).is_empty());
    opened.cx.simulate_keystrokes("down enter");
    assert!(opened.events.borrow().is_empty());
    assert_eq!(*opened.dismissed.borrow(), 0);
}

#[gpui::test]
fn workspace_keys_still_reach_the_workspace(cx: &mut TestAppContext) {
    let opened = palette(cx, &[]);
    opened.cx.simulate_keystrokes("secondary-shift-p");
    let commands = opened
        .host
        .read_with(opened.cx, |host, _| host.commands.clone());
    assert_eq!(commands, ["palette.open"]);
}

#[gpui::test]
fn the_query_edits_with_the_editing_keys(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    opened.cx.simulate_input("go to tab");
    opened
        .cx
        .simulate_keystrokes(&key_for("edit.delete-word-backward"));
    assert_eq!(palette_query(&mut opened), "go to ");
    opened
        .cx
        .simulate_keystrokes(&key_for("edit.delete-word-backward"));
    assert_eq!(palette_query(&mut opened), "go ");
    opened.cx.simulate_keystrokes("backspace");
    assert_eq!(palette_query(&mut opened), "go");
    opened.cx.simulate_keystrokes(&key_for("cursor.line-start"));
    opened.cx.simulate_input("x");
    assert_eq!(palette_query(&mut opened), "xgo");
    opened.cx.simulate_keystrokes("right delete");
    assert_eq!(palette_query(&mut opened), "xg");
}

#[gpui::test]
fn paste_puts_clipboard_text_on_one_line(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    opened
        .cx
        .update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("tab\nnew".into())));
    opened.cx.simulate_keystrokes(&key_for("edit.paste"));
    assert_eq!(palette_query(&mut opened), "tab new");
}

#[gpui::test]
fn ime_composition_edits_the_query(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    let picker = opened
        .modal
        .read_with(opened.cx, |palette, _| palette.picker().clone());
    let input = picker.read_with(opened.cx, |picker, _| picker.query_input().clone());
    opened.cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "に", Some(1..1), window, cx);
            input.replace_and_mark_text_in_range(None, "にほ", Some(2..2), window, cx);
        });
    });
    let marked = input.read_with(opened.cx, |input, _| input.marked_range());
    assert_eq!(marked, Some(0..6));
    opened.cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "日本", window, cx)
        });
    });
    assert_eq!(palette_query(&mut opened), "日本");
    assert_eq!(
        input.read_with(opened.cx, |input, _| input.marked_range()),
        None
    );
}

#[gpui::test]
fn clicking_a_row_runs_it(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    let rows = palette_rows(&mut opened);
    let theme = PickerTheme::default();
    let input = InputTheme::default();
    let list_top = theme.input_padding_y * 2. + input.line_height(input.query_font_size);
    let second_row = list_top + theme.row_height * 1.5;
    opened
        .cx
        .simulate_click(point(px(100.), second_row), Modifiers::none());
    assert_eq!(
        *opened.events.borrow(),
        [PaletteEvent::Run(rows[1].clone())]
    );
}

#[gpui::test]
fn mod_enter_binds_the_next_chord(cx: &mut TestAppContext) {
    let opened = palette(cx, &[]);
    opened.cx.simulate_input("sync now");
    opened.cx.simulate_keystrokes("secondary-enter");
    let capturing = opened.modal.read_with(opened.cx, |palette, _| {
        palette.capturing().map(str::to_string)
    });
    assert_eq!(capturing.as_deref(), Some("sync.now"));
    opened.cx.simulate_keystrokes("secondary-shift-k");
    let expected = PaletteEvent::Bind {
        command: "sync.now".into(),
        chord: "Mod+Shift+K".into(),
    };
    assert_eq!(*opened.events.borrow(), [expected]);
    assert_eq!(*opened.dismissed.borrow(), 1);
    let commands = opened
        .host
        .read_with(opened.cx, |host, _| host.commands.clone());
    assert!(commands.is_empty(), "{commands:?} fired while capturing");
}

#[gpui::test]
fn capture_refuses_plain_keys_and_escape_goes_back(cx: &mut TestAppContext) {
    let mut opened = palette(cx, &[]);
    opened.cx.simulate_input("bold");
    opened.cx.simulate_keystrokes("secondary-enter k");
    let (capturing, rejection) = opened.modal.read_with(opened.cx, |palette, _| {
        (
            palette.capturing().map(str::to_string),
            palette.capture_rejection(),
        )
    });
    assert_eq!(capturing.as_deref(), Some("format.bold"));
    assert!(rejection.is_some());
    assert_eq!(palette_query(&mut opened), "bold");
    opened.cx.simulate_keystrokes("escape");
    let capturing = opened.modal.read_with(opened.cx, |palette, _| {
        palette.capturing().map(str::to_string)
    });
    assert_eq!(capturing, None);
    assert_eq!(*opened.dismissed.borrow(), 0);
    opened.cx.simulate_input("x");
    assert_eq!(palette_query(&mut opened), "boldx");
    assert!(opened.events.borrow().is_empty());
}

#[gpui::test]
fn shortcuts_show_in_the_platform_style(cx: &mut TestAppContext) {
    let opened: Opened<'_, CommandPalette, PaletteEvent> = open(cx, |window, cx| {
        CommandPalette::for_platform(
            &RuleSet::defaults(),
            Vec::new(),
            Platform::Macos,
            window,
            cx,
        )
    });
    let picker = opened
        .modal
        .read_with(opened.cx, |palette, _| palette.picker().clone());
    let shortcuts = picker.read_with(opened.cx, |picker, _| {
        let find = |id: &str| {
            picker
                .delegate()
                .commands()
                .iter()
                .find(|command| command.id == id)
                .unwrap()
                .shortcuts
                .clone()
        };
        (
            find("app.export"),
            find("settings.open"),
            find("note.rename"),
        )
    });
    assert_eq!(shortcuts.0, ["⇧⌘S"]);
    assert_eq!(shortcuts.1, ["⌘,", "⌘L"]);
    assert!(shortcuts.2.is_empty());
}

fn switcher(cx: &mut TestAppContext) -> Opened<'_, QuickSwitcher, SwitcherEvent> {
    let paths = [
        "daily/2024-05-01.md",
        "projects/editor plan.md",
        "reading list.md",
    ];
    let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    open(cx, move |window, cx| {
        QuickSwitcher::new(paths, vec!["reading list.md".into()], window, cx)
    })
}

fn switcher_count(opened: &mut Opened<'_, QuickSwitcher, SwitcherEvent>) -> usize {
    let picker = opened
        .modal
        .read_with(opened.cx, |switcher, _| switcher.picker().clone());
    picker.read_with(opened.cx, |picker, _| {
        picker::PickerDelegate::match_count(picker.delegate())
    })
}

#[gpui::test]
fn switcher_opens_the_best_match(cx: &mut TestAppContext) {
    let mut opened = switcher(cx);
    assert_eq!(switcher_count(&mut opened), 3);
    opened.cx.simulate_input("plan");
    assert_eq!(switcher_count(&mut opened), 1);
    opened.cx.simulate_keystrokes("enter");
    let expected = SwitcherEvent::Open {
        path: "projects/editor plan.md".into(),
        new_tab: false,
    };
    assert_eq!(*opened.events.borrow(), [expected]);
    assert_eq!(*opened.dismissed.borrow(), 1);
}

#[gpui::test]
fn switcher_mod_enter_opens_a_new_tab(cx: &mut TestAppContext) {
    let opened = switcher(cx);
    opened.cx.simulate_keystrokes("secondary-enter");
    let expected = SwitcherEvent::Open {
        path: "reading list.md".into(),
        new_tab: true,
    };
    assert_eq!(*opened.events.borrow(), [expected]);
}

#[gpui::test]
fn switcher_offers_to_create_a_missing_note(cx: &mut TestAppContext) {
    let mut opened = switcher(cx);
    opened.cx.simulate_input("groceries");
    assert_eq!(switcher_count(&mut opened), 1);
    opened.cx.simulate_keystrokes("enter");
    assert_eq!(
        *opened.events.borrow(),
        [SwitcherEvent::Create("groceries".into())]
    );
}

#[gpui::test]
fn switcher_escape_dismisses(cx: &mut TestAppContext) {
    let opened = switcher(cx);
    opened.cx.simulate_keystrokes("escape");
    assert!(opened.events.borrow().is_empty());
    assert_eq!(*opened.dismissed.borrow(), 1);
}

const NOTE: &str = "# Trip\n\nIntro\n\n## Packing\n\nsocks\n\n### Shoes\n\n## Budget\n";

fn outline(cx: &mut TestAppContext, cursor: usize) -> Opened<'_, OutlinePicker, OutlineEvent> {
    open(cx, move |window, cx| {
        OutlinePicker::new(NOTE, cursor, window, cx)
    })
}

#[gpui::test]
fn outline_starts_on_the_current_heading(cx: &mut TestAppContext) {
    let cursor = NOTE.find("socks").unwrap();
    let opened = outline(cx, cursor);
    opened.cx.simulate_keystrokes("enter");
    let packing = NOTE.find("## Packing").unwrap();
    assert_eq!(*opened.events.borrow(), [OutlineEvent::Jump(packing)]);
}

#[gpui::test]
fn outline_typing_filters_and_arrows_move(cx: &mut TestAppContext) {
    let opened = outline(cx, 0);
    opened.cx.simulate_input("s");
    opened.cx.simulate_keystrokes("down enter");
    let events = opened.events.borrow().clone();
    assert_eq!(events.len(), 1);
    let OutlineEvent::Jump(offset) = events[0];
    assert!(NOTE[offset..].starts_with('#'));
    assert_eq!(*opened.dismissed.borrow(), 1);
}

#[gpui::test]
fn outline_escape_dismisses(cx: &mut TestAppContext) {
    let opened = outline(cx, 0);
    opened.cx.simulate_keystrokes("escape");
    assert!(opened.events.borrow().is_empty());
    assert_eq!(*opened.dismissed.borrow(), 1);
}
