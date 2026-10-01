//! The General page's AI app tiles, on a temporary home folder: which
//! apps show, connecting one from its tile or the keyboard, a file Gasp
//! won't touch, and a write that fails.

use std::fs;
use std::path::{Path, PathBuf};

use gasp_config::{CONFIG_DIR, RuleSet};
use gasp_desktop::settings_view::agent_apps::TileState;
use gasp_desktop::settings_view::{ControlRow, SettingsView};
use gasp_desktop::text_input;
use gasp_mcp::clients::{ClientApp, ClientHome, Connection};
use gpui::{Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext};
use tempfile::TempDir;

const BINARY: &str = "/Applications/Gasp.app/Contents/MacOS/gasp";

struct Folders {
    home: TempDir,
    vault: TempDir,
}

impl Folders {
    fn new() -> Self {
        Folders {
            home: tempfile::tempdir().unwrap(),
            vault: tempfile::tempdir().unwrap(),
        }
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.home.path().join(relative)
    }

    fn make_dir(&self, relative: &str) {
        fs::create_dir_all(self.at(relative)).unwrap();
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.at(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn turn_off_agent_access(&self) {
        let config = self.vault.path().join(CONFIG_DIR);
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("settings.toml"), "[mcp]\nenabled = false\n").unwrap();
    }
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    folders: &Folders,
) -> (Entity<SettingsView>, &'a mut VisualTestContext) {
    cx.update(|cx| text_input::bind_keys(&RuleSet::defaults(), cx));
    let root = folders.vault.path().to_path_buf();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        SettingsView::with_rules(root.clone(), &RuleSet::defaults(), window, cx)
    });
    let home = ClientHome::in_folder(folders.home.path());
    view.update(cx, |view, cx| {
        view.set_agent_apps(home, PathBuf::from(BINARY), cx)
    });
    cx.run_until_parked();
    (view, cx)
}

fn agent_rows(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Vec<ControlRow> {
    view.read_with(cx, |view, _| view.rows())
        .into_iter()
        .filter(|row| matches!(row, ControlRow::AgentApps | ControlRow::NoAgentApps))
        .collect()
}

fn tiles(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Vec<ClientApp> {
    view.read_with(cx, |view, _| view.agent_app_tiles())
}

fn connection(
    view: &Entity<SettingsView>,
    app: ClientApp,
    cx: &mut VisualTestContext,
) -> Option<Connection> {
    view.read_with(cx, |view, _| view.agent_app_connection(app))
}

fn state(view: &Entity<SettingsView>, app: ClientApp, cx: &mut VisualTestContext) -> TileState {
    view.read_with(cx, |view, cx| view.agent_app_state(app, cx))
        .expect("the app's tile")
}

fn label(view: &Entity<SettingsView>, app: ClientApp, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, cx| view.agent_app_label(app, cx))
        .expect("the app's tile")
}

fn bounds(cx: &mut VisualTestContext, selector: String) -> Option<Bounds<Pixels>> {
    cx.run_until_parked();
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
}

/// Where `app`'s tile is drawn.
fn tile_bounds(cx: &mut VisualTestContext, app: ClientApp) -> Bounds<Pixels> {
    bounds(cx, format!("agent-app-{}", app.id())).expect("the app's tile")
}

/// Where every tile is drawn, in order.
fn every_tile(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Vec<Bounds<Pixels>> {
    tiles(view, cx)
        .into_iter()
        .map(|app| tile_bounds(cx, app))
        .collect()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let found = bounds(cx, selector.to_string()).unwrap_or_else(|| panic!("{selector} drawn"));
    cx.simulate_click(found.center(), Modifiers::default());
    cx.run_until_parked();
}

fn error(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Option<(String, String)> {
    view.read_with(cx, |view, _| {
        view.last_error()
            .map(|(key, message)| (key.to_string(), message.to_string()))
    })
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

#[gpui::test]
fn only_installed_apps_show_and_connect_moves_nothing(cx: &mut TestAppContext) {
    let folders = Folders::new();
    folders.make_dir("Applications/Claude.app");
    folders.make_dir(".cursor");
    folders.turn_off_agent_access();
    let (view, cx) = open(cx, &folders);
    assert_eq!(agent_rows(&view, cx), vec![ControlRow::AgentApps]);
    assert_eq!(
        tiles(&view, cx),
        vec![ClientApp::ClaudeDesktop, ClientApp::Cursor]
    );
    assert_eq!(state(&view, ClientApp::Cursor, cx), TileState::NotConnected);
    assert_eq!(label(&view, ClientApp::Cursor, cx), "Connect Cursor");
    let before = every_tile(&view, cx);

    click(cx, "connect-cursor");

    assert_eq!(
        connection(&view, ClientApp::Cursor, cx),
        Some(Connection::Connected)
    );
    assert_eq!(label(&view, ClientApp::Cursor, cx), "Cursor is connected");
    assert_eq!(every_tile(&view, cx), before);
    assert!(bounds(cx, "connected-cursor".to_string()).is_some());
    let written: serde_json::Value =
        serde_json::from_str(&read(&folders.at(".cursor/mcp.json"))).unwrap();
    let vault = std::path::absolute(folders.vault.path()).unwrap();
    assert_eq!(
        written,
        serde_json::json!({ "mcpServers": { "gasp": {
            "command": BINARY,
            "args": ["mcp", vault.to_string_lossy()],
        } } })
    );
}

#[gpui::test]
fn another_vaults_entry_offers_update(cx: &mut TestAppContext) {
    let folders = Folders::new();
    folders.write(
        "Library/Application Support/Claude/claude_desktop_config.json",
        r#"{"mcpServers":{"gasp":{"command":"/old/gasp","args":["mcp","/elsewhere"]}},"theme":"dark"}"#,
    );
    let (view, cx) = open(cx, &folders);
    let app = ClientApp::ClaudeDesktop;
    assert_eq!(connection(&view, app, cx), Some(Connection::Stale));
    assert!(label(&view, app, cx).starts_with("Update Claude."));
    let before = tile_bounds(cx, app);

    click(cx, "connect-claude-desktop");

    assert_eq!(connection(&view, app, cx), Some(Connection::Connected));
    assert_eq!(state(&view, app, cx), TileState::NeedsRestart);
    assert_eq!(
        label(&view, app, cx),
        "Claude is connected. Restart Claude to use it."
    );
    assert_eq!(tile_bounds(cx, app), before);
    assert!(bounds(cx, "connected-claude-desktop".to_string()).is_some());
    let text = read(&folders.at("Library/Application Support/Claude/claude_desktop_config.json"));
    assert!(text.contains("\"theme\": \"dark\""));
    assert!(!text.contains("/old/gasp"));
}

#[gpui::test]
fn an_invalid_file_is_left_alone(cx: &mut TestAppContext) {
    let folders = Folders::new();
    folders.write(".codex/config.toml", "[mcp_servers\n");
    let (view, cx) = open(cx, &folders);
    assert_eq!(
        connection(&view, ClientApp::Codex, cx),
        Some(Connection::Unreadable)
    );
    assert!(bounds(cx, "connect-codex".to_string()).is_none());

    click(cx, "agent-app-codex");

    let (key, message) = error(&view, cx).expect("why Gasp left it alone");
    assert_eq!(key, "agent-apps");
    assert!(message.starts_with("Codex’s settings file isn’t valid TOML"));
    view.update(cx, |view, cx| view.connect_agent_app(ClientApp::Codex, cx));
    cx.run_until_parked();
    assert_eq!(read(&folders.at(".codex/config.toml")), "[mcp_servers\n");
}

#[cfg(unix)]
#[gpui::test]
fn a_failed_write_says_how_to_fix_it(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let folders = Folders::new();
    folders.make_dir(".cursor");
    let locked = folders.at(".cursor");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
    let (view, cx) = open(cx, &folders);

    click(cx, "connect-cursor");

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    let (key, message) = error(&view, cx).expect("an error");
    assert_eq!(key, "agent-apps");
    assert!(message.starts_with("Couldn’t write Cursor’s settings file."));
    assert_eq!(
        connection(&view, ClientApp::Cursor, cx),
        Some(Connection::NotConnected)
    );
    assert!(bounds(cx, "connect-cursor".to_string()).is_some());
}

#[gpui::test]
fn the_keyboard_moves_between_tiles_and_connects(cx: &mut TestAppContext) {
    let folders = Folders::new();
    folders.make_dir("Applications/Claude.app");
    folders.make_dir(".cursor");
    let (view, cx) = open(cx, &folders);
    let index = view
        .read_with(cx, |view, _| view.rows())
        .iter()
        .position(|row| *row == ControlRow::AgentApps)
        .expect("the tiles' row");
    view.update_in(cx, |view, window, cx| view.focus_control(index, window, cx));

    cx.simulate_keystrokes("right right space");
    cx.run_until_parked();

    assert_eq!(state(&view, ClientApp::Cursor, cx), TileState::Connected);
    assert_eq!(
        state(&view, ClientApp::ClaudeDesktop, cx),
        TileState::NotConnected
    );
}

#[gpui::test]
fn no_apps_shows_one_line_that_focus_skips(cx: &mut TestAppContext) {
    let folders = Folders::new();
    let (view, cx) = open(cx, &folders);
    assert_eq!(agent_rows(&view, cx), vec![ControlRow::NoAgentApps]);
    assert!(!ControlRow::NoAgentApps.is_focusable());
}

#[gpui::test]
fn without_a_home_no_rows_show(cx: &mut TestAppContext) {
    cx.update(|cx| text_input::bind_keys(&RuleSet::defaults(), cx));
    let vault = tempfile::tempdir().unwrap();
    let root = vault.path().to_path_buf();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        SettingsView::with_rules(root.clone(), &RuleSet::defaults(), window, cx)
    });
    cx.run_until_parked();
    assert!(agent_rows(&view, cx).is_empty());
}
