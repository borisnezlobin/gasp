//! How the desktop talks to GitHub while signing in: reqwest, which the
//! app already builds for link cards. A snapshot run never reaches
//! GitHub; it gets [`StandInGitHub`], which answers as GitHub would and
//! keeps its repositories as bare repositories in the sandbox, so every
//! screen of signing in can be drawn and set up.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use gasp_sync::github::{self, HttpClient, HttpMethod, HttpRequest, HttpResponse};
use gpui::{App, Global};
use reqwest::blocking::Client;

/// How long GitHub may take to answer.
const TIMEOUT: Duration = Duration::from_secs(20);

/// The client ID a snapshot run signs in with.
const STAND_IN_CLIENT: &str = "stand-in";

/// A stand-in GitHub that tests set, keeping its repositories in a folder.
struct StandInGlobal(PathBuf);

impl Global for StandInGlobal {}

/// Signs in to a stand-in GitHub keeping its repositories in `folder`
/// from now on, as tests do.
pub fn use_stand_in(folder: PathBuf, cx: &mut App) {
    cx.set_global(StandInGlobal(folder));
}

fn stand_in_folder(cx: &App) -> Option<PathBuf> {
    cx.try_global::<StandInGlobal>()
        .map(|stand_in| stand_in.0.clone())
        .or_else(|| crate::sandbox::folder("github"))
}

/// The HTTP client signing in uses: GitHub itself, or the stand-in in a
/// snapshot run or a test.
pub fn github_client(cx: &App) -> Arc<dyn HttpClient> {
    match stand_in_folder(cx) {
        Some(folder) => Arc::new(StandInGitHub::new(folder)),
        None => Arc::new(ReqwestGitHub),
    }
}

/// The OAuth App client ID to sign in as, when this build has one.
pub fn client_id(cx: &App) -> Option<String> {
    if stand_in_folder(cx).is_some() {
        return Some(STAND_IN_CLIENT.to_owned());
    }
    github::client_id().map(str::to_owned)
}

struct ReqwestGitHub;

fn client() -> Result<&'static Client, String> {
    static CLIENT: std::sync::OnceLock<Result<Client, String>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(TIMEOUT)
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

impl HttpClient for ReqwestGitHub {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let client = client()?;
        let mut builder = match request.method {
            HttpMethod::Get => client.get(&request.url),
            HttpMethod::Post => client.post(&request.url),
        };
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }
        let response = builder.send().map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let body = response.text().map_err(|error| error.to_string())?;
        Ok(HttpResponse { status, body })
    }
}

/// GitHub as a snapshot run sees it: a code, "not yet" once, then a
/// token; one account, `you`, with two repositories; and new ones made as
/// bare repositories in `folder`, which setting up can push to.
pub struct StandInGitHub {
    folder: PathBuf,
    polls: Mutex<u32>,
}

impl StandInGitHub {
    pub fn new(folder: PathBuf) -> Self {
        StandInGitHub {
            folder,
            polls: Mutex::new(0),
        }
    }

    fn answer(&self, request: &HttpRequest) -> (u16, String) {
        let url = request.url.as_str();
        if url.ends_with("/login/device/code") {
            return (200, DEVICE_CODE.to_owned());
        }
        if url.ends_with("/login/oauth/access_token") {
            return (200, self.poll().to_owned());
        }
        if url.ends_with("/user") {
            return (200, r#"{"login":"you"}"#.to_owned());
        }
        if url.contains("/user/repos?") {
            return (200, self.listed());
        }
        if url.ends_with("/user/repos") {
            return self.create(request);
        }
        (404, r#"{"message":"Not Found"}"#.to_owned())
    }

    fn poll(&self) -> &'static str {
        let mut polls = self.polls.lock().unwrap_or_else(PoisonError::into_inner);
        *polls += 1;
        if *polls < 2 {
            r#"{"error":"authorization_pending"}"#
        } else {
            r#"{"access_token":"stand-in-token","token_type":"bearer","scope":"repo"}"#
        }
    }

    fn repository_json(&self, name: &str, private: bool) -> serde_json::Value {
        let bare = self.folder.join(format!("{name}.git"));
        serde_json::json!({
            "full_name": format!("you/{name}"),
            "name": name,
            "clone_url": format!("file://{}", bare.display()),
            "private": private,
            "default_branch": "master",
        })
    }

    fn listed(&self) -> String {
        let mut repositories = vec![
            self.repository_json("notes", true),
            self.repository_json("website", false),
        ];
        let made = std::fs::read_dir(&self.folder)
            .into_iter()
            .flatten()
            .flatten();
        for entry in made {
            let name = entry.file_name().to_string_lossy().replace(".git", "");
            if !matches!(name.as_str(), "notes" | "website") {
                repositories.push(self.repository_json(&name, true));
            }
        }
        serde_json::Value::Array(repositories).to_string()
    }

    fn create(&self, request: &HttpRequest) -> (u16, String) {
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap_or("{}")).unwrap_or_default();
        let name = body["name"].as_str().unwrap_or("notes").to_owned();
        let bare = self.folder.join(format!("{name}.git"));
        let made = std::fs::create_dir_all(&self.folder)
            .map_err(|error| error.to_string())
            .and_then(|()| {
                git2::Repository::init_bare(&bare)
                    .map(drop)
                    .map_err(|error| error.to_string())
            });
        match made {
            Ok(()) => (201, self.repository_json(&name, true).to_string()),
            Err(message) => (500, serde_json::json!({ "message": message }).to_string()),
        }
    }
}

const DEVICE_CODE: &str = r#"{"device_code":"stand-in","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#;

impl HttpClient for StandInGitHub {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let (status, body) = self.answer(request);
        Ok(HttpResponse { status, body })
    }
}

#[cfg(test)]
mod tests {
    use gasp_sync::Token;
    use gasp_sync::github::{GitHub, PollState, SignIn};

    use super::*;

    #[test]
    fn the_stand_in_signs_in_and_makes_a_repository_setting_up_can_use() {
        let folder = tempfile::tempdir().unwrap();
        let github = StandInGitHub::new(folder.path().to_path_buf());
        let mut sign_in = SignIn::start(&github, STAND_IN_CLIENT).unwrap();
        assert_eq!(sign_in.code().user_code, "WDJB-MJHT");
        let waited = Duration::from_secs(5);
        assert!(matches!(
            sign_in.poll_once(&github, waited),
            PollState::Waiting { .. }
        ));
        let PollState::SignedIn(token) = sign_in.poll_once(&github, waited * 2) else {
            panic!("the second poll signs in");
        };
        assert_eq!(token, Token::new("stand-in-token"));

        let api = GitHub::new(&github, &token);
        let known = api.repositories().unwrap();
        let made = api.make_notes_repository(&known).unwrap();
        assert_eq!(made.full_name, "you/gasp-notes");
        assert!(folder.path().join("gasp-notes.git/HEAD").is_file());
    }
}
