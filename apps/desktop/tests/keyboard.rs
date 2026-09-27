//! Keyboard first, in a real workspace: every command can be reached from
//! the keyboard and runs, focus rings show only while the keyboard is
//! driving, and holding Mod shows the shortcuts for where the keyboard is.

use std::path::Path;
use std::time::Duration;

use editor_config::commands::BUILTIN_COMMANDS;
use editor_config::{Platform, RuleSet};
use editor_desktop::actions::bind_keys;
use editor_desktop::features;
use editor_desktop::keymap::all_bindings;
use editor_desktop::ui::focus_visible::keyboard_driving;
use editor_desktop::workspace::{FocusArea, HOLD_DELAY, OpenIn, Workspace, sheet_groups};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point, px};
use tempfile::TempDir;

const DESKTOP: [Platform; 3] = [Platform::Macos, Platform::Windows, Platform::Linux];

/// Commands for features that aren't built yet (PLAN.md, phase 4). They
/// have their keys already; the test fails once one runs, so it leaves
/// this list.
const NOT_BUILT_YET: [&str; 1] = ["prose.toggle-sentence-highlighting"];

fn vault() -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Note.md"), "# Note\n\nSome text.\n").unwrap();
    std::fs::create_dir(vault.path().join("Folder")).unwrap();
    std::fs::write(vault.path().join("Folder/Other.md"), "Other.\n").unwrap();
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

fn open_note(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
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
}

fn press(cx: &mut VisualTestContext, command: &str) {
    let key = all_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|binding| binding.command == command)
        .map(|binding| binding.keystroke)
        .unwrap_or_else(|| panic!("{command} has no key"));
    cx.simulate_keystrokes(&key);
    cx.run_until_parked();
}

fn driving(cx: &mut VisualTestContext) -> bool {
    cx.read(keyboard_driving)
}

/// PLAN.md: a CI test walks every command in the registry and fails if one
/// can't be reached from the keyboard. Reached means a key the app binds on
/// every desktop platform, or a place in the palette (which has a key), and
/// something in the app that runs it.
#[gpui::test]
fn every_command_is_reached_from_the_keyboard_and_runs(cx: &mut TestAppContext) {
    let vault = vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    let rules = RuleSet::defaults();
    let mut unreachable = Vec::new();
    for platform in DESKTOP {
        let bound: Vec<String> = all_bindings(&rules, platform)
            .into_iter()
            .map(|binding| binding.command)
            .collect();
        let palette_bound = bound.iter().any(|id| id == "palette.open");
        assert!(palette_bound, "the palette has no key on {platform:?}");
        for spec in BUILTIN_COMMANDS {
            let has_key = bound.iter().any(|id| id == spec.id);
            if !has_key && !spec.palette {
                unreachable.push(format!("{} on {platform:?}", spec.id));
            }
        }
    }
    assert!(
        unreachable.is_empty(),
        "no key or palette entry: {unreachable:?}"
    );
    let not_run: Vec<&str> = cx.read(|cx| {
        let workspace = workspace.read(cx);
        BUILTIN_COMMANDS
            .iter()
            .map(|spec| spec.id)
            .filter(|id| !workspace.can_run(id))
            .collect()
    });
    assert_eq!(not_run, NOT_BUILT_YET, "what nothing runs");
}

#[gpui::test]
fn rings_show_only_while_the_keyboard_drives(cx: &mut TestAppContext) {
    let vault = vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_note(&workspace, cx);
    assert!(!driving(cx), "nothing has been pressed yet");
    // Arrows in the editor move the caret; that isn't finding a control.
    cx.simulate_keystrokes("down");
    assert!(!driving(cx));
    press(cx, "file-tree.focus");
    cx.simulate_keystrokes("down");
    assert!(driving(cx), "arrows in the tree drive");
    // A click anywhere hands over to the pointer.
    cx.simulate_mouse_down(
        point(px(400.), px(300.)),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(px(400.), px(300.)),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(!driving(cx));
    // A screen opens without a ring, even right after keyboard use.
    press(cx, "file-tree.focus");
    cx.simulate_keystrokes("down");
    assert!(driving(cx));
    press(cx, "settings.open");
    assert!(!driving(cx), "settings open without a ring");
    cx.simulate_keystrokes("down");
    assert!(driving(cx), "moving through settings shows it");
}

#[gpui::test]
fn holding_mod_shows_the_shortcuts_for_where_the_keyboard_is(cx: &mut TestAppContext) {
    let vault = vault();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_note(&workspace, cx);
    let sheet = |cx: &mut VisualTestContext| cx.read(|cx| workspace.read(cx).shortcut_sheet());
    let hold = Modifiers::secondary_key();
    cx.simulate_modifiers_change(hold);
    cx.executor().advance_clock(HOLD_DELAY / 2);
    cx.run_until_parked();
    assert_eq!(sheet(cx), None, "a quick press shows nothing");
    cx.executor().advance_clock(HOLD_DELAY);
    cx.run_until_parked();
    assert_eq!(sheet(cx), Some(FocusArea::Editor));
    cx.simulate_modifiers_change(Modifiers::none());
    cx.run_until_parked();
    assert_eq!(sheet(cx), None, "letting go hides it");

    // Mod held as part of a shortcut never shows it.
    cx.simulate_modifiers_change(hold);
    press(cx, "sidebar.right.toggle");
    cx.executor().advance_clock(HOLD_DELAY * 2);
    cx.run_until_parked();
    assert_eq!(sheet(cx), None);
    cx.simulate_modifiers_change(Modifiers::none());

    // In the file tree it leads with the tree's own keys.
    press(cx, "file-tree.focus");
    cx.simulate_modifiers_change(Modifiers::none());
    cx.simulate_modifiers_change(hold);
    cx.executor()
        .advance_clock(HOLD_DELAY + Duration::from_millis(50));
    cx.run_until_parked();
    assert_eq!(sheet(cx), Some(FocusArea::FileTree));
    cx.simulate_modifiers_change(Modifiers::none());
}

#[gpui::test]
fn the_sheet_lists_what_runs_where(cx: &mut TestAppContext) {
    let vault = vault();
    let (_, cx) = open_workspace(cx, vault.path());
    let titles = |area, cx: &mut VisualTestContext| -> Vec<String> {
        cx.read(|cx| {
            sheet_groups(area, |_| true, cx)
                .iter()
                .flat_map(|group| group.rows.iter().map(|row| row.title.to_string()))
                .collect()
        })
    };
    let editor = titles(FocusArea::Editor, cx);
    assert!(editor.contains(&"Toggle bold".to_string()));
    assert!(editor.contains(&"Go to tabs 1–9".to_string()));
    assert!(!editor.iter().any(|title| title == "Go to tab 2"));
    assert!(!editor.contains(&"Undo".to_string()));
    let tree = titles(FocusArea::FileTree, cx);
    assert_eq!(tree.first().map(String::as_str), Some("Open"));
    assert!(
        !tree.contains(&"Toggle bold".to_string()),
        "bold needs the editor"
    );
    let groups = cx.read(|cx| sheet_groups(FocusArea::RightSidebar, |_| true, cx));
    assert_eq!(groups[0].title, "Sidebar");
}
