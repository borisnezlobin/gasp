//! Toolbars in a real workspace: the status bar is the built-in `status`
//! toolbar, the selection bar floats over selected text and presses its
//! commands into the note, a vault's toolbars.toml adds bars that show as
//! their behaviour says, and the keyboard moves through every bar. The
//! Toolbars settings page writes toolbars.toml from the keyboard.

use std::path::Path;
use std::time::Duration;

use gasp_config::{CONFIG_DIR, RuleSet, Toolbars};
use gasp_desktop::actions::bind_keys;
use gasp_desktop::features;
use gasp_desktop::settings_view::model::TOOLBARS_SECTION;
use gasp_desktop::settings_view::toolbars_page::ToolbarField;
use gasp_desktop::settings_view::{ControlRow, SettingsView};
use gasp_desktop::text_input;
use gasp_desktop::toolbar::{FocusStop, ToolbarFocus};
use gasp_desktop::workspace::{OpenIn, Workspace};
use gpui::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};
use tempfile::TempDir;

fn vault(toolbars: Option<&str>) -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Note.md"), "Some plain words here.\n").unwrap();
    if let Some(text) = toolbars {
        std::fs::create_dir_all(vault.path().join(CONFIG_DIR)).unwrap();
        std::fs::write(vault.path().join(CONFIG_DIR).join("toolbars.toml"), text).unwrap();
    }
    vault
}

fn open_workspace<'a>(
    cx: &'a mut TestAppContext,
    vault: &Path,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        bind_keys(cx);
        features::bind_view_keys(cx);
    });
    let vault = vault.to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let mut workspace = Workspace::new(&vault, window, cx);
        features::install(&mut workspace, window, cx);
        workspace
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            let path = workspace.vault().join("Note.md");
            workspace
                .open_path(&path, OpenIn::ActiveTab, window, cx)
                .unwrap();
            workspace.focus_active(window, cx);
        })
    });
    cx.run_until_parked();
    (workspace, cx)
}

fn select(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, from: usize, to: usize) {
    cx.update(|_, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.update(cx, |editor, cx| editor.select(from, to, cx));
    });
    cx.run_until_parked();
}

fn text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        workspace
            .read(cx)
            .active_editor(cx)
            .unwrap()
            .read(cx)
            .text()
    })
}

/// Whether an element was drawn. GPUI keeps an element's bounds after it
/// stops being drawn, so this only says something went up; whether a bar
/// is up now comes from [`docked`] and [`floating`].
fn drawn(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.run_until_parked();
    cx.debug_bounds(selector).is_some()
}

fn docked(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.run_until_parked();
    cx.read(|cx| workspace.read(cx).shown_toolbars(cx))
}

fn floating(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.run_until_parked();
    cx.read(|cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        let shown = editor.read(cx).shown_floating_toolbars();
        shown.iter().map(|toolbar| toolbar.id.clone()).collect()
    })
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} isn't drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_status_bar_is_the_status_toolbar(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (workspace, cx) = open_workspace(cx, dir.path());
    let bar = cx.debug_bounds("status-bar").expect("the status bar draws");
    let window = cx.update(|window, _| window.viewport_size());
    let ui = cx.update(|_, cx| gasp_desktop::ui::ui_theme(cx));
    assert_eq!(bar.size.height, ui.status_height);
    assert_eq!(bar.bottom(), window.height);
    let items = cx.read(|cx| workspace.read(cx).status().unwrap().items());
    assert_eq!(items, ["4 words", "22 characters", "1 min read", "1:1"]);
    assert!(
        !drawn(cx, "toolbar-status-add"),
        "the add button waits for the pointer"
    );
    cx.simulate_mouse_move(bar.center(), None, Modifiers::none());
    assert!(drawn(cx, "toolbar-status-add"));
}

#[gpui::test]
fn the_selection_bar_shows_with_a_selection_and_presses_into_the_note(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (workspace, cx) = open_workspace(cx, dir.path());
    assert!(floating(&workspace, cx).is_empty());
    assert!(!drawn(cx, "floating-toolbar-selection"));
    select(&workspace, cx, 5, 10);
    assert_eq!(floating(&workspace, cx), ["selection"]);
    let bar = cx
        .debug_bounds("floating-toolbar-selection")
        .expect("a selection shows the bar");
    let bold = cx.debug_bounds("toolbar-selection-0").unwrap();
    assert!(bar.contains(&bold.center()));
    click(cx, "toolbar-selection-0");
    assert_eq!(
        text(&workspace, cx),
        "Some **plain** words here.
"
    );
    let active = cx.read(|cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).active_commands()
    });
    assert!(active.contains(&"format.bold"), "{active:?}");
    select(&workspace, cx, 0, 0);
    assert!(floating(&workspace, cx).is_empty());
}

const BOTTOM_BAR: &str = "\
[toolbar.writing]
title     = \"Writing\"
place     = \"editor-bottom\"
behaviour = \"hide-while-typing\"
items     = [\"export.html\", \"separator\", \"format.bullet-list\", \"menu:insert\"]

[timing]
typing-pause = \"500ms\"
";

#[gpui::test]
fn a_vault_bar_hides_while_typing_and_comes_back(cx: &mut TestAppContext) {
    let dir = vault(Some(BOTTOM_BAR));
    let (workspace, cx) = open_workspace(cx, dir.path());
    assert!(docked(&workspace, cx).contains(&"writing".to_owned()));
    assert!(drawn(cx, "toolbar-writing-0"));
    cx.simulate_input("x");
    assert!(
        !docked(&workspace, cx).contains(&"writing".to_owned()),
        "typing puts it away"
    );
    cx.executor().advance_clock(Duration::from_millis(600));
    assert!(
        docked(&workspace, cx).contains(&"writing".to_owned()),
        "a pause brings it back"
    );
    click(cx, "toolbar-writing-2");
    assert!(
        text(&workspace, cx).starts_with("- "),
        "{}",
        text(&workspace, cx)
    );
}

#[gpui::test]
fn a_bar_menu_opens_its_commands(cx: &mut TestAppContext) {
    let dir = vault(Some(BOTTOM_BAR));
    let (workspace, cx) = open_workspace(cx, dir.path());
    click(cx, "toolbar-writing-3");
    let labels = cx.read(|cx| {
        let menu = workspace.read(cx).open_menu(cx).expect("the menu opens");
        menu.read(cx).labels()
    });
    assert_eq!(labels[0], "Insert table");
}

/// The status bar is right-aligned, so a cursor position growing from
/// "1:1" to "1:21" used to push every widget before it to the left.
#[gpui::test]
fn the_status_widgets_stay_put_as_the_cursor_moves(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (workspace, cx) = open_workspace(cx, dir.path());
    let before = cx.debug_bounds("status-word-count").expect("drawn");
    let position = cx.debug_bounds("status-cursor-position").expect("drawn");
    select(&workspace, cx, 20, 20);
    let status = cx.read(|cx| workspace.read(cx).status().unwrap().position_label());
    assert_eq!(status, "1:21");
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("status-word-count"), Some(before));
    assert_eq!(cx.debug_bounds("status-cursor-position"), Some(position));
    select(&workspace, cx, 0, 0);
    assert_eq!(cx.debug_bounds("status-word-count"), Some(before));
}

const HOVER_BAR: &str = "\
[toolbar.writing]
title     = \"Writing\"
place     = \"editor-bottom\"
behaviour = \"on-hover\"
items     = [\"format.bold\", \"menu:insert\"]
";

/// Rests the pointer on the strip along the notes' bottom edge, just
/// above the status bar, until the hover bar shows.
fn reveal_bottom_bar(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let status = cx.debug_bounds("status-bar").expect("the status bar draws");
    let edge = gpui::point(status.center().x, status.top() - gpui::px(2.));
    cx.simulate_mouse_move(edge, None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(300));
    assert_eq!(docked(workspace, cx), ["writing", "status"]);
}

fn rest(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_mouse_move(
        gpui::point(gpui::px(x), gpui::px(y)),
        None,
        Modifiers::none(),
    );
    cx.run_until_parked();
}

fn wait_out_the_hide_delay(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

/// The menu's backdrop covers the bar, which used to count as the pointer
/// leaving it: the bar went, taking the open menu's button with it.
#[gpui::test]
fn a_hover_bar_stays_while_its_menu_is_open(cx: &mut TestAppContext) {
    let dir = vault(Some(HOVER_BAR));
    let (workspace, cx) = open_workspace(cx, dir.path());
    reveal_bottom_bar(&workspace, cx);
    click(cx, "toolbar-writing-1");
    assert!(cx.read(|cx| workspace.read(cx).open_menu(cx).is_some()));
    rest(cx, 200., 200.);
    wait_out_the_hide_delay(cx);
    assert!(docked(&workspace, cx).contains(&"writing".to_owned()));
    cx.simulate_keystrokes("escape");
    rest(cx, 210., 200.);
    wait_out_the_hide_delay(cx);
    assert_eq!(docked(&workspace, cx), ["status"]);
}

/// A pointer that leaves straight from the edge strip, never touching
/// the bar that came up over it, still hides the bar.
#[gpui::test]
fn a_hover_bar_hides_when_the_pointer_leaves_from_its_edge(cx: &mut TestAppContext) {
    let dir = vault(Some(HOVER_BAR));
    let (workspace, cx) = open_workspace(cx, dir.path());
    reveal_bottom_bar(&workspace, cx);
    rest(cx, 200., 200.);
    assert!(
        docked(&workspace, cx).contains(&"writing".to_owned()),
        "not at once"
    );
    wait_out_the_hide_delay(cx);
    assert_eq!(docked(&workspace, cx), ["status"]);
}

#[gpui::test]
fn the_keyboard_moves_through_the_bars(cx: &mut TestAppContext) {
    let dir = vault(Some(BOTTOM_BAR));
    let (workspace, cx) = open_workspace(cx, dir.path());
    let focus =
        |cx: &mut VisualTestContext| cx.read(|cx| workspace.read(cx).toolbar_focus().cloned());
    cx.simulate_keystrokes("alt-shift-t");
    assert_eq!(
        focus(cx),
        Some(ToolbarFocus {
            toolbar: "writing".into(),
            stop: FocusStop::Item(0)
        })
    );
    cx.simulate_keystrokes("right");
    assert_eq!(
        focus(cx).unwrap().stop,
        FocusStop::Item(2),
        "the separator is skipped"
    );
    cx.simulate_keystrokes("tab");
    assert_eq!(focus(cx).unwrap().toolbar, "status");
    assert!(
        drawn(cx, "toolbar-status-add"),
        "the keyboard shows the add button"
    );
    cx.simulate_keystrokes("shift-tab right enter");
    assert_eq!(focus(cx), None, "pressing a command goes back to the note");
    assert!(text(&workspace, cx).starts_with("- "));
    select(&workspace, cx, 2, 6);
    cx.simulate_keystrokes("alt-shift-t");
    assert_eq!(
        focus(cx).unwrap().toolbar,
        "selection",
        "the bar by the selection comes first"
    );
    assert!(drawn(cx, "floating-toolbar-selection"));
    cx.simulate_keystrokes("right enter");
    assert_eq!(text(&workspace, cx), "- *Some* plain words here.\n");
    cx.simulate_keystrokes("alt-shift-t escape");
    assert_eq!(focus(cx), None);
}

// ---- The Toolbars settings page ----

fn open_settings<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
) -> (Entity<SettingsView>, &'a mut VisualTestContext) {
    cx.update(|cx| text_input::bind_keys(&RuleSet::defaults(), cx));
    let root = root.to_path_buf();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        SettingsView::with_rules(root.clone(), &RuleSet::defaults(), window, cx)
    });
    cx.update(|window, cx| {
        window.focus(&view.focus_handle(cx));
        view.update(cx, |view, cx| view.show_section(TOOLBARS_SECTION, cx));
    });
    cx.run_until_parked();
    (view, cx)
}

fn toolbars_file(root: &Path) -> String {
    std::fs::read_to_string(root.join(CONFIG_DIR).join("toolbars.toml")).unwrap_or_default()
}

fn focus_row(view: &Entity<SettingsView>, cx: &mut VisualTestContext, row: ControlRow) {
    view.update_in(cx, |view, window, cx| {
        let index = view.rows().iter().position(|known| *known == row).unwrap();
        view.focus_control(index, window, cx);
    });
}

fn status_items(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        let status = view.toolbars().get("status").unwrap();
        status.items.iter().map(ToString::to_string).collect()
    })
}

#[gpui::test]
fn the_page_adds_moves_and_removes_items_from_the_keyboard(cx: &mut TestAppContext) {
    let kept = "# My bars, kept by hand.\n[timing]\nhover-delay = \"200ms\"   # a little slower\n";
    let dir = vault(Some(kept));
    let (view, cx) = open_settings(cx, dir.path());
    view.update_in(cx, |view, window, cx| {
        view.start_adding_to("status", window, cx)
    });
    assert!(view.read_with(cx, |view, _| view.menu_open()));
    cx.simulate_input("export html");
    let options = view.read_with(cx, |view, _| view.menu_options());
    assert_eq!(options.first().map(String::as_str), Some("export.html"));
    cx.simulate_keystrokes("enter");
    let text = toolbars_file(dir.path());
    assert!(text.starts_with(kept), "{text}");
    assert!(text.contains("\"export.html\""), "{text}");
    let items = status_items(&view, cx);
    assert_eq!(items.last().map(String::as_str), Some("export.html"));
    let last = items.len() - 1;
    focus_row(
        &view,
        cx,
        ControlRow::ToolbarItem {
            toolbar: "status".into(),
            index: last,
        },
    );
    cx.simulate_keystrokes("alt-up");
    assert_eq!(status_items(&view, cx)[last - 1], "export.html");
    cx.simulate_keystrokes("delete");
    assert!(!status_items(&view, cx).contains(&"export.html".to_owned()));
    assert!(
        !toolbars_file(dir.path()).contains("items"),
        "the built-in items are back"
    );
}

#[gpui::test]
fn the_page_turns_bars_off_adds_one_and_resets(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx) = open_settings(cx, dir.path());
    focus_row(&view, cx, ControlRow::ToolbarHeader("selection".into()));
    cx.simulate_keystrokes("space");
    assert!(toolbars_file(dir.path()).contains("enabled = false"));
    let rows = view.read_with(cx, |view, _| view.rows());
    assert!(
        !rows.contains(&ControlRow::ToolbarAdd("selection".into())),
        "an off bar folds away"
    );
    let style = ControlRow::ToolbarField {
        toolbar: "status".into(),
        field: ToolbarField::Style,
    };
    focus_row(&view, cx, style);
    cx.simulate_keystrokes("right");
    assert!(toolbars_file(dir.path()).contains("style = \"icons-and-labels\""));
    focus_row(&view, cx, ControlRow::NewToolbar);
    cx.simulate_keystrokes("enter");
    let added = view.read_with(cx, |view, _| view.toolbars().get("toolbar").cloned());
    assert_eq!(
        added.unwrap().place,
        gasp_config::toolbars::Place::EditorTop
    );
    let place = ControlRow::ToolbarField {
        toolbar: "toolbar".into(),
        field: ToolbarField::Place,
    };
    focus_row(&view, cx, place);
    cx.simulate_keystrokes("enter");
    let places = view.read_with(cx, |view, _| view.menu_options());
    assert!(places.contains(&"window-left".to_owned()));
    cx.simulate_keystrokes("escape");
    focus_row(&view, cx, ControlRow::ResetToolbars);
    cx.simulate_keystrokes("enter");
    assert!(
        toolbars_file(dir.path()).contains("[toolbar.toolbar]"),
        "the first press asks"
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(toolbars_file(dir.path()).trim(), "");
    assert_eq!(
        view.read_with(cx, |view, _| view.toolbars().clone()),
        Toolbars::defaults()
    );
}
