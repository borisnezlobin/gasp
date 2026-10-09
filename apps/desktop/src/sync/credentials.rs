//! Where this device keeps the GitHub token for each remote: the Keychain
//! on macOS, Credential Manager on Windows and the Secret Service (GNOME
//! Keyring, KWallet) on Linux. A Linux desktop with no keyring running
//! gets a file in the user's config folder that only the user can read,
//! and a token from that file moves to the keyring once there is one.
//! Never the vault, which syncs.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub use gasp_sync::KeychainStore;
use gasp_sync::{CredentialStore, SyncError, SyncResult, Token};

/// The name tokens are filed under in the system's credential store.
pub const SERVICE: &str = gasp_sync::KEYCHAIN_SERVICE;

/// The store this platform uses.
pub fn default_store() -> Arc<dyn CredentialStore> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        Arc::new(KeychainStore)
    }
    #[cfg(target_os = "linux")]
    {
        Arc::new(FallbackStore {
            primary: KeychainStore,
            fallback: file_store(),
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Arc::new(file_store())
    }
}

/// The file in the config folder that holds tokens where there's no
/// credential store.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn file_store() -> FileStore {
    let folder = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
    FileStore::new(
        folder
            .join(gasp_config::APP_FOLDER)
            .join("credentials.toml"),
    )
}

/// Where the store keeps tokens, as the settings screen tells the person.
pub fn store_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "the Keychain"
    } else if cfg!(target_os = "windows") {
        "Credential Manager"
    } else if cfg!(target_os = "linux") {
        "your desktop’s keyring"
    } else {
        "a file only you can read in your config folder"
    }
}

/// Tokens in `primary`, or in `fallback` while `primary` can't be
/// reached, as the Secret Service can't on a desktop with no keyring
/// running. A token found only in `fallback` moves to `primary` when it
/// can.
pub struct FallbackStore<Primary, Fallback> {
    pub primary: Primary,
    pub fallback: Fallback,
}

impl<Primary: CredentialStore, Fallback: CredentialStore> CredentialStore
    for FallbackStore<Primary, Fallback>
{
    fn load(&self, remote_url: &str) -> SyncResult<Option<Token>> {
        match self.primary.load(remote_url) {
            Ok(Some(token)) => Ok(Some(token)),
            Ok(None) => {
                let Some(token) = self.fallback.load(remote_url)? else {
                    return Ok(None);
                };
                if self.primary.save(remote_url, &token).is_ok() {
                    let _ = self.fallback.delete(remote_url);
                }
                Ok(Some(token))
            }
            Err(_) => self.fallback.load(remote_url),
        }
    }

    fn save(&self, remote_url: &str, token: &Token) -> SyncResult<()> {
        if self.primary.save(remote_url, token).is_ok() {
            // An older token left in the file would answer if the keyring
            // later can't be reached.
            let _ = self.fallback.delete(remote_url);
            return Ok(());
        }
        self.fallback.save(remote_url, token)
    }

    fn delete(&self, remote_url: &str) -> SyncResult<()> {
        let primary = self.primary.delete(remote_url);
        let fallback = self.fallback.delete(remote_url);
        match (primary, fallback) {
            (Err(error), Err(_)) => Err(error),
            _ => Ok(()),
        }
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
    use gasp_sync::InMemoryCredentialStore;

    /// A credential store that can't be reached, as the Secret Service
    /// with no keyring running.
    struct Unreachable;

    impl CredentialStore for Unreachable {
        fn load(&self, _: &str) -> SyncResult<Option<Token>> {
            Err(io_error("no keyring"))
        }
        fn save(&self, _: &str, _: &Token) -> SyncResult<()> {
            Err(io_error("no keyring"))
        }
        fn delete(&self, _: &str) -> SyncResult<()> {
            Err(io_error("no keyring"))
        }
    }

    const URL: &str = "https://example.invalid/notes.git";

    #[test]
    fn with_no_keyring_tokens_go_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = FallbackStore {
            primary: Unreachable,
            fallback: FileStore::new(dir.path().join("credentials.toml")),
        };
        store.save(URL, &Token::new("synthetic-token")).unwrap();
        assert_eq!(
            store.load(URL).unwrap(),
            Some(Token::new("synthetic-token"))
        );
        store.delete(URL).unwrap();
        assert_eq!(store.load(URL).unwrap(), None);
    }

    #[test]
    fn a_token_in_the_file_moves_to_the_keyring() {
        let dir = tempfile::tempdir().unwrap();
        let file = FileStore::new(dir.path().join("credentials.toml"));
        file.save(URL, &Token::new("from-the-file")).unwrap();
        let store = FallbackStore {
            primary: InMemoryCredentialStore::default(),
            fallback: file,
        };
        assert_eq!(store.load(URL).unwrap(), Some(Token::new("from-the-file")));
        assert_eq!(
            store.primary.load(URL).unwrap(),
            Some(Token::new("from-the-file"))
        );
        assert_eq!(store.fallback.load(URL).unwrap(), None);
        store.save(URL, &Token::new("newer")).unwrap();
        assert_eq!(store.load(URL).unwrap(), Some(Token::new("newer")));
        assert_eq!(store.fallback.load(URL).unwrap(), None);
    }

    #[test]
    fn the_file_store_round_trips_and_only_its_owner_can_read_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().join("app/credentials.toml"));
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
