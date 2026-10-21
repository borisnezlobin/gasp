//! The slow half of every password step, apart from the session so it can
//! run off the main thread. Turning a password into a key takes a tenth
//! of a second or more, and a window must never wait on it. A step is a
//! job taken from the session, run anywhere, and handed back:
//!
//! ```ignore
//! let job = session.unlock_job(raw, password)?;   // quick, on the main thread
//! let key = job.run()?;                           // slow, anywhere else
//! let plain = session.finish_unlock(path, raw, key)?;  // quick again
//! ```

use std::path::PathBuf;

use gasp_crypt::{CryptError, KdfParams, Key, KeySource, VaultLock, peek, rewrap};
use zeroize::Zeroizing;

use super::{EncryptionError, opens_with_vault_key, read_note_file, require_password, write_lock};
use crate::files::atomic_write;
use crate::index::is_note_path;
use crate::ops::vault_files;

/// What a password opened: the vault, or one note with its own password.
pub enum PasswordKey {
    Vault(Key),
    Note(Key),
}

/// Unlocking an encrypted note, or the vault through it, with a password.
pub struct UnlockJob {
    pub(super) source: KeySource,
    pub(super) lock: Option<VaultLock>,
    pub(super) password: Zeroizing<String>,
}

impl UnlockJob {
    pub fn run(self) -> Result<PasswordKey, EncryptionError> {
        match self.source.note_password_key(&self.password) {
            Some(key) => Ok(PasswordKey::Note(key?)),
            None => {
                let lock = self.lock.ok_or(EncryptionError::NoVaultPassword)?;
                Ok(PasswordKey::Vault(lock.unlock(&self.password)?))
            }
        }
    }
}

/// Unlocking the vault itself with its password.
pub struct VaultUnlockJob {
    pub(super) lock: VaultLock,
    pub(super) password: Zeroizing<String>,
}

impl VaultUnlockJob {
    pub fn run(self) -> Result<Key, EncryptionError> {
        Ok(self.lock.unlock(&self.password)?)
    }
}

/// Making the key for a password: a vault's first, or a note's own.
pub struct NewPasswordJob {
    pub(super) password: Zeroizing<String>,
    pub(super) params: KdfParams,
}

impl NewPasswordJob {
    pub fn new(password: &str, params: KdfParams) -> Result<NewPasswordJob, EncryptionError> {
        require_password(password)?;
        Ok(NewPasswordJob {
            password: Zeroizing::new(password.to_owned()),
            params,
        })
    }

    /// A vault password's lock and key.
    pub fn run_for_vault(self) -> Result<(VaultLock, Key), EncryptionError> {
        Ok(VaultLock::create(&self.password, self.params)?)
    }

    /// A note password's key source and key.
    pub fn run_for_note(self) -> Result<(KeySource, Key), EncryptionError> {
        Ok(KeySource::new_note_password(&self.password, self.params)?)
    }
}

/// Changing the vault password: both keys made, every note the vault
/// password opens rewrapped, then the new lock file written last.
pub struct PasswordChangeJob {
    pub(super) root: PathBuf,
    pub(super) lock: VaultLock,
    pub(super) params: KdfParams,
    pub(super) old: Zeroizing<String>,
    pub(super) new: Zeroizing<String>,
}

/// A finished password change, for [`super::Encryption::finish_password_change`].
pub struct ChangedPassword {
    pub(super) lock: VaultLock,
    pub(super) key: Key,
    /// How many notes were rewrapped.
    pub changed: usize,
}

/// The new lock, written before any note is rewrapped. A change that
/// stops partway leaves it, and running the change again with the same
/// new password picks up its key instead of making another, so notes
/// already rewrapped still open.
pub const NEXT_LOCK_FILE: &str = "encryption.next.toml";

impl PasswordChangeJob {
    pub fn run(self) -> Result<ChangedPassword, EncryptionError> {
        let old_key = self.lock.unlock(&self.old)?;
        let (lock, key) = self.next_lock()?;
        let changed = rewrap_vault_notes(&self.root, &old_key, &key)?;
        write_lock(&self.root, &lock)?;
        let _ = std::fs::remove_file(next_lock_path(&self.root));
        Ok(ChangedPassword { lock, key, changed })
    }

    fn next_lock(&self) -> Result<(VaultLock, Key), EncryptionError> {
        let unfinished = std::fs::read_to_string(next_lock_path(&self.root))
            .ok()
            .and_then(|text| VaultLock::from_toml(&text).ok())
            .and_then(|lock| lock.unlock(&self.new).ok().map(|key| (lock, key)));
        if let Some(found) = unfinished {
            return Ok(found);
        }
        let (lock, key) = VaultLock::create(&self.new, self.params)?;
        let path = next_lock_path(&self.root);
        atomic_write(&path, &lock.to_toml())
            .map_err(|error| super::io_error(NEXT_LOCK_FILE, &error))?;
        Ok((lock, key))
    }
}

pub fn next_lock_path(root: &std::path::Path) -> PathBuf {
    super::Encryption::lock_path(root).with_file_name(NEXT_LOCK_FILE)
}

/// Rewraps every note the old vault key opens. A note already wrapped by
/// the new key (saved again partway, or by an earlier try that stopped)
/// is left as it is.
fn rewrap_vault_notes(
    root: &std::path::Path,
    old: &Key,
    new: &Key,
) -> Result<usize, EncryptionError> {
    let mut changed = 0;
    for path in vault_files(root)
        .into_iter()
        .filter(|path| is_note_path(path))
    {
        let Some(raw) = read_note_file(root, &path) else {
            continue;
        };
        if !opens_with_vault_key(&raw) {
            continue;
        }
        match rewrap(&raw, old, new) {
            Ok(rewrapped) => {
                write_if_unchanged(root, &path, &raw, &rewrapped)?;
                changed += 1;
            }
            Err(CryptError::WrongPassword) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(changed)
}

/// Writes `new` unless the file stopped holding `old` since it was read:
/// a save in the meantime wins, and the session seals it again with the
/// new key once the change finishes.
fn write_if_unchanged(
    root: &std::path::Path,
    path: &str,
    old: &str,
    new: &str,
) -> Result<(), EncryptionError> {
    if read_note_file(root, path).as_deref() != Some(old) {
        return Ok(());
    }
    atomic_write(&root.join(path), new).map_err(|error| super::io_error(path, &error))
}

/// The header's key source, for building an [`UnlockJob`].
pub(super) fn source_of(raw: &str) -> Result<KeySource, EncryptionError> {
    Ok(peek(raw)?.source)
}
