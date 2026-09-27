//! File recovery: while a note is edited, the version it's replacing is
//! kept as a local snapshot every few minutes, outside the vault, and
//! `note.recover` opens a dialog that lists a note's snapshots, shows how
//! each differs from the note now, and restores one.
//!
//! A snapshot is also kept just before something else replaces a note's
//! text, such as a sync bringing in another device's version or a
//! restore, so that can always be undone too. Snapshots older than the
//! `recovery.keep-days` setting are deleted when a vault opens.
//!
//! All reading and writing of snapshots happens off the main thread.

pub mod compare;
pub mod dialog;
pub mod store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use editor_config::settings::RecoverySettings;
use gpui::{App, AppContext, Context, Global, Window};

use self::dialog::{RecoveryDialog, RecoveryEvent};
use self::store::SnapshotStore;
use crate::workspace::Workspace;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// The open vault's snapshots and when each note was last kept.
#[derive(Default)]
struct Recovery {
    vault: PathBuf,
    store: Option<Arc<SnapshotStore>>,
    settings: RecoverySettings,
    /// When each note (by full path) was last kept this session.
    kept: HashMap<PathBuf, Instant>,
}

impl Global for Recovery {}

impl Recovery {
    fn interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.settings.interval_minutes) * 60)
    }

    /// The store and the note's path in the vault, if it's in the vault.
    fn locate(&self, note: &Path) -> Option<(Arc<SnapshotStore>, PathBuf)> {
        let store = self.store.clone()?;
        let relative = note.strip_prefix(&self.vault).ok()?.to_path_buf();
        Some((store, relative))
    }
}

/// Starts keeping snapshots for `workspace`'s vault, deletes old ones, and
/// wires `note.recover`.
pub fn install(workspace: &mut Workspace, cx: &mut Context<Workspace>) {
    let vault = workspace.vault().to_path_buf();
    let settings = workspace.config().settings.recovery.clone();
    let store = SnapshotStore::for_vault(&vault).map(Arc::new);
    if let Some(store) = store.clone() {
        let keep = DAY * settings.keep_days;
        cx.background_spawn(async move { store.prune(keep, SystemTime::now()) })
            .detach();
    }
    cx.set_global(Recovery {
        vault,
        store,
        settings,
        kept: HashMap::new(),
    });
    workspace.on_command("note.recover", open_dialog);
}

/// Follows the recovery settings.
pub fn configure(settings: &RecoverySettings, cx: &mut App) {
    if let Some(recovery) = cx.try_global::<Recovery>()
        && recovery.settings != *settings
    {
        cx.global_mut::<Recovery>().settings = settings.clone();
    }
}

/// A note at `path` is about to be saved over `previous`: keeps
/// `previous` if the note's last snapshot is old enough.
pub fn before_save(path: &Path, previous: &str, cx: &mut App) {
    let Some(recovery) = cx.try_global::<Recovery>() else {
        return;
    };
    let interval = recovery.interval();
    let recent = recovery
        .kept
        .get(path)
        .is_some_and(|kept| kept.elapsed() < interval);
    if !recent {
        keep(path, previous, interval, cx);
    }
}

/// Keeps `text` as a version of the note at `path` now, as before a sync
/// or a restore replaces it.
pub fn keep_version(path: &Path, text: &str, cx: &mut App) {
    keep(path, text, Duration::ZERO, cx);
}

fn keep(path: &Path, text: &str, interval: Duration, cx: &mut App) {
    let Some(recovery) = cx.try_global::<Recovery>() else {
        return;
    };
    let Some((store, relative)) = recovery.locate(path) else {
        return;
    };
    cx.global_mut::<Recovery>()
        .kept
        .insert(path.to_path_buf(), Instant::now());
    let text = text.to_owned();
    cx.background_spawn(async move {
        if let Err(error) = store.record(&relative, &text, SystemTime::now(), interval) {
            eprintln!(
                "could not keep a snapshot of {}: {error}",
                relative.display()
            );
        }
    })
    .detach();
}

/// `note.recover`: the dialog for the active note.
fn open_dialog(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let (Some(path), Some(editor)) = (workspace.active_path(cx), workspace.active_editor(cx))
    else {
        return;
    };
    let Some((store, relative)) = cx
        .try_global::<Recovery>()
        .and_then(|recovery| recovery.locate(&path))
    else {
        return;
    };
    let current = editor.read(cx).text();
    let settings = cx.global::<Recovery>().settings.clone();
    let editor = editor.downgrade();
    workspace.toggle_modal(window, cx, move |_, cx| {
        let dialog = RecoveryDialog::new(store, relative, current, settings, cx);
        cx.subscribe(&cx.entity(), move |_, _, event: &RecoveryEvent, cx| {
            let RecoveryEvent::Restore(text) = event;
            let Some(editor) = editor.upgrade() else {
                return;
            };
            let now = editor.read(cx).text();
            keep_version(&path, &now, cx);
            editor.update(cx, |editor, cx| editor.replace_all_text(text, cx));
        })
        .detach();
        dialog
    });
}
