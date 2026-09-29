//! Where the app listens for a vault, and the token that proves a
//! request comes from someone who can read the user's files.
//!
//! Both live in a folder only the user can open: the runtime folder
//! (`$XDG_RUNTIME_DIR` on Linux), else the app's local data folder. Each
//! vault gets its own names there, from a hash of its canonical path, so
//! two vaults open in two windows don't collide.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const BRIDGE_DIR: &str = "mcp";

/// What the app writes beside its socket.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointInfo {
    pub token: String,
    /// The localhost port, where there are no Unix sockets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// The socket and token file for one vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    dir: PathBuf,
    key: String,
}

impl Endpoint {
    /// The endpoint for `vault` (a canonical path) in this user's folders.
    pub fn for_vault(vault: &Path) -> Option<Endpoint> {
        let base = dirs::runtime_dir().or_else(dirs::data_local_dir)?;
        Some(Endpoint::in_dir(
            &base.join(editor_config::APP_FOLDER).join(BRIDGE_DIR),
            vault,
        ))
    }

    /// The endpoint for `vault` in `dir`, for tests.
    pub fn in_dir(dir: &Path, vault: &Path) -> Endpoint {
        Endpoint {
            dir: dir.to_path_buf(),
            key: vault_key(vault),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The Unix domain socket.
    pub fn socket_path(&self) -> PathBuf {
        self.dir.join(format!("{}.sock", self.key))
    }

    /// The token (and port) file.
    pub fn info_path(&self) -> PathBuf {
        self.dir.join(format!("{}.json", self.key))
    }

    /// Makes the folder, readable only by the user.
    pub fn prepare_dir(&self) -> io::Result<()> {
        create_private_dir(&self.dir)
    }

    /// Writes the token file afresh, readable only by the user.
    pub fn write_info(&self, info: &EndpointInfo) -> io::Result<()> {
        let path = self.info_path();
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        let text = serde_json::to_string(info).map_err(io::Error::other)?;
        let mut file = create_private_file(&path)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    }

    pub fn read_info(&self) -> io::Result<EndpointInfo> {
        let text = std::fs::read_to_string(self.info_path())?;
        serde_json::from_str(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Removes the socket and token file, when the app stops listening.
    pub fn remove(&self) {
        std::fs::remove_file(self.socket_path()).ok();
        std::fs::remove_file(self.info_path()).ok();
    }
}

/// A short, stable name for a vault: FNV-1a over its path, so the app
/// and the server agree on it whatever built them.
fn vault_key(vault: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in vault.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A fresh random token, as hex.
pub fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Compares tokens in time that doesn't depend on where they differ.
pub fn tokens_match(given: &str, expected: &str) -> bool {
    let (given, expected) = (given.as_bytes(), expected.as_bytes());
    if given.len() != expected.len() {
        return false;
    }
    given
        .iter()
        .zip(expected)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    // The local data folder is already the user's alone.
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_vault_gets_its_own_names() {
        let dir = Path::new("/run/user/1000/editor/mcp");
        let a = Endpoint::in_dir(dir, Path::new("/home/me/Vault"));
        let b = Endpoint::in_dir(dir, Path::new("/home/me/Other"));
        assert_ne!(a.socket_path(), b.socket_path());
        assert_eq!(a, Endpoint::in_dir(dir, Path::new("/home/me/Vault")));
        assert!(a.socket_path().to_string_lossy().ends_with(".sock"));
        // Short enough for a socket path on every platform.
        let name = a.socket_path().file_name().unwrap().len();
        assert_eq!(name, 16 + ".sock".len());
    }

    #[test]
    fn tokens_are_random_and_compared_whole() {
        let (a, b) = (new_token().unwrap(), new_token().unwrap());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert!(tokens_match(&a, &a.clone()));
        assert!(!tokens_match(&a, &b));
        assert!(!tokens_match(&a, &a[..63]));
    }

    #[cfg(unix)]
    #[test]
    fn the_token_file_is_the_users_alone() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::in_dir(&temp.path().join("mcp"), Path::new("/v"));
        endpoint.prepare_dir().unwrap();
        let info = EndpointInfo {
            token: "secret".into(),
            port: None,
        };
        endpoint.write_info(&info).unwrap();
        endpoint.write_info(&info).unwrap();
        assert_eq!(endpoint.read_info().unwrap(), info);
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&endpoint.info_path()), 0o600);
        assert_eq!(mode(endpoint.dir()), 0o700);
        endpoint.remove();
        assert!(!endpoint.info_path().exists());
    }
}
