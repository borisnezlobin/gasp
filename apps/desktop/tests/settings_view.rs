//! Drives the settings screen through GPUI's test platform on a temporary
//! vault: keyboard navigation, toggles, choices, numbers, text, search,
//! resets and what ends up in `.editor/settings.toml`.

use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use editor_config::RuleSet;
use editor_desktop::settings_view::{
    ControlRow, SectionRef, SettingsEvent, SettingsFocus, SettingsView,
};
use gpui::{DismissEvent, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};
use serde_json::Value;
use tempfile::TempDir;

const USER_FILE: &str = "\
# Tuned for the travel laptop.
[files]
attachments-folder = \"./assets\"   # matches the website

[sidebar.files]
mode = \"push\"
";

fn vault(settings: Option<&str>) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    if let Some(text) = settings {
        fs::create_dir_all(dir.path().join(".editor")).unwrap();
        fs::write(settings_file(dir.path()), text).unwrap();
    }
    dir
}

fn settings_file(root: &Path) -> std::path::PathBuf {
    root.join(".editor/settings.toml")
}

fn read_settings(root: &Path) -> String {
    fs::read_to_string(settings_file(root)).unwrap_or_default()
}

struct Recorded {
    changed: Vec<String>,
    dismissed: usize,
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
) -> (
    Entity<SettingsView>,
    &'a mut VisualTestContext,
    Rc<RefCell<Recorded>>,
) {
    let root = root.to_path_buf();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        SettingsView::with_rules(root.clone(), &RuleSet::defaults(), window, cx)
    });
    let recorded = Rc::new(RefCell::new(Recorded {
        changed: Vec::new(),
        dismissed: 0,
    }));
    let (changed, dismissed) = (recorded.clone(), recorded.clone());
    cx.update(|window, cx| {
        window.focus(&view.focus_handle(cx));
        cx.subscribe(&view, move |_, event: &SettingsEvent, _| {
            let SettingsEvent::Changed(key) = event;
            changed.borrow_mut().changed.push(key.clone());
        })
        .detach();
        cx.subscribe(&view, move |_, _: &DismissEvent, _| {
            dismissed.borrow_mut().dismissed += 1;
        })
        .detach();
    });
    cx.run_until_parked();
    (view, cx, recorded)
}

/// Selects the section titled `title` from the keyboard and moves into
/// its first control.
fn go_to_section(view: &Entity<SettingsView>, title: &str, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| view.focus_sections(window, cx));
    let index = view.read_with(cx, |view, _| {
        view.visible_sections()
            .into_iter()
            .position(|section| view.section_title(section) == title)
            .unwrap_or_else(|| panic!("no section {title}"))
    });
    let current = view.read_with(cx, |view, _| {
        view.visible_sections()
            .iter()
            .position(|s| Some(*s) == view.current_section())
            .unwrap()
    });
    let key = if index >= current { "down" } else { "up" };
    for _ in 0..index.abs_diff(current) {
        cx.simulate_keystrokes(key);
    }
    cx.simulate_keystrokes("right");
}

/// Moves down to the control for `key` in the current section.
fn go_to_control(view: &Entity<SettingsView>, key: &str, cx: &mut VisualTestContext) {
    let index = view.read_with(cx, |view, _| {
        view.rows()
            .iter()
            .position(|row| row.item().is_some_and(|item| item.key == key))
            .unwrap_or_else(|| panic!("no control for {key}"))
    });
    for _ in 0..index {
        cx.simulate_keystrokes("down");
    }
    assert_eq!(
        view.read_with(cx, |view, _| view.focus_state()),
        SettingsFocus::Control(index)
    );
}

fn value(view: &Entity<SettingsView>, key: &str, cx: &mut VisualTestContext) -> Value {
    view.read_with(cx, |view, _| view.value(key)).unwrap()
}

#[gpui::test]
fn space_toggles_a_setting_and_writes_only_changes(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Editor", cx);
    go_to_control(&view, "editor.show-inline-title", cx);
    cx.simulate_keystrokes("space");
    assert_eq!(read_settings(root), "[editor]\nshow-inline-title = false\n");
    assert_eq!(recorded.borrow().changed, ["editor.show-inline-title"]);
    // Back to the default removes the key.
    cx.simulate_keystrokes("enter");
    assert_eq!(read_settings(root), "");
    assert_eq!(
        value(&view, "editor.show-inline-title", cx),
        Value::Bool(true)
    );
    cx.simulate_keystrokes("left");
    assert_eq!(
        value(&view, "editor.show-inline-title", cx),
        Value::Bool(false)
    );
}

#[gpui::test]
fn arrows_change_choices_and_delete_resets(cx: &mut TestAppContext) {
    let dir = vault(Some(USER_FILE));
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Files", cx);
    go_to_control(&view, "files.trash", cx);
    cx.simulate_keystrokes("right");
    assert_eq!(value(&view, "files.trash", cx), Value::from("vault"));
    cx.simulate_keystrokes("right right");
    assert_eq!(value(&view, "files.trash", cx), Value::from("delete"));
    let text = read_settings(root);
    assert!(
        text.starts_with("# Tuned for the travel laptop.\n[files]\n"),
        "{text}"
    );
    assert!(text.contains("attachments-folder = \"./assets\"   # matches the website"));
    assert!(text.contains("trash = \"delete\""));
    assert!(text.contains("[sidebar.files]\nmode = \"push\""));
    cx.simulate_keystrokes("delete");
    assert_eq!(read_settings(root), USER_FILE);
    assert_eq!(value(&view, "files.trash", cx), Value::from("system"));
}

#[gpui::test]
fn clicking_a_segment_or_toggle_changes_it(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Sidebar", cx);
    let always = cx
        .debug_bounds("choice-sidebar.files.reveal-always")
        .expect("segment drawn");
    cx.simulate_click(always.center(), Modifiers::default());
    assert_eq!(
        value(&view, "sidebar.files.reveal", cx),
        Value::from("always")
    );
    assert!(read_settings(root).contains("reveal = \"always\""));
    go_to_section(&view, "Files", cx);
    let toggle = cx
        .debug_bounds("toggle-files.update-links-on-rename")
        .expect("toggle drawn");
    cx.simulate_click(toggle.center(), Modifiers::default());
    assert_eq!(
        value(&view, "files.update-links-on-rename", cx),
        Value::Bool(false)
    );
}

#[gpui::test]
fn numbers_step_and_take_typed_digits(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Appearance", cx);
    go_to_control(&view, "appearance.base-font-size", cx);
    cx.simulate_keystrokes("right right");
    assert_eq!(read_settings(root), "[appearance]\nbase-font-size = 14\n");
    cx.simulate_input("20");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        value(&view, "appearance.base-font-size", cx),
        Value::from(20)
    );
    cx.simulate_keystrokes("backspace");
    assert_eq!(read_settings(root), "");
}

#[gpui::test]
fn numbers_never_go_below_zero(cx: &mut TestAppContext) {
    let dir = vault(Some("[prose.sentence-length]\nshort-below = 0\n"));
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Prose", cx);
    go_to_control(&view, "prose.sentence-length.short-below", cx);
    cx.simulate_keystrokes("left");
    assert_eq!(
        value(&view, "prose.sentence-length.short-below", cx),
        Value::from(0)
    );
    assert!(view.read_with(cx, |view, _| view.last_error().is_none()));
}

#[gpui::test]
fn text_settings_save_on_enter(cx: &mut TestAppContext) {
    let dir = vault(Some(USER_FILE));
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Files", cx);
    go_to_control(&view, "files.attachments-folder", cx);
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("./media");
    cx.simulate_keystrokes("enter");
    assert!(
        read_settings(root).contains("attachments-folder = \"./media\"   # matches the website")
    );
    assert_eq!(recorded.borrow().changed, ["files.attachments-folder"]);
    // Typing the default back removes the key.
    cx.simulate_keystrokes("enter secondary-a");
    cx.simulate_input("./images");
    cx.simulate_keystrokes("enter");
    assert!(!read_settings(root).contains("attachments-folder"));
}

#[gpui::test]
fn escape_in_a_text_field_reverts_it(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Files", cx);
    go_to_control(&view, "files.attachments-folder", cx);
    cx.simulate_input("xyz");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        value(&view, "files.attachments-folder", cx),
        Value::from("./images")
    );
    assert!(recorded.borrow().changed.is_empty());
    assert_eq!(recorded.borrow().dismissed, 0);
    // A second Escape, now outside the field, closes the screen.
    cx.simulate_keystrokes("escape");
    assert_eq!(recorded.borrow().dismissed, 1);
}

#[gpui::test]
fn search_filters_sections_and_rows(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, recorded) = open(cx, dir.path());
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    cx.simulate_input("trash");
    let (sections, rows) = view.read_with(cx, |view, _| (view.visible_sections(), view.rows()));
    // The files section, and the shortcut for "Move note to trash".
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[1], SectionRef::Shortcuts);
    let keys: Vec<String> = rows
        .iter()
        .filter_map(|row| row.item().map(|i| i.key.clone()))
        .collect();
    assert_eq!(keys, ["files.trash"]);
    cx.simulate_keystrokes("down right");
    assert_eq!(value(&view, "files.trash", cx), Value::from("vault"));
    // Escape clears the search, and a second one closes the screen.
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    cx.simulate_keystrokes("escape");
    assert_eq!(view.read_with(cx, |view, _| view.query().to_string()), "");
    assert_eq!(recorded.borrow().dismissed, 0);
    cx.simulate_keystrokes("escape");
    assert_eq!(recorded.borrow().dismissed, 1);
}

#[gpui::test]
fn search_finds_shortcuts_by_their_keys(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    cx.simulate_input("command palette");
    let (sections, rows) = view.read_with(cx, |view, _| (view.visible_sections(), view.rows()));
    assert_eq!(sections, [SectionRef::Shortcuts]);
    assert!(
        rows.iter()
            .any(|row| matches!(row, ControlRow::Shortcut(s) if s.id == "palette.open"))
    );
}

#[gpui::test]
fn typing_outside_a_field_starts_a_search(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    cx.simulate_input("font");
    assert_eq!(
        view.read_with(cx, |view, _| view.query().to_string()),
        "font"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.focus_state()),
        SettingsFocus::Search
    );
}

#[gpui::test]
fn tab_walks_search_sections_and_controls(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    let focus = |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.focus_state());
    assert_eq!(focus(cx), SettingsFocus::Sections);
    cx.simulate_keystrokes("tab");
    assert_eq!(focus(cx), SettingsFocus::Control(0));
    cx.simulate_keystrokes("shift-tab shift-tab");
    assert_eq!(focus(cx), SettingsFocus::Search);
    cx.simulate_keystrokes("tab");
    assert_eq!(focus(cx), SettingsFocus::Sections);
    cx.simulate_keystrokes("up");
    assert_eq!(focus(cx), SettingsFocus::Search);
}

#[gpui::test]
fn every_section_is_listed_with_shortcuts_last(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    let titles = view.read_with(cx, |view, _| {
        view.visible_sections()
            .into_iter()
            .map(|section| view.section_title(section))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        titles,
        [
            "Appearance",
            "Editor",
            "Files",
            "Markdown",
            "Prose",
            "Sidebar",
            "Keyboard shortcuts"
        ]
    );
    go_to_section(&view, "Keyboard shortcuts", cx);
    let rows = view.read_with(cx, |view, _| view.rows());
    let settings = rows.iter().find_map(|row| match row {
        ControlRow::Shortcut(shortcut) if shortcut.id == "settings.open" => Some(shortcut.clone()),
        _ => None,
    });
    assert_eq!(settings.unwrap().keys.len(), 2);
}

#[gpui::test]
fn map_entries_can_be_added_and_removed(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Markdown", cx);
    go_to_control(&view, "markdown.symbols.overrides", cx);
    cx.simulate_input("not a syntax");
    cx.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |view, _| view.last_error().is_some()));
    assert_eq!(read_settings(root), "");
    cx.simulate_keystrokes("enter secondary-a");
    cx.simulate_input("link url");
    cx.simulate_keystrokes("enter");
    let key = "markdown.symbols.overrides.link-url";
    assert_eq!(value(&view, key, cx), Value::from("always-shown"));
    assert_eq!(
        recorded.borrow().changed.last().unwrap(),
        "markdown.symbols.overrides"
    );
    cx.simulate_keystrokes("down right");
    assert_eq!(value(&view, key, cx), Value::from("around-cursor"));
    cx.simulate_keystrokes("delete");
    assert!(!read_settings(root).contains("link-url"));
}

#[gpui::test]
fn reload_picks_up_outside_edits(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    fs::create_dir_all(root.join(".editor")).unwrap();
    fs::write(settings_file(root), "[files]\ntrash = \"delete\"\n").unwrap();
    view.update(cx, |view, cx| view.reload(cx));
    assert_eq!(value(&view, "files.trash", cx), Value::from("delete"));
}
