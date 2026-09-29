//! The standalone views wired into a real workspace: each shortcut opens
//! its view, and the view's choice reaches the workspace.

use editor_config::CONFIG_DIR;
use std::path::Path;

use editor_config::{Platform, RuleSet};
use editor_desktop::actions::bind_keys;
use editor_desktop::features;
use editor_desktop::keymap::all_bindings;
use editor_desktop::outline::OutlinePicker;
use editor_desktop::palette::CommandPalette;
use editor_desktop::settings_view::SettingsView;
use editor_desktop::switcher::QuickSwitcher;
use editor_desktop::vault_search::VaultSearch;
use editor_desktop::workspace::{OpenIn, Workspace};
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};
use tempfile::TempDir;

fn vault_with(notes: &[(&str, &str)]) -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    for (name, text) in notes {
        let path = vault.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
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
    (workspace, cx)
}

/// The keystroke this platform's default rules bind to `command`.
fn key_for(command: &str) -> String {
    all_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|binding| binding.command == command)
        .map(|binding| binding.keystroke)
        .unwrap_or_else(|| panic!("{command} has no key"))
}

fn press(cx: &mut VisualTestContext, command: &str) {
    cx.simulate_keystrokes(&key_for(command));
    cx.run_until_parked();
}

fn open(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(Path::new(name), OpenIn::ActiveTab, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
}

fn active_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.read(|cx| {
        let editor = workspace.read(cx).active_editor(cx)?;
        Some(editor.read(cx).text())
    })
}

fn has_modal<V: 'static>(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).active_modal::<V>().is_some())
}

#[gpui::test]
fn quick_switcher_opens_a_note_by_typing(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Alpha.md", "alpha text"),
        ("folder/Beta note.md", "beta text"),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    press(cx, "switcher.open");
    assert!(has_modal::<QuickSwitcher>(&workspace, cx));
    cx.simulate_input("beta");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx).as_deref(), Some("beta text"));
    assert!(!has_modal::<QuickSwitcher>(&workspace, cx));
}

#[gpui::test]
fn quick_switcher_creates_a_missing_note(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Alpha.md", "alpha")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    press(cx, "switcher.open");
    cx.simulate_input("Fresh idea");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(vault.path().join("Fresh idea.md").exists());
    assert_eq!(active_text(&workspace, cx).as_deref(), Some(""));
}

#[gpui::test]
fn palette_runs_an_editor_command(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "word")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Note.md");
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| editor.select(0, 4, cx));
    press(cx, "palette.open");
    assert!(has_modal::<CommandPalette>(&workspace, cx));
    cx.simulate_input("toggle bold");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx).as_deref(), Some("**word**"));
}

#[gpui::test]
fn find_opens_a_bar_in_the_pane_and_highlights_matches(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "one two one")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Note.md");
    press(cx, "find.open");
    let has_toolbar = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .toolbar()
            .is_some()
    });
    assert!(has_toolbar);
    cx.simulate_input("one");
    cx.run_until_parked();
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    let matches = cx.read(|cx| {
        editor
            .read(cx)
            .highlights(editor_desktop::HighlightKind::SearchMatch)
            .to_vec()
    });
    assert_eq!(matches.len(), 2);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let has_toolbar = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .toolbar()
            .is_some()
    });
    assert!(!has_toolbar);
}

#[gpui::test]
fn outline_jumps_to_a_heading(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "# Top\ntext\n## Second\nmore")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Note.md");
    press(cx, "outline.jump-to-heading");
    assert!(has_modal::<OutlinePicker>(&workspace, cx));
    cx.simulate_input("second");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    assert_eq!(cx.read(|cx| editor.read(cx).cursor()), 11);
}

#[gpui::test]
fn settings_and_vault_search_open_as_modals(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "text")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Note.md");
    press(cx, "settings.open");
    assert!(has_modal::<SettingsView>(&workspace, cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!has_modal::<SettingsView>(&workspace, cx));
    press(cx, "search.open");
    assert!(has_modal::<VaultSearch>(&workspace, cx));
}

#[gpui::test]
fn the_file_tree_fills_the_left_panel(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "text")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    let has_panel = cx.read(|cx| workspace.read(cx).left_panel().view().is_some());
    assert!(has_panel);
}

#[gpui::test]
fn escape_from_a_revealed_file_tree_hides_it_and_returns_to_the_note(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "text")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(Path::new("Note.md"), OpenIn::ActiveTab, window, cx)
                .unwrap()
        })
    });
    cx.run_until_parked();
    let visible =
        |cx: &mut VisualTestContext| cx.read(|cx| workspace.read(cx).left_panel().is_visible());
    assert!(!visible(cx), "the default panel waits at the edge");
    press(cx, "file-tree.focus");
    assert!(visible(cx), "focusing the tree reveals it");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!visible(cx), "Escape lets the revealed panel hide");
    let editor_focused = cx.update(|window, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.focus_handle(cx).is_focused(window)
    });
    assert!(editor_focused, "the note has the keyboard again");
}

#[gpui::test]
fn the_appearance_page_previews_a_change_as_it_is_made(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "text")]);
    let settings = vault.path().join(CONFIG_DIR).join("settings.toml");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, "[appearance]\nbase-font-size = 24\n").unwrap();
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Note.md");
    press(cx, "settings.open");
    let view = cx.read(|cx| workspace.read(cx).active_modal::<SettingsView>().unwrap());
    // Typing outside a field searches, which lands on Appearance.
    cx.simulate_input("font size");
    cx.run_until_parked();
    assert!(cx.debug_bounds("appearance-preview").is_some());
    let body_size = |cx: &mut VisualTestContext| {
        cx.read(|cx| {
            let preview = view.read(cx).shown_preview().unwrap().clone();
            preview.read(cx).theme().body_font_size
        })
    };
    let before = body_size(cx);
    view.update(cx, |view, cx| view.reset("appearance.base-font-size", cx));
    cx.run_until_parked();
    assert!(body_size(cx) < before, "the preview follows the change");
}

#[gpui::test]
fn a_note_opens_again_where_it_was_left(cx: &mut TestAppContext) {
    let long = "A line of the note.\n\n".repeat(200);
    let vault = vault_with(&[("a.md", &long), ("b.md", "other")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md");
    let editor =
        |cx: &mut VisualTestContext| cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    let first = editor(cx);
    first.update(cx, |editor, cx| {
        editor.move_to(2000, false, cx);
        editor.scroll_by(gpui::px(1500.), cx);
    });
    cx.run_until_parked();
    let left = first.read_with(cx, |editor, _| editor.position());
    assert!(left.1 > 0, "scrolled down");

    // Replaced in its tab, then back.
    open(&workspace, cx, "b.md");
    open(&workspace, cx, "a.md");
    let again = editor(cx);
    assert_ne!(again, first, "a fresh editor");
    assert_eq!(again.read_with(cx, |editor, _| editor.position()), left);

    // And in the next session.
    let saved = cx.read(|cx| workspace.read(cx).device_state(cx).positions);
    assert!(
        saved
            .iter()
            .any(|kept| kept.path == "a.md" && kept.cursor == 2000)
    );
}
