//! The little HTTP a link card needs: a page's head and an image, each
//! with a timeout and a size cap, on a background thread.

use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::blocking::Client;

/// How long a page or image may take before the card gives up on it.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Some sites only send their tags to browsers they recognise.
const USER_AGENT: &str = "Mozilla/5.0 (compatible; EditorLinkCards/1.0)";

fn client() -> Result<&'static Client, String> {
    static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(TIMEOUT)
                .user_agent(USER_AGENT)
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Up to `limit` bytes of the body at `url`, when it answers with success.
pub fn get(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let response = client()?
        .get(url)
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("the page answered {}", response.status()));
    }
    let mut body = Vec::new();
    response
        .take(limit as u64)
        .read_to_end(&mut body)
        .map_err(|error| error.to_string())?;
    Ok(body)
}
