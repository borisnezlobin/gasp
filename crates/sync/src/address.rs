//! Repository addresses: what a person types, which kind of address it
//! is, and whether the server behind it answers at all.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

/// How long [`is_reachable`] waits for the server to answer.
const REACH_TIMEOUT: Duration = Duration::from_secs(4);

/// The address git uses for what a person typed: GitHub shorthand
/// (`you/notes`, `github.com/you/notes`) becomes an HTTPS address. An
/// address with a scheme, an SSH address such as
/// `git@github.com:you/notes.git` or an absolute folder path stays as it is.
pub fn repository_url(typed: &str) -> Option<String> {
    let typed = typed.trim().trim_end_matches('/');
    if typed.is_empty() || typed.chars().any(char::is_whitespace) {
        return None;
    }
    if typed.contains("://") || typed.starts_with('/') || scp_parts(typed).is_some() {
        return Some(typed.to_owned());
    }
    let path = typed
        .strip_prefix("github.com/")
        .or_else(|| typed.strip_prefix("www.github.com/"))
        .unwrap_or(typed);
    let parts: Vec<&str> = path.split('/').collect();
    let is_owner_and_name = parts.len() == 2 && parts.iter().all(|part| !part.is_empty());
    is_owner_and_name.then(|| format!("https://github.com/{path}"))
}

/// Whether the remote signs in with a token: GitHub over HTTPS does, a
/// folder on this device and an SSH address don't.
pub fn url_takes_token(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Whether the remote signs in with an SSH key.
pub fn url_uses_ssh(url: &str) -> bool {
    url.starts_with("ssh://") || scp_parts(url).is_some()
}

/// Whether the address is a folder on this device rather than a server.
pub fn url_is_local(url: &str) -> bool {
    url.starts_with("file://") || url.starts_with('/')
}

/// The host and path of git's short SSH form, `[user@]host:path`: no
/// scheme, and a colon before the first slash.
fn scp_parts(url: &str) -> Option<(&str, &str)> {
    if url.contains("://") {
        return None;
    }
    let (before, path) = url.split_once(':')?;
    let host = before.rsplit_once('@').map_or(before, |(_, host)| host);
    let well_formed = !host.is_empty() && !before.contains('/') && !path.is_empty();
    well_formed.then_some((host, path))
}

/// The server an address names and the port git connects to, or `None`
/// for a folder on this device.
pub fn host_and_port(url: &str) -> Option<(String, u16)> {
    if let Some((host, _)) = scp_parts(url) {
        return Some((host.to_owned(), 22));
    }
    let (scheme, rest) = url.split_once("://")?;
    let default_port = match scheme {
        "https" => 443,
        "http" => 80,
        "ssh" => 22,
        "git" => 9418,
        _ => return None,
    };
    let authority = rest.split('/').next()?;
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    match host_port.rsplit_once(':') {
        Some((host, port)) => Some((host.to_owned(), port.parse().ok()?)),
        None => Some((host_port.to_owned(), default_port)),
    }
}

/// Whether the remote is there to answer: a folder that exists, or a
/// server that accepts a connection. This is how a failed fetch tells
/// "offline" from "the server said no".
pub fn is_reachable(url: &str) -> bool {
    let Some((host, port)) = host_and_port(url) else {
        let folder = url.strip_prefix("file://").unwrap_or(url);
        return Path::new(folder).exists();
    };
    let Ok(addresses) = (host.as_str(), port).to_socket_addrs() else {
        return false;
    };
    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, REACH_TIMEOUT).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_addresses_stay_as_typed() {
        for typed in [
            "git@github.com:you/notes.git",
            "git@github.com:you/notes",
            "github.com:you/notes.git",
            "ssh://git@github.com/you/notes.git",
            "ssh://git@example.com:2222/srv/notes.git",
        ] {
            assert_eq!(repository_url(typed).as_deref(), Some(typed), "{typed}");
            assert!(url_uses_ssh(typed), "{typed}");
            assert!(!url_takes_token(typed), "{typed}");
            assert!(!url_is_local(typed), "{typed}");
        }
    }

    #[test]
    fn github_shorthand_becomes_https() {
        for typed in ["you/notes", "github.com/you/notes", " you/notes/ "] {
            assert_eq!(
                repository_url(typed).as_deref(),
                Some("https://github.com/you/notes")
            );
        }
        assert!(!url_uses_ssh("https://github.com/you/notes"));
        assert_eq!(
            repository_url("file:///tmp/notes.git").as_deref(),
            Some("file:///tmp/notes.git")
        );
        assert_eq!(repository_url("/tmp/notes.git").as_deref(), Some("/tmp/notes.git"));
        assert!(url_is_local("/tmp/notes.git"));
        assert!(!url_takes_token("/tmp/notes.git"));
        assert_eq!(repository_url("you/my notes"), None);
        assert_eq!(repository_url("you"), None);
        assert_eq!(repository_url("you/notes/more"), None);
    }

    #[test]
    fn each_kind_of_address_names_its_server() {
        let cases = [
            ("git@github.com:you/notes.git", Some(("github.com", 22))),
            ("ssh://git@example.com:2222/notes.git", Some(("example.com", 2222))),
            ("https://github.com/you/notes.git", Some(("github.com", 443))),
            ("http://localhost:8080/notes.git", Some(("localhost", 8080))),
            ("/Users/you/notes.git", None),
            ("file:///Users/you/notes.git", None),
        ];
        for (url, expected) in cases {
            let found = host_and_port(url);
            let expected = expected.map(|(host, port)| (host.to_owned(), port));
            assert_eq!(found, expected, "{url}");
        }
    }

    #[test]
    fn a_folder_is_reachable_while_it_exists() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().to_str().unwrap().to_owned();
        assert!(is_reachable(&path));
        assert!(is_reachable(&format!("file://{path}")));
        assert!(!is_reachable(&format!("{path}/missing.git")));
    }
}
