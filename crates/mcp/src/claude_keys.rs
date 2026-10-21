//! Keys for the encrypted notes Claude may always read.
//!
//! The apps keep each key behind Touch ID, which the connector can't ask
//! for: it runs with no window. When a note is set to let Claude read it
//! always, the app also keeps its key with [`remember`] in an item the
//! connector reads without a prompt. The item's service is [`SERVICE`] and
//! its account is the key's id, as
//! [`gasp_vault::encryption::Encryption::key_id_for`] names it (`vault-…`
//! or `note-…`). [`forget`] removes it when no note needs it any more.
//!
//! A vault key kept here opens every note the vault password seals, so the
//! connector checks each note's sealed options before handing over text.

use std::collections::HashMap;
use std::io;
use std::sync::{Mutex, PoisonError};

use gasp_config::app_name;
use gasp_crypt::Key;

/// The keychain service the connector's keys are filed under.
pub const SERVICE: &str = concat!(app_name!(), " encryption for Claude");

/// Where the connector finds keys by id.
pub trait ClaudeKeys: Send + Sync {
    /// The key named `id`, if one is kept for Claude.
    fn key(&self, id: &str) -> Option<Key>;
}

/// The system's credential store: the Keychain on macOS, Credential
/// Manager on Windows, the Secret Service on Linux.
pub struct KeychainKeys;

impl ClaudeKeys for KeychainKeys {
    fn key(&self, id: &str) -> Option<Key> {
        system::load(id)
    }
}

/// Keys held in memory, for tests.
#[derive(Default)]
pub struct MemoryKeys(Mutex<HashMap<String, Key>>);

impl MemoryKeys {
    pub fn insert(&self, id: &str, key: Key) {
        self.lock().insert(id.to_owned(), key);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Key>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ClaudeKeys for MemoryKeys {
    fn key(&self, id: &str) -> Option<Key> {
        self.lock().get(id).cloned()
    }
}

/// Keeps `key` for Claude under `id`, replacing any key kept there.
pub fn remember(id: &str, key: &Key) -> io::Result<()> {
    system::save(id, key)
}

/// Removes the key kept for Claude under `id`; nothing kept is fine.
pub fn forget(id: &str) -> io::Result<()> {
    system::delete(id)
}

#[cfg(all(
    feature = "keychain",
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "linux"
    )
))]
mod system {
    use std::io;

    use gasp_crypt::Key;

    use super::SERVICE;

    fn entry(id: &str) -> io::Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, id).map_err(io::Error::other)
    }

    pub fn load(id: &str) -> Option<Key> {
        entry(id).ok()?.get_secret().ok().and_then(key_from)
    }

    pub fn save(id: &str, key: &Key) -> io::Result<()> {
        entry(id)?.set_secret(key.bytes()).map_err(io::Error::other)
    }

    pub fn delete(id: &str) -> io::Result<()> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(io::Error::other(error)),
        }
    }

    fn key_from(mut secret: Vec<u8>) -> Option<Key> {
        let bytes = <[u8; 32]>::try_from(secret.as_slice()).ok();
        secret.fill(0);
        bytes.map(Key::from_bytes)
    }

    #[cfg(test)]
    #[test]
    fn only_a_whole_key_is_read_back() {
        assert_eq!(key_from(vec![7; 32]), Some(Key::from_bytes([7; 32])));
        assert_eq!(key_from(vec![7; 31]), None);
    }
}

#[cfg(not(all(
    feature = "keychain",
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "linux"
    )
)))]
mod system {
    use std::io;

    use gasp_crypt::Key;

    pub fn load(_id: &str) -> Option<Key> {
        None
    }

    pub fn save(_id: &str, _key: &Key) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "This build of Gasp can't keep keys in the system keychain.",
        ))
    }

    pub fn delete(_id: &str) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_keys_find_what_was_kept() {
        let keys = MemoryKeys::default();
        let key = Key::random();
        keys.insert("vault-abc", key.clone());
        assert_eq!(keys.key("vault-abc"), Some(key));
        assert_eq!(keys.key("note-xyz"), None);
    }
}
