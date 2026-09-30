//! Signing in with GitHub on the phone: the device flow's code and its
//! polling, the person's repositories, and a new private one for their
//! notes. The requests go out through the app's URLSession, handed in as
//! an [`HttpTransport`], so the core needs no network stack of its own.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use gasp_sync::Token;
use gasp_sync::github::{
    self, GitHub, GitHubError, HttpClient, HttpRequest, HttpResponse, PollState, Repository, SignIn,
};

/// One header of a request.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

/// A request for the app to send.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HttpCall {
    /// `GET` or `POST`.
    pub method: String,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    pub body: Option<String>,
}

/// What came back.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HttpReply {
    pub status: u16,
    pub body: String,
}

/// Why a request got no answer.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum TransportError {
    #[error("{message}")]
    Unreachable { message: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for TransportError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        TransportError::Unreachable {
            message: error.reason,
        }
    }
}

/// Sends requests for the core: the app's URLSession.
#[uniffi::export(with_foreign)]
pub trait HttpTransport: Send + Sync {
    fn send(&self, call: HttpCall) -> Result<HttpReply, TransportError>;
}

struct Transport(Arc<dyn HttpTransport>);

impl HttpClient for Transport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let call = HttpCall {
            method: request.method.as_str().to_owned(),
            url: request.url.clone(),
            headers: request
                .headers
                .iter()
                .map(|(name, value)| HttpHeader {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
            body: request.body.clone(),
        };
        let reply = self.0.send(call).map_err(|error| error.to_string())?;
        Ok(HttpResponse {
            status: reply.status,
            body: reply.body,
        })
    }
}

/// Why something with GitHub didn't work, in words for the person.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum GitHubProblem {
    #[error("{message}")]
    Refused { message: String },
}

impl From<GitHubError> for GitHubProblem {
    fn from(error: GitHubError) -> Self {
        GitHubProblem::Refused {
            message: error.to_string(),
        }
    }
}

/// The client ID of this build's GitHub OAuth App, or nothing when it
/// isn't filled in and GitHub sign-in isn't available.
#[uniffi::export]
pub fn github_client_id() -> Option<String> {
    github::client_id().map(str::to_owned)
}

/// Where someone without a GitHub account makes one.
#[uniffi::export]
pub fn github_sign_up_page() -> String {
    github::SIGN_UP_PAGE.to_owned()
}

/// Where the person types the code.
#[uniffi::export]
pub fn github_device_page() -> String {
    github::DEVICE_PAGE.to_owned()
}

/// What the app does after a poll.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum SignInStep {
    /// Poll again after this many seconds.
    Waiting {
        next_poll_seconds: f64,
    },
    /// Approved. The token goes to setting up sync, which keeps it.
    SignedIn {
        token: String,
    },
    Failed {
        message: String,
    },
}

/// Signing in with a short code: the code to show, and polling GitHub
/// until the person approves it.
#[derive(uniffi::Object)]
pub struct GitHubSignIn {
    transport: Transport,
    sign_in: Mutex<SignIn>,
}

impl GitHubSignIn {
    fn sign_in(&self) -> MutexGuard<'_, SignIn> {
        self.sign_in.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[uniffi::export]
impl GitHubSignIn {
    /// Asks GitHub for a code, as the OAuth App `client_id`.
    #[uniffi::constructor]
    pub fn start(
        transport: Arc<dyn HttpTransport>,
        client_id: String,
    ) -> Result<Arc<Self>, GitHubProblem> {
        let transport = Transport(transport);
        let sign_in = SignIn::start(&transport, &client_id)?;
        Ok(Arc::new(GitHubSignIn {
            transport,
            sign_in: Mutex::new(sign_in),
        }))
    }

    /// The short code the person types on GitHub, such as `WDJB-MJHT`.
    pub fn user_code(&self) -> String {
        self.sign_in().code().user_code.clone()
    }

    /// Where they type it.
    pub fn verification_page(&self) -> String {
        self.sign_in().code().verification_uri.clone()
    }

    /// How long to wait before the first poll.
    pub fn first_wait_seconds(&self) -> f64 {
        self.sign_in().first_wait().as_secs_f64()
    }

    /// Asks GitHub once, `waited_seconds` after the code was handed out.
    pub fn poll(&self, waited_seconds: f64) -> SignInStep {
        let waited = Duration::from_secs_f64(waited_seconds.max(0.));
        match self.sign_in().poll_once(&self.transport, waited) {
            PollState::Waiting { next_poll } => SignInStep::Waiting {
                next_poll_seconds: next_poll.as_secs_f64(),
            },
            PollState::SignedIn(token) => SignInStep::SignedIn {
                token: token.secret().to_owned(),
            },
            PollState::Failed(failure) => SignInStep::Failed {
                message: failure.sentence(),
            },
        }
    }
}

/// One of the person's repositories.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct GitHubRepository {
    /// `owner/name`.
    pub full_name: String,
    pub name: String,
    pub clone_url: String,
    pub private: bool,
    /// The branch sync uses for it; empty means Gasp's usual one.
    pub branch: String,
}

impl From<Repository> for GitHubRepository {
    fn from(repository: Repository) -> Self {
        GitHubRepository {
            full_name: repository.full_name,
            name: repository.name,
            clone_url: repository.clone_url,
            private: repository.private,
            branch: repository.default_branch,
        }
    }
}

impl From<GitHubRepository> for Repository {
    fn from(repository: GitHubRepository) -> Self {
        Repository {
            full_name: repository.full_name,
            name: repository.name,
            clone_url: repository.clone_url,
            private: repository.private,
            default_branch: repository.branch,
        }
    }
}

/// Who signed in, and the repositories they own.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct GitHubAccount {
    pub login: String,
    pub repositories: Vec<GitHubRepository>,
}

/// The person `token` belongs to and their repositories, most recently
/// changed first.
#[uniffi::export]
pub fn github_account(
    transport: Arc<dyn HttpTransport>,
    token: String,
) -> Result<GitHubAccount, GitHubProblem> {
    let transport = Transport(transport);
    let token = Token::new(token);
    let github = GitHub::new(&transport, &token);
    let account = github.account()?;
    let repositories = github.repositories()?;
    Ok(GitHubAccount {
        login: account.login,
        repositories: repositories.into_iter().map(Into::into).collect(),
    })
}

/// Makes a private repository for the notes: `notes`, or `gasp-notes`
/// when `known` has one called that. A new repository has no branch yet,
/// so its `branch` is empty and sync starts Gasp's usual one.
#[uniffi::export]
pub fn github_make_notes_repository(
    transport: Arc<dyn HttpTransport>,
    token: String,
    known: Vec<GitHubRepository>,
) -> Result<GitHubRepository, GitHubProblem> {
    let transport = Transport(transport);
    let token = Token::new(token);
    let known: Vec<Repository> = known.into_iter().map(Into::into).collect();
    let made = GitHub::new(&transport, &token).make_notes_repository(&known)?;
    Ok(GitHubRepository {
        branch: String::new(),
        ..made.into()
    })
}

/// The name a new notes repository would get, for the button that makes it.
#[uniffi::export]
pub fn github_notes_repository_name(known: Vec<GitHubRepository>) -> String {
    let taken: Vec<String> = known
        .into_iter()
        .map(|repository| repository.name)
        .collect();
    github::notes_repository_name(&taken)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers as GitHub would to one user and one repository.
    struct FakeGitHub;

    impl HttpTransport for FakeGitHub {
        fn send(&self, call: HttpCall) -> Result<HttpReply, TransportError> {
            let body = if call.url.ends_with("/user") {
                r#"{"login":"you"}"#
            } else {
                r#"[{"full_name":"you/notes","name":"notes","clone_url":"https://github.com/you/notes.git","private":true,"default_branch":"master"}]"#
            };
            let authorized = call
                .headers
                .iter()
                .any(|header| header.name == "Authorization" && header.value == "Bearer gho_abc");
            let status = if authorized { 200 } else { 401 };
            Ok(HttpReply {
                status,
                body: body.to_owned(),
            })
        }
    }

    #[test]
    fn reads_the_account_and_its_repositories_through_the_app() {
        let account = github_account(Arc::new(FakeGitHub), "gho_abc".into()).unwrap();
        assert_eq!(account.login, "you");
        assert_eq!(account.repositories[0].full_name, "you/notes");
        assert_eq!(account.repositories[0].branch, "master");
        assert_eq!(
            github_notes_repository_name(account.repositories),
            "gasp-notes"
        );
    }

    #[test]
    fn a_token_github_refuses_says_to_sign_in_again() {
        let refused = github_account(Arc::new(FakeGitHub), "gho_old".into());
        assert!(
            matches!(refused, Err(GitHubProblem::Refused { message }) if message.contains("Sign in again"))
        );
    }
}
