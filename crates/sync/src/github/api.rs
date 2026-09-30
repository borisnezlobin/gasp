//! The few calls to GitHub's REST API that sync needs once signed in: who
//! the person is, the repositories they own, and making a private one for
//! their notes.

use serde::Deserialize;

use super::GitHubError;
use super::http::{HttpClient, HttpMethod, HttpRequest, HttpResponse};
use crate::credentials::Token;

const API: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";

/// The name a new notes repository gets, unless the person has one by
/// that name already.
pub const NOTES_REPOSITORY: &str = "notes";
/// The name it gets when they do.
pub const GASP_NOTES_REPOSITORY: &str = concat!(gasp_config::command_name!(), "-notes");

/// How many pages of repositories to read, a hundred to a page.
const MOST_PAGES: u32 = 5;

/// The person signed in.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Account {
    pub login: String,
}

/// One of the person's repositories.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Repository {
    /// `owner/name`.
    pub full_name: String,
    pub name: String,
    /// The HTTPS address git clones from.
    pub clone_url: String,
    pub private: bool,
    /// Empty for a repository GitHub didn't say one for.
    #[serde(default)]
    pub default_branch: String,
}

/// GitHub's API, as the person with `token`.
pub struct GitHub<'a> {
    http: &'a dyn HttpClient,
    token: &'a Token,
}

impl<'a> GitHub<'a> {
    pub fn new(http: &'a dyn HttpClient, token: &'a Token) -> Self {
        GitHub { http, token }
    }

    pub fn account(&self) -> Result<Account, GitHubError> {
        let response = self.send(self.request(HttpMethod::Get, "/user"))?;
        parse(&response)
    }

    /// The repositories the person owns, most recently changed first.
    pub fn repositories(&self) -> Result<Vec<Repository>, GitHubError> {
        let mut all = Vec::new();
        for page in 1..=MOST_PAGES {
            let path =
                format!("/user/repos?affiliation=owner&sort=updated&per_page=100&page={page}");
            let response = self.send(self.request(HttpMethod::Get, &path))?;
            let batch: Vec<Repository> = parse(&response)?;
            let last = batch.len() < 100;
            all.extend(batch);
            if last {
                break;
            }
        }
        Ok(all)
    }

    /// Makes a private repository called `name` in the person's account.
    pub fn create_private_repository(&self, name: &str) -> Result<Repository, GitHubError> {
        let body = serde_json::json!({
            "name": name,
            "private": true,
            "description": "Notes, kept in step by Gasp",
            "has_issues": false,
            "has_projects": false,
            "has_wiki": false,
        });
        let request = self.request(HttpMethod::Post, "/user/repos").json(&body);
        let response = self.send(request)?;
        if response.status == 422 {
            return Err(GitHubError::NameTaken(name.to_owned()));
        }
        parse(&response)
    }

    /// Makes a private repository for the notes, named `notes`, or
    /// `gasp-notes` when that's taken, and so on. `known` are the person's
    /// repositories as already read; a name GitHub still says is taken is
    /// skipped for the next.
    pub fn make_notes_repository(&self, known: &[Repository]) -> Result<Repository, GitHubError> {
        let mut taken: Vec<String> = known.iter().map(|repo| repo.name.clone()).collect();
        for _ in 0..3 {
            let name = notes_repository_name(&taken);
            match self.create_private_repository(&name) {
                Err(GitHubError::NameTaken(name)) => taken.push(name),
                result => return result,
            }
        }
        Err(GitHubError::NameTaken(notes_repository_name(&taken)))
    }

    fn request(&self, method: HttpMethod, path: &str) -> HttpRequest {
        HttpRequest::new(method, format!("{API}{path}"))
            .header("Accept", "application/vnd.github+json")
            .header("Authorization", format!("Bearer {}", self.token.secret()))
            .header("X-GitHub-Api-Version", API_VERSION)
    }

    fn send(&self, request: HttpRequest) -> Result<HttpResponse, GitHubError> {
        let response = self.http.send(&request).map_err(GitHubError::Unreachable)?;
        match response.status {
            401 => Err(GitHubError::SignedOut),
            403 if response.body.contains("rate limit") => Err(GitHubError::RateLimited),
            _ => Ok(response),
        }
    }
}

fn parse<T: for<'de> Deserialize<'de>>(response: &HttpResponse) -> Result<T, GitHubError> {
    if !response.is_success() {
        return Err(GitHubError::Status(response.status));
    }
    serde_json::from_str(&response.body).map_err(|error| GitHubError::Unreadable(error.to_string()))
}

/// The first of `notes`, `gasp-notes`, `gasp-notes-2`, `gasp-notes-3`…
/// that isn't in `taken`. GitHub compares names without case.
pub fn notes_repository_name(taken: &[String]) -> String {
    let is_taken = |name: &str| taken.iter().any(|other| other.eq_ignore_ascii_case(name));
    let candidates = [
        NOTES_REPOSITORY.to_owned(),
        GASP_NOTES_REPOSITORY.to_owned(),
    ]
    .into_iter()
    .chain((2..).map(|number| format!("{GASP_NOTES_REPOSITORY}-{number}")));
    candidates
        .into_iter()
        .find(|name| !is_taken(name))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Answers each request from a list, as GitHub would, and keeps what
    /// was asked.
    #[derive(Default)]
    struct ScriptedGitHub {
        answers: Mutex<Vec<HttpResponse>>,
        asked: Mutex<Vec<HttpRequest>>,
    }

    impl ScriptedGitHub {
        fn answering(answers: &[(u16, &str)]) -> Self {
            let answers = answers
                .iter()
                .rev()
                .map(|(status, body)| HttpResponse {
                    status: *status,
                    body: (*body).to_owned(),
                })
                .collect();
            ScriptedGitHub {
                answers: Mutex::new(answers),
                asked: Mutex::default(),
            }
        }

        fn asked(&self) -> Vec<HttpRequest> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl HttpClient for ScriptedGitHub {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            self.asked.lock().unwrap().push(request.clone());
            self.answers
                .lock()
                .unwrap()
                .pop()
                .ok_or_else(|| "no answer left".to_owned())
        }
    }

    fn repository(name: &str) -> Repository {
        Repository {
            full_name: format!("you/{name}"),
            name: name.to_owned(),
            clone_url: format!("https://github.com/you/{name}.git"),
            private: true,
            default_branch: "main".to_owned(),
        }
    }

    const CREATED: &str = r#"{"full_name":"you/gasp-notes","name":"gasp-notes","clone_url":"https://github.com/you/gasp-notes.git","private":true,"default_branch":"main"}"#;

    #[test]
    fn a_new_repository_is_called_notes_unless_that_is_taken() {
        let taken = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(notes_repository_name(&[]), "notes");
        assert_eq!(notes_repository_name(&taken(&["Notes"])), "gasp-notes");
        assert_eq!(
            notes_repository_name(&taken(&["notes", "gasp-notes", "gasp-notes-2"])),
            "gasp-notes-3"
        );
    }

    #[test]
    fn makes_a_private_repository_under_the_next_free_name() {
        let github = ScriptedGitHub::answering(&[(201, CREATED)]);
        let token = Token::new("gho_abc");
        let made = GitHub::new(&github, &token)
            .make_notes_repository(&[repository("notes")])
            .unwrap();
        assert_eq!(made.full_name, "you/gasp-notes");
        let asked = github.asked();
        assert_eq!(asked[0].url, "https://api.github.com/user/repos");
        assert_eq!(
            asked[0].header_value("authorization"),
            Some("Bearer gho_abc")
        );
        let body: serde_json::Value =
            serde_json::from_str(asked[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["name"], "gasp-notes");
        assert_eq!(body["private"], true);
    }

    #[test]
    fn a_name_github_says_is_taken_moves_on_to_the_next() {
        let github = ScriptedGitHub::answering(&[
            (
                422,
                r#"{"message":"Repository creation failed.","errors":[{"message":"name already exists on this account"}]}"#,
            ),
            (201, CREATED),
        ]);
        let token = Token::new("gho_abc");
        GitHub::new(&github, &token)
            .make_notes_repository(&[])
            .unwrap();
        let names: Vec<String> = github
            .asked()
            .iter()
            .map(|request| {
                let body: serde_json::Value =
                    serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                body["name"].as_str().unwrap().to_owned()
            })
            .collect();
        assert_eq!(names, ["notes", "gasp-notes"]);
    }

    #[test]
    fn reads_every_page_of_repositories() {
        let first_page = format!(
            "[{}]",
            (0..100)
                .map(|index| serde_json::to_string(&serde_json::json!({
                    "full_name": format!("you/r{index}"), "name": format!("r{index}"),
                    "clone_url": format!("https://github.com/you/r{index}.git"), "private": false
                }))
                .unwrap())
                .collect::<Vec<_>>()
                .join(",")
        );
        let github =
            ScriptedGitHub::answering(&[(200, &first_page), (200, &format!("[{CREATED}]"))]);
        let token = Token::new("gho_abc");
        let repositories = GitHub::new(&github, &token).repositories().unwrap();
        assert_eq!(repositories.len(), 101);
        assert_eq!(repositories[100].name, "gasp-notes");
        assert!(github.asked()[1].url.ends_with("page=2"));
    }

    #[test]
    fn a_token_github_no_longer_takes_reads_as_signed_out() {
        let github = ScriptedGitHub::answering(&[(401, r#"{"message":"Bad credentials"}"#)]);
        let token = Token::new("gho_old");
        assert_eq!(
            GitHub::new(&github, &token).account(),
            Err(GitHubError::SignedOut)
        );
    }
}
