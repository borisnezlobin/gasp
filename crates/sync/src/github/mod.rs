//! Signing in with GitHub, for people who sync through a repository
//! rather than iCloud: the device flow (a short code typed on
//! github.com), then the person's repositories, or a new private one for
//! their notes. The token it ends with is kept like a pasted one, under
//! the repository's address in the Keychain.
//!
//! Gasp signs in as an OAuth App, registered once on github.com by
//! whoever builds it, whose client ID goes in [`GITHUB_CLIENT_ID`]. The
//! device flow needs no client secret, so nothing secret ships in the app.
//! Until the ID is filled in, [`client_id`] is `None` and the apps say
//! GitHub sign-in isn't available in this build.

mod api;
mod device_flow;
mod http;

pub use api::{
    Account, GASP_NOTES_REPOSITORY, GitHub, NOTES_REPOSITORY, Repository, notes_repository_name,
};
pub use device_flow::{
    DeviceCode, DevicePoll, PollState, SignInFailure, TokenAnswer, device_code_request,
    read_device_code, read_token_answer, token_request,
};
pub use http::{HttpClient, HttpMethod, HttpRequest, HttpResponse};

use std::time::Duration;

/// The client ID of Gasp's OAuth App on GitHub (Settings, Developer
/// settings, OAuth Apps, with "Enable Device Flow" ticked). It isn't a
/// secret. Replace the placeholder to turn GitHub sign-in on.
pub const GITHUB_CLIENT_ID: &str = "PASTE-OAUTH-APP-CLIENT-ID";

const CLIENT_ID_PLACEHOLDER: &str = "PASTE-OAUTH-APP-CLIENT-ID";

/// What the token may do. Classic OAuth Apps have no narrower scope that
/// can make a private repository and push to it: `public_repo` stops at
/// public ones.
pub const SCOPE: &str = "repo";

/// Where the person types the short code.
pub const DEVICE_PAGE: &str = "https://github.com/login/device";

/// Where someone without a GitHub account makes one.
pub const SIGN_UP_PAGE: &str = "https://github.com/signup";

/// The client ID, once someone has filled it in.
pub fn client_id() -> Option<&'static str> {
    let id = GITHUB_CLIENT_ID.trim();
    (!id.is_empty() && id != CLIENT_ID_PLACEHOLDER).then_some(id)
}

/// What can go wrong talking to GitHub.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GitHubError {
    /// This build has no OAuth App client ID.
    #[error("GitHub sign-in isn’t set up in this build of Gasp.")]
    NotConfigured,
    /// The OAuth App doesn't have "Enable Device Flow" ticked.
    #[error("Gasp’s GitHub app doesn’t allow signing in with a code yet.")]
    DeviceFlowDisabled,
    /// GitHub doesn't know the client ID.
    #[error("GitHub doesn’t recognise this build of Gasp.")]
    UnknownClient,
    /// No answer at all, such as no connection.
    #[error("GitHub can’t be reached ({0}). Check your connection and try again.")]
    Unreachable(String),
    /// The token was revoked or expired.
    #[error("GitHub no longer accepts this sign-in. Sign in again.")]
    SignedOut,
    #[error("GitHub asks to wait a while before trying again.")]
    RateLimited,
    /// A repository by this name exists already.
    #[error("You already have a repository called {0}.")]
    NameTaken(String),
    /// Anything else GitHub refused, in its words.
    #[error("GitHub said no: {0}.")]
    Refused(String),
    #[error("GitHub answered with status {0}.")]
    Status(u16),
    #[error("GitHub’s answer couldn’t be read ({0}).")]
    Unreadable(String),
}

/// Signing in with a short code, from asking GitHub for it to the token.
#[derive(Clone, Debug)]
pub struct SignIn {
    client: String,
    code: DeviceCode,
    poll: DevicePoll,
}

impl SignIn {
    /// Asks GitHub for the codes, as the OAuth App `client` (normally
    /// [`client_id`]).
    pub fn start(http: &dyn HttpClient, client: &str) -> Result<Self, GitHubError> {
        let response = http
            .send(&device_code_request(client, SCOPE))
            .map_err(GitHubError::Unreachable)?;
        let code = read_device_code(&response)?;
        let poll = DevicePoll::new(&code);
        Ok(SignIn {
            client: client.to_owned(),
            code,
            poll,
        })
    }

    /// The codes GitHub handed out: the short one to show, and where to
    /// type it.
    pub fn code(&self) -> &DeviceCode {
        &self.code
    }

    /// How long to wait before the first poll.
    pub fn first_wait(&self) -> Duration {
        self.poll.interval()
    }

    /// Asks GitHub once whether the person approved, `waited` after the
    /// code was handed out. A poll that gets no answer waits and tries
    /// again: a dropped connection while the person is on GitHub
    /// shouldn't end it.
    pub fn poll_once(&mut self, http: &dyn HttpClient, waited: Duration) -> PollState {
        let answer = http
            .send(&token_request(&self.client, &self.code.device_code))
            .map_err(GitHubError::Unreachable)
            .and_then(|response| read_token_answer(&response));
        self.poll
            .answer(answer.unwrap_or(TokenAnswer::Pending), waited)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::credentials::Token;

    #[test]
    fn the_placeholder_client_id_counts_as_not_configured() {
        if GITHUB_CLIENT_ID == CLIENT_ID_PLACEHOLDER {
            assert_eq!(client_id(), None);
        } else {
            assert_eq!(client_id(), Some(GITHUB_CLIENT_ID));
        }
    }

    /// GitHub's sign-in endpoints: a code, then "not yet" until the
    /// person approves on the third poll, with one dropped connection.
    struct SignInGitHub {
        polls: Mutex<u32>,
    }

    impl HttpClient for SignInGitHub {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            let answer = |body: &str| {
                Ok(HttpResponse {
                    status: 200,
                    body: body.to_owned(),
                })
            };
            if request.url.ends_with("/device/code") {
                return answer(
                    r#"{"device_code":"long","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#,
                );
            }
            let mut polls = self.polls.lock().unwrap();
            *polls += 1;
            match *polls {
                1 => answer(r#"{"error":"authorization_pending"}"#),
                2 => Err("The network connection was lost.".to_owned()),
                _ => answer(r#"{"access_token":"gho_abc","token_type":"bearer","scope":"repo"}"#),
            }
        }
    }

    #[test]
    fn signing_in_waits_through_a_dropped_connection_for_the_token() {
        let github = SignInGitHub {
            polls: Mutex::new(0),
        };
        let mut sign_in = SignIn::start(&github, "Ov23li-client").unwrap();
        assert_eq!(sign_in.code().user_code, "WDJB-MJHT");
        let five = Duration::from_secs(5);
        let waiting = PollState::Waiting { next_poll: five };
        assert_eq!(sign_in.poll_once(&github, five), waiting);
        assert_eq!(sign_in.poll_once(&github, five * 2), waiting);
        assert_eq!(
            sign_in.poll_once(&github, five * 3),
            PollState::SignedIn(Token::new("gho_abc"))
        );
    }
}
