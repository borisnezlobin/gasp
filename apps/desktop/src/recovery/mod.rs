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
pub use gasp_vault::recovery as store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gasp_config::settings::RecoverySettings;
use gpui::{App, AppContext, Context, Global, Window};

use self::dialog::{RecoveryDialog, RecoveryEvent};
use self::store::SnapshotStore;
use crate::workspace::Workspace;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// One open vault's snapshots and settings.
struct VaultSnapshots {
    vault: PathBuf,
    store: Option<Arc<SnapshotStore>>,
    settings: RecoverySettings,
}

/// The snapshots of every open vault, each window's its own, and when
/// each note was last kept.
#[derive(Default)]
struct Recovery {
    vaults: Vec<VaultSnapshots>,
    /// When each note (by full path) was last kept this session.
    kept: HashMap<PathBuf, Instant>,
}

impl Global for Recovery {}

impl Recovery {
    /// The open vault `note` is in: the deepest one, if vaults nest.
    fn vault_of(&self, note: &Path) -> Option<&VaultSnapshots> {
        self.vaults
            .iter()
            .filter(|vault| note.starts_with(&vault.vault))
            .max_by_key(|vault| vault.vault.components().count())
    }

    /// The store and the note's path in its vault, and how often it's kept.
    /// A note named through a link to its vault is found by its real path.
    fn locate(&self, note: &Path) -> Option<(Arc<SnapshotStore>, PathBuf, Duration)> {
        let real;
        let note = if self.vault_of(note).is_some() {
            note
        } else {
            real = crate::workspace::files::canonical_path(note)?;
            &real
        };
        let vault = self.vault_of(note)?;
        let store = vault.store.clone()?;
        let relative = note.strip_prefix(&vault.vault).ok()?.to_path_buf();
        let interval = Duration::from_secs(u64::from(vault.settings.interval_minutes) * 60);
        Some((store, relative, interval))
    }

    fn settings_of(&self, note: &Path) -> RecoverySettings {
        self.vault_of(note)
            .map(|vault| vault.settings.clone())
            .unwrap_or_default()
    }
}

/// Starts keeping snapshots for `workspace`'s vault, deletes old ones, and
/// wires `note.recover`. Each vault window keeps to its own store.
pub fn install(workspace: &mut Workspace, cx: &mut Context<Workspace>) {
    let vault = workspace.vault().to_path_buf();
    let settings = workspace.config().settings.recovery.clone();
    let store = SnapshotStore::for_vault(&vault).map(Arc::new);
    if let Some(store) = store.clone() {
        let keep = DAY * settings.keep_days;
        cx.background_spawn(async move { store.prune(keep, SystemTime::now()) })
            .detach();
    }
    let recovery = cx.default_global::<Recovery>();
    recovery.vaults.retain(|open| open.vault != vault);
    recovery.vaults.push(VaultSnapshots {
        vault,
        store,
        settings,
    });
    workspace.on_command("note.recover", open_dialog);
}

/// Follows the recovery settings of the vault at `vault`.
pub fn configure(vault: &Path, settings: &RecoverySettings, cx: &mut App) {
    let Some(recovery) = cx.try_global::<Recovery>() else {
        return;
    };
    let changed = recovery
        .vaults
        .iter()
        .any(|open| open.vault == vault && open.settings != *settings);
    if changed {
        let recovery = cx.global_mut::<Recovery>();
        for open in recovery
            .vaults
            .iter_mut()
            .filter(|open| open.vault == vault)
        {
            open.settings = settings.clone();
        }
    }
}

/// A note at `path` is about to be saved over `previous`: keeps
/// `previous` if the note's last snapshot is old enough.
pub fn before_save(path: &Path, previous: &str, cx: &mut App) {
    let Some(recovery) = cx.try_global::<Recovery>() else {
        return;
    };
    let Some((_, _, interval)) = recovery.locate(path) else {
        return;
    };
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
    let Some((store, relative, _)) = recovery.locate(path) else {
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

/// A note or folder moved from `from` to `to` (full paths): its
/// snapshots move with it, so its history survives a rename.
pub fn moved(from: &Path, to: &Path, cx: &mut App) {
    let Some(recovery) = cx.try_global::<Recovery>() else {
        return;
    };
    let (Some((store, old, _)), Some((_, new, _))) = (recovery.locate(from), recovery.locate(to))
    else {
        return;
    };
    let kept = &mut cx.global_mut::<Recovery>().kept;
    let moved: Vec<PathBuf> = kept
        .keys()
        .filter(|path| path.starts_with(from))
        .cloned()
        .collect();
    for path in moved {
        if let (Some(when), Ok(rest)) = (kept.remove(&path), path.strip_prefix(from)) {
            kept.insert(to.join(rest), when);
        }
    }
    cx.background_spawn(async move {
        if let Err(error) = store.moved(&old, &new) {
            eprintln!("could not move the snapshots of {}: {error}", old.display());
        }
    })
    .detach();
}

/// The store a note's snapshots are kept in, and its path there.
pub fn store_for(note: &Path, cx: &App) -> Option<(Arc<SnapshotStore>, PathBuf)> {
    let (store, relative, _) = cx.try_global::<Recovery>()?.locate(note)?;
    Some((store, relative))
}

/// `note.recover`: the dialog for the active note.
fn open_dialog(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let (Some(path), Some(editor)) = (workspace.active_path(cx), workspace.active_editor(cx))
    else {
        return;
    };
    let Some((store, relative)) = store_for(&path, cx) else {
        return;
    };
    let current = editor.read(cx).text();
    let settings = cx.global::<Recovery>().settings_of(&path);
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
