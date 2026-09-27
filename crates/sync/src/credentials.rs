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
}
