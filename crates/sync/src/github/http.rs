//! The requests GitHub sign-in makes, described rather than sent. Each app
//! sends them with the HTTP client it already has (the desktop's reqwest,
//! the iPhone's URLSession) through [`HttpClient`], so this crate reads
//! and writes GitHub's answers without a network stack of its own, and
//! tests answer as GitHub would.

/// How a request is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
        }
    }
}

/// One request to GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl HttpRequest {
    pub(crate) fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        HttpRequest {
            method,
            url: url.into(),
            headers: vec![("User-Agent".to_owned(), USER_AGENT.to_owned())],
            body: None,
        }
    }

    pub(crate) fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    /// A form body, as GitHub's sign-in endpoints take.
    pub(crate) fn form(self, fields: &[(&str, &str)]) -> Self {
        let body = fields
            .iter()
            .map(|(name, value)| format!("{}={}", form_encoded(name), form_encoded(value)))
            .collect::<Vec<_>>()
            .join("&");
        let mut request = self.header("Content-Type", "application/x-www-form-urlencoded");
        request.body = Some(body);
        request
    }

    /// A JSON body, as GitHub's REST API takes.
    pub(crate) fn json(self, body: &serde_json::Value) -> Self {
        let mut request = self.header("Content-Type", "application/json");
        request.body = Some(body.to_string());
        request
    }

    /// The value of the header `name`, ignoring case.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// GitHub's answer: its status and its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Sends a request and waits for the answer. An error is a request that
/// got no answer at all, such as no connection, in words for a person.
pub trait HttpClient: Send + Sync {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String>;
}

/// GitHub turns away requests without one.
const USER_AGENT: &str = concat!(gasp_config::command_name!(), "-notes");

/// `application/x-www-form-urlencoded`: letters, digits and `-._~` as
/// they are, spaces as `+`, everything else as `%XX` bytes.
fn form_encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            b' ' => encoded.push('+'),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_bodies_escape_what_needs_it() {
        let request = HttpRequest::new(HttpMethod::Post, "https://example.invalid")
            .form(&[("scope", "repo read:user"), ("grant", "a:b/c")]);
        assert_eq!(
            request.body.as_deref(),
            Some("scope=repo+read%3Auser&grant=a%3Ab%2Fc")
        );
        assert_eq!(
            request.header_value("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert!(request.header_value("User-Agent").is_some());
    }
}
