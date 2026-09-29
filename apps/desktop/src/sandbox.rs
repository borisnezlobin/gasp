//! What a snapshot run keeps the app from doing.
//!
//! `gasp --snapshot` drives the whole app with no window on screen, on a
//! copy of a vault the tool makes itself. Once [`enter`] is called the
//! app keeps to that copy and to its own temporary data folder:
//!
//! - the app's folders (the last vault, recovery snapshots, the link card
//!   cache) live under the sandbox's data folder;
//! - nothing opens another window, a panel, a browser, Finder or another
//!   app, and a deleted note never goes to the system trash;
//! - unless writes are allowed, notes, the window's state and edit times
//!   aren't saved and the vault isn't watched. A note that would have been
//!   saved looks saved, as it would a moment after typing.
//!
//! Outside a snapshot run none of this applies and every check here
//! answers as the app always behaves.

use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use futures::channel::oneshot;
use gasp_config::APP_FOLDER;
use gasp_config::settings::TrashMode;
use gpui::{App, PathPromptOptions};

/// The limits a snapshot run sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sandbox {
    /// Where the app's own folders go instead of the user's.
    pub data_root: PathBuf,
    /// Whether notes are saved and the vault watched, as the app does.
    pub allow_writes: bool,
}

static SANDBOX: OnceLock<Sandbox> = OnceLock::new();

/// Puts the app in `sandbox` for the rest of the process. Only the first
/// call counts.
pub fn enter(sandbox: Sandbox) {
    gasp_vault::recovery::use_data_dir(sandbox.data_root.join("data"));
    SANDBOX.get_or_init(|| sandbox);
}

/// Whether this is a snapshot run.
pub fn is_active() -> bool {
    SANDBOX.get().is_some()
}

/// Whether the app saves notes and its state, and watches the vault.
pub fn writes_allowed() -> bool {
    SANDBOX.get().is_none_or(|sandbox| sandbox.allow_writes)
}

/// Whether the app may start syncing, or answer `gasp mcp`.
pub fn reaches_outside() -> bool {
    !is_active()
}

/// The app's folder for settings that belong to this machine.
pub fn config_folder() -> Option<PathBuf> {
    app_folder("config", dirs::config_dir)
}

/// The app's folder for data it keeps, such as caches.
pub fn cache_folder() -> Option<PathBuf> {
    app_folder("cache", dirs::cache_dir)
}

fn app_folder(kind: &str, system: fn() -> Option<PathBuf>) -> Option<PathBuf> {
    match SANDBOX.get() {
        Some(sandbox) => Some(sandbox.data_root.join(kind)),
        None => system().map(|base| base.join(APP_FOLDER)),
    }
}

/// Answers true, after saying what was left out, when a snapshot run
/// keeps the app from doing `action`.
pub fn blocks(action: impl Display) -> bool {
    if !is_active() {
        return false;
    }
    eprintln!(
        "{} --snapshot: left out: {action}",
        gasp_config::COMMAND_NAME
    );
    true
}

/// How a note is deleted: never into the system trash in a snapshot run,
/// where the vault is a temporary copy.
pub fn trash_mode(mode: TrashMode) -> TrashMode {
    match mode {
        TrashMode::System if is_active() => TrashMode::Vault,
        mode => mode,
    }
}

/// Opens `url` in the browser, outside a snapshot run.
pub fn open_url(url: &str, cx: &App) {
    if !blocks(format_args!("opening {url}")) {
        cx.open_url(url);
    }
}

/// Opens `path` in its default app, outside a snapshot run.
pub fn open_with_system(path: &Path, cx: &App) {
    if !blocks(format_args!("opening {}", path.display())) {
        cx.open_with_system(path);
    }
}

/// Shows `path` in Finder, outside a snapshot run.
pub fn reveal_path(path: &Path, cx: &App) {
    if !blocks(format_args!("revealing {}", path.display())) {
        cx.reveal_path(path);
    }
}

/// Asks for files or folders; a snapshot run answers as if the panel
/// were cancelled.
pub fn prompt_for_paths(
    options: PathPromptOptions,
    cx: &App,
) -> oneshot::Receiver<anyhow::Result<Option<Vec<PathBuf>>>> {
    if !blocks("the open panel") {
        return cx.prompt_for_paths(options);
    }
    cancelled()
}

/// Asks where to save; a snapshot run answers as if the panel were
/// cancelled.
pub fn prompt_for_new_path(
    folder: &Path,
    name: Option<&str>,
    cx: &App,
) -> oneshot::Receiver<anyhow::Result<Option<PathBuf>>> {
    if !blocks("the save panel") {
        return cx.prompt_for_new_path(folder, name);
    }
    cancelled()
}

fn cancelled<T>() -> oneshot::Receiver<anyhow::Result<Option<T>>> {
    let (sender, receiver) = oneshot::channel();
    let _ = sender.send(Ok(None));
    receiver
}
