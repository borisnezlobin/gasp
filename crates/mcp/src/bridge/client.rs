//! The server's half of the bridge: one request per connection to the
//! app that has the vault open, if one does.

use std::io;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::endpoint::Endpoint;
use super::listener::{connect_to, exchange};
use super::{Buffer, Envelope, Reply, Request};

/// How long the app gets to answer. Requests run on its main thread
/// between frames, so a slow answer means something is stuck.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// Why the app couldn't answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppError {
    /// No app has this vault open, or its bridge is off.
    NotRunning,
    /// The app answered with an error, or the exchange broke.
    Failed(String),
}

impl AppError {
    /// The error in words for the agent.
    pub fn message(&self) -> String {
        match self {
            AppError::NotRunning => "The app isn't running with this vault open, so there's no \
                editor to ask. Open the vault in the app (with `mcp.enabled` on) and try again."
                .to_string(),
            AppError::Failed(message) => format!("The app couldn't do it: {message}"),
        }
    }
}

/// Where to find the app for one vault.
#[derive(Clone, Debug)]
pub struct AppLink {
    endpoint: Option<Endpoint>,
}

impl AppLink {
    /// `None` means there's never an app, as in tests of the headless
    /// tools.
    pub fn new(endpoint: Option<Endpoint>) -> AppLink {
        AppLink { endpoint }
    }

    /// Sends `request` and returns the app's result.
    pub fn call(&self, request: Request) -> Result<Value, AppError> {
        let endpoint = self.endpoint.as_ref().ok_or(AppError::NotRunning)?;
        let info = endpoint.read_info().map_err(|_| AppError::NotRunning)?;
        let stream = connect_to(endpoint, info.port).map_err(not_running_or_failed)?;
        let envelope = Envelope {
            token: info.token,
            request,
        };
        let mut line =
            serde_json::to_vec(&envelope).map_err(|error| AppError::Failed(error.to_string()))?;
        line.push(b'\n');
        let answer =
            exchange(stream, &line, ANSWER_TIMEOUT).map_err(|error| failed("no answer", &error))?;
        if answer.trim().is_empty() {
            return Err(AppError::Failed(
                "it closed the connection without answering (a stale token?)".into(),
            ));
        }
        let reply: Reply = serde_json::from_str(&answer)
            .map_err(|error| AppError::Failed(format!("its answer didn't parse: {error}")))?;
        reply.into_result().map_err(AppError::Failed)
    }

    /// [`AppLink::call`] with the result read as `T`.
    pub fn call_as<T: DeserializeOwned>(&self, request: Request) -> Result<T, AppError> {
        let value = self.call(request)?;
        serde_json::from_value(value)
            .map_err(|error| AppError::Failed(format!("its answer didn't parse: {error}")))
    }

    /// The note as the app has it, or `None` when no app is running.
    /// Any other failure is an error, so a write never goes around an
    /// app that has the note open.
    pub fn buffer(&self, path: &str) -> Result<Option<Buffer>, AppError> {
        let request = Request::Buffer {
            path: path.to_string(),
        };
        match self.call_as::<Buffer>(request) {
            Ok(buffer) => Ok(Some(buffer)),
            Err(AppError::NotRunning) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

fn not_running_or_failed(error: io::Error) -> AppError {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => AppError::NotRunning,
        _ => failed("couldn't connect", &error),
    }
}

fn failed(what: &str, error: &io::Error) -> AppError {
    AppError::Failed(format!("{what}: {error}"))
}
