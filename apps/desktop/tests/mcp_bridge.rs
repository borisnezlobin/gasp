//! The app's end of `editor mcp`: what it reports about the window, and
//! a patch to a note with unsaved edits landing in its editor as one
//! undoable edit, both called directly and through the real socket.

use std::path::Path;

use editor_desktop::actions::bind_keys;
use editor_desktop::features;
use editor_desktop::workspace::{OpenIn, Workspace};
use editor_mcp::bridge::Request;
use editor_mcp::bridge::endpoint::Endpoint;
use editor_mcp::tool::Output;
use gpui::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

fn vault_with(notes: &[(&str, &str)]) -> tempfile::TempDir {
    let vault = tempfile::tempdir().unwrap();
    for (name, text) in notes {
        std::fs::write(vault.path().join(name), text).unwrap();
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

/// Opens `name` and types in front of its text, leaving unsaved edits.
fn open_and_edit(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, name: &str) {
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            let path = workspace.vault().join(name);
            workspace
                .open_path(&path, OpenIn::ActiveTab, window, cx)
                .unwrap();
        })
    });
    cx.run_until_parked();
    cx.simulate_input("Draft. ");
    cx.run_until_parked();
}

fn ask(workspace: &Entity<Workspace>, cx: &mut VisualTestContext, request: Request) -> Value {
    let answer = cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.answer_mcp(request, window, cx)
        })
    });
    cx.run_until_parked();
    answer.unwrap()
}

fn editor_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.read(|cx| {
        let editor = workspace.read(cx).active_editor(cx).unwrap();
        editor.read(cx).text()
    })
}

#[gpui::test]
fn a_patch_to_an_edited_note_is_one_undo_step(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Plan.md", "# Plan\n\nOne.\n")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    open_and_edit(&workspace, cx, "Plan.md");
    let edited = editor_text(&workspace, cx);
    assert_eq!(edited, "Draft. # Plan\n\nOne.\n");

    let buffer = ask(
        &workspace,
        cx,
        Request::Buffer {
            path: "Plan.md".into(),
        },
    );
    assert_eq!(buffer, json!({"open": true, "dirty": true, "text": edited}));

    let patched = "Draft. # Plan\n\nOne.\nTwo.\n".to_string();
    ask(
        &workspace,
        cx,
        Request::Edit {
            path: "Plan.md".into(),
            old: edited.clone(),
            new: patched.clone(),
        },
    );
    assert_eq!(editor_text(&workspace, cx), patched);
    // It's saved, typing and patch together.
    let on_disk = std::fs::read_to_string(vault.path().join("Plan.md")).unwrap();
    assert_eq!(on_disk, patched);

    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| editor.undo(cx));
    assert_eq!(editor_text(&workspace, cx), edited);

    // A patch made against text the editor no longer has is refused.
    let stale = cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            let request = Request::Edit {
                path: "Plan.md".into(),
                old: patched.clone(),
                new: "x".into(),
            };
            workspace.answer_mcp(request, window, cx)
        })
    });
    assert!(stale.unwrap_err().contains("changed in the editor"));
}

#[gpui::test]
fn the_state_shows_tabs_and_the_cursor(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Plan.md", "one\ntwo\nthree\n"), ("Other.md", "x")]);
    let (workspace, cx) = open_workspace(cx, vault.path());
    let opened = ask(
        &workspace,
        cx,
        Request::OpenNote {
            path: "Plan.md".into(),
            line: Some(3),
        },
    );
    assert_eq!(opened["offset"], 8);
    let state = ask(&workspace, cx, Request::State);
    assert_eq!(state["active_note"], "Plan.md");
    assert_eq!(state["cursor"]["line"], 3);
    assert_eq!(state["cursor"]["column"], 1);
    let tabs = state["panes"][0]["tabs"].as_array().unwrap();
    assert!(tabs.iter().any(|tab| tab["path"] == "Plan.md"), "{state}");

    ask(
        &workspace,
        cx,
        Request::RunCommand {
            id: "pane.split-right".into(),
        },
    );
    let state = ask(&workspace, cx, Request::State);
    assert_eq!(state["panes"].as_array().unwrap().len(), 2);
    let unknown = cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.answer_mcp(
                Request::RunCommand {
                    id: "no.such".into(),
                },
                window,
                cx,
            )
        })
    });
    assert!(unknown.is_err());
}

#[gpui::test]
fn the_server_patches_through_the_socket(cx: &mut TestAppContext) {
    let vault = vault_with(&[("Plan.md", "# Plan\n\nOne.\n")]);
    let runtime = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::in_dir(&runtime.path().join("mcp"), vault.path());
    let (workspace, cx) = open_workspace(cx, vault.path());
    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.start_mcp_bridge_at(endpoint.clone(), window, cx)
        })
    });
    assert!(cx.read(|cx| workspace.read(cx).mcp_bridge_running()));
    open_and_edit(&workspace, cx, "Plan.md");

    // The server side runs on its own thread, as `editor mcp` would;
    // the test keeps the app's main thread turning until it's done.
    let root = vault.path().to_path_buf();
    let server = std::thread::spawn(move || {
        let context = editor_mcp::Context::with_endpoint(&root, Some(endpoint)).unwrap();
        let tools = editor_mcp::tools::all();
        let patch = tools.iter().find(|tool| tool.name == "patch_note").unwrap();
        let arguments = json!({"path": "Plan", "operation": "append", "content": "Two."});
        patch.call(&context, arguments)
    });
    while !server.is_finished() {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let said = server.join().unwrap().unwrap();
    assert!(
        matches!(&said, Output::Text(text) if text.contains("open editor")),
        "{said:?}"
    );
    assert_eq!(editor_text(&workspace, cx), "Draft. # Plan\n\nOne.\nTwo.\n");
    let editor = cx.read(|cx| workspace.read(cx).active_editor(cx).unwrap());
    editor.update(cx, |editor, cx| editor.undo(cx));
    assert_eq!(editor_text(&workspace, cx), "Draft. # Plan\n\nOne.\n");
}
