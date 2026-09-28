//! The server talking to a fake app over the real bridge: requests carry
//! the token, notes with unsaved edits change through the app, and the
//! tools say when there's no app.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use editor_mcp::Context;
use editor_mcp::bridge::endpoint::{Endpoint, EndpointInfo};
use editor_mcp::bridge::listener::BridgeListener;
use editor_mcp::bridge::{Buffer, CursorState, EditorState, PaneState, Request, TabState};
use editor_mcp::tool::Output;
use serde_json::{Value, json};

/// What the fake app has open: path → (text, unsaved).
type Open = Arc<Mutex<HashMap<String, (String, bool)>>>;

struct FakeApp {
    open: Open,
    requests: Arc<Mutex<Vec<Request>>>,
    listener: BridgeListener,
}

fn answer(open: &Open, request: Request) -> Result<Value, String> {
    let mut open = open.lock().unwrap();
    match request {
        Request::State => Ok(serde_json::to_value(state()).unwrap()),
        Request::Buffer { path } => {
            let buffer = open
                .get(&path)
                .map_or_else(Buffer::default, |(text, dirty)| Buffer {
                    open: true,
                    dirty: *dirty,
                    text: Some(text.clone()),
                });
            Ok(serde_json::to_value(buffer).unwrap())
        }
        Request::Edit { path, old, new } => {
            let (text, dirty) = open.get_mut(&path).ok_or("not open")?;
            if *text != old {
                return Err("the note changed in the editor".into());
            }
            *text = new;
            *dirty = false;
            Ok(Value::Null)
        }
        Request::RunCommand { id } if id == "tab.new" => Ok(Value::Null),
        Request::RunCommand { id } => Err(format!("no command {id}")),
        Request::OpenNote { .. } => Ok(json!({"pane": 0})),
        Request::Move { .. } => Ok(json!({"updated_notes": []})),
    }
}

fn state() -> EditorState {
    EditorState {
        vault: "/vault".into(),
        panes: vec![PaneState {
            tabs: vec![TabState {
                path: "Open.md".into(),
                title: "Open".into(),
                dirty: true,
            }],
            active_tab: Some(0),
        }],
        active_pane: 0,
        active_note: Some("Open.md".into()),
        cursor: Some(CursorState {
            offset: 2,
            line: 1,
            column: 3,
            anchor: 2,
            selection: String::new(),
        }),
    }
}

fn start_app(endpoint: Endpoint) -> FakeApp {
    let open: Open = Arc::default();
    let requests: Arc<Mutex<Vec<Request>>> = Arc::default();
    let (seen, answering) = (requests.clone(), open.clone());
    let listener = BridgeListener::start(endpoint, move |request| {
        seen.lock().unwrap().push(request.clone());
        answer(&answering, request)
    })
    .unwrap();
    FakeApp {
        open,
        requests,
        listener,
    }
}

fn setup() -> (tempfile::TempDir, tempfile::TempDir, Endpoint, Context) {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Open.md"), "saved text\n").unwrap();
    std::fs::write(vault.path().join("Closed.md"), "closed\n").unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::in_dir(&runtime.path().join("mcp"), vault.path());
    let context = Context::with_endpoint(vault.path(), Some(endpoint.clone())).unwrap();
    (vault, runtime, endpoint, context)
}

fn call(context: &Context, name: &str, arguments: Value) -> Result<Output, String> {
    let tools = editor_mcp::tools::all();
    let tool = tools.iter().find(|tool| tool.name == name).unwrap();
    tool.call(context, arguments)
        .map_err(|error| error.message().to_string())
}

fn json_of(output: Output) -> Value {
    match output {
        Output::Json(value) => value,
        other => panic!("expected JSON, got {other:?}"),
    }
}

#[test]
fn state_commands_and_opening_go_through_the_app() {
    let (_vault, _runtime, endpoint, context) = setup();
    let app = start_app(endpoint);
    let state = json_of(call(&context, "editor_state", json!({})).unwrap());
    assert_eq!(state["active_note"], "Open.md");
    assert_eq!(state["cursor"]["column"], 3);
    call(&context, "run_command", json!({"id": "tab.new"})).unwrap();
    let error = call(&context, "run_command", json!({"id": "pane.close"})).unwrap_err();
    assert!(
        error.contains("The app couldn't do it: no command"),
        "{error}"
    );
    call(&context, "open_note", json!({"path": "Closed", "line": 4})).unwrap();
    let requests = app.requests.lock().unwrap().clone();
    assert!(requests.contains(&Request::OpenNote {
        path: "Closed.md".into(),
        line: Some(4),
    }));
}

#[test]
fn unsaved_notes_change_in_the_app_and_others_on_disk() {
    let (vault, _runtime, endpoint, context) = setup();
    let app = start_app(endpoint);
    app.open
        .lock()
        .unwrap()
        .insert("Open.md".into(), ("unsaved text\n".into(), true));

    // Reading sees the editor's text, not the file's.
    let read = json_of(call(&context, "read_note", json!({"path": "Open"})).unwrap());
    assert_eq!(
        (read["text"].as_str(), read["source"].as_str()),
        (Some("unsaved text\n"), Some("app"))
    );

    let said = call(
        &context,
        "patch_note",
        json!({"path": "Open", "operation": "replace", "find": "unsaved", "content": "patched"}),
    )
    .unwrap();
    assert!(
        matches!(said, Output::Text(ref text) if text.contains("open editor")),
        "{said:?}"
    );
    assert_eq!(app.open.lock().unwrap()["Open.md"].0, "patched text\n");
    // The file is the app's to save.
    let on_disk = std::fs::read_to_string(vault.path().join("Open.md")).unwrap();
    assert_eq!(on_disk, "saved text\n");

    // A note without unsaved edits is written to its file.
    call(
        &context,
        "write_note",
        json!({"path": "Closed", "text": "new\n"}),
    )
    .unwrap();
    let closed = std::fs::read_to_string(vault.path().join("Closed.md")).unwrap();
    assert_eq!(closed, "new\n");

    // Deleting a note with unsaved edits is refused.
    app.open.lock().unwrap().get_mut("Open.md").unwrap().1 = true;
    let error = call(&context, "delete_note", json!({"path": "Open"})).unwrap_err();
    assert!(error.contains("unsaved edits"), "{error}");
}

#[test]
fn a_wrong_token_gets_no_answer() {
    let (_vault, _runtime, endpoint, context) = setup();
    let app = start_app(endpoint.clone());
    endpoint
        .write_info(&EndpointInfo {
            token: "0".repeat(64),
            port: None,
        })
        .unwrap();
    let error = call(&context, "editor_state", json!({})).unwrap_err();
    assert!(error.contains("without answering"), "{error}");
    assert!(app.requests.lock().unwrap().is_empty());
}

#[test]
fn without_the_app_tools_say_so_and_files_still_work() {
    let (vault, _runtime, endpoint, context) = setup();
    let app = start_app(endpoint.clone());
    drop(app.listener);
    assert!(!endpoint.info_path().exists());
    let error = call(&context, "editor_state", json!({})).unwrap_err();
    assert!(error.contains("isn't running"), "{error}");
    call(&context, "write_note", json!({"path": "Open", "text": "x"})).unwrap();
    assert_eq!(
        std::fs::read_to_string(vault.path().join("Open.md")).unwrap(),
        "x"
    );
}

#[cfg(unix)]
#[test]
fn a_second_app_on_the_same_vault_is_refused() {
    let (_vault, _runtime, endpoint, _context) = setup();
    let _first = start_app(endpoint.clone());
    let second = BridgeListener::start(endpoint, |_| Ok(Value::Null));
    assert!(second.is_err());
}
