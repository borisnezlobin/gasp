//! Encrypted notes on the phone: opening a note as plain, locked or
//! unlocked, unlocking it with a password or a key kept behind Face ID,
//! and encrypting, decrypting and changing a note's options. The plain
//! text of an encrypted note only ever lives in memory; every write goes
//! through the vault's encryption session, which seals it first.

use std::io::Read;
use std::path::Path;
use std::sync::MutexGuard;

use gasp_crypt::{ClaudeAccess, CryptError, Key, NameVisibility, NoteOptions};
use gasp_vault::encryption::{Encryption, EncryptionError, NoteText, hidden_file_name};
use gasp_vault::files::atomic_write;
use gasp_vault::link_update::{file_name, parent_dir};

use crate::vault::{VaultError, VaultFolder};

/// How a note opens.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum OpenedNote {
    /// An ordinary note.
    Plain { text: String },
    /// An encrypted note there's no key for yet.
    Locked { note: LockedNote },
    /// An encrypted note, opened: `text` is its plain text.
    Unlocked { text: String },
}

/// What the locked view needs to know about a locked note.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct LockedNote {
    /// Whether the note has a password of its own rather than the vault's.
    pub has_own_password: bool,
    /// Names the key that opens it in the keychain.
    pub key_id: Option<String>,
}

/// Who may read an encrypted note through Claude's Gasp connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoteClaudeAccess {
    Never,
    WhileUnlocked,
    Always,
}

/// An unlocked encrypted note's options.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EncryptedNoteOptions {
    pub claude: NoteClaudeAccess,
    /// Whether its file has a stand-in name.
    pub hides_name: bool,
    pub has_own_password: bool,
    /// The note's own name, kept inside the seal.
    pub title: String,
}

/// A key to keep in the keychain behind Face ID, and its name there.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct KeptKey {
    pub id: String,
    pub bytes: Vec<u8>,
}

/// Where the vault's password stands.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EncryptionState {
    pub has_vault_password: bool,
    pub vault_unlocked: bool,
}

/// Whether `text` is an encrypted note's file.
#[uniffi::export]
pub fn is_encrypted_text(text: String) -> bool {
    gasp_crypt::is_encrypted(&text)
}

impl From<EncryptionError> for VaultError {
    fn from(error: EncryptionError) -> Self {
        match error {
            EncryptionError::Crypt(CryptError::WrongPassword) => VaultError::WrongPassword {
                message: error.to_string(),
            },
            _ => VaultError::Refused {
                message: error.to_string(),
            },
        }
    }
}

#[uniffi::export]
impl VaultFolder {
    /// The note at `path` as it should open: plain, locked, or opened with
    /// a key already in hand.
    pub fn open_note(&self, path: String) -> Result<OpenedNote, VaultError> {
        let raw = self.read_note(path.clone())?;
        let mut encryption = self.encryption();
        Ok(match encryption.read(&path, &raw) {
            NoteText::Plain(text) => OpenedNote::Plain { text },
            NoteText::Unlocked(plain) => OpenedNote::Unlocked {
                text: plain.body.clone(),
            },
            NoteText::Locked(header) => OpenedNote::Locked {
                note: LockedNote {
                    has_own_password: header.source.id().is_some(),
                    key_id: encryption.key_id_for(&raw),
                },
            },
        })
    }

    /// Opens the encrypted note at `path` with its own password or the
    /// vault's, and answers its plain text.
    pub fn unlock_note(&self, path: String, password: String) -> Result<String, VaultError> {
        let raw = self.read_note(path.clone())?;
        let plain = self.encryption().unlock_note(&path, &raw, &password)?;
        Ok(plain.body.clone())
    }

    /// Opens the encrypted note at `path` with a key kept behind Face ID.
    /// The vault's key unlocks the whole vault, as its password does.
    pub fn unlock_note_with_key(&self, path: String, key: Vec<u8>) -> Result<String, VaultError> {
        let raw = self.read_note(path.clone())?;
        let key = key_from(key)?;
        let mut encryption = self.encryption();
        let opens_vault = encryption.key_id_for(&raw) == encryption.vault_key_id();
        if opens_vault {
            encryption.unlock_vault_with_key(key.clone())?;
        }
        let plain = encryption.unlock_note_with_key(&path, &raw, key)?;
        Ok(plain.body.clone())
    }

    pub fn encryption_state(&self) -> EncryptionState {
        let encryption = self.encryption();
        EncryptionState {
            has_vault_password: encryption.has_vault_password(),
            vault_unlocked: encryption.is_vault_unlocked(),
        }
    }

    /// Gives the vault its first password, and unlocks it.
    pub fn set_vault_password(&self, password: String) -> Result<(), VaultError> {
        Ok(self.encryption().set_vault_password(&password)?)
    }

    pub fn unlock_vault(&self, password: String) -> Result<(), VaultError> {
        Ok(self.encryption().unlock_vault(&password)?)
    }

    /// A new vault password. Answers how many notes it rewrapped.
    pub fn change_vault_password(&self, old: String, new: String) -> Result<u32, VaultError> {
        let changed = self.encryption().change_vault_password(&old, &new)?;
        Ok(changed as u32)
    }

    /// Forgets every key, so every encrypted note needs unlocking again.
    pub fn lock_encrypted_notes(&self) {
        self.encryption().lock_all();
    }

    /// Whether `path` is an encrypted note open in memory.
    pub fn is_note_unlocked(&self, path: String) -> bool {
        self.encryption().is_unlocked(&path)
    }

    /// The key that opened the note at `path`, to keep behind Face ID.
    pub fn note_key(&self, path: String) -> Option<KeptKey> {
        let encryption = self.encryption();
        let (id, key) = encryption.wrapping_key(&path)?;
        Some(KeptKey {
            id,
            bytes: key.bytes().to_vec(),
        })
    }

    /// The vault password's key while the vault is unlocked.
    pub fn vault_key(&self) -> Option<KeptKey> {
        let encryption = self.encryption();
        Some(KeptKey {
            id: encryption.vault_key_id()?,
            bytes: encryption.vault_key()?.bytes().to_vec(),
        })
    }

    /// Encrypts the note at `path`, whose text is `text`, with its own
    /// password or the vault's, starting from the vault's settings for new
    /// encrypted notes. Answers its path, which changes when new notes
    /// hide their names.
    pub fn encrypt_note(
        &self,
        path: String,
        text: String,
        own_password: Option<String>,
    ) -> Result<String, VaultError> {
        let full = self.note_path(&path)?;
        let options = self.new_note_options();
        {
            let mut encryption = self.encryption();
            let sealed = encryption.encrypt(&path, &text, options, own_password.as_deref())?;
            atomic_write(&full, &sealed)?;
        }
        self.reindex(std::slice::from_ref(&path));
        self.forget_snapshots(&path);
        match options.name {
            NameVisibility::Hidden => self.rename_to_hidden_name(&path),
            NameVisibility::Shown => Ok(path),
        }
    }

    /// Saves the encrypted note at `path` as the plain `text` from now on.
    /// A note with a stand-in name gets its own name back. Answers its path.
    pub fn decrypt_note(&self, path: String, text: String) -> Result<String, VaultError> {
        let full = self.note_path(&path)?;
        let hidden_title = {
            let mut encryption = self.encryption();
            let options = encryption
                .options(&path)
                .ok_or(EncryptionError::VaultLocked)?;
            let title = encryption.title(&path).map(str::to_owned);
            encryption.decrypt(&path);
            atomic_write(&full, &text)?;
            title.filter(|_| options.name == NameVisibility::Hidden)
        };
        self.reindex(std::slice::from_ref(&path));
        match hidden_title {
            Some(title) => self.rename_to_free_name(&path, &title),
            None => Ok(path),
        }
    }

    /// The options of the unlocked encrypted note at `path`.
    pub fn encrypted_note_options(&self, path: String) -> Option<EncryptedNoteOptions> {
        let encryption = self.encryption();
        let options = encryption.options(&path)?;
        let own_password = has_own_password(&encryption, &path);
        Some(EncryptedNoteOptions {
            claude: options.claude.into(),
            hides_name: options.name == NameVisibility::Hidden,
            has_own_password: own_password,
            title: encryption
                .title(&path)
                .map_or_else(|| note_title(&path), str::to_owned),
        })
    }

    /// Changes who may read the unlocked note at `path` and whether its
    /// name shows, saving `text` sealed with the new options. Hiding the
    /// name gives the file a stand-in name; showing it gives the note's own
    /// back. Answers its path.
    pub fn set_encrypted_note_options(
        &self,
        path: String,
        text: String,
        claude: NoteClaudeAccess,
        hides_name: bool,
    ) -> Result<String, VaultError> {
        let full = self.note_path(&path)?;
        let name = visibility(hides_name);
        let (was, title) = {
            let mut encryption = self.encryption();
            let was = encryption
                .options(&path)
                .ok_or(EncryptionError::VaultLocked)?
                .name;
            let title = encryption
                .title(&path)
                .map_or_else(|| note_title(&path), str::to_owned);
            let options = NoteOptions {
                claude: claude.into(),
                name,
            };
            encryption.set_options(&path, options)?;
            self.write_sealed(&encryption, &path, &full, &text)?;
            (was, title)
        };
        match (was, name) {
            (NameVisibility::Shown, NameVisibility::Hidden) => self.rename_to_hidden_name(&path),
            (NameVisibility::Hidden, NameVisibility::Shown) => {
                self.rename_to_free_name(&path, &title)
            }
            _ => Ok(path),
        }
    }

    /// Gives the unlocked note at `path` a password of its own, or with
    /// `None` hands it back to the vault password, sealing `text` with the
    /// new key. Its options and name stay as they were.
    pub fn set_note_password(
        &self,
        path: String,
        text: String,
        password: Option<String>,
    ) -> Result<(), VaultError> {
        let full = self.note_path(&path)?;
        let mut encryption = self.encryption();
        let options = encryption
            .options(&path)
            .ok_or(EncryptionError::VaultLocked)?;
        require_new_key(&encryption, password.as_deref())?;
        let title = encryption
            .title(&path)
            .map_or_else(|| note_title(&path), str::to_owned);
        // `encrypt` names a note's sealed title after its path, so it's
        // made under a stand-in path with the right title, then moved.
        let titled = format!("\0/{title}.md");
        encryption.decrypt(&path);
        let sealed = encryption.encrypt(&titled, &text, options, password.as_deref())?;
        encryption.rename(&titled, &path);
        atomic_write(&full, &sealed)?;
        Ok(())
    }
}

impl VaultFolder {
    pub(crate) fn encryption(&self) -> MutexGuard<'_, Encryption> {
        self.encryption
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The file to write for `text` at `path`: sealed when it's an unlocked
    /// encrypted note. Plain text never replaces an encrypted file, such as
    /// a save that arrives after the notes locked.
    pub(crate) fn file_text(
        encryption: &Encryption,
        path: &str,
        full: &Path,
        text: String,
    ) -> Result<String, VaultError> {
        if let Some(sealed) = encryption.seal(path, &text) {
            return Ok(sealed);
        }
        if !gasp_crypt::is_encrypted(&text) && is_encrypted_file(full) {
            return Err(VaultError::Refused {
                message: "This note is locked, so it can't be saved. Unlock it and try again."
                    .to_owned(),
            });
        }
        Ok(text)
    }

    /// Whether `path` is an encrypted note, open or not.
    pub(crate) fn is_encrypted_note(&self, path: &str) -> bool {
        self.encryption().is_unlocked(path) || is_encrypted_file(&self.root.join(path))
    }

    /// The title a note shows: a hidden-name note's own name once it's
    /// unlocked, or "Locked note" until then.
    pub(crate) fn shown_title(&self, path: &str, file_title: String) -> (String, bool) {
        let Some(header) = encrypted_header(&self.root.join(path)) else {
            return (file_title, false);
        };
        if header.options.name == NameVisibility::Shown {
            return (file_title, true);
        }
        let encryption = self.encryption();
        let title = encryption.title(path).map(str::to_owned);
        (title.unwrap_or_else(|| LOCKED_TITLE.to_owned()), true)
    }

    /// Carries unlocked notes' keys along when a note or folder moves from
    /// `from` to `to`.
    pub(crate) fn follow_encrypted_notes(&self, from: &str, to: &str) {
        let mut encryption = self.encryption();
        encryption.rename(from, to);
        let (_, notes) = gasp_vault::build::scan(&self.root, &self.root.join(to));
        for note in notes {
            if let Some(inside) = note.strip_prefix(&format!("{to}/")) {
                encryption.rename(&format!("{from}/{inside}"), &note);
            }
        }
    }

    fn write_sealed(
        &self,
        encryption: &Encryption,
        path: &str,
        full: &Path,
        text: &str,
    ) -> Result<(), VaultError> {
        let sealed = encryption
            .seal(path, text)
            .ok_or(EncryptionError::VaultLocked)?;
        atomic_write(full, &sealed)?;
        Ok(())
    }

    fn new_note_options(&self) -> NoteOptions {
        let settings = &self.config().settings.encryption;
        NoteOptions {
            claude: claude_from_setting(settings.claude_access),
            name: visibility(settings.hide_names),
        }
    }

    fn rename_to_hidden_name(&self, path: &str) -> Result<String, VaultError> {
        let name = hidden_file_name();
        let title = name.strip_suffix(".md").unwrap_or(&name);
        self.rename_note(path.to_owned(), title.to_owned())
    }

    /// Renames the note at `path` to `title`, or `title 2` and so on when
    /// another note has it.
    fn rename_to_free_name(&self, path: &str, title: &str) -> Result<String, VaultError> {
        let folder = Path::new(parent_dir(path));
        let free = gasp_vault::ops::unique_name(
            &self.root,
            folder,
            title,
            gasp_vault::entries::EntryKind::Note,
        );
        self.rename_note(path.to_owned(), free)
    }

    /// Earlier versions of a note that's now encrypted hold its plain
    /// text, so they go.
    fn forget_snapshots(&self, path: &str) {
        let Some(store) = self.snapshot_store() else {
            return;
        };
        for snapshot in store.list(Path::new(path)) {
            std::fs::remove_file(&snapshot.path).ok();
        }
    }
}

/// What a hidden-name note is called until it's unlocked.
const LOCKED_TITLE: &str = "Locked note";

/// How long a key is, in bytes.
const KEY_LEN: usize = 32;

/// More than an encrypted note's header takes.
const HEADER_BYTES: u64 = 2048;

fn encrypted_header(full: &Path) -> Option<gasp_crypt::Header> {
    let head = file_head(full)?;
    if !gasp_crypt::is_encrypted(&head) {
        return None;
    }
    gasp_crypt::peek(&head).ok()
}

fn is_encrypted_file(full: &Path) -> bool {
    file_head(full).is_some_and(|head| gasp_crypt::is_encrypted(&head))
}

fn file_head(full: &Path) -> Option<String> {
    let file = std::fs::File::open(full).ok()?;
    let mut bytes = Vec::new();
    file.take(HEADER_BYTES).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn has_own_password(encryption: &Encryption, path: &str) -> bool {
    let id = encryption.wrapping_key(path).map(|(id, _)| id);
    id.is_some() && id != encryption.vault_key_id()
}

/// Refuses a key change that couldn't finish: an empty password, or the
/// vault's when it's locked.
fn require_new_key(encryption: &Encryption, password: Option<&str>) -> Result<(), VaultError> {
    match password {
        Some("") => Err(EncryptionError::EmptyPassword.into()),
        Some(_) => Ok(()),
        None if !encryption.has_vault_password() => Err(EncryptionError::NoVaultPassword.into()),
        None if !encryption.is_vault_unlocked() => Err(EncryptionError::VaultLocked.into()),
        None => Ok(()),
    }
}

fn key_from(bytes: Vec<u8>) -> Result<Key, VaultError> {
    let bytes: [u8; KEY_LEN] = bytes.try_into().map_err(|_| VaultError::WrongPassword {
        message: CryptError::WrongPassword.to_string(),
    })?;
    Ok(Key::from_bytes(bytes))
}

fn visibility(hides_name: bool) -> NameVisibility {
    match hides_name {
        true => NameVisibility::Hidden,
        false => NameVisibility::Shown,
    }
}

fn note_title(path: &str) -> String {
    let name = file_name(path);
    name.strip_suffix(".md").unwrap_or(name).to_owned()
}

fn claude_from_setting(access: gasp_config::settings::EncryptedClaudeAccess) -> ClaudeAccess {
    use gasp_config::settings::EncryptedClaudeAccess as Setting;
    match access {
        Setting::Never => ClaudeAccess::Never,
        Setting::WhileUnlocked => ClaudeAccess::WhileUnlocked,
        Setting::Always => ClaudeAccess::Always,
    }
}

impl From<ClaudeAccess> for NoteClaudeAccess {
    fn from(access: ClaudeAccess) -> Self {
        match access {
            ClaudeAccess::Never => NoteClaudeAccess::Never,
            ClaudeAccess::WhileUnlocked => NoteClaudeAccess::WhileUnlocked,
            ClaudeAccess::Always => NoteClaudeAccess::Always,
        }
    }
}

impl From<NoteClaudeAccess> for ClaudeAccess {
    fn from(access: NoteClaudeAccess) -> Self {
        match access {
            NoteClaudeAccess::Never => ClaudeAccess::Never,
            NoteClaudeAccess::WhileUnlocked => ClaudeAccess::WhileUnlocked,
            NoteClaudeAccess::Always => ClaudeAccess::Always,
        }
    }
}

#[cfg(test)]
#[path = "encryption_tests.rs"]
mod tests;
