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
    is_encrypted, open, peek, rewrap, seal,
};

use crate::files::atomic_write;
use crate::index::is_note_path;
use crate::ops::vault_files;

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
    pub fn set_vault_password(&mut self, password: &str) -> Result<(), EncryptionError> {
        if self.lock.is_some() {
            return Err(EncryptionError::HasVaultPassword);
        }
        require_password(password)?;
        let (lock, key) = VaultLock::create(password, self.params)?;
        self.write_lock(&lock)?;
        self.lock = Some(lock);
        self.vault_key = Some(key);
        Ok(())
    }

    pub fn unlock_vault(&mut self, password: &str) -> Result<(), EncryptionError> {
        let lock = self.lock.as_ref().ok_or(EncryptionError::NoVaultPassword)?;
        self.vault_key = Some(lock.unlock(password)?);
        Ok(())
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

    /// A new vault password: every note it opens gets its key rewrapped,
    /// and the lock file changes last. Returns how many notes changed.
    pub fn change_vault_password(
        &mut self,
        old: &str,
        new: &str,
    ) -> Result<usize, EncryptionError> {
        require_password(new)?;
        let lock = self.lock.as_ref().ok_or(EncryptionError::NoVaultPassword)?;
        let old_key = lock.unlock(old)?;
        let (new_lock, new_key) = VaultLock::create(new, self.params)?;
        let changed = self.rewrap_vault_notes(&old_key, &new_key)?;
        self.write_lock(&new_lock)?;
        for note in self.open_notes.values_mut() {
            if note.source == KeySource::Vault {
                note.wrapping_key = new_key.clone();
            }
        }
        self.lock = Some(new_lock);
        self.vault_key = Some(new_key);
        Ok(changed)
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
        let header = peek(raw)?;
        let key = match header.source.note_password_key(password) {
            Some(key) => key?,
            None => {
                self.unlock_vault(password)?;
                self.vault_key.clone().ok_or(EncryptionError::VaultLocked)?
            }
        };
        self.open_with(path, raw, key)
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
    /// file. With `own_password` it has its own; otherwise the vault's,
    /// which must be unlocked.
    pub fn encrypt(
        &mut self,
        path: &str,
        body: &str,
        options: NoteOptions,
        own_password: Option<&str>,
    ) -> Result<String, EncryptionError> {
        let (source, wrapping_key) = self.new_note_source(own_password)?;
        self.open_notes.insert(
            path.to_owned(),
            OpenNote {
                note_key: Key::random(),
                wrapping_key,
                source,
                options,
                title: Some(note_title(path)),
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

    /// Keeps a note's keys with it when it's renamed or moved.
    pub fn rename(&mut self, from: &str, to: &str) {
        if let Some(note) = self.open_notes.remove(from) {
            self.open_notes.insert(to.to_owned(), note);
        }
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

    fn new_note_source(
        &self,
        own_password: Option<&str>,
    ) -> Result<(KeySource, Key), EncryptionError> {
        match own_password {
            Some(password) => {
                require_password(password)?;
                Ok(KeySource::new_note_password(password, self.params)?)
            }
            None if self.lock.is_none() => Err(EncryptionError::NoVaultPassword),
            None => {
                let key = self.vault_key.clone().ok_or(EncryptionError::VaultLocked)?;
                Ok((KeySource::Vault, key))
            }
        }
    }

    fn rewrap_vault_notes(&self, old_key: &Key, new_key: &Key) -> Result<usize, EncryptionError> {
        let mut changed = 0;
        for path in vault_files(&self.root)
            .into_iter()
            .filter(|path| is_note_path(path))
        {
            let absolute = self.root.join(&path);
            let Ok(raw) = std::fs::read_to_string(&absolute) else {
                continue;
            };
            if !opens_with_vault_key(&raw) {
                continue;
            }
            let rewrapped = rewrap(&raw, old_key, new_key)?;
            atomic_write(&absolute, &rewrapped).map_err(|error| io_error(&path, &error))?;
            changed += 1;
        }
        Ok(changed)
    }

    fn write_lock(&self, lock: &VaultLock) -> Result<(), EncryptionError> {
        let path = Self::lock_path(&self.root);
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|error| io_error(LOCK_FILE, &error))?;
        }
        atomic_write(&path, &lock.to_toml()).map_err(|error| io_error(LOCK_FILE, &error))
    }
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
