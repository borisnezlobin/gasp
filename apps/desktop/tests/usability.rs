//! Whole workflows found wanting in the usability pass: each test walks
//! one through a real workspace with every feature installed.

use std::path::Path;
use std::time::Duration;

use gasp_config::{Platform, RuleSet};
use gasp_desktop::actions::bind_keys;
use gasp_desktop::features;
use gasp_desktop::keymap::all_bindings;
use gasp_desktop::notices::{self, NoticeKind};
use gasp_desktop::vault_search::VaultSearch;
use gasp_desktop::workspace::{OpenIn, Workspace};
use gpui::{Entity, TestAppContext, VisualTestContext};
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

fn run(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, id: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            if !workspace.run_command(id, window, cx) {
                let command = gasp_desktop::keymap::RunCommand {
                    id: id.to_owned().into(),
                };
                window.dispatch_action(Box::new(command), cx);
            }
        })
    });
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

fn shown_notices(cx: &mut VisualTestContext) -> Vec<(NoticeKind, String)> {
    let window = cx.update(|window, _| window.window_handle());
    cx.read(|cx| {
        notices::shown_in(window, cx)
            .into_iter()
            .map(|(_, notice)| (notice.kind, notice.message.to_string()))
            .collect()
    })
}

#[gpui::test]
fn a_notice_reports_what_a_command_did_then_leaves(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "A[^2] and B[^1]\n\n[^1]: one\n[^2]: two\n")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Note.md");
    run(&workspace, cx, "footnote.tidy");
    assert_eq!(
        shown_notices(cx),
        vec![(NoticeKind::Done, "Footnotes renumbered.".to_owned())]
    );
    cx.executor().advance_clock(Duration::from_secs(7));
    cx.run_until_parked();
    assert!(shown_notices(cx).is_empty());
}

#[gpui::test]
fn a_problem_notice_stays_until_dismissed(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Note.md", "text")]);
    let (_, cx) = open_workspace(cx, vault.path());
    let id = cx.update(|_, cx| notices::problem("Couldn’t save “Note”", cx));
    cx.executor().advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    assert_eq!(shown_notices(cx).len(), 1);
    cx.update(|_, cx| notices::dismiss(id, cx));
    assert!(shown_notices(cx).is_empty());
}

#[gpui::test]
fn opening_a_search_result_closes_search_over_the_match(cx: &mut TestAppContext) {
    let vault = vault_with(&[
        ("Start.md", "nothing here"),
        ("Optics.md", "lenses bend light through a prism"),
    ]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open(&workspace, cx, "Start.md");
    press(cx, "search.open");
    cx.simulate_input("prism");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let search_open = cx.read(|cx| workspace.read(cx).active_modal::<VaultSearch>().is_some());
    assert!(!search_open, "search should close once a result opens");
    assert_eq!(
        active_text(&workspace, cx).as_deref(),
        Some("lenses bend light through a prism")
    );
}
