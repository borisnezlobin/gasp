//! Where this device keeps the GitHub token for each remote: the Keychain
//! on macOS and Credential Manager on Windows. On Linux, where the Secret
//! Service needs D-Bus libraries that aren't always there, it's a file in
//! the user's config folder that only the user can read. Never the vault,
//! which syncs.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use editor_sync::KeychainStore;
use editor_sync::{CredentialStore, SyncError, SyncResult, Token};

/// The name tokens are filed under in the system's credential store.
pub const SERVICE: &str = editor_sync::KEYCHAIN_SERVICE;

/// The store this platform uses.
pub fn default_store() -> Arc<dyn CredentialStore> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        Arc::new(KeychainStore)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let folder = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
        Arc::new(FileStore::new(
            folder.join("editor").join("credentials.toml"),
        ))
    }
}

/// Where the store keeps tokens, as the settings screen tells the person.
pub fn store_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "the Keychain"
    } else if cfg!(target_os = "windows") {
        "Credential Manager"
    } else {
        "a file only you can read in your config folder"
    }
}

fn io_error(error: impl std::fmt::Display) -> SyncError {
    SyncError::Io(io::Error::other(error.to_string()))
}

/// Tokens in a TOML file (remote URL = token) readable only by its owner.
pub struct FileStore {
    path: PathBuf,
    /// Serializes read-modify-write cycles within this process.
    lock: Mutex<()>,
}

impl FileStore {
    pub fn new(path: PathBuf) -> Self {
        FileStore {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn read(&self) -> SyncResult<BTreeMap<String, String>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text).map_err(io_error),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, tokens: &BTreeMap<String, String>) -> SyncResult<()> {
        let text = toml::to_string(tokens).map_err(io_error)?;
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let temporary = self.path.with_extension("toml.tmp");
        write_private(&temporary, text.as_bytes())?;
        std::fs::rename(&temporary, &self.path)?;
        Ok(())
    }

    fn edit(&self, change: impl FnOnce(&mut BTreeMap<String, String>)) -> SyncResult<()> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut tokens = self.read()?;
        change(&mut tokens);
        self.write(&tokens)
    }
}

/// Creates `path` readable and writable by its owner only, before any
/// secret is written into it.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let _ = std::fs::remove_file(path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

impl CredentialStore for FileStore {
    fn load(&self, remote_url: &str) -> SyncResult<Option<Token>> {
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(self.read()?.get(remote_url).cloned().map(Token::new))
    }

    fn save(&self, remote_url: &str, token: &Token) -> SyncResult<()> {
        self.edit(|tokens| {
            tokens.insert(remote_url.to_owned(), token.secret().to_owned());
        })
    }

    fn delete(&self, remote_url: &str) -> SyncResult<()> {
        self.edit(|tokens| {
            tokens.remove(remote_url);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_store_round_trips_and_only_its_owner_can_read_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().join("editor/credentials.toml"));
        let url = "https://example.invalid/notes.git";
        assert_eq!(store.load(url).unwrap(), None);
        store.save(url, &Token::new("synthetic-token")).unwrap();
        store
            .save("https://example.invalid/other.git", &Token::new("second"))
            .unwrap();
        assert_eq!(
            store.load(url).unwrap(),
            Some(Token::new("synthetic-token"))
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(store.path())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        store.delete(url).unwrap();
        assert_eq!(store.load(url).unwrap(), None);
        assert!(
            store
                .load("https://example.invalid/other.git")
                .unwrap()
                .is_some()
        );
    }
}
