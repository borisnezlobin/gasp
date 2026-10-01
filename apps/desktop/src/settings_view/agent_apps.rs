//! The General page's AI app rows, under Agent access: one per AI app on
//! this Mac, each with a button that sets the app up to start `gasp mcp`
//! for this vault. The files are read and written, and Claude Code's
//! command run, off the main thread.
//!
//! `gasp mcp` runs without the app, so the rows connect whether or not
//! `mcp.enabled` is on; that setting only starts the running app's bridge.

use std::collections::HashSet;
use std::path::PathBuf;

use gasp_mcp::clients::{ClientApp, ClientError, ClientHome, Connection, ServerLaunch};
use gpui::{AnyElement, App, ClickEvent, Context, Global, SharedString, div, prelude::*};

use super::controls::{button, inert_button, widest_element_of};
use super::view::{ControlRow, SettingsView};
use crate::icons::{IconName, icon};
use crate::ui::Selectable;

/// The line shown in place of the rows when no AI app is installed.
pub const NO_AGENT_APPS: &str =
    "Gasp can connect Claude, Claude Code, Cursor and Codex once one of them is installed.";

const CONNECT: &str = "Connect";
const UPDATE: &str = "Update";
const CONNECTED: &str = "Connected";
const CONNECTING: &str = "Connecting…";
const RESTART_CLAUDE: &str = "Restart Claude to use it.";
const STALE: &str = "It’s set up for another vault or an older copy of Gasp.";

/// What pressing Connect does, for each app.
pub fn agent_app_description(app: ClientApp) -> &'static str {
    match app {
        ClientApp::ClaudeDesktop => "Lets Claude read and edit this vault’s notes.",
        ClientApp::ClaudeCode => "Lets Claude Code read and edit this vault’s notes.",
        ClientApp::Cursor => "Lets Cursor’s agent read and edit this vault’s notes.",
        ClientApp::Codex => "Lets Codex read and edit this vault’s notes.",
    }
}

/// Why Gasp won't touch the app's settings file.
fn unreadable_message(app: ClientApp) -> &'static str {
    match app {
        ClientApp::Codex => {
            "Codex’s settings file isn’t valid TOML, so Gasp left it alone. Fix the file, then reopen Settings."
        }
        ClientApp::Cursor => {
            "Cursor’s settings file isn’t valid JSON, so Gasp left it alone. Fix the file, then reopen Settings."
        }
        _ => {
            "Claude’s settings file isn’t valid JSON, so Gasp left it alone. Fix the file, then reopen Settings."
        }
    }
}

fn write_failed_message(app: ClientApp) -> &'static str {
    match app {
        ClientApp::Codex => {
            "Couldn’t write Codex’s settings file. Check that you can edit ~/.codex/config.toml, then try again."
        }
        ClientApp::Cursor => {
            "Couldn’t write Cursor’s settings file. Check that you can edit ~/.cursor/mcp.json, then try again."
        }
        _ => {
            "Couldn’t write Claude’s settings file. Check that Claude isn’t open in a way that locks it, then try again."
        }
    }
}

/// What a failed connect says, in the note under the row's button.
pub fn agent_app_error(app: ClientApp, error: ClientError) -> &'static str {
    match error {
        ClientError::Unreadable => unreadable_message(app),
        ClientError::WriteFailed => write_failed_message(app),
        ClientError::CliMissing => {
            "Gasp couldn’t find Claude Code’s claude command. Reinstall Claude Code, then try again."
        }
        ClientError::CliFailed => {
            "Claude Code didn’t add Gasp. Check that Claude Code is up to date and signed in, then try again."
        }
    }
}

/// The key a row's connect error is kept under.
pub(super) fn error_key(app: ClientApp) -> String {
    format!("agent-app.{}", app.id())
}

/// One app's row: what its config says, while known, and whether it's
/// being connected now.
#[derive(Clone, Debug)]
pub(super) struct AgentAppRow {
    pub app: ClientApp,
    /// `None` while its config is still being read.
    pub connection: Option<Connection>,
    pub connecting: bool,
}

/// The AI apps the General page lists, and how to reach them.
pub(super) struct AgentApps {
    home: ClientHome,
    launch: ServerLaunch,
    pub rows: Vec<AgentAppRow>,
}

/// The apps Gasp connected while it has been running. Claude reads its
/// servers only when it starts, so it needs a restart after one of these.
#[derive(Default)]
struct ConnectedThisRun(HashSet<ClientApp>);

impl Global for ConnectedThisRun {}

fn connected_this_run(app: ClientApp, cx: &App) -> bool {
    cx.try_global::<ConnectedThisRun>()
        .is_some_and(|connected| connected.0.contains(&app))
}

/// What the background check found: the apps installed, with the login
/// shell's folders searched too, and each one's connection.
struct Checked {
    home: ClientHome,
    connections: Vec<(ClientApp, Connection)>,
}

/// Finds apps the quick look missed by asking a login shell for its
/// `PATH`, then reads every installed app's config.
fn check_apps(mut home: ClientHome, launch: &ServerLaunch) -> Checked {
    home.search_login_shell();
    let connections = ClientApp::installed(&home)
        .into_iter()
        .map(|app| (app, app.connection(&home, launch)))
        .collect();
    Checked { home, connections }
}

impl SettingsView {
    /// Lists the AI apps installed under `home` on the General page, with
    /// buttons that point them at `binary mcp <this vault>`. Until this is
    /// called the page shows no AI apps; tests pass a temporary home.
    pub fn set_agent_apps(&mut self, home: ClientHome, binary: PathBuf, cx: &mut Context<Self>) {
        let vault = std::path::absolute(&self.vault_root).unwrap_or(self.vault_root.clone());
        let launch = ServerLaunch::new(&binary, &vault);
        let rows = ClientApp::installed(&home)
            .into_iter()
            .map(|app| AgentAppRow {
                app,
                connection: None,
                connecting: false,
            })
            .collect();
        self.agent_apps = Some(AgentApps {
            home: home.clone(),
            launch: launch.clone(),
            rows,
        });
        self.invalidate_layouts();
        let checking = cx.background_spawn(async move { check_apps(home, &launch) });
        cx.spawn(async move |view, cx| {
            let checked = checking.await;
            view.update(cx, |view, cx| view.finish_check(checked, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_check(&mut self, checked: Checked, cx: &mut Context<Self>) {
        let Some(apps) = self.agent_apps.as_mut() else {
            return;
        };
        let before: Vec<ClientApp> = apps.rows.iter().map(|row| row.app).collect();
        apps.home = checked.home;
        apps.rows = checked
            .connections
            .into_iter()
            .map(|(app, connection)| AgentAppRow {
                app,
                connection: Some(connection),
                connecting: false,
            })
            .collect();
        let after: Vec<ClientApp> = apps.rows.iter().map(|row| row.app).collect();
        if before != after {
            self.invalidate_layouts();
        }
        cx.notify();
    }

    /// The rows for the AI apps: one per installed app, or the line that
    /// says which apps Gasp can connect.
    pub(super) fn agent_app_rows(&self) -> Vec<ControlRow> {
        let Some(apps) = self.agent_apps.as_ref() else {
            return Vec::new();
        };
        if apps.rows.is_empty() {
            return vec![ControlRow::NoAgentApps];
        }
        apps.rows
            .iter()
            .map(|row| ControlRow::AgentApp(row.app))
            .collect()
    }

    fn agent_app_row(&self, app: ClientApp) -> Option<&AgentAppRow> {
        self.agent_apps
            .as_ref()?
            .rows
            .iter()
            .find(|row| row.app == app)
    }

    /// What `app`'s config says, once it has been read.
    pub fn agent_app_connection(&self, app: ClientApp) -> Option<Connection> {
        self.agent_app_row(app)?.connection
    }

    /// Whether pressing the row's button would connect the app now.
    fn can_connect(&self, app: ClientApp) -> bool {
        self.agent_app_row(app).is_some_and(|row| {
            !row.connecting
                && matches!(
                    row.connection,
                    Some(Connection::NotConnected | Connection::Stale)
                )
        })
    }

    /// Sets `app` up to start Gasp's server for this vault, off the main
    /// thread, then reads its config again.
    pub fn connect_agent_app(&mut self, app: ClientApp, cx: &mut Context<Self>) {
        if !self.can_connect(app) {
            return;
        }
        let Some(apps) = self.agent_apps.as_mut() else {
            return;
        };
        let (home, launch) = (apps.home.clone(), apps.launch.clone());
        if let Some(row) = apps.rows.iter_mut().find(|row| row.app == app) {
            row.connecting = true;
        }
        self.error = None;
        let connecting = cx.background_spawn(async move {
            let result = app.connect(&home, &launch);
            (result, app.connection(&home, &launch))
        });
        cx.spawn(async move |view, cx| {
            let (result, connection) = connecting.await;
            view.update(cx, |view, cx| {
                view.finish_connect(app, result, connection, cx)
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_connect(
        &mut self,
        app: ClientApp,
        result: Result<(), ClientError>,
        connection: Connection,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self
            .agent_apps
            .as_mut()
            .and_then(|apps| apps.rows.iter_mut().find(|row| row.app == app))
        {
            row.connecting = false;
            row.connection = Some(connection);
        }
        match result {
            Ok(()) => {
                cx.default_global::<ConnectedThisRun>().0.insert(app);
            }
            Err(error) => {
                self.error = Some((error_key(app), agent_app_error(app, error).to_string()));
            }
        }
        cx.notify();
    }

    /// Lasting notes under an app's description: why Gasp left its file
    /// alone, that it points elsewhere, or that Claude needs a restart.
    pub(super) fn agent_app_notes(&self, app: ClientApp, cx: &App) -> Vec<AnyElement> {
        let muted = |text: &'static str| {
            div()
                .text_color(self.style.text_muted)
                .child(text)
                .into_any_element()
        };
        let note = match self.agent_app_connection(app) {
            Some(Connection::Unreadable) => {
                Some(div().child(unreadable_message(app)).into_any_element())
            }
            Some(Connection::Stale) => Some(muted(STALE)),
            Some(Connection::Connected)
                if app == ClientApp::ClaudeDesktop && connected_this_run(app, cx) =>
            {
                Some(muted(RESTART_CLAUDE))
            }
            _ => None,
        };
        note.into_iter().collect()
    }

    /// The row's button, or a check once connected. It's as wide as the
    /// widest of these, so a change of state moves nothing.
    pub(super) fn agent_app_control(
        &self,
        app: ClientApp,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = self.agent_app_row(app);
        let connecting = row.is_some_and(|row| row.connecting);
        let shown = match row.and_then(|row| row.connection) {
            _ if connecting => {
                inert_button("agent-app-busy", CONNECTING, &self.style).into_any_element()
            }
            Some(Connection::Connected) => self
                .connected_mark()
                .selector(move || format!("connected-{}", app.id()))
                .into_any_element(),
            Some(Connection::Unreadable) => {
                inert_button("agent-app-refused", CONNECT, &self.style).into_any_element()
            }
            Some(connection) => self.connect_button(app, connection, focused, cx),
            None => div().into_any_element(),
        };
        let widest = [CONNECT, UPDATE, CONNECTING]
            .map(|label| {
                let id = SharedString::from(format!("agent-app-sizer-{label}"));
                button(id, label, false, false, &self.style).into_any_element()
            })
            .into_iter()
            .chain(std::iter::once(self.connected_mark().into_any_element()));
        widest_element_of(div().flex().justify_end().child(shown), widest).into_any_element()
    }

    fn connect_button(
        &self,
        app: ClientApp,
        connection: Connection,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if connection == Connection::Stale {
            UPDATE
        } else {
            CONNECT
        };
        let selector = format!("connect-{}", app.id());
        button(
            SharedString::from(selector.clone()),
            label,
            false,
            focused,
            &self.style,
        )
        .selector(|| selector)
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.connect_agent_app(app, cx)))
        .into_any_element()
    }

    fn connected_mark(&self) -> gpui::Div {
        let style = &self.style;
        div()
            .h(style.control_height)
            .flex()
            .items_center()
            .gap(style.gap_sm)
            .whitespace_nowrap()
            .text_color(style.text_muted)
            .child(
                icon(IconName::Check)
                    .flex_none()
                    .size(style.small_icon_size)
                    .text_color(style.accent),
            )
            .child(CONNECTED)
    }

    /// Space or Enter presses the row's Connect or Update.
    pub(super) fn agent_app_key(
        &mut self,
        app: ClientApp,
        key: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let presses = matches!(key, "space" | "enter") && self.can_connect(app);
        if presses {
            self.connect_agent_app(app, cx);
        }
        presses
    }
}
