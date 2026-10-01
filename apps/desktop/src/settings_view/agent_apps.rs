//! The General page's AI apps, under Agent access: one tile per AI app on
//! this Mac, side by side on one row, each a button that sets the app up
//! to start `gasp mcp` for this vault. The files are read and written,
//! Claude Code's command run and the apps' icons read off the main thread.
//!
//! `gasp mcp` runs without the app, so the tiles connect whether or not
//! `mcp.enabled` is on; that setting only starts the running app's bridge.

use std::collections::HashSet;
use std::path::PathBuf;

use gasp_mcp::clients::{ClientApp, ClientError, ClientHome, Connection, ServerLaunch};
use gpui::{App, Context, Global, prelude::*};

use super::model::words_match;
use super::view::{ControlRow, SettingsView};

/// The line shown in place of the tiles when no AI app is installed.
pub const NO_AGENT_APPS: &str =
    "Gasp can connect Claude, Claude Code, Cursor and Codex once one of them is installed.";

/// The line above the tiles, said once for all of them.
pub const AGENT_APPS_LINE: &str = "Connect an AI app so it can read and edit this vault’s notes.";

/// The title the tiles' row is searched and announced by.
pub const AGENT_APPS_TITLE: &str = "AI apps";

/// The key a failed connect's message, or why Gasp won't touch an app's
/// file, is kept under.
pub const AGENT_APPS_ERROR_KEY: &str = "agent-apps";

const STALE: &str = "It’s set up for another vault or an older copy of Gasp.";

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

/// What a failed connect says, in the note under the tiles.
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

/// Where an app's tile has got to, which its look and its one line of
/// status follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileState {
    /// Its config is still being read.
    Checking,
    NotConnected,
    /// It starts Gasp for another vault or an older copy of Gasp.
    Stale,
    Connecting,
    Connected,
    /// Connected while Gasp has been running, and Claude reads its
    /// servers only when it starts.
    NeedsRestart,
    /// Its settings file can't be read, so Gasp leaves it alone.
    Unreadable,
}

impl TileState {
    /// Every state an app's tile can be in, so the tile can be as wide
    /// as the widest of them.
    pub(super) fn all_for(app: ClientApp) -> Vec<TileState> {
        let mut states = vec![
            TileState::Checking,
            TileState::NotConnected,
            TileState::Stale,
            TileState::Connecting,
            TileState::Connected,
            TileState::Unreadable,
        ];
        if app == ClientApp::ClaudeDesktop {
            states.push(TileState::NeedsRestart);
        }
        states
    }

    /// The short line under the app's name.
    pub(super) fn status(self) -> &'static str {
        match self {
            TileState::Checking => "Checking…",
            TileState::NotConnected => "Connect",
            TileState::Stale => "Update",
            TileState::Connecting => "Connecting…",
            TileState::Connected => "Connected",
            TileState::NeedsRestart => "Restart to use it",
            TileState::Unreadable => "Can’t read settings",
        }
    }

    /// Whether pressing the tile connects the app.
    pub(super) fn connects(self) -> bool {
        matches!(self, TileState::NotConnected | TileState::Stale)
    }

    /// Whether the app is set up for this vault.
    pub(super) fn is_connected(self) -> bool {
        matches!(self, TileState::Connected | TileState::NeedsRestart)
    }

    /// What the tile does or says, in words, for its tooltip and for
    /// anything that reads the screen.
    fn label(self, app: ClientApp) -> String {
        let name = app.name();
        match self {
            TileState::Checking => format!("Checking {name}…"),
            TileState::NotConnected => format!("Connect {name}"),
            TileState::Stale => format!("Update {name}. {STALE}"),
            TileState::Connecting => format!("Connecting {name}…"),
            TileState::Connected => format!("{name} is connected"),
            TileState::NeedsRestart => format!("{name} is connected. Restart {name} to use it."),
            TileState::Unreadable => unreadable_message(app).to_string(),
        }
    }
}

/// One app's tile: what its config says, while known, whether it's being
/// connected now, and the bundle its icon comes from.
#[derive(Clone, Debug)]
pub(super) struct AgentAppRow {
    pub app: ClientApp,
    /// `None` while its config is still being read.
    pub connection: Option<Connection>,
    pub connecting: bool,
    /// `None` when there's no app bundle to take an icon from.
    pub icon_bundle: Option<PathBuf>,
}

/// The AI apps the General page lists, and how to reach them.
pub(super) struct AgentApps {
    home: ClientHome,
    launch: ServerLaunch,
    pub rows: Vec<AgentAppRow>,
    /// The tile left and right move between while the row has focus.
    pub focused: usize,
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

/// The bundle an app's icon is read from. Claude Code has no bundle of
/// its own, so it borrows Claude's when Claude is installed.
fn icon_bundle(app: ClientApp, home: &ClientHome) -> Option<PathBuf> {
    match app {
        ClientApp::ClaudeCode => ClientApp::ClaudeDesktop.app_bundle(home),
        _ => app.app_bundle(home),
    }
}

fn agent_app_row(app: ClientApp, connection: Option<Connection>, home: &ClientHome) -> AgentAppRow {
    AgentAppRow {
        app,
        connection,
        connecting: false,
        icon_bundle: icon_bundle(app, home),
    }
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

impl AgentAppRow {
    pub(super) fn state(&self, cx: &App) -> TileState {
        if self.connecting {
            return TileState::Connecting;
        }
        match self.connection {
            None => TileState::Checking,
            Some(Connection::NotConnected) => TileState::NotConnected,
            Some(Connection::Stale) => TileState::Stale,
            Some(Connection::Unreadable) => TileState::Unreadable,
            Some(Connection::Connected) if self.needs_restart(cx) => TileState::NeedsRestart,
            Some(Connection::Connected) => TileState::Connected,
        }
    }

    fn needs_restart(&self, cx: &App) -> bool {
        self.app == ClientApp::ClaudeDesktop && connected_this_run(self.app, cx)
    }
}

impl SettingsView {
    /// Lists the AI apps installed under `home` on the General page, with
    /// tiles that point them at `binary mcp <this vault>`. Until this is
    /// called the page shows no AI apps; tests pass a temporary home.
    pub fn set_agent_apps(&mut self, home: ClientHome, binary: PathBuf, cx: &mut Context<Self>) {
        let vault = std::path::absolute(&self.vault_root).unwrap_or(self.vault_root.clone());
        let launch = ServerLaunch::new(&binary, &vault);
        let rows = ClientApp::installed(&home)
            .into_iter()
            .map(|app| agent_app_row(app, None, &home))
            .collect();
        self.agent_apps = Some(AgentApps {
            home: home.clone(),
            launch: launch.clone(),
            rows,
            focused: 0,
        });
        self.invalidate_layouts();
        self.load_agent_app_icons(cx);
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
        apps.rows = checked
            .connections
            .into_iter()
            .map(|(app, connection)| agent_app_row(app, Some(connection), &checked.home))
            .collect();
        apps.home = checked.home;
        apps.focused = apps.focused.min(apps.rows.len().saturating_sub(1));
        let after: Vec<ClientApp> = apps.rows.iter().map(|row| row.app).collect();
        if before != after {
            self.invalidate_layouts();
        }
        self.load_agent_app_icons(cx);
        cx.notify();
    }

    fn load_agent_app_icons(&mut self, cx: &mut Context<Self>) {
        let bundles = self
            .agent_app_list()
            .iter()
            .filter_map(|row| row.icon_bundle.clone())
            .collect();
        crate::app_icons::load_app_icons(bundles, cx);
    }

    /// The tiles' row, or the line that says which apps Gasp can
    /// connect, when it matches `query`.
    pub(super) fn agent_app_rows(&self, query: &str) -> Vec<ControlRow> {
        let Some(apps) = self.agent_apps.as_ref() else {
            return Vec::new();
        };
        if apps.rows.is_empty() {
            return words_match(NO_AGENT_APPS, query)
                .then_some(ControlRow::NoAgentApps)
                .into_iter()
                .collect();
        }
        let names = apps.rows.iter().map(|row| row.app.name());
        let haystack = [AGENT_APPS_TITLE, AGENT_APPS_LINE]
            .into_iter()
            .chain(names)
            .collect::<Vec<_>>()
            .join(" ");
        words_match(&haystack, query)
            .then_some(ControlRow::AgentApps)
            .into_iter()
            .collect()
    }

    pub(super) fn agent_app_list(&self) -> &[AgentAppRow] {
        self.agent_apps
            .as_ref()
            .map_or(&[], |apps| apps.rows.as_slice())
    }

    fn agent_app_row(&self, app: ClientApp) -> Option<&AgentAppRow> {
        self.agent_app_list().iter().find(|row| row.app == app)
    }

    /// The apps shown, in the order their tiles are.
    pub fn agent_app_tiles(&self) -> Vec<ClientApp> {
        self.agent_app_list().iter().map(|row| row.app).collect()
    }

    /// What `app`'s config says, once it has been read.
    pub fn agent_app_connection(&self, app: ClientApp) -> Option<Connection> {
        self.agent_app_row(app)?.connection
    }

    /// Where `app`'s tile has got to.
    pub fn agent_app_state(&self, app: ClientApp, cx: &App) -> Option<TileState> {
        Some(self.agent_app_row(app)?.state(cx))
    }

    /// What `app`'s tile does or says, in words: its tooltip, and its
    /// name for anything that reads the screen, such as "Connect Claude".
    pub fn agent_app_label(&self, app: ClientApp, cx: &App) -> Option<String> {
        Some(self.agent_app_state(app, cx)?.label(app))
    }

    /// Whether pressing the tile would connect the app now.
    fn can_connect(&self, app: ClientApp, cx: &App) -> bool {
        self.agent_app_state(app, cx)
            .is_some_and(TileState::connects)
    }

    /// Presses `app`'s tile: connects it, or says why Gasp won't touch
    /// its settings file.
    pub fn press_agent_app(&mut self, app: ClientApp, cx: &mut Context<Self>) {
        if self.agent_app_state(app, cx) == Some(TileState::Unreadable) {
            self.error = Some((
                AGENT_APPS_ERROR_KEY.to_string(),
                unreadable_message(app).to_string(),
            ));
            cx.notify();
            return;
        }
        self.connect_agent_app(app, cx);
    }

    /// Sets `app` up to start Gasp's server for this vault, off the main
    /// thread, then reads its config again.
    pub fn connect_agent_app(&mut self, app: ClientApp, cx: &mut Context<Self>) {
        if !self.can_connect(app, cx) {
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
                self.error = Some((
                    AGENT_APPS_ERROR_KEY.to_string(),
                    agent_app_error(app, error).to_string(),
                ));
            }
        }
        cx.notify();
    }

    /// The tile with keyboard focus while the row has it.
    pub(super) fn focused_agent_app(&self) -> usize {
        self.agent_apps.as_ref().map_or(0, |apps| apps.focused)
    }

    pub(super) fn focus_agent_app(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(apps) = self.agent_apps.as_mut() {
            apps.focused = index.min(apps.rows.len().saturating_sub(1));
            cx.notify();
        }
    }

    /// Left and right move between the tiles; Space or Enter presses the
    /// focused one.
    pub(super) fn agent_apps_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let focused = self.focused_agent_app();
        match key {
            "left" => self.focus_agent_app(focused.saturating_sub(1), cx),
            "right" => self.focus_agent_app(focused + 1, cx),
            "space" | "enter" => {
                let Some(app) = self.agent_app_list().get(focused).map(|row| row.app) else {
                    return false;
                };
                self.press_agent_app(app, cx);
            }
            _ => return false,
        }
        true
    }
}
