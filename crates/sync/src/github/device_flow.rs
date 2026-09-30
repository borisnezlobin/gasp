//! GitHub's device flow, the sign-in that needs no password typed into
//! Gasp and no secret shipped with it. Gasp asks GitHub for a pair of
//! codes, shows the person the short one, and opens
//! github.com/login/device, where they type it and approve. Meanwhile
//! Gasp asks GitHub every few seconds whether they have, with the long
//! code, until GitHub hands over a token, says no, or the codes expire.
//!
//! [`DevicePoll`] is that waiting as a state machine with no clock or
//! network of its own: it's told how long it has waited and what GitHub
//! answered, and says how long to wait before asking again.

use std::time::Duration;

use serde::Deserialize;

use super::GitHubError;
use super::http::{HttpMethod, HttpRequest, HttpResponse};
use crate::credentials::Token;

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// How much longer to wait each time GitHub says to slow down, when it
/// doesn't say how long. The device flow's specification says five seconds.
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);

/// What GitHub hands out to start signing in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCode {
    /// The long code Gasp polls with. Never shown.
    pub device_code: String,
    /// The short code the person types on GitHub, such as `WDJB-MJHT`.
    pub user_code: String,
    /// Where they type it: github.com/login/device.
    pub verification_uri: String,
    /// How long the codes last.
    pub expires_in: Duration,
    /// How long to wait between polls.
    pub interval: Duration,
}

#[derive(Deserialize)]
struct DeviceCodeBody {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// Asks GitHub for a pair of codes for the OAuth App `client_id`, with
/// the permissions `scope` names.
pub fn device_code_request(client_id: &str, scope: &str) -> HttpRequest {
    HttpRequest::new(HttpMethod::Post, DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("scope", scope)])
}

/// Reads GitHub's answer to [`device_code_request`].
pub fn read_device_code(response: &HttpResponse) -> Result<DeviceCode, GitHubError> {
    if let Some(error) = oauth_error(response) {
        return Err(sign_in_refused(&error));
    }
    if !response.is_success() {
        return Err(GitHubError::Status(response.status));
    }
    let body: DeviceCodeBody = serde_json::from_str(&response.body)
        .map_err(|error| GitHubError::Unreadable(error.to_string()))?;
    Ok(DeviceCode {
        device_code: body.device_code,
        user_code: body.user_code,
        verification_uri: body.verification_uri,
        expires_in: Duration::from_secs(body.expires_in),
        interval: Duration::from_secs(body.interval.max(1)),
    })
}

/// Asks GitHub whether the person has approved `device_code` yet.
pub fn token_request(client_id: &str, device_code: &str) -> HttpRequest {
    HttpRequest::new(HttpMethod::Post, TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id),
            ("device_code", device_code),
            ("grant_type", DEVICE_GRANT),
        ])
}

/// What GitHub said to one poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenAnswer {
    /// Approved: here is the token.
    Token(Token),
    /// Not yet.
    Pending,
    /// Asking too often; wait this long between polls from now on.
    SlowDown(Option<Duration>),
    /// The codes ran out before the person approved.
    Expired,
    /// The person pressed Cancel on GitHub.
    Denied,
    /// Anything else GitHub refused, such as the OAuth App not allowing
    /// the device flow, in its own words.
    Refused(String),
}

#[derive(Deserialize)]
struct OAuthErrorBody {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct TokenBody {
    access_token: String,
}

/// Reads GitHub's answer to [`token_request`]. GitHub answers 200 with an
/// `error` field while it waits, so the body decides, not the status.
pub fn read_token_answer(response: &HttpResponse) -> Result<TokenAnswer, GitHubError> {
    if let Ok(body) = serde_json::from_str::<TokenBody>(&response.body) {
        return Ok(TokenAnswer::Token(Token::new(body.access_token)));
    }
    if let Some(error) = oauth_error(response) {
        return Ok(answer_for(error));
    }
    if !response.is_success() {
        return Err(GitHubError::Status(response.status));
    }
    Err(GitHubError::Unreadable(
        "GitHub's answer had neither a token nor a reason".to_owned(),
    ))
}

fn oauth_error(response: &HttpResponse) -> Option<OAuthErrorBody> {
    serde_json::from_str::<OAuthErrorBody>(&response.body).ok()
}

fn answer_for(error: OAuthErrorBody) -> TokenAnswer {
    match error.error.as_str() {
        "authorization_pending" => TokenAnswer::Pending,
        "slow_down" => TokenAnswer::SlowDown(error.interval.map(Duration::from_secs)),
        "expired_token" | "token_expired" => TokenAnswer::Expired,
        "access_denied" => TokenAnswer::Denied,
        _ => TokenAnswer::Refused(described(&error)),
    }
}

fn described(error: &OAuthErrorBody) -> String {
    error
        .error_description
        .clone()
        .unwrap_or_else(|| error.error.replace('_', " "))
}

/// The reason GitHub gave for not starting sign-in at all.
fn sign_in_refused(error: &OAuthErrorBody) -> GitHubError {
    match error.error.as_str() {
        "device_flow_disabled" => GitHubError::DeviceFlowDisabled,
        "incorrect_client_credentials" => GitHubError::UnknownClient,
        _ => GitHubError::Refused(described(error)),
    }
}

/// Where waiting for the person to approve has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollState {
    /// Ask GitHub again after this long.
    Waiting {
        next_poll: Duration,
    },
    SignedIn(Token),
    Failed(SignInFailure),
}

/// Why signing in stopped without a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInFailure {
    Expired,
    Denied,
    Refused(String),
}

impl SignInFailure {
    /// What went wrong and what to do, for the person.
    pub fn sentence(&self) -> String {
        match self {
            SignInFailure::Expired => {
                "The code ran out before GitHub heard back. Start again for a fresh one.".to_owned()
            }
            SignInFailure::Denied => {
                "GitHub says the request was cancelled. Start again if you meant to allow it."
                    .to_owned()
            }
            SignInFailure::Refused(reason) => format!("GitHub stopped signing in: {reason}."),
        }
    }
}

/// Waiting for the person to approve a [`DeviceCode`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevicePoll {
    interval: Duration,
    expires_after: Duration,
}

impl DevicePoll {
    pub fn new(code: &DeviceCode) -> Self {
        DevicePoll {
            interval: code.interval,
            expires_after: code.expires_in,
        }
    }

    /// How long to wait before the first poll, and between polls.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Takes GitHub's `answer`, given `waited` since the codes were
    /// handed out, and says what comes next.
    pub fn answer(&mut self, answer: TokenAnswer, waited: Duration) -> PollState {
        match answer {
            TokenAnswer::Token(token) => PollState::SignedIn(token),
            TokenAnswer::Expired => PollState::Failed(SignInFailure::Expired),
            TokenAnswer::Denied => PollState::Failed(SignInFailure::Denied),
            TokenAnswer::Refused(reason) => PollState::Failed(SignInFailure::Refused(reason)),
            TokenAnswer::SlowDown(interval) => {
                self.interval = interval.unwrap_or(self.interval + SLOW_DOWN_STEP);
                self.wait_or_expire(waited)
            }
            TokenAnswer::Pending => self.wait_or_expire(waited),
        }
    }

    /// Another poll after the interval, unless the codes will have run out
    /// by then.
    fn wait_or_expire(&self, waited: Duration) -> PollState {
        if waited + self.interval > self.expires_after {
            return PollState::Failed(SignInFailure::Expired);
        }
        PollState::Waiting {
            next_poll: self.interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            body: body.to_owned(),
        }
    }

    fn code() -> DeviceCode {
        read_device_code(&answer(
            200,
            r#"{"device_code":"long-code","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#,
        ))
        .unwrap()
    }

    #[test]
    fn reads_the_codes_github_hands_out() {
        let code = code();
        assert_eq!(code.user_code, "WDJB-MJHT");
        assert_eq!(code.verification_uri, "https://github.com/login/device");
        assert_eq!(code.expires_in, Duration::from_secs(900));
        assert_eq!(code.interval, Duration::from_secs(5));
    }

    #[test]
    fn an_oauth_app_without_the_device_flow_says_so() {
        let refused = read_device_code(&answer(
            400,
            r#"{"error":"device_flow_disabled","error_description":"Device Flow must be explicitly enabled for this App"}"#,
        ));
        assert_eq!(refused, Err(GitHubError::DeviceFlowDisabled));
    }

    #[test]
    fn the_requests_carry_the_client_and_ask_for_json() {
        let request = token_request("Iv1.client", "long-code");
        assert_eq!(request.method, HttpMethod::Post);
        assert_eq!(request.header_value("accept"), Some("application/json"));
        let body = request.body.unwrap();
        assert!(body.contains("client_id=Iv1.client"));
        assert!(body.contains("device_code=long-code"));
        assert!(body.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
    }

    #[test]
    fn reads_each_kind_of_poll_answer() {
        let read = |body: &str| read_token_answer(&answer(200, body)).unwrap();
        assert_eq!(
            read(r#"{"access_token":"gho_abc","token_type":"bearer","scope":"repo"}"#),
            TokenAnswer::Token(Token::new("gho_abc"))
        );
        assert_eq!(
            read(r#"{"error":"authorization_pending"}"#),
            TokenAnswer::Pending
        );
        assert_eq!(
            read(r#"{"error":"slow_down","interval":10}"#),
            TokenAnswer::SlowDown(Some(Duration::from_secs(10)))
        );
        assert_eq!(read(r#"{"error":"expired_token"}"#), TokenAnswer::Expired);
        assert_eq!(read(r#"{"error":"access_denied"}"#), TokenAnswer::Denied);
        assert_eq!(
            read(r#"{"error":"unsupported_grant_type"}"#),
            TokenAnswer::Refused("unsupported grant type".to_owned())
        );
    }

    #[test]
    fn polling_waits_the_interval_until_a_token_comes() {
        let mut poll = DevicePoll::new(&code());
        let five = Duration::from_secs(5);
        assert_eq!(
            poll.answer(TokenAnswer::Pending, five),
            PollState::Waiting { next_poll: five }
        );
        assert_eq!(
            poll.answer(TokenAnswer::Token(Token::new("gho_abc")), five * 2),
            PollState::SignedIn(Token::new("gho_abc"))
        );
    }

    #[test]
    fn slowing_down_lengthens_every_wait_after() {
        let mut poll = DevicePoll::new(&code());
        let waited = Duration::from_secs(5);
        assert_eq!(
            poll.answer(TokenAnswer::SlowDown(None), waited),
            PollState::Waiting {
                next_poll: Duration::from_secs(10)
            }
        );
        assert_eq!(
            poll.answer(TokenAnswer::Pending, waited * 3),
            PollState::Waiting {
                next_poll: Duration::from_secs(10)
            }
        );
        poll.answer(
            TokenAnswer::SlowDown(Some(Duration::from_secs(20))),
            waited * 5,
        );
        assert_eq!(poll.interval(), Duration::from_secs(20));
    }

    #[test]
    fn polling_stops_once_the_codes_would_run_out() {
        let mut poll = DevicePoll::new(&code());
        assert_eq!(
            poll.answer(TokenAnswer::Pending, Duration::from_secs(896)),
            PollState::Failed(SignInFailure::Expired)
        );
    }

    #[test]
    fn a_refusal_or_expiry_ends_polling() {
        let mut poll = DevicePoll::new(&code());
        let waited = Duration::from_secs(5);
        assert_eq!(
            poll.answer(TokenAnswer::Denied, waited),
            PollState::Failed(SignInFailure::Denied)
        );
        assert_eq!(
            poll.answer(TokenAnswer::Expired, waited),
            PollState::Failed(SignInFailure::Expired)
        );
    }
}
