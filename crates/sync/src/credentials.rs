use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use crate::error::SyncResult;

/// A personal access token for an HTTPS remote. Debug output never shows it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    pub fn secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Token(<redacted>)")
    }
}

/// Where a device keeps the token for each remote.
///
/// Platform implementations (Keychain, Credential Manager, Secret Service)
/// live outside this crate; they only need to implement this trait.
pub trait CredentialStore: Send + Sync {
    fn load(&self, remote_url: &str) -> SyncResult<Option<Token>>;
    fn save(&self, remote_url: &str, token: &Token) -> SyncResult<()>;
    fn delete(&self, remote_url: &str) -> SyncResult<()>;
}

/// A store that forgets everything when dropped, for tests and first-run setup.
#[derive(Default)]
pub struct InMemoryCredentialStore {
    tokens: Mutex<HashMap<String, Token>>,
}

impl InMemoryCredentialStore {
    fn tokens(&self) -> std::sync::MutexGuard<'_, HashMap<String, Token>> {
        self.tokens
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn load(&self, remote_url: &str) -> SyncResult<Option<Token>> {
        Ok(self.tokens().get(remote_url).cloned())
    }

    fn save(&self, remote_url: &str, token: &Token) -> SyncResult<()> {
        self.tokens().insert(remote_url.to_owned(), token.clone());
        Ok(())
    }

    fn delete(&self, remote_url: &str) -> SyncResult<()> {
        self.tokens().remove(remote_url);
        Ok(())
    }
}

/// The name tokens are filed under in the system's credential store, keyed
/// by the remote's URL.
pub const KEYCHAIN_SERVICE: &str = concat!(editor_config::command_name!(), "-sync");

/// The name tokens were filed under before the app was called Gasp.
pub const LEGACY_KEYCHAIN_SERVICE: &str = "editor-sync";

/// Tokens in `current`, where a token found only in `legacy` moves the
/// first time it's read.
pub struct MigratingStore<Current, Legacy> {
    pub current: Current,
    pub legacy: Legacy,
}

impl<Current: CredentialStore, Legacy: CredentialStore> CredentialStore
    for MigratingStore<Current, Legacy>
{
    fn load(&self, remote_url: &str) -> SyncResult<Option<Token>> {
        if let Some(token) = self.current.load(remote_url)? {
            return Ok(Some(token));
        }
        let Some(token) = self.legacy.load(remote_url)? else {
            return Ok(None);
        };
        self.current.save(remote_url, &token)?;
        // A legacy entry left behind is harmless: `current` answers first from now on.
        let _ = self.legacy.delete(remote_url);
        Ok(Some(token))
    }

    fn save(&self, remote_url: &str, token: &Token) -> SyncResult<()> {
        self.current.save(remote_url, token)
    }

    fn delete(&self, remote_url: &str) -> SyncResult<()> {
        self.current.delete(remote_url)?;
        self.legacy.delete(remote_url)
    }
}

/// The system's credential store: the Keychain on macOS and iOS,
/// Credential Manager on Windows. Tokens filed under
/// [`LEGACY_KEYCHAIN_SERVICE`] move to [`KEYCHAIN_SERVICE`] when read.
#[cfg(all(
    feature = "keychain",
    any(target_os = "macos", target_os = "ios", target_os = "windows")
))]
pub struct KeychainStore;

#[cfg(all(
    feature = "keychain",
    any(target_os = "macos", target_os = "ios", target_os = "windows")
))]
impl KeychainStore {
    fn migrating() -> MigratingStore<KeychainService, KeychainService> {
        MigratingStore {
            current: KeychainService(KEYCHAIN_SERVICE),
            legacy: KeychainService(LEGACY_KEYCHAIN_SERVICE),
        }
    }
}

#[cfg(all(
    feature = "keychain",
    any(target_os = "macos", target_os = "ios", target_os = "windows")
))]
impl CredentialStore for KeychainStore {
    fn load(&self, remote_url: &str) -> SyncResult<Option<Token>> {
        Self::migrating().load(remote_url)
    }

    fn save(&self, remote_url: &str, token: &Token) -> SyncResult<()> {
        Self::migrating().save(remote_url, token)
    }

    fn delete(&self, remote_url: &str) -> SyncResult<()> {
        Self::migrating().delete(remote_url)
    }
}

/// The tokens filed under one service name in the system's credential store.
#[cfg(all(
    feature = "keychain",
    any(target_os = "macos", target_os = "ios", target_os = "windows")
))]
struct KeychainService(&'static str);

#[cfg(all(
    feature = "keychain",
    any(target_os = "macos", target_os = "ios", target_os = "windows")
))]
impl KeychainService {
    fn entry(&self, remote_url: &str) -> SyncResult<keyring::Entry> {
        keyring::Entry::new(self.0, remote_url).map_err(keychain_error)
    }
}

#[cfg(all(
    feature = "keychain",
    any(target_os = "macos", target_os = "ios", target_os = "windows")
))]
fn keychain_error(error: keyring::Error) -> crate::error::SyncError {
    crate::error::SyncError::Io(std::io::Error::other(error.to_string()))
}

#[cfg(all(
    feature = "keychain",
    any(target_os = "macos", target_os = "ios", target_os = "windows")
))]
impl CredentialStore for KeychainService {
    fn load(&self, remote_url: &str) -> SyncResult<Option<Token>> {
        match self.entry(remote_url)?.get_password() {
            Ok(secret) => Ok(Some(Token::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain_error(error)),
        }
    }

    fn save(&self, remote_url: &str, token: &Token) -> SyncResult<()> {
        self.entry(remote_url)?
            .set_password(token.secret())
            .map_err(keychain_error)
    }

    fn delete(&self, remote_url: &str) -> SyncResult<()> {
        match self.entry(remote_url)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain_error(error)),
        }
    }
}

/// GitHub accepts any user name with a token; this is the conventional one.
const TOKEN_USER: &str = "x-access-token";

/// Remote callbacks that answer credential requests with `token`, once.
///
/// libgit2 calls the credential callback again after a rejected attempt, so
/// the second call fails instead of looping forever.
pub(crate) fn remote_callbacks<'a>(token: Option<&'a Token>) -> git2::RemoteCallbacks<'a> {
    let mut callbacks = git2::RemoteCallbacks::new();
    let mut attempts = 0;
    callbacks.credentials(move |_url, username, _allowed| {
        attempts += 1;
        let Some(token) = token.filter(|_| attempts == 1) else {
            return Err(git2::Error::new(
                git2::ErrorCode::Auth,
                git2::ErrorClass::Http,
                "no usable token for this remote",
            ));
        };
        git2::Cred::userpass_plaintext(username.unwrap_or(TOKEN_USER), token.secret())
    });
    callbacks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_hides_the_secret() {
        let token = Token::new("synthetic-secret");
        assert!(!format!("{token:?}").contains("synthetic-secret"));
    }

    #[test]
    fn in_memory_store_round_trips() {
        let store = InMemoryCredentialStore::default();
        let url = "https://example.invalid/vault.git";
        assert_eq!(store.load(url).unwrap(), None);
        store.save(url, &Token::new("abc")).unwrap();
        assert_eq!(store.load(url).unwrap(), Some(Token::new("abc")));
        store.delete(url).unwrap();
        assert_eq!(store.load(url).unwrap(), None);
    }

    fn migrating() -> MigratingStore<InMemoryCredentialStore, InMemoryCredentialStore> {
        MigratingStore {
            current: InMemoryCredentialStore::default(),
            legacy: InMemoryCredentialStore::default(),
        }
    }

    #[test]
    fn a_legacy_token_moves_to_the_current_store_when_read() {
        let store = migrating();
        let url = "https://example.invalid/vault.git";
        store.legacy.save(url, &Token::new("old")).unwrap();

        assert_eq!(store.load(url).unwrap(), Some(Token::new("old")));
        assert_eq!(store.current.load(url).unwrap(), Some(Token::new("old")));
        assert_eq!(store.legacy.load(url).unwrap(), None);
    }

    #[test]
    fn a_current_token_wins_over_a_legacy_one() {
        let store = migrating();
        let url = "https://example.invalid/vault.git";
        store.current.save(url, &Token::new("new")).unwrap();
        store.legacy.save(url, &Token::new("old")).unwrap();

        assert_eq!(store.load(url).unwrap(), Some(Token::new("new")));
        assert_eq!(store.legacy.load(url).unwrap(), Some(Token::new("old")));
    }

    #[test]
    fn deleting_a_token_removes_it_under_both_names() {
        let store = migrating();
        let url = "https://example.invalid/vault.git";
        store.legacy.save(url, &Token::new("old")).unwrap();
        store.save(url, &Token::new("new")).unwrap();

        store.delete(url).unwrap();
        assert_eq!(store.load(url).unwrap(), None);
    }

    #[test]
    fn the_keychain_service_is_named_after_the_app() {
        assert_eq!(KEYCHAIN_SERVICE, "gasp-sync");
    }
}
