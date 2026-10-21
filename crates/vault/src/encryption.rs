//! Encrypted notes in one vault: the vault password's lock file, the keys
//! unlocked so far, and turning a note's file into text to edit and back.
//! The desktop app, the iPhone (through the FFI) and the MCP server share
//! it, so a note seals and opens the same everywhere. Plain text only
//! lives in the callers' memory; this never writes it to disk.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use gasp_config::CONFIG_DIR;
use gasp_crypt::{
    CryptError, Header, KdfParams, Key, KeySource, LOCK_FILE, NoteOptions, Plain, VaultLock,
    is_encrypted, open, peek, seal,
};

use zeroize::Zeroizing;

use crate::files::atomic_write;

mod jobs;

pub use jobs::{
    ChangedPassword, NEXT_LOCK_FILE, NewPasswordJob, PasswordChangeJob, PasswordKey, UnlockJob,
    VaultUnlockJob,
};

/// Why an encryption step couldn't happen, as a sentence to show.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EncryptionError {
    #[error(transparent)]
    Crypt(#[from] CryptError),
    #[error("Set a vault password first.")]
    NoVaultPassword,
    #[error("This vault already has a password.")]
    HasVaultPassword,
    #[error("Unlock the vault first.")]
    VaultLocked,
    #[error("Choose a password with at least one character.")]
    EmptyPassword,
    #[error("Couldn't save {0}: {1}")]
    Io(String, String),
}

/// A note's text as the app should treat it.
#[derive(Debug, PartialEq, Eq)]
pub enum NoteText {
    /// An ordinary note.
    Plain(String),
    /// An encrypted note there's no key for yet.
    Locked(Header),
    /// An encrypted note, opened.
    Unlocked(Plain),
}

/// What sealing an opened note again takes.
struct OpenNote {
    note_key: Key,
    wrapping_key: Key,
    source: KeySource,
    options: NoteOptions,
    /// Kept inside the seal; it's the note's name when its file's hides it.
    title: Option<String>,
}

/// One vault's encryption: its lock and the keys unlocked so far.
pub struct Encryption {
    root: PathBuf,
    params: KdfParams,
    lock: Option<VaultLock>,
    vault_key: Option<Key>,
    open_notes: HashMap<String, OpenNote>,
}

impl Encryption {
    /// The vault at `root`, with nothing unlocked.
    pub fn load(root: &Path) -> Encryption {
        Self::load_with(root, KdfParams::STANDARD)
    }

    /// As [`Encryption::load`], making new keys with `params`; tests pass
    /// [`KdfParams::QUICK`].
    pub fn load_with(root: &Path, params: KdfParams) -> Encryption {
        let mut encryption = Encryption {
            root: root.to_path_buf(),
            params,
            lock: None,
            vault_key: None,
            open_notes: HashMap::new(),
        };
        encryption.reload_lock();
        encryption
    }

    pub fn lock_path(root: &Path) -> PathBuf {
        root.join(CONFIG_DIR).join(LOCK_FILE)
    }

    /// Reads the lock file again, as after sync changed it. A vault key
    /// the new lock doesn't accept (the password changed elsewhere) is
    /// forgotten.
    pub fn reload_lock(&mut self) {
        self.lock = std::fs::read_to_string(Self::lock_path(&self.root))
            .ok()
            .and_then(|text| VaultLock::from_toml(&text).ok());
        let still_fits = match (&self.lock, &self.vault_key) {
            (Some(lock), Some(key)) => lock.accepts(key),
            _ => false,
        };
        if !still_fits {
            self.vault_key = None;
        }
    }

    pub fn has_vault_password(&self) -> bool {
        self.lock.is_some()
    }

    pub fn is_vault_unlocked(&self) -> bool {
        self.vault_key.is_some()
    }

    /// The vault's key while it's unlocked, for keeping behind Touch ID.
    pub fn vault_key(&self) -> Option<&Key> {
        self.vault_key.as_ref()
    }

    /// Gives a vault with no password its first one, and unlocks it.
    /// Makes the key on this thread; apps use [`Encryption::new_password_job`].
    pub fn set_vault_password(&mut self, password: &str) -> Result<(), EncryptionError> {
        if self.lock.is_some() {
            return Err(EncryptionError::HasVaultPassword);
        }
        let (lock, key) = self.new_password_job(password)?.run_for_vault()?;
        self.finish_first_password(lock, key)
    }

    /// Unlocks the vault, making its key on this thread; apps use
    /// [`Encryption::vault_unlock_job`].
    pub fn unlock_vault(&mut self, password: &str) -> Result<(), EncryptionError> {
        let key = self.vault_unlock_job(password)?.run()?;
        self.unlock_vault_with_key(key)
    }

    /// Making a key for a new password, to run off the main thread.
    pub fn new_password_job(&self, password: &str) -> Result<NewPasswordJob, EncryptionError> {
        NewPasswordJob::new(password, self.params)
    }

    /// Takes a first vault password made by a [`NewPasswordJob`], writes
    /// its lock file and unlocks the vault.
    pub fn finish_first_password(
        &mut self,
        lock: VaultLock,
        key: Key,
    ) -> Result<(), EncryptionError> {
        if self.lock.is_some() {
            return Err(EncryptionError::HasVaultPassword);
        }
        write_lock(&self.root, &lock)?;
        self.lock = Some(lock);
        self.vault_key = Some(key);
        Ok(())
    }

    /// Unlocking the vault with its password, to run off the main thread;
    /// hand its key to [`Encryption::unlock_vault_with_key`].
    pub fn vault_unlock_job(&self, password: &str) -> Result<VaultUnlockJob, EncryptionError> {
        let lock = self.lock.clone().ok_or(EncryptionError::NoVaultPassword)?;
        Ok(VaultUnlockJob {
            lock,
            password: Zeroizing::new(password.to_owned()),
        })
    }

    /// Unlocking the encrypted note `raw` with a password, to run off the
    /// main thread; hand its key to [`Encryption::finish_unlock`].
    pub fn unlock_job(&self, raw: &str, password: &str) -> Result<UnlockJob, EncryptionError> {
        Ok(UnlockJob {
            source: jobs::source_of(raw)?,
            lock: self.lock.clone(),
            password: Zeroizing::new(password.to_owned()),
        })
    }

    /// Opens the note `raw` at `path` with what an [`UnlockJob`] made; the
    /// vault password also unlocks the vault.
    pub fn finish_unlock(
        &mut self,
        path: &str,
        raw: &str,
        key: PasswordKey,
    ) -> Result<Plain, EncryptionError> {
        let key = match key {
            PasswordKey::Vault(key) => {
                self.unlock_vault_with_key(key.clone())?;
                key
            }
            PasswordKey::Note(key) => key,
        };
        self.open_with(path, raw, key)
    }

    /// Unlocks with a key kept from an earlier unlock, as Touch ID gives.
    pub fn unlock_vault_with_key(&mut self, key: Key) -> Result<(), EncryptionError> {
        let lock = self.lock.as_ref().ok_or(EncryptionError::NoVaultPassword)?;
        if !lock.accepts(&key) {
            return Err(CryptError::WrongPassword.into());
        }
        self.vault_key = Some(key);
        Ok(())
    }

    /// A name for the vault password's key in the system keychain.
    pub fn vault_key_id(&self) -> Option<String> {
        self.lock.as_ref().map(VaultLock::id)
    }

    /// The key that opens the unlocked note at `path`, and its keychain
    /// name: the vault's, or the note password's.
    pub fn wrapping_key(&self, path: &str) -> Option<(String, &Key)> {
        let note = self.open_notes.get(path)?;
        let id = note.source.id().or_else(|| self.vault_key_id())?;
        Some((id, &note.wrapping_key))
    }

    /// The unlocked note's own key and its keychain name, for keeping it
    /// where Claude's connector can read it: that key opens this one note.
    pub fn claude_key(&self, path: &str) -> Option<(String, &Key)> {
        let note = self.open_notes.get(path)?;
        Some((gasp_crypt::claude_key_id(&note.note_key), &note.note_key))
    }

    /// The keychain name of the key that opens the note `raw`, without
    /// opening it.
    pub fn key_id_for(&self, raw: &str) -> Option<String> {
        let header = peek(raw).ok()?;
        header.source.id().or_else(|| self.vault_key_id())
    }

    /// Opens the note `raw` at `path` with a key kept in the keychain.
    pub fn unlock_note_with_key(
        &mut self,
        path: &str,
        raw: &str,
        key: Key,
    ) -> Result<Plain, EncryptionError> {
        self.open_with(path, raw, key)
    }

    /// A new vault password, all on this thread: every note it opens gets
    /// its key rewrapped, and the lock file changes last. Returns how many
    /// notes changed. Apps use [`Encryption::password_change_job`].
    pub fn change_vault_password(
        &mut self,
        old: &str,
        new: &str,
    ) -> Result<usize, EncryptionError> {
        let changed = self.password_change_job(old, new)?.run()?;
        let count = changed.changed;
        self.finish_password_change(changed);
        Ok(count)
    }

    /// Changing the vault password, to run off the main thread.
    pub fn password_change_job(
        &self,
        old: &str,
        new: &str,
    ) -> Result<PasswordChangeJob, EncryptionError> {
        require_password(new)?;
        let lock = self.lock.clone().ok_or(EncryptionError::NoVaultPassword)?;
        Ok(PasswordChangeJob {
            root: self.root.clone(),
            lock,
            params: self.params,
            old: Zeroizing::new(old.to_owned()),
            new: Zeroizing::new(new.to_owned()),
        })
    }

    /// Takes the new password a [`PasswordChangeJob`] made. Returns the
    /// open notes the vault password seals: save each again, since a save
    /// while the job ran was sealed with the old key.
    pub fn finish_password_change(&mut self, changed: ChangedPassword) -> Vec<String> {
        let mut resave = Vec::new();
        for (path, note) in &mut self.open_notes {
            if note.source == KeySource::Vault {
                note.wrapping_key = changed.key.clone();
                resave.push(path.clone());
            }
        }
        self.lock = Some(changed.lock);
        self.vault_key = Some(changed.key);
        resave.sort();
        resave
    }

    /// Forgets every key, so every encrypted note needs unlocking again.
    pub fn lock_all(&mut self) {
        self.vault_key = None;
        self.open_notes.clear();
    }

    /// What `raw`, the file at `path`, holds. An encrypted note opens
    /// when a key for it is in hand.
    pub fn read(&mut self, path: &str, raw: &str) -> NoteText {
        if !is_encrypted(raw) {
            return NoteText::Plain(raw.to_owned());
        }
        let Ok(header) = peek(raw) else {
            return NoteText::Plain(raw.to_owned());
        };
        match self.key_for(path, &header) {
            Some(key) => self
                .open_with(path, raw, key)
                .map_or(NoteText::Locked(header), NoteText::Unlocked),
            None => NoteText::Locked(header),
        }
    }

    /// Opens the encrypted note `raw` at `path` with `password`: the note's
    /// own password, or the vault password, which also unlocks the vault.
    pub fn unlock_note(
        &mut self,
        path: &str,
        raw: &str,
        password: &str,
    ) -> Result<Plain, EncryptionError> {
        let key = self.unlock_job(raw, password)?.run()?;
        self.finish_unlock(path, raw, key)
    }

    /// The file for the unlocked note at `path` holding `body`; `None`
    /// when `path` isn't an unlocked encrypted note, so it's saved as is.
    pub fn seal(&self, path: &str, body: &str) -> Option<String> {
        let note = self.open_notes.get(path)?;
        let plain = Plain {
            body: body.to_owned(),
            title: note.title.clone(),
        };
        Some(seal(
            &plain,
            note.options,
            note.source.clone(),
            &note.wrapping_key,
            &note.note_key,
        ))
    }

    /// Makes the note at `path` encrypted from now on and returns its
    /// file. With `own_password` it has its own, made on this thread;
    /// otherwise the vault's, which must be unlocked. Apps make a note
    /// password with [`Encryption::new_password_job`] and call
    /// [`Encryption::encrypt_with`].
    pub fn encrypt(
        &mut self,
        path: &str,
        body: &str,
        options: NoteOptions,
        own_password: Option<&str>,
    ) -> Result<String, EncryptionError> {
        let own = match own_password {
            Some(password) => Some(self.new_password_job(password)?.run_for_note()?),
            None => None,
        };
        self.encrypt_with(path, body, options, own)
    }

    /// As [`Encryption::encrypt`], with a note password's source and key
    /// already made by a [`NewPasswordJob`], or `None` for the vault's.
    /// A note that's already encrypted keeps its own key and title, so a
    /// new password changes only what wraps it.
    pub fn encrypt_with(
        &mut self,
        path: &str,
        body: &str,
        options: NoteOptions,
        own: Option<(KeySource, Key)>,
    ) -> Result<String, EncryptionError> {
        let (source, wrapping_key) = match own {
            Some(own) => own,
            None => self.vault_source()?,
        };
        let previous = self.open_notes.remove(path);
        let title = previous.as_ref().and_then(|note| note.title.clone());
        self.open_notes.insert(
            path.to_owned(),
            OpenNote {
                note_key: previous.map_or_else(Key::random, |note| note.note_key),
                wrapping_key,
                source,
                options,
                title: title.or_else(|| Some(note_title(path))),
            },
        );
        Ok(self.seal(path, body).expect("the note was just opened"))
    }

    /// Stops treating `path` as encrypted; the caller saves its plain text.
    pub fn decrypt(&mut self, path: &str) {
        self.open_notes.remove(path);
    }

    /// Changes who may read the unlocked note at `path`, or whether its
    /// name shows, from its next save.
    pub fn set_options(&mut self, path: &str, options: NoteOptions) -> Result<(), EncryptionError> {
        let note = self
            .open_notes
            .get_mut(path)
            .ok_or(EncryptionError::VaultLocked)?;
        note.options = options;
        Ok(())
    }

    /// The title sealed inside the unlocked note at `path`: what to call a
    /// note whose file name is hidden.
    pub fn title(&self, path: &str) -> Option<&str> {
        self.open_notes.get(path)?.title.as_deref()
    }

    pub fn options(&self, path: &str) -> Option<NoteOptions> {
        self.open_notes.get(path).map(|note| note.options)
    }

    pub fn is_unlocked(&self, path: &str) -> bool {
        self.open_notes.contains_key(path)
    }

    /// Keeps a note's keys with it when it's renamed or moved. While its
    /// name shows, the new name is also the title sealed inside, so hiding
    /// the name later keeps the name it had then.
    pub fn rename(&mut self, from: &str, to: &str) {
        let Some(mut note) = self.open_notes.remove(from) else {
            return;
        };
        if note.options.name == gasp_crypt::NameVisibility::Shown {
            note.title = Some(note_title(to));
        }
        self.open_notes.insert(to.to_owned(), note);
    }

    fn key_for(&self, path: &str, header: &Header) -> Option<Key> {
        if let Some(note) = self.open_notes.get(path) {
            return Some(note.wrapping_key.clone());
        }
        match header.source {
            KeySource::Vault => self.vault_key.clone(),
            KeySource::Note { .. } => None,
        }
    }

    fn open_with(&mut self, path: &str, raw: &str, key: Key) -> Result<Plain, EncryptionError> {
        let opened = open(raw, &key)?;
        let title = opened
            .plain
            .title
            .clone()
            .or_else(|| Some(note_title(path)));
        self.open_notes.insert(
            path.to_owned(),
            OpenNote {
                note_key: opened.note_key,
                wrapping_key: key,
                source: opened.header.source.clone(),
                options: opened.header.options,
                title,
            },
        );
        Ok(opened.plain)
    }

    fn vault_source(&self) -> Result<(KeySource, Key), EncryptionError> {
        if self.lock.is_none() {
            return Err(EncryptionError::NoVaultPassword);
        }
        let key = self.vault_key.clone().ok_or(EncryptionError::VaultLocked)?;
        Ok((KeySource::Vault, key))
    }
}

/// Writes the vault password's lock file, making `.gasp` when needed.
fn write_lock(root: &Path, lock: &VaultLock) -> Result<(), EncryptionError> {
    let path = Encryption::lock_path(root);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| io_error(LOCK_FILE, &error))?;
    }
    atomic_write(&path, &lock.to_toml()).map_err(|error| io_error(LOCK_FILE, &error))
}

/// A stand-in file name for a note whose name is hidden, such as
/// `Encrypted 3f9a2c1b.md`.
pub fn hidden_file_name() -> String {
    let random = gasp_crypt::random_salt();
    let hex: String = random[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("Encrypted {hex}.md")
}

fn read_note_file(root: &Path, path: &str) -> Option<String> {
    std::fs::read_to_string(root.join(path)).ok()
}

fn opens_with_vault_key(raw: &str) -> bool {
    is_encrypted(raw) && peek(raw).is_ok_and(|header| header.source == KeySource::Vault)
}

fn require_password(password: &str) -> Result<(), EncryptionError> {
    if password.is_empty() {
        Err(EncryptionError::EmptyPassword)
    } else {
        Ok(())
    }
}

/// The title a note's path gives: its file name without `.md`.
fn note_title(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name).to_owned()
}

fn io_error(what: &str, error: &io::Error) -> EncryptionError {
    EncryptionError::Io(what.to_owned(), error.to_string())
}

#[cfg(test)]
mod tests;
