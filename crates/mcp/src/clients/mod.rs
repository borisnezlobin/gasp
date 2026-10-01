//! Connecting AI apps on this Mac to `gasp mcp`, so nobody edits an app's
//! config by hand: finding which apps are installed, whether each already
//! starts Gasp's server for a vault, and adding or repointing it.
//!
//! Claude, Cursor and Codex keep their servers in a file Gasp edits in
//! place ([`json_config`], [`toml_config`]), leaving every other key as it
//! was. Claude Code is changed through its own `claude mcp` command
//! ([`claude_cli`]) and read from `~/.claude.json`.
//!
//! Every path comes from a [`ClientHome`], so tests point it at a
//! temporary folder and never touch the real files.

pub mod claude_cli;
pub mod config_file;
pub mod json_config;
pub mod toml_config;

use std::io;
use std::path::{Path, PathBuf};

/// The name every app knows Gasp's server by.
pub const SERVER_NAME: &str = gasp_config::command_name!();

/// How an app starts Gasp's server: the app's own binary, then
/// `mcp <vault>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerLaunch {
    pub command: String,
    pub args: Vec<String>,
}

impl ServerLaunch {
    /// `binary mcp vault`.
    pub fn new(binary: &Path, vault: &Path) -> Self {
        ServerLaunch {
            command: binary.to_string_lossy().into_owned(),
            args: vec!["mcp".to_string(), vault.to_string_lossy().into_owned()],
        }
    }

    /// Whether an entry with `command` and `args` starts this server.
    pub fn is_started_by(&self, command: Option<&str>, args: Option<Vec<&str>>) -> bool {
        command == Some(self.command.as_str())
            && args.is_some_and(|args| args.iter().eq(self.args.iter()))
    }
}

/// Whether an app starts Gasp's server for this vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    NotConnected,
    Connected,
    /// It has a `gasp` server, set up for another vault or an older copy
    /// of the app.
    Stale,
    /// Its config file isn't valid, so Gasp leaves it alone.
    Unreadable,
}

/// Why connecting an app failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientError {
    /// The app's config file isn't valid JSON or TOML, or isn't shaped as
    /// the app documents it.
    Unreadable,
    /// Reading or writing the app's config file failed.
    WriteFailed,
    /// The app's command line tool isn't where Gasp looks.
    CliMissing,
    /// The app's command line tool ran and refused, or didn't finish.
    CliFailed,
}

impl From<io::Error> for ClientError {
    fn from(_: io::Error) -> Self {
        ClientError::WriteFailed
    }
}

/// Where the AI apps keep their files: the home folder, the folders apps
/// are installed in, and the folders their command line tools live in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientHome {
    pub home: PathBuf,
    pub application_dirs: Vec<PathBuf>,
    /// Folders outside the home a command line tool may be in. Apps opened
    /// from the Dock don't get the shell's `PATH`, so these are searched
    /// by hand.
    pub bin_dirs: Vec<PathBuf>,
    /// Whether a login shell may be asked for its `PATH` when a command
    /// line tool isn't in [`Self::bin_dirs`].
    pub ask_login_shell: bool,
}

/// Where Homebrew and other installers put command line tools on a Mac.
const SYSTEM_BIN_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

/// Where command line tools sit under the home folder.
const HOME_BIN_DIRS: &[&str] = &[".claude/local", ".local/bin"];

impl ClientHome {
    /// The signed-in user's folders.
    pub fn current() -> Option<Self> {
        let home = dirs::home_dir()?;
        Some(ClientHome {
            application_dirs: vec![PathBuf::from("/Applications"), home.join("Applications")],
            bin_dirs: SYSTEM_BIN_DIRS.iter().map(PathBuf::from).collect(),
            ask_login_shell: true,
            home,
        })
    }

    /// Folders under `home` only, for tests.
    pub fn in_folder(home: &Path) -> Self {
        ClientHome {
            home: home.to_path_buf(),
            application_dirs: vec![home.join("Applications")],
            bin_dirs: vec![home.join("bin")],
            ask_login_shell: false,
        }
    }

    /// Whether an app bundle named `bundle` is installed.
    fn has_app(&self, bundle: &str) -> bool {
        self.find_app(bundle).is_some()
    }

    /// Where the app bundle named `bundle` is installed, such as
    /// `/Applications/Claude.app`.
    pub fn find_app(&self, bundle: &str) -> Option<PathBuf> {
        self.application_dirs
            .iter()
            .map(|folder| folder.join(bundle))
            .find(|path| path.is_dir())
    }

    /// Every folder a command line tool is looked for in, home folders first.
    pub fn tool_dirs(&self) -> Vec<PathBuf> {
        HOME_BIN_DIRS
            .iter()
            .map(|folder| self.home.join(folder))
            .chain(self.bin_dirs.iter().cloned())
            .collect()
    }

    /// The command line tool `name`, if it's in one of [`Self::tool_dirs`].
    pub fn find_tool(&self, name: &str) -> Option<PathBuf> {
        self.tool_dirs()
            .into_iter()
            .map(|folder| folder.join(name))
            .find(|path| path.is_file())
    }

    /// Adds the folders on `path` (a `PATH` value) to those searched.
    pub fn add_search_path(&mut self, path: &str) {
        for folder in std::env::split_paths(path) {
            if folder.is_absolute() && !self.bin_dirs.contains(&folder) {
                self.bin_dirs.push(folder);
            }
        }
    }

    /// Adds the login shell's `PATH` to the folders searched, when a tool
    /// an app is found by isn't in them yet. Blocks for up to a few
    /// seconds, so it runs off the main thread.
    pub fn search_login_shell(&mut self) {
        let missing = [claude_cli::TOOL, "codex"]
            .iter()
            .any(|tool| self.find_tool(tool).is_none());
        if !self.ask_login_shell || !missing {
            return;
        }
        if let Some(path) = claude_cli::login_shell_path() {
            self.add_search_path(&path);
        }
    }

    fn claude_desktop_dir(&self) -> PathBuf {
        self.home.join("Library/Application Support/Claude")
    }
}

/// An AI app Gasp can connect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClientApp {
    ClaudeDesktop,
    ClaudeCode,
    Cursor,
    Codex,
}

/// How an app's config is changed.
enum Route {
    JsonFile(PathBuf),
    TomlFile(PathBuf),
    ClaudeCli,
}

impl ClientApp {
    pub const ALL: [ClientApp; 4] = [
        ClientApp::ClaudeDesktop,
        ClientApp::ClaudeCode,
        ClientApp::Cursor,
        ClientApp::Codex,
    ];

    /// A stable id, such as `claude-desktop`.
    pub fn id(self) -> &'static str {
        match self {
            ClientApp::ClaudeDesktop => "claude-desktop",
            ClientApp::ClaudeCode => "claude-code",
            ClientApp::Cursor => "cursor",
            ClientApp::Codex => "codex",
        }
    }

    /// The app's name, as its maker writes it.
    pub fn name(self) -> &'static str {
        match self {
            ClientApp::ClaudeDesktop => "Claude",
            ClientApp::ClaudeCode => "Claude Code",
            ClientApp::Cursor => "Cursor",
            ClientApp::Codex => "Codex",
        }
    }

    /// The app's own bundle, for its icon. Claude Code is only a command,
    /// so it has none.
    pub fn app_bundle(self, home: &ClientHome) -> Option<PathBuf> {
        let bundle = match self {
            ClientApp::ClaudeDesktop => "Claude.app",
            ClientApp::Cursor => "Cursor.app",
            ClientApp::Codex => "Codex.app",
            ClientApp::ClaudeCode => return None,
        };
        home.find_app(bundle)
    }

    /// The file Gasp reads to tell whether the app is connected.
    pub fn config_path(self, home: &ClientHome) -> PathBuf {
        match self {
            ClientApp::ClaudeDesktop => {
                home.claude_desktop_dir().join("claude_desktop_config.json")
            }
            ClientApp::ClaudeCode => home.home.join(".claude.json"),
            ClientApp::Cursor => home.home.join(".cursor/mcp.json"),
            ClientApp::Codex => home.home.join(".codex/config.toml"),
        }
    }

    fn route(self, home: &ClientHome) -> Route {
        let path = self.config_path(home);
        match self {
            ClientApp::ClaudeDesktop | ClientApp::Cursor => Route::JsonFile(path),
            ClientApp::Codex => Route::TomlFile(path),
            ClientApp::ClaudeCode => Route::ClaudeCli,
        }
    }

    /// Whether the app is on this Mac. Only looks at files, so it's quick
    /// enough to ask while a frame is drawn.
    pub fn is_installed(self, home: &ClientHome) -> bool {
        match self {
            ClientApp::ClaudeDesktop => {
                home.has_app("Claude.app") || home.claude_desktop_dir().is_dir()
            }
            ClientApp::ClaudeCode => home.find_tool(claude_cli::TOOL).is_some(),
            ClientApp::Cursor => home.has_app("Cursor.app") || home.home.join(".cursor").is_dir(),
            ClientApp::Codex => {
                home.home.join(".codex").is_dir() || home.find_tool("codex").is_some()
            }
        }
    }

    /// The apps installed on this Mac, in [`Self::ALL`]'s order.
    pub fn installed(home: &ClientHome) -> Vec<ClientApp> {
        ClientApp::ALL
            .into_iter()
            .filter(|app| app.is_installed(home))
            .collect()
    }

    /// Whether the app starts Gasp's server for `launch`'s vault.
    pub fn connection(self, home: &ClientHome, launch: &ServerLaunch) -> Connection {
        let text = match config_file::read(&self.config_path(home)) {
            Ok(text) => text,
            Err(_) => return Connection::Unreadable,
        };
        match self.route(home) {
            Route::JsonFile(_) => json_config::connection(text.as_deref(), launch),
            Route::TomlFile(_) => toml_config::connection(text.as_deref(), launch),
            // Claude Code's own file is only read; a broken one is the
            // command's business, and it may still add Gasp.
            Route::ClaudeCli => match json_config::connection(text.as_deref(), launch) {
                Connection::Unreadable => Connection::NotConnected,
                connection => connection,
            },
        }
    }

    /// Sets the app up to start Gasp's server for `launch`'s vault,
    /// replacing any `gasp` server it had.
    pub fn connect(self, home: &ClientHome, launch: &ServerLaunch) -> Result<(), ClientError> {
        match self.route(home) {
            Route::JsonFile(path) => {
                config_file::rewrite(&path, |text| json_config::with_server(text, launch))
            }
            Route::TomlFile(path) => {
                config_file::rewrite(&path, |text| toml_config::with_server(text, launch))
            }
            Route::ClaudeCli => {
                let replacing = self.connection(home, launch) == Connection::Stale;
                claude_cli::connect(home, launch, replacing)
            }
        }
    }
}

#[cfg(test)]
mod tests;
