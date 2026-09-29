//! The app's end of the bridge `gasp mcp` talks to: what the window
//! shows, running commands, opening notes, and changing notes that have
//! unsaved edits so the change is one undoable edit.
//!
//! `editor_mcp::bridge::listener` blocks in `accept` on its own thread
//! and hands each request here over a channel; the answer is worked out
//! on the main thread, between frames, and sent back. Nothing runs while
//! no agent asks, and nothing starts until the first frame is on screen.
//! The `mcp.enabled` setting starts and stops it.

use std::path::Path;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use editor_config::config_files::known_commands;
use editor_mcp::bridge::endpoint::Endpoint;
use editor_mcp::bridge::listener::BridgeListener;
use editor_mcp::bridge::{Buffer, CursorState, EditorState, PaneState, Request, TabState};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{AnyWindowHandle, App, AppContext, Context, Entity, Task, Window};
use serde_json::{Value, json};

use super::{OpenIn, Pane, Workspace};
use crate::editor::EditorView;
use crate::keymap::RunCommand;

/// How long the bridge's thread waits for the main thread's answer. The
/// server gives up a little later.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(25);

type Answer = Result<Value, String>;

/// A request waiting for the main thread.
struct Job {
    request: Request,
    reply: SyncSender<Answer>,
}

/// The listening bridge and the task answering it.
struct Running {
    _listener: BridgeListener,
    _answering: Task<()>,
}

/// Whether the bridge runs, and the window it answers for.
#[derive(Default)]
pub(crate) struct McpBridge {
    window: Option<AnyWindowHandle>,
    /// Where to listen, when not the vault's usual endpoint (tests).
    endpoint: Option<Endpoint>,
    running: Option<Running>,
}

impl Workspace {
    /// Starts answering `gasp mcp` once the first frame is on screen,
    /// if the `mcp.enabled` setting is on.
    pub fn start_mcp_bridge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mcp.window = Some(window.window_handle());
        if crate::first_frame::is_waiting() {
            let this = cx.weak_entity();
            crate::first_frame::defer(move |cx| {
                this.update(cx, |workspace, cx| workspace.sync_mcp_bridge(cx))
                    .ok();
            });
            return;
        }
        self.sync_mcp_bridge(cx);
    }

    /// Like [`Workspace::start_mcp_bridge`], listening at `endpoint`.
    pub fn start_mcp_bridge_at(
        &mut self,
        endpoint: Endpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mcp.endpoint = Some(endpoint);
        self.start_mcp_bridge(window, cx);
    }

    /// Whether the bridge is listening.
    pub fn mcp_bridge_running(&self) -> bool {
        self.mcp.running.is_some()
    }

    /// Starts or stops the bridge to match the `mcp.enabled` setting.
    pub(crate) fn sync_mcp_bridge(&mut self, cx: &mut Context<Self>) {
        let enabled = self.config.settings.mcp.enabled;
        if !enabled {
            self.mcp.running = None;
            return;
        }
        let Some(window) = self.mcp.window else {
            return;
        };
        if self.mcp.running.is_none() {
            self.mcp.running = self.listen(window, cx);
        }
    }

    fn listen(&mut self, window: AnyWindowHandle, cx: &mut Context<Self>) -> Option<Running> {
        let vault = std::fs::canonicalize(&self.vault).unwrap_or_else(|_| self.vault.clone());
        let endpoint = self
            .mcp
            .endpoint
            .clone()
            .or_else(|| Endpoint::for_vault(&vault))?;
        let (sender, mut jobs) = unbounded::<Job>();
        let listener = match BridgeListener::start(endpoint, move |request| ask(&sender, request)) {
            Ok(listener) => listener,
            Err(error) => {
                // Such as a second window on the same vault: the first answers.
                eprintln!("gasp mcp bridge not started: {error}");
                return None;
            }
        };
        let answering = cx.spawn(async move |workspace, cx| {
            while let Some(job) = jobs.next().await {
                let answer = cx.update_window(window, |_, window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.answer_mcp(job.request, window, cx)
                    })
                });
                let answer = match answer {
                    Ok(Ok(answer)) => answer,
                    _ => Err("the window is closing".to_string()),
                };
                job.reply.send(answer).ok();
            }
        });
        Some(Running {
            _listener: listener,
            _answering: answering,
        })
    }

    /// Answers one request from `gasp mcp`.
    pub fn answer_mcp(
        &mut self,
        request: Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Answer {
        match request {
            Request::State => Ok(json!(self.editor_state(cx))),
            Request::RunCommand { id } => self.run_command_for_mcp(&id, window, cx),
            Request::OpenNote { path, line } => self.open_note_for_mcp(&path, line, window, cx),
            Request::Buffer { path } => Ok(json!(self.buffer(&path, cx))),
            Request::Edit { path, old, new } => self.edit_for_mcp(&path, &old, &new, cx),
            Request::Move { from, to } => self.move_for_mcp(&from, &to, cx),
        }
    }

    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.vault)
            .map(editor_vault::ops::slash_path)
            .unwrap_or_default()
    }

    fn editor_state(&self, cx: &App) -> EditorState {
        let panes = self.panes();
        let active_pane = panes
            .iter()
            .position(|pane| *pane == self.active_pane)
            .unwrap_or(0);
        let editor = self.active_editor(cx);
        EditorState {
            vault: self.vault.to_string_lossy().into_owned(),
            panes: panes.iter().map(|pane| self.pane_state(pane, cx)).collect(),
            active_pane,
            active_note: self.active_path(cx).map(|path| self.relative_path(&path)),
            cursor: editor.map(|editor| cursor_state(&editor, cx)),
        }
    }

    fn pane_state(&self, pane: &Entity<Pane>, cx: &App) -> PaneState {
        let pane = pane.read(cx);
        let tabs = pane
            .tabs()
            .iter()
            .map(|tab| TabState {
                path: tab
                    .path(cx)
                    .map(|path| self.relative_path(path))
                    .unwrap_or_default(),
                title: tab.title(cx),
                dirty: tab.note().is_some_and(|note| note.doc.read(cx).is_dirty()),
            })
            .collect();
        PaneState {
            tabs,
            active_tab: (!pane.is_empty()).then(|| pane.active_index()),
        }
    }

    fn run_command_for_mcp(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Answer {
        if self.run_command(id, window, cx) {
            return Ok(Value::Null);
        }
        if !known_commands().contains(&id) {
            return Err(format!("there's no command {id:?}"));
        }
        // An editing command goes to the focused note, as from the palette.
        self.focus_active(window, cx);
        window.dispatch_action(
            Box::new(RunCommand {
                id: id.to_string().into(),
            }),
            cx,
        );
        Ok(Value::Null)
    }

    fn open_note_for_mcp(
        &mut self,
        path: &str,
        line: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Answer {
        let absolute = self.vault.join(path);
        self.open_path(&absolute, OpenIn::NewTab, window, cx)
            .map_err(|error| format!("couldn't open {path}: {error}"))?;
        self.focus_active(window, cx);
        let (Some(line), Some(editor)) = (line, self.active_editor(cx)) else {
            return Ok(json!({ "line": null }));
        };
        let offset = editor.update(cx, |editor, cx| {
            let doc = editor.doc();
            let last = doc.line_count().saturating_sub(1);
            let offset = doc.line_start(line.saturating_sub(1).min(last));
            editor.select(offset, offset, cx);
            offset
        });
        Ok(json!({ "line": line, "offset": offset }))
    }

    fn buffer(&self, path: &str, cx: &App) -> Buffer {
        let Some(doc) = self.doc_for_path(&self.vault.join(path), cx) else {
            return Buffer::default();
        };
        let doc = doc.read(cx);
        Buffer {
            open: doc.has_editors(),
            dirty: doc.is_dirty(),
            text: Some(doc.current_text(cx)),
        }
    }

    /// Replaces an open note's text as one undoable edit, if it still
    /// reads `old`, then saves it.
    fn edit_for_mcp(&mut self, path: &str, old: &str, new: &str, cx: &mut Context<Self>) -> Answer {
        let absolute = self.vault.join(path);
        let doc = self
            .doc_for_path(&absolute, cx)
            .ok_or_else(|| format!("{path} isn't open any more; try again"))?;
        if doc.read(cx).current_text(cx) != old {
            return Err(format!(
                "{path} changed in the editor since it was read; read it and try again"
            ));
        }
        crate::knowledge::edit::edit_note(self, &absolute, |_| Some(new.to_string()), cx)
            .map_err(|error| format!("couldn't change {path}: {error}"))?;
        doc.update(cx, |doc, cx| doc.save_or_log(cx));
        Ok(Value::Null)
    }

    /// Moves a note or file as the file tree does: open notes follow it
    /// and links change in their editors.
    fn move_for_mcp(&mut self, from: &str, to: &str, cx: &mut Context<Self>) -> Answer {
        let (old, new) = (self.vault.join(from), self.vault.join(to));
        for doc in self.docs.clone() {
            if doc.read(cx).path().starts_with(&old) {
                doc.update(cx, |doc, cx| doc.save_or_log(cx));
            }
        }
        editor_vault::ops::rename(&self.vault, Path::new(from), Path::new(to), false)
            .map_err(|error| format!("couldn't move {from}: {error}"))?;
        let updated: Vec<String> = self
            .entry_moved(&old, &new, cx)
            .iter()
            .map(|path| self.relative_path(path))
            .collect();
        Ok(json!({
            "from": from,
            "to": to,
            "links_updated": self.config.settings.files.update_links_on_rename,
            "updated_notes": updated,
        }))
    }
}

/// Hands a request to the main thread and waits for its answer. Runs on
/// the bridge's thread.
fn ask(jobs: &UnboundedSender<Job>, request: Request) -> Answer {
    let (reply, answer) = sync_channel(1);
    jobs.unbounded_send(Job { request, reply })
        .map_err(|_| "the window is closing".to_string())?;
    answer
        .recv_timeout(ANSWER_TIMEOUT)
        .map_err(|_| "the app didn't answer in time".to_string())?
}

fn cursor_state(editor: &Entity<EditorView>, cx: &App) -> CursorState {
    let editor = editor.read(cx);
    let (offset, anchor) = (editor.cursor(), editor.anchor());
    let doc = editor.doc();
    let line = doc.line_of_offset(offset);
    let text = editor.text();
    let line_start = doc.line_start(line).min(offset);
    let selected = anchor.min(offset)..anchor.max(offset);
    CursorState {
        offset,
        line: line + 1,
        column: text
            .get(line_start..offset)
            .map_or(0, |part| part.chars().count())
            + 1,
        anchor,
        selection: text.get(selected).unwrap_or_default().to_string(),
    }
}
