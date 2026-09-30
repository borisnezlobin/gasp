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

/// Asks GitHub for the codes that start signing in.
pub fn start_sign_in(http: &dyn HttpClient) -> Result<DeviceCode, GitHubError> {
    let client = client_id().ok_or(GitHubError::NotConfigured)?;
    let response = http
        .send(&device_code_request(client, SCOPE))
        .map_err(GitHubError::Unreachable)?;
    read_device_code(&response)
}

/// Asks GitHub once whether the person approved `code`, `waited` after it
/// was handed out. A poll that gets no answer waits and tries again: a
/// dropped connection while the person is on GitHub shouldn't end it.
pub fn poll_sign_in(
    http: &dyn HttpClient,
    code: &DeviceCode,
    poll: &mut DevicePoll,
    waited: Duration,
) -> PollState {
    let Some(client) = client_id() else {
        return PollState::Failed(SignInFailure::Refused(
            GitHubError::NotConfigured.to_string(),
        ));
    };
    let answer = http
        .send(&token_request(client, &code.device_code))
        .map_err(GitHubError::Unreachable)
        .and_then(|response| read_token_answer(&response));
    match answer {
        Ok(answer) => poll.answer(answer, waited),
        Err(_) => poll.answer(TokenAnswer::Pending, waited),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholder_client_id_counts_as_not_configured() {
        if GITHUB_CLIENT_ID == CLIENT_ID_PLACEHOLDER {
            assert_eq!(client_id(), None);
        } else {
            assert_eq!(client_id(), Some(GITHUB_CLIENT_ID));
        }
    }
}
