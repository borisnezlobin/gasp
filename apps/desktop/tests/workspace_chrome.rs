//! The workspace chrome driven by the mouse: the sidebar's buttons and
//! menus, the tab bar, the note header, the help dialog and the note's
//! right-click menu. Every test works in a temporary vault with the
//! standalone views wired in.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use editor_config::Platform;
use editor_desktop::actions::bind_keys;
use editor_desktop::features;
use editor_desktop::settings_view::SettingsView;
use editor_desktop::ui::{MenuItem, Tooltip, hints};
use editor_desktop::vault_search::VaultSearch;
use editor_desktop::workspace::help::ShortcutsHelp;
use editor_desktop::workspace::{OpenIn, Workspace};
use gpui::{
    Entity, Focusable, Modifiers, MouseButton, MouseDownEvent, TestAppContext, VisualTestContext,
};
use tempfile::TempDir;

/// Shows the sidebar pushed beside the notes, as the reference does.
const PINNED_SIDEBAR: &str = "[sidebar.files]\nreveal = \"always\"\nmode = \"push\"\n";

fn vault_with(notes: &[(&str, &str)]) -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    for (name, text) in notes {
        let path = vault.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let settings = vault.path().join(".editor/settings.toml");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(settings, PINNED_SIDEBAR).unwrap();
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
    (workspace, cx)
}

fn open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str, open_in: OpenIn) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(Path::new(name), open_in, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} isn't drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn is_drawn(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.run_until_parked();
    cx.debug_bounds(selector).is_some()
}

fn titles(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        pane.tabs().iter().map(|tab| tab.title(cx)).collect()
    })
}

fn active_title(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        pane.active_tab().unwrap().title(cx)
    })
}

fn menu_labels(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    cx.read(|cx| {
        let menu = workspace.read(cx).open_menu(cx)?;
        Some(menu.read(cx).labels())
    })
}

fn editor_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).text()
    })
}

fn vault_path(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> PathBuf {
    cx.read(|cx| workspace.read(cx).vault().to_path_buf())
}

fn panel_visible(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).left_panel().is_visible())
}

#[gpui::test]
fn tooltips_name_the_command_and_its_shortcut(cx: &mut TestAppContext) {
    cx.update(|cx| {
        hints::set_platform(Platform::Macos, cx);
        assert_eq!(Tooltip::for_command("note.new", cx).text(), "New note ⌘N");
        assert_eq!(
            Tooltip::for_command("sidebar.files.toggle", cx).text(),
            "Toggle file sidebar ⌘\\"
        );
        hints::set_platform(Platform::Linux, cx);
        assert_eq!(Tooltip::for_command("tab.new", cx).text(), "New tab Ctrl+T");
    });
}

#[gpui::test]
fn sidebar_buttons_run_their_commands(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert!(panel_visible(&workspace, cx));

    click(cx, "sidebar-new-note");
    assert_eq!(active_title(&workspace, cx), "Untitled");
    assert!(vault_path(&workspace, cx).join("Untitled.md").is_file());

    click(cx, "sidebar-settings");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<SettingsView>().is_some()));
    cx.simulate_keystrokes("escape");

    click(cx, "sidebar-search");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<VaultSearch>().is_some()));
    cx.simulate_keystrokes("escape");

    cx.simulate_keystrokes("secondary-shift-e");
    let tree_focused = cx.update(|window, cx| {
        let tree = workspace.read(cx).file_tree().unwrap().clone();
        tree.focus_handle(cx).is_focused(window)
    });
    assert!(tree_focused);
}

#[gpui::test]
fn the_sidebar_button_moves_to_the_tab_bar_while_hidden(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert!(!is_drawn(cx, "pane-sidebar-toggle"));
    click(cx, "sidebar-toggle");
    assert!(!panel_visible(&workspace, cx));
    assert!(is_drawn(cx, "pane-sidebar-toggle"));
    click(cx, "pane-sidebar-toggle");
    assert!(panel_visible(&workspace, cx));
    // Drawn bounds outlive the element in tests, so ask the pane.
    let shown = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .show_sidebar_toggle
    });
    assert!(!shown);
}

#[gpui::test]
fn tree_tools_sort_collapse_and_make_folders(cx: &mut TestAppContext) {
    let vault = vault_with(&[("b.md", ""), ("a.md", ""), ("Sub/c.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    let tree = cx.read(|cx| workspace.read(cx).file_tree().unwrap().clone());
    let labels = |cx: &mut VisualTestContext| -> Vec<String> {
        tree.read_with(cx, |tree, _| {
            tree.rows()
                .iter()
                .map(|row| row.entry.label().to_string())
                .collect()
        })
    };
    assert_eq!(labels(cx), ["Sub", "a", "b"]);

    click(cx, "sidebar-sort");
    let items = menu_labels(&workspace, cx).expect("the sort menu is open");
    assert_eq!(items.len(), 4);
    click(cx, "menu-item-File name (Z to A)");
    assert!(menu_labels(&workspace, cx).is_none());
    assert_eq!(labels(cx), ["Sub", "b", "a"]);

    let sub = vault_path(&workspace, cx).join("Sub/c.md");
    tree.update(cx, |tree, cx| tree.reveal(&sub, cx));
    assert!(labels(cx).contains(&"c".to_owned()));
    click(cx, "sidebar-collapse-all");
    assert!(tree.read_with(cx, |tree, _| tree.expanded_folders().is_empty()));

    click(cx, "sidebar-new-folder");
    assert!(tree.read_with(cx, |tree, _| tree.editing_field().is_some()));
}

#[gpui::test]
fn the_tab_list_shows_every_tab_and_switches(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A"), ("b.md", "B"), ("c.md", "C")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    open(&workspace, cx, "b.md", OpenIn::NewTab);
    open(&workspace, cx, "c.md", OpenIn::NewTab);

    click(cx, "pane-tab-list");
    let items = menu_labels(&workspace, cx).expect("the tab list is open");
    assert_eq!(items[..3], ["a", "b", "c"]);
    assert!(items.contains(&"New tab".to_owned()));
    assert!(items.contains(&"Reopen closed tab".to_owned()));
    click(cx, "menu-item-a");
    assert_eq!(active_title(&workspace, cx), "a");

    // The keyboard works the same way: the third row down is "c".
    click(cx, "pane-tab-list");
    cx.simulate_keystrokes("down down down enter");
    cx.run_until_parked();
    assert_eq!(active_title(&workspace, cx), "c");

    click(cx, "pane-tab-list");
    cx.simulate_keystrokes("escape");
    assert!(menu_labels(&workspace, cx).is_none());
    let editor_focused = cx.update(|window, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).focus_handle(cx).is_focused(window)
    });
    assert!(editor_focused);
}

#[gpui::test]
fn tab_buttons_open_and_close_tabs(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    click(cx, "pane-new-tab");
    assert_eq!(titles(&workspace, cx), ["a", "New tab"]);
    click(cx, "close-tab-1");
    assert_eq!(titles(&workspace, cx), ["a"]);
}

#[gpui::test]
fn a_breadcrumb_reveals_its_folder_in_the_tree(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Essays/Drafts/idea.md", "text"), ("z.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Essays/Drafts/idea.md", OpenIn::ActiveTab);
    click(cx, "sidebar-collapse-all");
    click(cx, "crumb-Drafts");
    let root = vault_path(&workspace, cx);
    let tree = cx.read(|cx| workspace.read(cx).file_tree().unwrap().clone());
    let (selected, expanded) = tree.read_with(cx, |tree, _| {
        (tree.selected_path(), tree.expanded_folders())
    });
    assert_eq!(selected, Some(root.join("Essays/Drafts")));
    assert!(expanded.contains(&PathBuf::from("Essays")));
    assert!(expanded.contains(&PathBuf::from("Essays/Drafts")));
    assert!(panel_visible(&workspace, cx));
}

#[gpui::test]
fn back_and_forward_are_disabled_without_history(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A"), ("b.md", "B")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let can = |cx: &mut VisualTestContext| {
        cx.read(|cx| {
            let pane = workspace.read(cx).active_pane().read(cx);
            (pane.can_go_back(), pane.can_go_forward())
        })
    };
    assert_eq!(can(cx), (false, false));
    click(cx, "pane-back");
    assert_eq!(active_title(&workspace, cx), "a");

    open(&workspace, cx, "b.md", OpenIn::ActiveTab);
    assert_eq!(can(cx), (true, false));
    click(cx, "pane-back");
    assert_eq!(active_title(&workspace, cx), "a");
    assert_eq!(can(cx), (false, true));
    click(cx, "pane-forward");
    assert_eq!(active_title(&workspace, cx), "b");
}

#[gpui::test]
fn the_more_menu_lists_note_actions_and_splits(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    click(cx, "pane-more");
    let items = menu_labels(&workspace, cx).expect("the menu is open");
    for label in [
        "Rename",
        "Move to trash",
        "Reveal in file tree",
        "Split right",
        "Split down",
        "Export to PDF",
        "Print",
        "Copy path",
        "Open in default app",
        "Find",
        "Replace",
    ] {
        assert!(items.contains(&label.to_owned()), "{label} in {items:?}");
    }
    click(cx, "menu-item-Copy path");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    let expected = vault_path(&workspace, cx).join("a.md");
    assert_eq!(copied, Some(expected.to_string_lossy().into_owned()));

    click(cx, "pane-more");
    click(cx, "menu-item-Split right");
    assert_eq!(cx.read(|cx| workspace.read(cx).panes().len()), 2);
}

#[gpui::test]
fn the_reading_button_runs_its_command(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let runs = Rc::new(Cell::new(0));
    let counter = runs.clone();
    workspace.update(cx, |workspace, _| {
        workspace.on_command("markdown.cycle-symbols", move |_, _, _| {
            counter.set(counter.get() + 1)
        });
    });
    click(cx, "pane-reading");
    assert_eq!(runs.get(), 1);
}

#[gpui::test]
fn the_note_menu_formats_the_selection(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "hello world")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| editor.select(0, 5, cx));
    let note = cx
        .debug_bounds("pane-gutter-left")
        .expect("the note is drawn");
    cx.simulate_event(MouseDownEvent {
        position: note.center(),
        button: MouseButton::Right,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    let items = menu_labels(&workspace, cx).expect("the note menu is open");
    assert_eq!(
        items[..10],
        [
            "Undo",
            "Redo",
            "-",
            "Cut",
            "Copy",
            "Paste",
            "Paste as plain text",
            "-",
            "Select all",
            "-"
        ]
    );
    assert!(items.contains(&"Format".to_owned()));
    assert!(items.contains(&"Insert footnote".to_owned()));
    let format = cx.debug_bounds("menu-item-Format").unwrap();
    cx.simulate_mouse_move(format.center(), None, Modifiers::none());
    cx.run_until_parked();
    click(cx, "menu-item-Bold");
    assert_eq!(editor_text(&workspace, cx), "**hello** world");
}

/// The labels of the open menu's entries that can't be chosen.
fn disabled_items(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.read(|cx| {
        let menu = workspace.read(cx).open_menu(cx).unwrap();
        menu.read(cx)
            .items()
            .iter()
            .filter_map(|item| match item {
                MenuItem::Entry(entry) if entry.disabled => Some(entry.label.to_string()),
                _ => None,
            })
            .collect()
    })
}

fn right_click_note(cx: &mut VisualTestContext) {
    let note = cx
        .debug_bounds("pane-gutter-left")
        .expect("the note is drawn");
    cx.simulate_event(MouseDownEvent {
        position: note.center(),
        button: MouseButton::Right,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn the_note_menu_disables_what_has_nothing_to_act_on(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "hello world")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    right_click_note(cx);
    assert_eq!(
        disabled_items(&workspace, cx),
        [
            "Undo",
            "Redo",
            "Cut",
            "Copy",
            "Paste",
            "Paste as plain text"
        ]
    );
    cx.simulate_keystrokes("escape");
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| {
        editor.replace(0..5, "howdy", cx);
        editor.select(0, 5, cx);
    });
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("x".into()));
    right_click_note(cx);
    assert_eq!(disabled_items(&workspace, cx), ["Redo"]);
    let undo = menu_labels(&workspace, cx).unwrap();
    assert_eq!(undo[0], "Undo");
    click(cx, "menu-item-Undo");
    assert_eq!(editor_text(&workspace, cx), "hello world");
}

#[gpui::test]
fn the_note_menu_works_from_the_keyboard(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "hello world")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| editor.select(6, 11, cx));
    let note = cx
        .debug_bounds("pane-gutter-right")
        .expect("the note is drawn");
    cx.simulate_event(MouseDownEvent {
        position: note.center(),
        button: MouseButton::Right,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    // Down past the disabled items (nothing to undo or paste) to Cut,
    // Copy, Select all and Format, Right into it, Down to Italic.
    cx.simulate_keystrokes("down down down down right");
    cx.run_until_parked();
    let submenu = cx.read(|cx| {
        let menu = workspace.read(cx).open_menu(cx).unwrap();
        menu.read(cx).submenu()
    });
    assert!(submenu.is_some(), "Format opened");
    cx.simulate_keystrokes("left");
    let closed = cx.read(|cx| {
        let menu = workspace.read(cx).open_menu(cx).unwrap();
        menu.read(cx).submenu().is_none()
    });
    assert!(closed, "Left closes the submenu");
    cx.simulate_keystrokes("right down enter");
    cx.run_until_parked();
    assert_eq!(editor_text(&workspace, cx), "hello *world*");
    assert!(menu_labels(&workspace, cx).is_none());
}

#[gpui::test]
fn the_help_dialog_lists_shortcuts_and_runs_them(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    click(cx, "sidebar-help");
    let help = cx.read(|cx| workspace.read(cx).active_modal::<ShortcutsHelp>());
    let commands = help.unwrap().read_with(cx, |help, _| help.commands());
    assert!(commands.contains(&"palette.open"));
    assert!(commands.contains(&"format.bold"));
    click(cx, "help-tab.new");
    assert!(cx.read(|cx| workspace.read(cx).active_modal::<ShortcutsHelp>().is_none()));
    assert_eq!(titles(&workspace, cx), ["a", "New tab"]);
}

#[gpui::test]
fn the_vault_switcher_offers_another_vault(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    click(cx, "sidebar-vault");
    let items = menu_labels(&workspace, cx).expect("the vault menu is open");
    assert_eq!(
        items.last().map(String::as_str),
        Some("Open another vault…")
    );
}
