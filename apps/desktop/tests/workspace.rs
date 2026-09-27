//! Drives the workspace through GPUI's test platform: tabs, panes, saving,
//! changes on disk, new notes, history, the status bar, the sidebar and
//! the modal slot. Every test works in a temporary vault.

use std::path::{Path, PathBuf};
use std::time::Duration;

use editor_desktop::actions::bind_keys;
use editor_desktop::workspace::note_doc::{AUTOSAVE_DELAY, Conflict};
use editor_desktop::workspace::watcher::DiskChange;
use editor_desktop::workspace::{OpenIn, Workspace};
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Modifiers,
    MouseButton, Render, TestAppContext, VisualTestContext, Window, div, point, prelude::*, px,
};
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
    cx.update(bind_keys);
    let vault = vault.to_path_buf();
    let (workspace, cx) = cx.add_window_view(move |window, cx| Workspace::new(&vault, window, cx));
    cx.run_until_parked();
    (workspace, cx)
}

fn run(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| workspace.run_command(id, window, cx))
    });
    cx.run_until_parked();
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

fn editor_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).text()
    })
}

fn pane_count(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    cx.read(|cx| workspace.read(cx).panes().len())
}

fn vault_path(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> PathBuf {
    cx.read(|cx| workspace.read(cx).vault().to_path_buf())
}

fn is_dirty(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str) -> bool {
    let path = vault_path(workspace, cx).join(name);
    cx.read(|cx| {
        let doc = workspace.read(cx).doc_for_path(&path, cx).unwrap();
        doc.read(cx).is_dirty()
    })
}

fn apply(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, changes: Vec<DiskChange>) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.apply_disk_changes(changes, window, cx)
        })
    });
    cx.run_until_parked();
}

fn advance(cx: &mut VisualTestContext, duration: Duration) {
    cx.executor().advance_clock(duration);
    cx.run_until_parked();
}

#[gpui::test]
fn opening_notes_adds_tabs_and_reuses_open_ones(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A"), ("b.md", "B"), ("sub/c.md", "C")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert_eq!(titles(&workspace, cx), vec!["New tab"]);
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    open(&workspace, cx, "b.md", OpenIn::NewTab);
    open(&workspace, cx, "sub/c.md", OpenIn::NewTab);
    assert_eq!(titles(&workspace, cx), vec!["a", "b", "c"]);
    open(&workspace, cx, "a.md", OpenIn::NewTab);
    assert_eq!(titles(&workspace, cx), vec!["a", "b", "c"]);
    assert_eq!(active_title(&workspace, cx), "a");
    assert_eq!(editor_text(&workspace, cx), "A");
}

#[gpui::test]
fn tabs_switch_by_number_and_cycle(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", ""), ("b.md", ""), ("c.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    open(&workspace, cx, "b.md", OpenIn::NewTab);
    open(&workspace, cx, "c.md", OpenIn::NewTab);
    run(&workspace, cx, "tab.go-1");
    assert_eq!(active_title(&workspace, cx), "a");
    run(&workspace, cx, "tab.go-9");
    assert_eq!(active_title(&workspace, cx), "c");
    run(&workspace, cx, "tab.go-2");
    assert_eq!(active_title(&workspace, cx), "b");
    run(&workspace, cx, "tab.go-7");
    assert_eq!(active_title(&workspace, cx), "b");
    run(&workspace, cx, "tab.next");
    run(&workspace, cx, "tab.next");
    assert_eq!(active_title(&workspace, cx), "a");
    run(&workspace, cx, "tab.previous");
    assert_eq!(active_title(&workspace, cx), "c");
}

#[gpui::test]
fn closing_and_reopening_tabs(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", ""), ("b.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    open(&workspace, cx, "b.md", OpenIn::NewTab);
    run(&workspace, cx, "tab.close");
    assert_eq!(titles(&workspace, cx), vec!["a"]);
    run(&workspace, cx, "tab.close");
    assert_eq!(titles(&workspace, cx), vec!["New tab"]);
    run(&workspace, cx, "tab.reopen");
    assert_eq!(titles(&workspace, cx), vec!["a"]);
    run(&workspace, cx, "tab.reopen");
    assert_eq!(titles(&workspace, cx), vec!["a", "b"]);
    assert_eq!(active_title(&workspace, cx), "b");
}

#[gpui::test]
fn keys_reach_the_workspace_through_the_editor(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    cx.simulate_keystrokes("ctrl-t");
    assert_eq!(titles(&workspace, cx), vec!["a", "New tab"]);
    cx.simulate_keystrokes("ctrl-1");
    assert_eq!(active_title(&workspace, cx), "a");
    cx.simulate_keystrokes("ctrl-w");
    assert_eq!(titles(&workspace, cx), vec!["New tab"]);
}

#[gpui::test]
fn the_launcher_opens_recent_notes_with_the_keyboard(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A"), ("b.md", "B")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    open(&workspace, cx, "b.md", OpenIn::NewTab);
    run(&workspace, cx, "tab.new");
    assert_eq!(active_title(&workspace, cx), "New tab");
    cx.simulate_keystrokes("down enter");
    assert_eq!(active_title(&workspace, cx), "a");
    assert_eq!(titles(&workspace, cx), vec!["a", "b"]);
}

#[gpui::test]
fn splits_move_focus_and_close_with_their_last_tab(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A"), ("b.md", "B")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    open(&workspace, cx, "b.md", OpenIn::SplitRight);
    assert_eq!(pane_count(&workspace, cx), 2);
    assert_eq!(active_title(&workspace, cx), "b");
    run(&workspace, cx, "pane.focus-left");
    assert_eq!(active_title(&workspace, cx), "a");
    run(&workspace, cx, "pane.focus-right");
    assert_eq!(active_title(&workspace, cx), "b");
    run(&workspace, cx, "pane.split-down");
    assert_eq!(pane_count(&workspace, cx), 3);
    assert_eq!(active_title(&workspace, cx), "b");
    run(&workspace, cx, "pane.focus-left");
    assert_eq!(active_title(&workspace, cx), "a");
    run(&workspace, cx, "pane.focus-right");
    run(&workspace, cx, "pane.focus-right");
    run(&workspace, cx, "tab.close");
    assert_eq!(pane_count(&workspace, cx), 2);
    run(&workspace, cx, "pane.close");
    assert_eq!(pane_count(&workspace, cx), 1);
    assert_eq!(titles(&workspace, cx), vec!["a"]);
}

#[gpui::test]
fn edits_in_one_pane_show_in_the_other(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    run(&workspace, cx, "pane.split-right");
    cx.simulate_input("x");
    run(&workspace, cx, "pane.focus-left");
    assert_eq!(editor_text(&workspace, cx), "xA");
}

#[gpui::test]
fn autosave_waits_for_typing_to_pause(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "note")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    cx.simulate_input("my ");
    assert!(is_dirty(&workspace, cx, "a.md"));
    advance(cx, AUTOSAVE_DELAY / 2);
    cx.simulate_input("new ");
    advance(cx, AUTOSAVE_DELAY / 2);
    let path = vault.path().join("a.md");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "note");
    advance(cx, AUTOSAVE_DELAY);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "my new note");
    assert!(!is_dirty(&workspace, cx, "a.md"));
}

#[gpui::test]
fn closing_a_tab_saves_at_once_and_keeps_line_endings(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "one\r\ntwo")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    cx.update(|_, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.update(cx, |editor, cx| {
            let end = editor.doc().len();
            editor.replace(end..end, "\nthree", cx);
        });
    });
    run(&workspace, cx, "tab.close");
    let saved = std::fs::read_to_string(vault.path().join("a.md")).unwrap();
    assert_eq!(saved, "one\r\ntwo\r\nthree");
}

#[gpui::test]
fn outside_changes_reload_a_clean_note(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "old")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let path = vault_path(&workspace, cx).join("a.md");
    std::fs::write(&path, "new from sync").unwrap();
    apply(&workspace, cx, vec![DiskChange::Changed(path)]);
    assert_eq!(editor_text(&workspace, cx), "new from sync");
    assert!(!is_dirty(&workspace, cx, "a.md"));
}

#[gpui::test]
fn outside_changes_under_edits_mark_a_conflict(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "old")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    cx.simulate_input("mine ");
    let path = vault_path(&workspace, cx).join("a.md");
    std::fs::write(&path, "theirs").unwrap();
    apply(&workspace, cx, vec![DiskChange::Changed(path.clone())]);
    assert_eq!(editor_text(&workspace, cx), "mine old");
    let conflict = cx.read(|cx| {
        let doc = workspace.read(cx).doc_for_path(&path, cx).unwrap();
        doc.read(cx).conflict()
    });
    assert_eq!(conflict, Some(Conflict::ChangedOnDisk));
    advance(cx, AUTOSAVE_DELAY * 3);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs");

    run(&workspace, cx, "tab.close");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Keep my version");
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine old");
    assert_eq!(titles(&workspace, cx), vec!["New tab"]);
}

#[gpui::test]
fn renames_and_deletes_on_disk_follow_the_tabs(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "A"), ("b.md", "B")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    open(&workspace, cx, "b.md", OpenIn::NewTab);
    let root = vault_path(&workspace, cx);
    std::fs::rename(root.join("a.md"), root.join("renamed.md")).unwrap();
    apply(
        &workspace,
        cx,
        vec![DiskChange::Renamed {
            from: root.join("a.md"),
            to: root.join("renamed.md"),
        }],
    );
    assert_eq!(titles(&workspace, cx), vec!["renamed", "b"]);
    std::fs::remove_file(root.join("b.md")).unwrap();
    apply(&workspace, cx, vec![DiskChange::Removed(root.join("b.md"))]);
    assert_eq!(titles(&workspace, cx), vec!["renamed"]);
}

#[gpui::test]
fn new_notes_count_up_and_rename_from_the_title(cx: &mut TestAppContext) {
    let vault = vault_with(&[]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    run(&workspace, cx, "note.new");
    run(&workspace, cx, "note.new");
    assert_eq!(titles(&workspace, cx), vec!["Untitled", "Untitled 1"]);
    let root = vault_path(&workspace, cx);
    assert!(root.join("Untitled.md").is_file());
    assert!(root.join("Untitled 1.md").is_file());
    // The title has the cursor with its text selected, so typing replaces it.
    cx.simulate_input("Plans");
    cx.simulate_keystrokes("enter");
    assert_eq!(titles(&workspace, cx), vec!["Untitled", "Plans"]);
    assert!(root.join("Plans.md").is_file());
    assert!(!root.join("Untitled 1.md").exists());
    // Enter moved the cursor into the note.
    cx.simulate_input("body");
    assert_eq!(editor_text(&workspace, cx), "body");
}

#[gpui::test]
fn a_taken_title_is_refused(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", ""), ("b.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    run(&workspace, cx, "note.rename");
    cx.simulate_input("b");
    cx.simulate_keystrokes("enter");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("OK");
    assert_eq!(titles(&workspace, cx), vec!["a"]);
    assert!(vault.path().join("a.md").is_file());
}

#[gpui::test]
fn deleting_asks_then_moves_the_note_away(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("a.md", ""),
        (".editor/settings.toml", "[files]\ntrash = \"vault\"\n"),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    run(&workspace, cx, "note.delete");
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(vault.path().join("a.md").exists());
    run(&workspace, cx, "note.delete");
    cx.simulate_prompt_answer("Move to trash");
    cx.run_until_parked();
    assert!(!vault.path().join("a.md").exists());
    assert!(vault.path().join(".trash/a.md").exists());
    assert_eq!(titles(&workspace, cx), vec!["New tab"]);
}

#[gpui::test]
fn history_goes_back_and_forward_across_notes_and_jumps(cx: &mut TestAppContext) {
    let long: String = (0..60).map(|line| format!("line {line}\n")).collect();
    let vault = vault_with(&[("a.md", &long), ("b.md", "B")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let far = long.find("line 50").unwrap();
    cx.update(|_, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.update(cx, |editor, cx| editor.select(far, far, cx));
    });
    cx.run_until_parked();
    open(&workspace, cx, "b.md", OpenIn::ActiveTab);
    assert_eq!(titles(&workspace, cx), vec!["b"]);

    run(&workspace, cx, "history.back");
    assert_eq!(active_title(&workspace, cx), "a");
    let cursor = |cx: &mut VisualTestContext| {
        cx.read(|cx| {
            workspace
                .read(cx)
                .active_editor(cx)
                .unwrap()
                .read(cx)
                .cursor()
        })
    };
    assert_eq!(cursor(cx), far);
    run(&workspace, cx, "history.back");
    assert_eq!(cursor(cx), 0);
    run(&workspace, cx, "history.forward");
    assert_eq!(cursor(cx), far);
    run(&workspace, cx, "history.forward");
    assert_eq!(active_title(&workspace, cx), "b");
}

#[gpui::test]
fn the_status_bar_counts_the_note_or_the_selection(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "one two three\nfour")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    assert!(cx.read(|cx| workspace.read(cx).status().is_none()));
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let status = cx.read(|cx| workspace.read(cx).status().cloned().unwrap());
    assert_eq!(status.stats.words, 4);
    assert_eq!(status.stats.characters, 17);
    assert_eq!(status.position_label(), "1:1");
    assert_eq!(status.reading_label(), "1 min read");
    cx.update(|_, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.update(cx, |editor, cx| editor.select(4, 13, cx));
    });
    cx.run_until_parked();
    let status = cx.read(|cx| workspace.read(cx).status().cloned().unwrap());
    assert_eq!(status.words_label(), "2 words selected");
    assert_eq!(status.position_label(), "1:14");
}

#[gpui::test]
fn the_window_title_names_the_note_and_vault(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let vault_name = vault_path(&workspace, cx)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(cx.window_title(), Some(format!("a — {vault_name}")));
}

#[gpui::test]
fn open_tabs_are_remembered_per_vault(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", ""), ("sub/b.md", "")]);
    {
        let (workspace, cx) = open_workspace(cx, vault.path());
        open(&workspace, cx, "a.md", OpenIn::ActiveTab);
        open(&workspace, cx, "sub/b.md", OpenIn::NewTab);
        run(&workspace, cx, "tab.go-1");
        let device = cx.read(|cx| workspace.read(cx).device_state(cx));
        assert_eq!(device.open_tabs, vec!["a.md", "sub/b.md"]);
        assert_eq!(device.active_tab, Some(0));
        workspace.update(cx, |workspace, cx| workspace.prepare_to_close(cx));
    }
    let vault_path = vault.path().to_path_buf();
    let (restored, cx) = cx.add_window_view(move |window, cx| {
        let mut workspace = Workspace::new(&vault_path, window, cx);
        workspace.restore_session(window, cx);
        workspace
    });
    cx.run_until_parked();
    assert_eq!(titles(&restored, cx), vec!["a", "b"]);
    assert_eq!(active_title(&restored, cx), "a");
}

struct Panel {
    focus_handle: FocusHandle,
}

impl Focusable for Panel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().track_focus(&self.focus_handle)
    }
}

impl EventEmitter<DismissEvent> for Panel {}

fn panel_visible(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.read(|cx| workspace.read(cx).left_panel().is_visible())
}

#[gpui::test]
fn the_left_panel_toggles_and_takes_focus(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("a.md", ""),
        (
            ".editor/settings.toml",
            "[sidebar.files]\nreveal = \"toggle\"\nmode = \"push\"\n",
        ),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let panel = cx.update(|window, cx| {
        let panel = cx.new(|cx| Panel {
            focus_handle: cx.focus_handle(),
        });
        let focus = panel.read(cx).focus_handle.clone();
        workspace.update(cx, |workspace, cx| {
            workspace.set_left_panel(panel.clone().into(), Some(focus), cx)
        });
        let _ = window;
        panel
    });
    assert!(!panel_visible(&workspace, cx));
    cx.simulate_keystrokes("ctrl-\\");
    assert!(panel_visible(&workspace, cx));
    cx.simulate_keystrokes("ctrl-\\");
    assert!(!panel_visible(&workspace, cx));
    cx.simulate_keystrokes("ctrl-shift-e");
    assert!(panel_visible(&workspace, cx));
    let focused = cx.update(|window, cx| panel.read(cx).focus_handle.is_focused(window));
    assert!(focused);
    run(&workspace, cx, "sidebar.files.toggle");
    let editor_focused = cx.update(|window, cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).focus_handle(cx).is_focused(window)
    });
    assert!(editor_focused);
}

#[gpui::test]
fn the_left_panel_reveals_on_hover_and_hides_after_leaving(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    cx.update(|_, cx| {
        let panel = cx.new(|cx| Panel {
            focus_handle: cx.focus_handle(),
        });
        workspace.update(cx, |workspace, cx| {
            workspace.set_left_panel(panel.into(), None, cx)
        });
    });
    assert!(!panel_visible(&workspace, cx));
    cx.simulate_mouse_move(point(px(2.), px(200.)), None, Modifiers::none());
    assert!(panel_visible(&workspace, cx));
    cx.simulate_mouse_move(point(px(100.), px(200.)), None, Modifiers::none());
    cx.simulate_mouse_move(point(px(700.), px(200.)), None, Modifiers::none());
    advance(cx, Duration::from_millis(200));
    assert!(panel_visible(&workspace, cx));
    advance(cx, Duration::from_millis(200));
    assert!(!panel_visible(&workspace, cx));
}

#[gpui::test]
fn modals_close_on_escape_and_click_outside_and_give_focus_back(cx: &mut TestAppContext) {
    let vault = vault_with(&[("a.md", "")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "a.md", OpenIn::ActiveTab);
    let editor_focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            let editor = workspace.read(cx).active_editor(cx).unwrap();
            editor.read(cx).focus_handle(cx).is_focused(window)
        })
    };
    let toggle = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |_, cx| Panel {
                    focus_handle: cx.focus_handle(),
                })
            })
        });
        cx.run_until_parked();
    };
    let modal_open = |cx: &mut VisualTestContext| {
        cx.read(|cx| workspace.read(cx).active_modal::<Panel>().is_some())
    };

    toggle(cx);
    assert!(modal_open(cx));
    assert!(!editor_focused(cx));
    cx.simulate_keystrokes("escape");
    assert!(!modal_open(cx));
    assert!(editor_focused(cx));

    toggle(cx);
    toggle(cx);
    assert!(!modal_open(cx));
    assert!(editor_focused(cx));

    toggle(cx);
    let modal = cx.read(|cx| workspace.read(cx).active_modal::<Panel>().unwrap());
    modal.update(cx, |_, cx| cx.emit(DismissEvent));
    cx.run_until_parked();
    assert!(!modal_open(cx));
    assert!(editor_focused(cx));

    toggle(cx);
    cx.simulate_click(point(px(5.), px(5.)), Modifiers::none());
    assert!(!modal_open(cx));
    let _ = MouseButton::Left;
}
