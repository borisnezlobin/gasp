//! Drives the settings screen through GPUI's test platform on a temporary
//! vault: layout, keyboard navigation, toggles, dropdowns, numbers, text,
//! search, resets, theme fonts and colours, and keyboard shortcuts, and
//! what ends up in `.editor/settings.toml`, `theme.toml` and `rules.toml`.

use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use editor_config::{KeyChord, Platform, RuleSet};
use editor_desktop::settings_view::{
    ControlRow, Page, SettingsEvent, SettingsFocus, SettingsRequest, SettingsView, modal_size,
};
use editor_desktop::text_input;
use editor_desktop::theme::SettingsTheme;
use gpui::{
    Bounds, DismissEvent, Entity, Focusable, Modifiers, Pixels, TestAppContext, VisualTestContext,
    px, size,
};
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
        write_config(dir.path(), "settings.toml", text);
    }
    dir
}

fn write_config(root: &Path, file: &str, text: &str) {
    fs::create_dir_all(root.join(".editor")).unwrap();
    fs::write(root.join(".editor").join(file), text).unwrap();
}

fn read_config(root: &Path, file: &str) -> String {
    fs::read_to_string(root.join(".editor").join(file)).unwrap_or_default()
}

fn read_settings(root: &Path) -> String {
    read_config(root, "settings.toml")
}

#[derive(Default)]
struct Recorded {
    changed: Vec<String>,
    requests: Vec<SettingsRequest>,
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
    cx.update(|cx| text_input::bind_keys(&RuleSet::defaults(), cx));
    let root = root.to_path_buf();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        SettingsView::with_rules(root.clone(), &RuleSet::defaults(), window, cx)
    });
    let recorded = Rc::new(RefCell::new(Recorded::default()));
    let (changed, requests, dismissed) = (recorded.clone(), recorded.clone(), recorded.clone());
    cx.update(|window, cx| {
        window.focus(&view.focus_handle(cx));
        cx.subscribe(&view, move |_, event: &SettingsEvent, _| {
            let SettingsEvent::Changed(key) = event;
            changed.borrow_mut().changed.push(key.clone());
        })
        .detach();
        cx.subscribe(&view, move |_, request: &SettingsRequest, _| {
            requests.borrow_mut().requests.push(request.clone());
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

fn titles(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        view.visible_sections()
            .into_iter()
            .map(|page| view.section_title(page))
            .collect()
    })
}

/// Selects the section titled `title` from the keyboard and moves into
/// its first control.
fn go_to_section(view: &Entity<SettingsView>, title: &str, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| view.focus_sections(window, cx));
    let index = titles(view, cx)
        .iter()
        .position(|known| known == title)
        .unwrap_or_else(|| panic!("no section {title}"));
    let current = view.read_with(cx, |view, _| {
        view.visible_sections()
            .iter()
            .position(|page| Some(*page) == view.current_section())
            .unwrap()
    });
    let key = if index >= current { "down" } else { "up" };
    for _ in 0..index.abs_diff(current) {
        cx.simulate_keystrokes(key);
    }
    cx.simulate_keystrokes("right");
}

/// Moves down to the first row that `wanted` picks on the current page.
fn go_to_row(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    wanted: impl Fn(&ControlRow) -> bool,
) -> usize {
    let (index, first) = view.read_with(cx, |view, _| {
        let rows = view.rows();
        let index = rows.iter().position(&wanted).expect("row on this page");
        let first = rows.iter().position(ControlRow::is_focusable).unwrap();
        let skipped = rows[first..index]
            .iter()
            .filter(|row| !row.is_focusable())
            .count();
        (index, index - first - skipped)
    });
    for _ in 0..first {
        cx.simulate_keystrokes("down");
    }
    assert_eq!(
        view.read_with(cx, |view, _| view.focus_state()),
        SettingsFocus::Control(index)
    );
    index
}

fn go_to_control(view: &Entity<SettingsView>, key: &str, cx: &mut VisualTestContext) {
    go_to_row(view, cx, |row| {
        row.item().is_some_and(|item| item.key == key)
    });
}

fn value(view: &Entity<SettingsView>, key: &str, cx: &mut VisualTestContext) -> Value {
    view.read_with(cx, |view, _| view.value(key)).unwrap()
}

fn token(view: &Entity<SettingsView>, name: &str, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.token(name)).unwrap()
}

fn bounds(cx: &mut VisualTestContext, selector: String) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    cx.run_until_parked();
    let found = bounds(cx, selector.to_string()).unwrap_or_else(|| panic!("{selector} drawn"));
    cx.simulate_click(found.center(), Modifiers::default());
}

// ---- Layout ----

/// Every row's text column and control column sit side by side without
/// touching, and both stay inside the screen.
fn assert_rows_do_not_overlap(view: &Entity<SettingsView>, cx: &mut VisualTestContext) {
    let pages = view.read_with(cx, |view, _| view.visible_sections());
    let screen = modal_size(
        cx.update(|window, _| window.viewport_size()),
        &SettingsTheme::default(),
    );
    for (page_index, page) in pages.iter().enumerate() {
        view.update(cx, |view, cx| view.show_section(page_id(*page), cx));
        cx.run_until_parked();
        let rows = view.read_with(cx, |view, _| view.rows());
        for (index, row) in rows.iter().enumerate() {
            view.update(cx, |view, cx| view.reveal_row(index, cx));
            cx.run_until_parked();
            let name = format!("{}-{index}", page_id(*page));
            let text = bounds(cx, format!("settings-text-{name}"))
                .unwrap_or_else(|| panic!("{page:?} row {index} has no text"));
            assert!(
                text.size.width > px(0.),
                "{page:?} row {index} text has no room"
            );
            assert!(
                text.right() <= screen.width,
                "{page:?} row {index} text overflows"
            );
            let Some(control) = bounds(cx, format!("settings-control-{name}")) else {
                // The version, and the repository of a vault that isn't a
                // git clone, have nothing to change.
                assert!(
                    matches!(row, ControlRow::Version | ControlRow::SyncRemote),
                    "{page:?} row {index} has no control"
                );
                continue;
            };
            assert!(
                !text.intersects(&control),
                "{page:?} (page {page_index}) row {index}: text {text:?} overlaps control {control:?}"
            );
            assert!(
                text.right() <= control.left(),
                "{page:?} row {index}: text runs into control"
            );
            assert!(
                control.right() <= screen.width,
                "{page:?} row {index} control overflows {control:?} {screen:?} {row:?}"
            );
        }
    }
}

fn page_id(page: Page) -> &'static str {
    match page {
        Page::General => "general",
        Page::Sync => "sync",
        Page::Appearance => "appearance",
        Page::Sidebar => "sidebar",
        Page::Shortcuts => "keyboard-shortcuts",
        Page::Editor => "editor",
        Page::Files => "files",
        Page::DailyNotes => "daily-notes",
        Page::Prose => "prose",
        Page::Snippets => "snippets",
    }
}

#[gpui::test]
fn rows_never_overlap_their_controls(cx: &mut TestAppContext) {
    let dir = vault(Some(
        "[markdown.symbols.overrides]\nlink-url = \"always-hidden\"\n",
    ));
    let (view, cx, _) = open(cx, dir.path());
    assert_rows_do_not_overlap(&view, cx);
    // A narrow window squeezes the text column, which wraps instead.
    cx.simulate_resize(size(px(760.), px(560.)));
    cx.run_until_parked();
    assert_rows_do_not_overlap(&view, cx);
}

#[gpui::test]
fn the_modal_is_a_share_of_the_window_up_to_a_maximum(cx: &mut TestAppContext) {
    let style = SettingsTheme::default();
    let small = modal_size(size(px(1000.), px(700.)), &style);
    assert_eq!(small, size(px(800.), px(560.)));
    let huge = modal_size(size(px(4000.), px(3000.)), &style);
    assert_eq!(huge, size(style.modal_max_width, style.modal_max_height));
    let dir = vault(None);
    let (_, cx, recorded) = open(cx, dir.path());
    click(cx, "settings-close");
    assert_eq!(recorded.borrow().dismissed, 1);
}

// ---- Sections and search ----

#[gpui::test]
fn every_section_is_listed_in_order(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    assert_eq!(
        titles(&view, cx),
        [
            "General",
            "Sync",
            "Appearance",
            "Sidebar",
            "Keyboard shortcuts",
            "Editor",
            "Files and links",
            "Daily notes and templates",
            "Prose",
            "Snippets and replacements"
        ]
    );
    go_to_section(&view, "Keyboard shortcuts", cx);
    let layout = view.read_with(cx, |view, _| view.layout());
    let settings = layout.rows.iter().find_map(|row| match row {
        ControlRow::Shortcut(shortcut) if shortcut.id == "settings.open" => Some(shortcut.clone()),
        _ => None,
    });
    assert_eq!(settings.unwrap().keys.len(), 2);
    // Shortcuts are grouped on one card per category.
    let first = layout.cards.first().unwrap();
    assert_eq!(first.title.as_deref(), Some("Formatting"));
}

#[gpui::test]
fn general_shows_the_vault_and_opens_another(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, recorded) = open(cx, dir.path());
    go_to_section(&view, "General", cx);
    let rows = view.read_with(cx, |view, _| view.rows());
    assert_eq!(rows, [ControlRow::Vault, ControlRow::Version]);
    assert!(
        ControlRow::Version
            .title()
            .contains(env!("CARGO_PKG_VERSION"))
    );
    // Version has nothing to press, so Down stays on the vault row.
    cx.simulate_keystrokes("down");
    assert_eq!(
        view.read_with(cx, |view, _| view.focus_state()),
        SettingsFocus::Control(0)
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(
        recorded.borrow().requests,
        [SettingsRequest::RunCommand("vault.open".into())]
    );
    assert_eq!(recorded.borrow().dismissed, 1);
}

#[gpui::test]
fn search_filters_sections_and_rows(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, recorded) = open(cx, dir.path());
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    cx.simulate_input("trash");
    let (sections, rows) = view.read_with(cx, |view, _| (view.visible_sections(), view.rows()));
    // "Move note to trash" in shortcuts, and the files page.
    assert_eq!(sections, [Page::Shortcuts, Page::Files]);
    go_to_section(&view, "Files and links", cx);
    let rows_now = view.read_with(cx, |view, _| view.rows());
    let keys: Vec<String> = rows_now
        .iter()
        .filter_map(|row| row.item().map(|i| i.key.clone()))
        .collect();
    assert!(!rows.is_empty());
    assert_eq!(keys, ["files.trash"]);
    cx.simulate_keystrokes("right");
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
fn search_finds_theme_rows_and_shortcuts(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    cx.simulate_input("interface font");
    let rows = view.read_with(cx, |view, _| view.rows());
    assert!(rows.contains(&ControlRow::Font(
        editor_desktop::settings_view::FontSlot::Interface
    )));
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("command palette");
    let (sections, rows) = view.read_with(cx, |view, _| (view.visible_sections(), view.rows()));
    assert_eq!(sections, [Page::Shortcuts]);
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
    // General has one control: the vault button. Version is skipped.
    cx.simulate_keystrokes("tab");
    assert_eq!(focus(cx), SettingsFocus::Control(0));
    cx.simulate_keystrokes("tab");
    assert_eq!(focus(cx), SettingsFocus::Search);
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(focus(cx), SettingsFocus::Control(0));
    cx.simulate_keystrokes("up");
    assert_eq!(focus(cx), SettingsFocus::Sections);
    cx.simulate_keystrokes("up");
    assert_eq!(focus(cx), SettingsFocus::Search);
}

// ---- Settings ----

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
fn pasted_quotes_wait_on_smart_quotes(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Editor", cx);
    go_to_control(&view, "editor.smart-quotes", cx);
    cx.simulate_keystrokes("space");
    assert_eq!(read_settings(root), "[editor]\nsmart-quotes = false\n");
    cx.simulate_keystrokes("down space");
    assert_eq!(
        read_settings(root),
        "[editor]\nsmart-quotes = false\n",
        "the paste switch does nothing while smart quotes are off"
    );
    cx.simulate_keystrokes("up space down space");
    assert_eq!(
        read_settings(root),
        "[editor]\ncurl-pasted-quotes = false\n"
    );
}

#[gpui::test]
fn arrows_change_choices_and_delete_resets(cx: &mut TestAppContext) {
    let dir = vault(Some(USER_FILE));
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Files and links", cx);
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
    cx.simulate_keystrokes("delete");
    assert_eq!(read_settings(root), USER_FILE);
    assert_eq!(value(&view, "files.trash", cx), Value::from("system"));
}

#[gpui::test]
fn enter_opens_a_dropdown_that_arrows_and_enter_pick_from(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Sidebar", cx);
    go_to_control(&view, "sidebar.files.reveal", cx);
    cx.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |view, _| view.menu_open()));
    // The menu opens on the current choice, "hover", the last option.
    cx.simulate_keystrokes("up up enter");
    assert!(!view.read_with(cx, |view, _| view.menu_open()));
    assert_eq!(
        value(&view, "sidebar.files.reveal", cx),
        Value::from("always")
    );
    assert_eq!(recorded.borrow().changed, ["sidebar.files.reveal"]);
    // Escape closes an open menu without closing the screen.
    cx.simulate_keystrokes("space escape");
    assert!(!view.read_with(cx, |view, _| view.menu_open()));
    assert_eq!(recorded.borrow().dismissed, 0);
}

#[gpui::test]
fn clicking_a_dropdown_option_or_toggle_changes_it(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Sidebar", cx);
    click(cx, "dropdown-sidebar.files.reveal");
    click(cx, "menu-option-always");
    assert_eq!(
        value(&view, "sidebar.files.reveal", cx),
        Value::from("always")
    );
    assert!(read_settings(root).contains("reveal = \"always\""));
    go_to_section(&view, "Files and links", cx);
    click(cx, "toggle-files.update-links-on-rename");
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
    click(cx, "increase-appearance.base-font-size");
    assert_eq!(
        value(&view, "appearance.base-font-size", cx),
        Value::from(13)
    );
}

#[gpui::test]
fn the_font_size_has_a_floor(cx: &mut TestAppContext) {
    let dir = vault(Some("[appearance]\nbase-font-size = 6\n"));
    let (view, cx, _) = open(cx, dir.path());
    go_to_section(&view, "Appearance", cx);
    go_to_control(&view, "appearance.base-font-size", cx);
    cx.simulate_keystrokes("left");
    assert_eq!(
        value(&view, "appearance.base-font-size", cx),
        Value::from(6)
    );
    cx.simulate_input("2");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        value(&view, "appearance.base-font-size", cx),
        Value::from(6)
    );
    assert!(view.read_with(cx, |view, _| view.last_error().is_none()));
}

#[gpui::test]
fn text_settings_save_on_enter(cx: &mut TestAppContext) {
    let dir = vault(Some(USER_FILE));
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Files and links", cx);
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
    let (view, cx, recorded) = open(cx, dir.path());
    go_to_section(&view, "Files and links", cx);
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
fn map_entries_can_be_added_and_removed(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Editor", cx);
    let add = go_to_row(&view, cx, |row| matches!(row, ControlRow::MapAdd(_)));
    // The names come from a menu, so nobody has to know them.
    cx.simulate_keystrokes("enter");
    let options = view.read_with(cx, |view, _| view.menu_options());
    assert_eq!(options.len(), 18);
    let link_url = options.iter().position(|o| o == "link-url").unwrap();
    for _ in 0..link_url {
        cx.simulate_keystrokes("down");
    }
    cx.simulate_keystrokes("enter");
    assert!(!view.read_with(cx, |view, _| view.menu_open()));
    assert_eq!(
        view.read_with(cx, |view, _| view.focus_state()),
        SettingsFocus::Control(add)
    );
    let key = "markdown.symbols.overrides.link-url";
    assert_eq!(value(&view, key, cx), Value::from("always-shown"));
    assert_eq!(
        recorded.borrow().changed.last().unwrap(),
        "markdown.symbols.overrides"
    );
    let title = view.read_with(cx, |view, _| {
        let rows = view.rows();
        let entry = rows
            .iter()
            .position(|row| matches!(row, ControlRow::MapEntry { .. }))
            .unwrap();
        rows[entry].title()
    });
    assert_eq!(title, "Link addresses");
    // A name that's been added isn't offered again.
    cx.simulate_keystrokes("enter");
    let options = view.read_with(cx, |view, _| view.menu_options());
    assert!(options.len() == 17 && !options.contains(&"link-url".to_string()));
    cx.simulate_keystrokes("escape down right");
    assert_eq!(value(&view, key, cx), Value::from("around-cursor"));
    cx.simulate_keystrokes("delete");
    assert!(!read_settings(root).contains("link-url"));
}

#[gpui::test]
fn line_height_and_width_step_and_write_the_theme(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Appearance", cx);
    go_to_row(
        &view,
        cx,
        |row| matches!(row, ControlRow::Setting(item) if item.key == "theme.font.line-height.body"),
    );
    cx.simulate_keystrokes("right right");
    assert!(
        read_config(root, "theme.toml").contains("body = 1.7"),
        "{}",
        read_config(root, "theme.toml")
    );
    assert_eq!(
        recorded.borrow().changed.last().unwrap(),
        "theme.font.line-height.body"
    );
    cx.simulate_keystrokes("down right");
    assert!(read_config(root, "theme.toml").contains("editor-max-width = 740"));
    // Delete puts the built-in value back.
    cx.simulate_keystrokes("up delete");
    assert!(!read_config(root, "theme.toml").contains("body ="));
    assert_eq!(
        value(&view, "theme.font.line-height.body", cx),
        Value::from(1.6)
    );
}

#[gpui::test]
fn reload_picks_up_outside_edits(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    write_config(root, "settings.toml", "[files]\ntrash = \"delete\"\n");
    write_config(root, "theme.toml", "[font]\ncode = \"Iosevka\"\n");
    view.update(cx, |view, cx| view.reload(cx));
    assert_eq!(value(&view, "files.trash", cx), Value::from("delete"));
    assert_eq!(token(&view, "font.code", cx), "Iosevka");
}

// ---- Appearance: theme tokens ----

const FONTS: [&str; 5] = [
    "DejaVu Sans",
    "Noto Serif",
    "Liberation Serif",
    "Liberation Mono",
    "Georgia",
];

fn with_fonts(view: &Entity<SettingsView>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.set_font_names(FONTS.map(String::from).to_vec(), cx)
    });
}

#[gpui::test]
fn picking_a_font_writes_the_theme_and_reports_it(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    write_config(
        root,
        "theme.toml",
        "# Mine.\n[color]\nbackground = \"#fdf6e3\"\n",
    );
    let (view, cx, recorded) = open(cx, root);
    with_fonts(&view, cx);
    go_to_section(&view, "Appearance", cx);
    go_to_row(&view, cx, |row| {
        *row == ControlRow::Font(editor_desktop::settings_view::FontSlot::Text)
    });
    cx.simulate_keystrokes("enter");
    // The filter has focus: typing narrows the list, Enter picks.
    cx.simulate_input("serif");
    let shown = view.read_with(cx, |view, _| view.menu_options());
    assert_eq!(shown, ["Liberation Serif", "Noto Serif"]);
    cx.simulate_keystrokes("down enter");
    assert_eq!(token(&view, "font.text", cx), "Noto Serif");
    let text = read_config(root, "theme.toml");
    assert!(
        text.starts_with("# Mine.\n[color]\nbackground = \"#fdf6e3\"\n"),
        "{text}"
    );
    assert!(text.contains("[font]\ntext = \"Noto Serif\"\n"), "{text}");
    assert_eq!(recorded.borrow().changed, ["theme.font.text"]);
    // The built-in font, picked again, leaves the file.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("charter");
    cx.simulate_keystrokes("enter");
    assert_eq!(token(&view, "font.text", cx), "Charter");
    assert!(!read_config(root, "theme.toml").contains("[font]"));
}

#[gpui::test]
fn the_font_menu_lists_the_current_font_first(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    with_fonts(&view, cx);
    go_to_section(&view, "Appearance", cx);
    click(cx, "dropdown-font.code");
    let shown = view.read_with(cx, |view, _| view.menu_options());
    assert_eq!(shown[0], "Courier New");
    assert_eq!(shown.len(), FONTS.len() + 1);
    assert_eq!(shown[1], "DejaVu Sans");
    click(cx, "menu-option-Liberation Mono");
    assert_eq!(token(&view, "font.code", cx), "Liberation Mono");
    // Clicking outside closes a menu without picking.
    click(cx, "dropdown-font.ui");
    assert!(view.read_with(cx, |view, _| view.menu_open()));
    click(cx, "settings-text-appearance-0");
    assert!(!view.read_with(cx, |view, _| view.menu_open()));
    assert_eq!(token(&view, "font.ui", cx), "Charter");
}

#[gpui::test]
fn the_accent_comes_from_swatches_or_hex(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Appearance", cx);
    go_to_row(&view, cx, |row| *row == ControlRow::Accent);
    cx.simulate_keystrokes("right");
    assert_eq!(token(&view, "color.accent", cx), "#2f5fd0");
    assert_eq!(
        read_config(root, "theme.toml"),
        "[color]\naccent = \"#2f5fd0\"\n"
    );
    assert_eq!(recorded.borrow().changed, ["theme.color.accent"]);
    // The screen's own switches follow the accent.
    let accent = view.read_with(cx, |view, _| view.style().accent);
    assert_eq!(
        accent,
        editor_desktop::theme::parse_color("#2f5fd0").unwrap()
    );
    click(cx, "swatch-#7048c8");
    assert_eq!(token(&view, "color.accent", cx), "#7048c8");
    // Enter moves into the hex field; a bad colour is refused.
    cx.simulate_keystrokes("enter secondary-a");
    cx.simulate_input("#12");
    cx.simulate_keystrokes("enter");
    assert!(view.read_with(cx, |view, _| view.last_error().is_some()));
    assert_eq!(token(&view, "color.accent", cx), "#7048c8");
    cx.simulate_keystrokes("enter secondary-a");
    cx.simulate_input("#123456");
    cx.simulate_keystrokes("enter");
    assert_eq!(token(&view, "color.accent", cx), "#123456");
    // Delete goes back to the built-in accent and empties the file.
    cx.simulate_keystrokes("delete");
    assert_eq!(token(&view, "color.accent", cx), "#000000");
    assert_eq!(read_config(root, "theme.toml"), "");
}

// ---- Keyboard shortcuts ----

fn shortcut(
    view: &Entity<SettingsView>,
    id: &str,
    cx: &mut VisualTestContext,
) -> editor_desktop::settings_view::model::ShortcutRow {
    view.read_with(cx, |view, _| {
        view.rows().into_iter().find_map(|row| match row {
            ControlRow::Shortcut(shortcut) if shortcut.id == id => Some(shortcut),
            _ => None,
        })
    })
    .unwrap_or_else(|| panic!("no shortcut row for {id}"))
}

/// A chord as this platform's shortcut rows write it: `Ctrl+N` or `⌘N`.
fn label(keys: &str) -> String {
    let platform = Platform::current();
    let chord = KeyChord::parse_for(keys, platform).unwrap();
    editor_desktop::picker::shortcut::shortcut_label(chord, platform)
}

#[gpui::test]
fn a_captured_chord_is_added_to_the_rules(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, recorded) = open(cx, root);
    go_to_section(&view, "Keyboard shortcuts", cx);
    go_to_row(
        &view,
        cx,
        |row| matches!(row, ControlRow::Shortcut(s) if s.id == "tab.new"),
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(
        view.read_with(cx, |view, _| view.capturing().map(str::to_string)),
        Some("tab.new".into())
    );
    // A plain letter can't be a shortcut; the row says why and keeps waiting.
    cx.simulate_keystrokes("j");
    assert!(view.read_with(cx, |view, _| view.capturing().is_some()));
    cx.simulate_keystrokes("secondary-alt-j");
    assert!(view.read_with(cx, |view, _| view.capturing().is_none()));
    let rules = read_config(root, "rules.toml");
    assert!(rules.contains("id = \"user.key.tab.new\""), "{rules}");
    assert!(rules.contains("keys = \"Mod+Alt+J\""), "{rules}");
    assert!(rules.contains("do = \"tab.new\""), "{rules}");
    assert_eq!(recorded.borrow().changed, ["rules"]);
    let row = shortcut(&view, "tab.new", cx);
    let added = row
        .keys
        .iter()
        .find(|key| key.label == label("Mod+Alt+J"))
        .unwrap();
    assert_eq!(added.user_rule.as_deref(), Some("user.key.tab.new"));
    assert!(row.conflicts.is_empty());
    // Escape while waiting gives up without writing anything.
    cx.simulate_keystrokes("enter escape");
    assert!(view.read_with(cx, |view, _| view.capturing().is_none()));
    assert_eq!(recorded.borrow().changed, ["rules"]);
    assert_eq!(recorded.borrow().dismissed, 0);
}

#[gpui::test]
fn a_chord_another_command_uses_is_added_with_a_warning(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Keyboard shortcuts", cx);
    let index = go_to_row(
        &view,
        cx,
        |row| matches!(row, ControlRow::Shortcut(s) if s.id == "tab.new"),
    );
    click(cx, "add-key-tab.new");
    cx.simulate_keystrokes("secondary-,");
    let row = shortcut(&view, "tab.new", cx);
    assert_eq!(
        row.conflicts,
        [(label("Mod+,"), "Open settings".to_string())]
    );
    let settings = shortcut(&view, "settings.open", cx);
    assert_eq!(
        settings.conflicts,
        [(label("Mod+,"), "New tab".to_string())]
    );
    assert!(read_config(root, "rules.toml").contains("keys = \"Mod+,\""));
    cx.run_until_parked();
    let text = bounds(cx, format!("settings-text-keyboard-shortcuts-{index}")).unwrap();
    assert!(
        text.size.height > px(30.),
        "the warning shows under the title"
    );
    // The same chord again on the same command isn't added twice: the
    // capture says why and keeps waiting, and the rows don't move.
    let before = bounds(
        cx,
        format!("settings-text-keyboard-shortcuts-{}", index + 1),
    );
    cx.simulate_keystrokes("enter secondary-,");
    let rules = read_config(root, "rules.toml");
    assert_eq!(rules.matches("keys = \"Mod+,\"").count(), 1, "{rules}");
    let rejection = view.read_with(cx, |view, _| view.capture_rejection().map(str::to_string));
    assert_eq!(
        rejection,
        Some(format!("{} already runs this.", label("Mod+,")))
    );
    cx.run_until_parked();
    let after = bounds(
        cx,
        format!("settings-text-keyboard-shortcuts-{}", index + 1),
    );
    assert_eq!(before, after, "the message moved the next row");
}

#[gpui::test]
fn user_shortcuts_are_removed_by_their_cross_or_delete(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    write_config(
        root,
        "rules.toml",
        "# Keep this.\n[[rule]]\nid = \"user.key.note.new\"\non = \"key\"\nkeys = \"Mod+Alt+N\"\ndo = \"note.new\"\n\n\
         [[rule]]\nid = \"user.key.note.new~2\"\non = \"key\"\nkeys = \"F6\"\ndo = \"note.new\"\n",
    );
    let (view, cx, recorded) = open(cx, root);
    view.update(cx, |view, cx| {
        let rules = editor_desktop::settings_view::config_files::load_rules(view.vault_root());
        view.set_rules(&rules, cx)
    });
    go_to_section(&view, "Keyboard shortcuts", cx);
    go_to_row(
        &view,
        cx,
        |row| matches!(row, ControlRow::Shortcut(s) if s.id == "note.new"),
    );
    assert_eq!(shortcut(&view, "note.new", cx).keys.len(), 3);
    click(cx, "remove-key-user.key.note.new");
    let rules = read_config(root, "rules.toml");
    assert!(!rules.contains("Mod+Alt+N"), "{rules}");
    assert!(rules.starts_with("# Keep this.\n"), "{rules}");
    assert_eq!(recorded.borrow().changed, ["rules"]);
    // Delete removes the row's last key, then the built-in one, which
    // the file turns off by its id; once none is left, Delete puts the
    // built-in keys back.
    cx.simulate_keystrokes("delete");
    let row = shortcut(&view, "note.new", cx);
    assert_eq!(row.labels(), [label("Mod+N")]);
    assert!(row.changed_from.is_none());
    cx.simulate_keystrokes("delete");
    let row = shortcut(&view, "note.new", cx);
    assert!(row.labels().is_empty());
    let rules = read_config(root, "rules.toml");
    assert!(
        rules.contains("id = \"key.note.new\"\ndelete = true"),
        "{rules}"
    );
    cx.simulate_keystrokes("delete");
    let row = shortcut(&view, "note.new", cx);
    assert_eq!(row.labels(), [label("Mod+N")]);
    assert!(row.changed_from.is_none());
    assert_eq!(read_config(root, "rules.toml").trim(), "# Keep this.");
    assert_eq!(recorded.borrow().changed.len(), 4);
}

#[gpui::test]
fn a_built_in_key_is_removed_by_its_cross_and_reset_brings_it_back(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    let (view, cx, _) = open(cx, root);
    go_to_section(&view, "Keyboard shortcuts", cx);
    let index = go_to_row(
        &view,
        cx,
        |row| matches!(row, ControlRow::Shortcut(s) if s.id == "format.bold"),
    );
    let text = bounds(cx, format!("settings-text-keyboard-shortcuts-{index}")).unwrap();
    click(cx, "remove-key-key.format.bold");
    let row = shortcut(&view, "format.bold", cx);
    assert!(row.keys.is_empty());
    let defaults: Vec<String> = row
        .changed_from
        .clone()
        .unwrap()
        .iter()
        .map(|shortcut| shortcut.label())
        .collect();
    assert_eq!(defaults, [label("Mod+B")]);
    let rules = read_config(root, "rules.toml");
    assert_eq!(rules, "[[rule]]\nid = \"key.format.bold\"\ndelete = true\n");
    // The row now says what reset brings back, under its title.
    cx.run_until_parked();
    let changed = bounds(cx, format!("settings-text-keyboard-shortcuts-{index}")).unwrap();
    assert!(changed.size.height > text.size.height);
    // Editing is remove and add: a new key goes on beside the removed one.
    click(cx, "add-key-format.bold");
    cx.simulate_keystrokes("secondary-alt-b");
    assert_eq!(
        shortcut(&view, "format.bold", cx).labels(),
        [label("Mod+Alt+B")]
    );
    click(cx, "reset-format.bold");
    let row = shortcut(&view, "format.bold", cx);
    assert_eq!(row.labels(), [label("Mod+B")]);
    assert!(row.changed_from.is_none());
    assert_eq!(read_config(root, "rules.toml").trim(), "");
}

#[gpui::test]
fn typing_keys_in_the_search_finds_the_commands_they_run(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    // "cmd" is the platform's main modifier: Command on a Mac, Ctrl elsewhere.
    cx.simulate_input("cmd p");
    let ids = |view: &Entity<SettingsView>, cx: &mut VisualTestContext| -> Vec<String> {
        view.read_with(cx, |view, _| {
            view.rows()
                .into_iter()
                .filter_map(|row| match row {
                    ControlRow::Shortcut(shortcut) => Some(shortcut.id),
                    _ => None,
                })
                .collect()
        })
    };
    assert_eq!(ids(&view, cx), ["palette.open"]);
    // The label the rows show reads back as the same keys.
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input(&label("Mod+P"));
    assert_eq!(ids(&view, cx), ["palette.open"]);
    // Naming the page shows all of it.
    view.update_in(cx, |view, window, cx| view.focus_search(window, cx));
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("shortcuts");
    let all = view.read_with(cx, |view, _| view.rows().len());
    assert_eq!(all, editor_config::commands::BUILTIN_COMMANDS.len());
}

#[gpui::test]
fn search_by_keys_waits_for_a_chord_and_searches_for_it(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    click(cx, "search-by-keys");
    assert!(view.read_with(cx, |view, _| view.searching_by_keys()));
    cx.simulate_keystrokes("secondary-p");
    assert!(!view.read_with(cx, |view, _| view.searching_by_keys()));
    assert_eq!(
        view.read_with(cx, |view, _| view.query().to_string()),
        label("Mod+P")
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.current_section()),
        Some(Page::Shortcuts)
    );
    let found = shortcut(&view, "palette.open", cx);
    assert!(found.labels().contains(&label("Mod+P")));
    // A chord nothing uses leaves the page empty, with a way back.
    click(cx, "search-by-keys");
    cx.simulate_keystrokes("secondary-alt-shift-f12");
    assert!(view.read_with(cx, |view, _| view.rows().is_empty()));
    click(cx, "clear-search");
    assert_eq!(view.read_with(cx, |view, _| view.query().to_string()), "");
    // Escape stops waiting without searching.
    click(cx, "search-by-keys");
    cx.simulate_keystrokes("escape");
    assert!(!view.read_with(cx, |view, _| view.searching_by_keys()));
    assert_eq!(view.read_with(cx, |view, _| view.query().to_string()), "");
}

// ---- Dropdown menus: placement, long lists, fonts listed late ----

/// A long font list, as a Mac with many fonts has.
fn many_fonts() -> Vec<String> {
    (0..300).map(|n| format!("Family {n:03}")).collect()
}

fn with_many_fonts(view: &Entity<SettingsView>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.set_font_names(many_fonts(), cx));
}

fn resize(view: &Entity<SettingsView>, height: f32, cx: &mut VisualTestContext) {
    cx.simulate_resize(size(px(1920.), px(height)));
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

fn drawn(cx: &mut VisualTestContext, selector: &str) -> Bounds<Pixels> {
    cx.run_until_parked();
    bounds(cx, selector.to_string()).unwrap_or_else(|| panic!("{selector} drawn"))
}

fn assert_near(a: Pixels, b: Pixels, what: &str) {
    assert!((a - b).abs() < px(0.5), "{what}: {a:?} is not {b:?}");
}

#[gpui::test]
fn a_menu_near_the_window_bottom_opens_above_its_button(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    with_many_fonts(&view, cx);
    go_to_section(&view, "Appearance", cx);
    let style = view.read_with(cx, |view, _| view.style().clone());
    // A short window leaves the Interface font's button too little room
    // below for its menu, and more above.
    resize(&view, 600., cx);
    let button = drawn(cx, "dropdown-font.ui");
    let room_below = px(600.) - button.bottom() - style.menu_offset - style.menu_margin;
    let room_above = button.top() - style.menu_offset - style.menu_margin;
    assert!(room_below < style.menu_max_height && room_above > room_below);
    click(cx, "dropdown-font.ui");
    let menu = drawn(cx, "settings-menu");
    // It hangs over the button, right edges aligned, and is cut to the
    // room above rather than slid up the window or past its top.
    assert_near(
        menu.bottom(),
        button.top() - style.menu_offset,
        "menu bottom",
    );
    assert_near(menu.right(), button.right(), "menu right edge");
    assert!(menu.top() >= style.menu_margin, "{menu:?} runs off the top");
    assert_near(menu.size.height, room_above, "menu height");
    // The options scroll inside it: the ones in view are drawn.
    drawn(cx, "menu-option-Family 000");

    // With room below, the same menu hangs under its button.
    cx.simulate_keystrokes("escape");
    resize(&view, 1080., cx);
    let button = drawn(cx, "dropdown-font.ui");
    click(cx, "dropdown-font.ui");
    let menu = drawn(cx, "settings-menu");
    assert_near(menu.top(), button.bottom() + style.menu_offset, "menu top");
    assert_near(menu.right(), button.right(), "menu right edge");
}

#[gpui::test]
fn a_short_menu_opens_below_its_button(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    go_to_section(&view, "Appearance", cx);
    let style = view.read_with(cx, |view, _| view.style().clone());
    resize(&view, 600., cx);
    let button = drawn(cx, "dropdown-appearance.theme");
    click(cx, "dropdown-appearance.theme");
    let menu = drawn(cx, "settings-menu");
    assert_near(menu.top(), button.bottom() + style.menu_offset, "menu top");
    assert_near(menu.right(), button.right(), "menu right edge");
    // Every option shows; nothing scrolls.
    let options = view.read_with(cx, |view, _| view.menu_options());
    for option in options {
        drawn(cx, &format!("menu-option-{option}"));
    }
}

#[gpui::test]
fn the_font_menu_builds_only_the_options_in_view(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    with_many_fonts(&view, cx);
    go_to_section(&view, "Appearance", cx);
    click(cx, "dropdown-font.ui");
    cx.run_until_parked();
    let shown = view.read_with(cx, |view, _| view.menu_options());
    assert_eq!(shown.len(), 301);
    let built = view.read_with(cx, |view, _| view.menu_rows_built());
    // About ten fit; the list builds those and one it measures.
    assert!((1..=14).contains(&built), "built {built} of 301 options");
    drawn(cx, "menu-option-Charter");
    drawn(cx, "menu-option-Family 005");
    assert!(bounds(cx, "menu-option-Family 250".into()).is_none());
    // Filtering builds only the matches in view, too.
    cx.simulate_input("family 2");
    cx.run_until_parked();
    let built = view.read_with(cx, |view, _| view.menu_rows_built());
    assert!((1..=14).contains(&built), "built {built} filtered options");
    drawn(cx, "menu-option-Family 200");
}

#[gpui::test]
fn arrow_keys_scroll_a_far_option_into_view(cx: &mut TestAppContext) {
    let dir = vault(None);
    let (view, cx, _) = open(cx, dir.path());
    with_many_fonts(&view, cx);
    go_to_section(&view, "Appearance", cx);
    click(cx, "dropdown-font.ui");
    for _ in 0..60 {
        cx.simulate_keystrokes("down");
    }
    // The current font is first, so the sixtieth step is Family 059.
    let menu = drawn(cx, "settings-menu");
    let option = drawn(cx, "menu-option-Family 059");
    assert!(
        option.top() >= menu.top() && option.bottom() <= menu.bottom(),
        "{option:?} is outside {menu:?}"
    );
    // Moving down brought it in at the list's bottom edge, not its top:
    // the options before it still show above it. (Debug bounds outlive
    // the frame that drew them, so positions, not absence, are checked.)
    let style = view.read_with(cx, |view, _| view.style().clone());
    assert_near(
        option.bottom(),
        menu.bottom() - style.gap_sm,
        "option bottom",
    );
    let earlier = drawn(cx, "menu-option-Family 050");
    let nine_rows = style.control_height * 9.;
    assert_near(earlier.top(), option.top() - nine_rows, "earlier option");
    // And back up to the top, where the current font is.
    cx.simulate_keystrokes("pageup pageup pageup pageup pageup pageup pageup pageup");
    let first = drawn(cx, "menu-option-Charter");
    let list_top = menu.top() + style.gap_sm + style.control_height + style.gap_xs;
    assert_near(first.top(), list_top, "first option");
    cx.simulate_keystrokes("down down enter");
    assert_eq!(token(&view, "font.ui", cx), "Family 001");
}

#[gpui::test]
fn fonts_listed_late_fill_an_open_menu_and_the_notes(cx: &mut TestAppContext) {
    let dir = vault(None);
    let root = dir.path();
    write_config(root, "theme.toml", "[font]\ntext = \"Missing Serif\"\n");
    // Nothing lists the fonts in tests until they're handed over.
    let (view, cx, _) = open(cx, root);
    let text = editor_desktop::settings_view::FontSlot::Text;
    // Until the list arrives a font is drawn by the name it's given.
    assert_eq!(
        view.read_with(cx, |view, _| view.shown_font(text).to_string()),
        "Missing Serif"
    );
    assert_eq!(view.read_with(cx, |view, _| view.font_note(text)), None);
    go_to_section(&view, "Appearance", cx);
    click(cx, "dropdown-font.text");
    assert!(view.read_with(cx, |view, _| view.menu_loading()));
    drawn(cx, "settings-menu-status");
    assert_eq!(
        view.read_with(cx, |view, _| view.menu_options()),
        ["Missing Serif", "Charter"]
    );
    cx.simulate_input("serif");

    cx.update(|_, cx| {
        editor_desktop::ui::set_installed_fonts(FONTS.map(String::from).to_vec(), cx)
    });
    cx.run_until_parked();
    // The open menu fills, keeping its filter, and stops saying it's
    // loading.
    assert!(!view.read_with(cx, |view, _| view.menu_loading()));
    assert_eq!(
        view.read_with(cx, |view, _| view.menu_options()),
        ["Missing Serif", "Liberation Serif", "Noto Serif"]
    );
    let menu = drawn(cx, "settings-menu");
    let last = drawn(cx, "menu-option-Noto Serif");
    // The loading line is gone: the last option ends the panel.
    let style = view.read_with(cx, |view, _| view.style().clone());
    assert_near(last.bottom(), menu.bottom() - style.gap_sm, "last option");
    // The row now says the named font is missing and what shows instead.
    let note = view.read_with(cx, |view, _| view.font_note(text));
    let note = note.expect("a note once the fonts are listed");
    assert!(note.starts_with("Missing Serif isn’t installed"), "{note}");
    assert_ne!(
        view.read_with(cx, |view, _| view.shown_font(text).to_string()),
        "Missing Serif"
    );
}
