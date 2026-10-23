//! Encrypted notes in the desktop app. Each open vault keeps one
//! [`Encryption`] here: its lock, the keys unlocked so far and which of
//! its notes are encrypted. Plain text of an encrypted note lives only in
//! the editors' memory; everything that writes a note seals it through
//! [`seal`] first, and everything that previews one asks
//! [`is_encrypted_note`] or [`gasp_crypt::is_encrypted`] before showing it.
//!
//! Locking forgets every key and swaps each open note for its locked
//! view, which [`lock_vault`] does for the lock command, and [`focus`]
//! does once the window has been away too long.

pub mod commands;
pub mod dialogs;
pub mod focus;
pub mod keychain;
pub mod lock_screen;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gasp_config::settings::EncryptionSettings;
use gasp_crypt::{KdfParams, NameVisibility, is_encrypted, peek};
use gasp_vault::encryption::{Encryption, NoteText};
use gpui::{App, AppContext, Entity, Global, WeakEntity};

use crate::workspace::note_doc::NoteDoc;

/// What a locked note is called where its file's name is hidden.
pub const LOCKED_NOTE_NAME: &str = "Locked note";

/// What a preview of an encrypted note shows in place of its text.
pub const LOCKED_PREVIEW: &str = "This note is encrypted. Open it to unlock it.";

/// How much of a file is read to tell whether it's encrypted.
const MARKER_BYTES: usize = 256;

/// One open vault's encryption.
struct VaultLocks {
    root: PathBuf,
    encryption: Encryption,
    settings: EncryptionSettings,
    /// The vault's encrypted notes by relative path, and whether each
    /// one's file name is hidden.
    notes: HashMap<String, NameVisibility>,
    /// Every open note of the vault, in any window, for locking them all.
    docs: Vec<WeakEntity<NoteDoc>>,
    /// Counts the vault's locks, so a password step that finishes after
    /// one can tell and drop its result.
    lock_epoch: u64,
    /// While this vault's password changes, its lock file changes under
    /// the session on purpose and isn't news from another device.
    changing_password: bool,
}

/// Every open vault's encryption.
struct EncryptedVaults {
    vaults: Vec<VaultLocks>,
    /// How new keys are made: [`KdfParams::STANDARD`] outside tests.
    params: KdfParams,
}

impl Default for EncryptedVaults {
    fn default() -> Self {
        EncryptedVaults {
            vaults: Vec::new(),
            params: KdfParams::STANDARD,
        }
    }
}

impl Global for EncryptedVaults {}

impl EncryptedVaults {
    /// The open vault `path` is in (the deepest, if vaults nest) and the
    /// path inside it.
    fn locate(&self, path: &Path) -> Option<(usize, String)> {
        let (index, vault) = self
            .vaults
            .iter()
            .enumerate()
            .filter(|(_, vault)| path.starts_with(&vault.root))
            .max_by_key(|(_, vault)| vault.root.components().count())?;
        let relative = crate::vault_index::vault_relative(&vault.root, path)?;
        Some((index, relative))
    }
}

/// A note's look in lists such as the file tree: encrypted, whether it's
/// open now, and the name to show in place of a hidden file name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteMark {
    pub unlocked: bool,
    pub shown_name: Option<String>,
}

/// An encrypted note's icon: a closed padlock while it's locked, an open
/// one while it's unlocked.
pub fn lock_icon(mark: &NoteMark) -> crate::icons::IconName {
    match mark.unlocked {
        true => crate::icons::IconName::LockSimpleOpen,
        false => crate::icons::IconName::LockSimple,
    }
}

/// Redraws `V` whenever a note locks, unlocks or becomes encrypted.
pub fn observe_marks<V: 'static>(cx: &mut gpui::Context<V>) -> gpui::Subscription {
    cx.observe_global::<EncryptedVaults>(|_, cx| cx.notify())
}

/// Makes new keys quickly, for tests: the keys a test makes open as any
/// others do, since a key's file says how it was made.
pub fn use_quick_keys(cx: &mut App) {
    cx.default_global::<EncryptedVaults>().params = KdfParams::QUICK;
}

/// Starts keeping `root`'s encryption, once per vault however many
/// windows show it, and finds its encrypted notes in the background.
pub fn register_vault(root: &Path, settings: &EncryptionSettings, cx: &mut App) {
    let vaults = cx.default_global::<EncryptedVaults>();
    if vaults.vaults.iter().any(|vault| vault.root == root) {
        configure(root, settings, cx);
        return;
    }
    let encryption = Encryption::load_with(root, vaults.params);
    vaults.vaults.push(VaultLocks {
        root: root.to_path_buf(),
        encryption,
        settings: settings.clone(),
        notes: HashMap::new(),
        docs: Vec::new(),
        lock_epoch: 0,
        changing_password: false,
    });
    keychain::check_availability(cx);
    let scan_root = root.to_path_buf();
    let found = cx.background_spawn(async move { encrypted_notes(&scan_root) });
    let root = root.to_path_buf();
    cx.spawn(async move |cx| {
        let notes = found.await;
        cx.update(|cx| {
            let vaults = cx.global_mut::<EncryptedVaults>();
            if let Some(vault) = vaults.vaults.iter_mut().find(|vault| vault.root == root) {
                vault.notes.extend(notes);
            }
        })
        .ok();
    })
    .detach();
}

/// Follows the encryption settings of the vault at `root`.
pub fn configure(root: &Path, settings: &EncryptionSettings, cx: &mut App) {
    let Some(vaults) = cx.try_global::<EncryptedVaults>() else {
        return;
    };
    let changed = vaults
        .vaults
        .iter()
        .any(|vault| vault.root == root && vault.settings != *settings);
    if !changed {
        return;
    }
    for vault in cx
        .global_mut::<EncryptedVaults>()
        .vaults
        .iter_mut()
        .filter(|vault| vault.root == root)
    {
        vault.settings = settings.clone();
    }
}

/// The encryption settings of the vault `path` is in.
pub fn settings_for(path: &Path, cx: &App) -> EncryptionSettings {
    cx.try_global::<EncryptedVaults>()
        .and_then(|vaults| {
            let (index, _) = vaults.locate(path)?;
            Some(vaults.vaults[index].settings.clone())
        })
        .unwrap_or_default()
}

/// Every encrypted note under `root`. Reads the start of each note, so
/// run it off the main thread.
fn encrypted_notes(root: &Path) -> HashMap<String, NameVisibility> {
    gasp_vault::ops::vault_files(root)
        .into_iter()
        .filter(|path| path.ends_with(".md"))
        .filter_map(|path| {
            let visibility = name_visibility_on_disk(&root.join(&path))?;
            Some((path, visibility))
        })
        .collect()
}

/// Whether the file at `path` is an encrypted note, and if so whether
/// its name is hidden. Reads only the start unless it's encrypted.
fn name_visibility_on_disk(path: &Path) -> Option<NameVisibility> {
    if !starts_encrypted(path) {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    Some(peek(&text).ok()?.options.name)
}

/// Whether the file at `path` starts as an encrypted note does.
pub fn starts_encrypted(path: &Path) -> bool {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut start = Vec::with_capacity(MARKER_BYTES);
    if file
        .take(MARKER_BYTES as u64)
        .read_to_end(&mut start)
        .is_err()
    {
        return false;
    }
    is_encrypted(&String::from_utf8_lossy(&start))
}

/// `text` as a preview may show it: an encrypted note's sealed text
/// becomes [`LOCKED_PREVIEW`].
pub fn preview_text(text: &str) -> &str {
    match is_encrypted(text) {
        true => LOCKED_PREVIEW,
        false => text,
    }
}

/// Runs `action` on the encryption of the vault `path` is in, with the
/// path inside it. `None` when no open vault holds `path`.
pub fn with_encryption<R>(
    path: &Path,
    cx: &mut App,
    action: impl FnOnce(&mut Encryption, &str) -> R,
) -> Option<R> {
    let vaults = cx.try_global::<EncryptedVaults>()?;
    let (index, relative) = vaults.locate(path)?;
    let vault = &mut cx.global_mut::<EncryptedVaults>().vaults[index];
    Some(action(&mut vault.encryption, &relative))
}

/// Reads `action`'s answer from the encryption of the vault `path` is in.
pub fn read_encryption<R>(
    path: &Path,
    cx: &App,
    action: impl FnOnce(&Encryption, &str) -> R,
) -> Option<R> {
    let vaults = cx.try_global::<EncryptedVaults>()?;
    let (index, relative) = vaults.locate(path)?;
    Some(action(&vaults.vaults[index].encryption, &relative))
}

/// What the file at `path`, holding `raw`, shows: plain text, a locked
/// note, or an encrypted note opened with a key already in hand. An
/// encrypted note outside every open vault stays locked.
pub fn read(path: &Path, raw: &str, cx: &mut App) -> NoteText {
    if !is_encrypted(raw) {
        return NoteText::Plain(raw.to_owned());
    }
    let read = with_encryption(path, cx, |encryption, relative| {
        encryption.read(relative, raw)
    });
    let text = read.unwrap_or_else(|| match peek(raw) {
        Ok(header) => NoteText::Locked(header),
        Err(_) => NoteText::Plain(raw.to_owned()),
    });
    if let Ok(header) = peek(raw) {
        mark(path, Some(header.options.name), cx);
    }
    text
}

/// The file for the unlocked encrypted note at `path` holding `body`;
/// `None` when it isn't one.
pub fn seal(path: &Path, body: &str, cx: &App) -> Option<String> {
    read_encryption(path, cx, |encryption, relative| {
        encryption.seal(relative, body)
    })
    .flatten()
}

/// Whether the note at `path` is encrypted, as far as this session knows.
pub fn is_encrypted_note(path: &Path, cx: &App) -> bool {
    let Some(vaults) = cx.try_global::<EncryptedVaults>() else {
        return false;
    };
    vaults
        .locate(path)
        .is_some_and(|(index, relative)| vaults.vaults[index].notes.contains_key(&relative))
}

/// Whether the encrypted note at `path` is open in this session.
pub fn is_unlocked(path: &Path, cx: &App) -> bool {
    read_encryption(path, cx, |encryption, relative| {
        encryption.is_unlocked(relative)
    })
    .unwrap_or(false)
}

/// How lists should show the note at `path`; `None` for a plain note.
pub fn note_mark(path: &Path, cx: &App) -> Option<NoteMark> {
    let vaults = cx.try_global::<EncryptedVaults>()?;
    let (index, relative) = vaults.locate(path)?;
    let vault = &vaults.vaults[index];
    let visibility = *vault.notes.get(&relative)?;
    let unlocked = vault.encryption.is_unlocked(&relative);
    let shown_name = (visibility == NameVisibility::Hidden).then(|| {
        vault
            .encryption
            .title(&relative)
            .filter(|_| unlocked)
            .map_or_else(|| LOCKED_NOTE_NAME.to_owned(), str::to_owned)
    });
    Some(NoteMark {
        unlocked,
        shown_name,
    })
}

/// Records whether the note at `path` is encrypted (`Some`, with its name
/// visibility) or not.
pub fn mark(path: &Path, visibility: Option<NameVisibility>, cx: &mut App) {
    let Some(vaults) = cx.try_global::<EncryptedVaults>() else {
        return;
    };
    let Some((index, relative)) = vaults.locate(path) else {
        return;
    };
    if vaults.vaults[index].notes.get(&relative).copied() == visibility {
        return;
    }
    let notes = &mut cx.global_mut::<EncryptedVaults>().vaults[index].notes;
    match visibility {
        Some(visibility) => notes.insert(relative, visibility),
        None => notes.remove(&relative),
    };
}

/// Follows a note moved from `from` to `to`: its keys and its mark go
/// with it.
pub fn moved(from: &Path, to: &Path, cx: &mut App) {
    let Some(vaults) = cx.try_global::<EncryptedVaults>() else {
        return;
    };
    let (Some((index, old)), Some((_, new))) = (vaults.locate(from), vaults.locate(to)) else {
        return;
    };
    let vault = &mut cx.global_mut::<EncryptedVaults>().vaults[index];
    vault.encryption.rename(&old, &new);
    if let Some(visibility) = vault.notes.remove(&old) {
        vault.notes.insert(new, visibility);
    }
}

/// Reads again whether each changed file is encrypted, after the watcher
/// saw it change, move in or go. The files are read off the main thread.
pub fn files_changed(changed: &[PathBuf], removed: &[PathBuf], cx: &mut App) {
    for path in removed {
        forget_under(path, cx);
    }
    if changed.is_empty() || cx.try_global::<EncryptedVaults>().is_none() {
        return;
    }
    let changed = changed.to_vec();
    let reading = cx.background_spawn(async move { marks_on_disk(&changed) });
    cx.spawn(async move |cx| {
        let marks = reading.await;
        cx.update(|cx| {
            for (path, visibility) in marks {
                mark(&path, visibility, cx);
            }
        })
        .ok();
    })
    .detach();
}

/// Whether each of `paths` is an encrypted note now, and every
/// encrypted note in those that are folders.
fn marks_on_disk(paths: &[PathBuf]) -> Vec<(PathBuf, Option<NameVisibility>)> {
    let mut marks = Vec::new();
    for path in paths {
        if path.is_dir() {
            marks.extend(
                encrypted_notes(path)
                    .into_iter()
                    .map(|(relative, visibility)| (path.join(relative), Some(visibility))),
            );
        } else if is_note_file(path) {
            marks.push((path.clone(), name_visibility_on_disk(path)));
        }
    }
    marks
}

/// Forgets the marks of the note at `path`, or of every note under it
/// when it was a folder.
fn forget_under(path: &Path, cx: &mut App) {
    let Some(vaults) = cx.try_global::<EncryptedVaults>() else {
        return;
    };
    let Some((index, relative)) = vaults.locate(path) else {
        return;
    };
    let folder = format!("{relative}/");
    let notes = &vaults.vaults[index].notes;
    if !notes
        .keys()
        .any(|note| *note == relative || note.starts_with(&folder))
    {
        return;
    }
    cx.global_mut::<EncryptedVaults>().vaults[index]
        .notes
        .retain(|note, _| *note != relative && !note.starts_with(&folder));
}

fn is_note_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "md")
}

/// Keeps `doc` among the notes [`lock_vault`] locks.
pub fn register_doc(doc: &Entity<NoteDoc>, cx: &mut App) {
    let path = doc.read(cx).path().to_path_buf();
    let Some(vaults) = cx.try_global::<EncryptedVaults>() else {
        return;
    };
    let Some((index, _)) = vaults.locate(&path) else {
        return;
    };
    let docs = &mut cx.global_mut::<EncryptedVaults>().vaults[index].docs;
    docs.retain(|known| known.upgrade().is_some());
    docs.push(doc.downgrade());
}

/// The open notes of the vault `path` is in.
fn vault_docs(path: &Path, cx: &App) -> Vec<Entity<NoteDoc>> {
    let Some(vaults) = cx.try_global::<EncryptedVaults>() else {
        return Vec::new();
    };
    let Some((index, _)) = vaults.locate(path) else {
        return Vec::new();
    };
    vaults.vaults[index]
        .docs
        .iter()
        .filter_map(WeakEntity::upgrade)
        .collect()
}

/// Locks every encrypted note of the vault `path` is in: open ones save
/// (sealed, while their keys are still in hand) and show their locked
/// view, then every key is forgotten.
pub fn lock_vault(path: &Path, cx: &mut App) {
    for doc in vault_docs(path, cx) {
        doc.update(cx, |doc, cx| doc.relock(cx));
    }
    with_encryption(path, cx, |encryption, _| encryption.lock_all());
    with_vault(path, cx, |vault| vault.lock_epoch += 1);
    cx.refresh_windows();
}

/// Runs `action` on the open vault `path` is in.
fn with_vault<R>(
    path: &Path,
    cx: &mut App,
    action: impl FnOnce(&mut VaultLocks) -> R,
) -> Option<R> {
    let vaults = cx.try_global::<EncryptedVaults>()?;
    let (index, _) = vaults.locate(path)?;
    Some(action(
        &mut cx.global_mut::<EncryptedVaults>().vaults[index],
    ))
}

/// How many times the vault `path` is in has locked: a password step
/// notes it when it starts, and drops its result if it changed.
pub fn lock_epoch(path: &Path, cx: &App) -> u64 {
    cx.try_global::<EncryptedVaults>()
        .and_then(|vaults| {
            let (index, _) = vaults.locate(path)?;
            Some(vaults.vaults[index].lock_epoch)
        })
        .unwrap_or_default()
}

/// Marks the vault `path` is in as changing its password, or done.
pub fn set_changing_password(path: &Path, changing: bool, cx: &mut App) {
    with_vault(path, cx, |vault| vault.changing_password = changing);
}

/// Saves the open notes at `relative_paths` of the vault `path` is in
/// again, sealed with the keys in hand now, as after a password change.
pub fn reseal_notes(path: &Path, relative_paths: &[String], cx: &mut App) {
    for doc in vault_docs(path, cx) {
        let relative = cx
            .try_global::<EncryptedVaults>()
            .and_then(|vaults| vaults.locate(doc.read(cx).path()))
            .map(|(_, relative)| relative);
        if relative.is_some_and(|relative| relative_paths.contains(&relative)) {
            doc.update(cx, |doc, cx| doc.reseal(cx)).ok();
        }
    }
}

/// After a password or Touch ID opened the vault, every locked note the
/// vault's key opens shows its text.
pub fn reopen_locked_notes(path: &Path, cx: &mut App) {
    for doc in vault_docs(path, cx) {
        doc.update(cx, |doc, cx| doc.reopen(cx));
    }
    cx.refresh_windows();
}

/// Whether any note of the vault `path` is unlocked now.
pub fn has_unlocked_notes(path: &Path, cx: &App) -> bool {
    vault_docs(path, cx)
        .iter()
        .any(|doc| doc.read(cx).is_unlocked_encrypted())
}

/// The id Claude's connector keeps the open note at `path`'s key under.
pub fn claude_key_id(path: &Path, cx: &App) -> Option<String> {
    read_encryption(path, cx, |encryption, relative| {
        encryption.claude_key(relative).map(|(id, _)| id)
    })
    .flatten()
}

/// Gives Claude's connector the open note at `path`'s own key while its
/// Claude access is "always", and takes it back otherwise. The vault's key
/// never goes to Claude.
pub fn follow_claude_access(path: &Path, cx: &mut App) {
    let found = read_encryption(path, cx, |encryption, relative| {
        let (id, key) = encryption.claude_key(relative)?;
        let always = encryption.options(relative)?.claude == gasp_crypt::ClaudeAccess::Always;
        Some((id, key.clone(), always))
    })
    .flatten();
    let Some((id, key, always)) = found else {
        return;
    };
    match always {
        true => keep_for_claude(id, key, cx),
        false => forget_for_claude(id, cx),
    }
}

fn keep_for_claude(id: String, key: gasp_crypt::Key, cx: &App) {
    if !keychain::keeps_keys() {
        return;
    }
    cx.background_spawn(async move {
        if let Err(error) = gasp_mcp::claude_keys::remember(&id, &key) {
            eprintln!("could not keep a key for Claude: {error}");
        }
    })
    .detach();
}

/// Takes the key kept for Claude under `id` back.
pub fn forget_for_claude(id: String, cx: &App) {
    if !keychain::keeps_keys() {
        return;
    }
    cx.background_spawn(async move { gasp_mcp::claude_keys::forget(&id).ok() })
        .detach();
}

/// Reads the vault password's lock file again, as after sync brought in
/// a password changed on another device. When the vault's key no longer
/// fits, every encrypted note locks (saving first) to ask for the new
/// password. Does nothing while this app changes the password itself.
pub fn lock_file_changed(path: &Path, cx: &mut App) {
    let Some((changing, was_unlocked)) = read_vault(path, cx, |vault| {
        (
            vault.changing_password,
            vault.encryption.is_vault_unlocked(),
        )
    }) else {
        return;
    };
    if changing {
        return;
    }
    with_encryption(path, cx, |encryption, _| encryption.reload_lock());
    let still_unlocked =
        read_encryption(path, cx, |encryption, _| encryption.is_vault_unlocked()).unwrap_or(false);
    if was_unlocked && !still_unlocked {
        lock_vault(path, cx);
    }
}

fn read_vault<R>(path: &Path, cx: &App, action: impl FnOnce(&VaultLocks) -> R) -> Option<R> {
    let vaults = cx.try_global::<EncryptedVaults>()?;
    let (index, _) = vaults.locate(path)?;
    Some(action(&vaults.vaults[index]))
}

/// What to call the note at `path` everywhere it's named: its sealed
/// title while it's unlocked and its file name is hidden, "Locked note"
/// while it's locked, and otherwise its file name.
pub fn shown_title(path: &Path, cx: &App) -> String {
    note_mark(path, cx)
        .and_then(|mark| mark.shown_name)
        .unwrap_or_else(|| crate::workspace::files::note_title(path))
}

/// Whether the note at `path` is encrypted with its file name hidden.
pub fn hides_name(path: &Path, cx: &App) -> bool {
    note_mark(path, cx).is_some_and(|mark| mark.shown_name.is_some())
}

/// The shown title of every encrypted note under `vault` whose file
/// name is hidden, by vault-relative path.
pub fn shown_names(vault: &Path, cx: &App) -> HashMap<String, String> {
    let Some(hidden) = read_vault(vault, cx, |locks| {
        locks
            .notes
            .iter()
            .filter(|(_, visibility)| **visibility == NameVisibility::Hidden)
            .map(|(relative, _)| locks.root.join(relative))
            .collect::<Vec<_>>()
    }) else {
        return HashMap::new();
    };
    hidden
        .into_iter()
        .filter_map(|path| {
            let relative = crate::vault_index::vault_relative(vault, &path)?;
            Some((relative, shown_title(&path, cx)))
        })
        .collect()
}

/// Gives the unlocked note at `path`, whose file name is hidden, a new
/// title sealed inside it. The file keeps its stand-in name.
pub fn set_title(path: &Path, title: &str, cx: &mut App) -> Result<(), String> {
    with_encryption(path, cx, |encryption, relative| {
        encryption.set_title(relative, title)
    })
    .unwrap_or(Err(gasp_vault::encryption::EncryptionError::VaultLocked))
    .map_err(|error| error.to_string())?;
    cx.refresh_windows();
    Ok(())
}
