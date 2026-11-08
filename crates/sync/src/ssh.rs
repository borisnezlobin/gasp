//! Signing in to a remote over SSH, for devices that can't use a token.
//!
//! A connection offers, in turn, the keys an SSH agent holds, each key
//! pair in `~/.ssh` that has no passphrase, and this device's own key.
//! Gasp makes that key the first time it's asked for. The person adds its
//! public half to GitHub once, and the device syncs from then on.
//!
//! GitHub's host keys are checked against the fingerprints GitHub
//! publishes, so a device with no `known_hosts` file still knows it's
//! talking to GitHub. Any other server goes to libgit2's own check of
//! `~/.ssh/known_hosts`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use git2::{CertificateCheckStatus, Cred, ErrorClass, ErrorCode};
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};

use crate::error::{SyncError, SyncResult};

/// The folder this device's own key lives in, set once as the app starts.
static KEY_FOLDER: OnceLock<PathBuf> = OnceLock::new();

/// The file name of this device's private key; the public half adds `.pub`.
const DEVICE_KEY_FILE: &str = concat!(gasp_config::command_name!(), "_ed25519");

/// The user name GitHub, GitLab and most git servers take over SSH.
const SSH_USER: &str = "git";

/// The names GitHub's SSH servers answer to.
const GITHUB_HOSTS: [&str; 2] = ["github.com", "ssh.github.com"];

/// SHA-256 fingerprints of GitHub's SSH host keys, as GitHub publishes
/// them at `https://api.github.com/meta` (`ssh_key_fingerprints`).
const GITHUB_HOST_KEYS: [&str; 3] = [
    "+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU",
    "p2QAMXNIC1TJYWeIOttrVc98/R1BUFWu3/LiyKgUfQM",
    "uNiVztksCsDhcc0u9e8BujQXVUpKZIDTMczCvj3tD2s",
];

/// Says where this device's own key is kept. Only the first call counts.
pub fn use_key_folder(folder: impl Into<PathBuf>) {
    let _ = KEY_FOLDER.set(folder.into());
}

/// The public half of this device's own key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceKey {
    /// One line, as GitHub's "New SSH key" page takes it.
    pub public: String,
    /// `SHA256:…`, as GitHub lists the key once it's added.
    pub fingerprint: String,
}

/// This device's own key, made the first time it's asked for. `label`
/// names the device in the key's comment, so it reads well in GitHub's
/// list of keys.
pub fn device_key(label: &str) -> SyncResult<DeviceKey> {
    let private = device_key_path().ok_or_else(no_key_folder)?;
    if !private.exists() {
        make_key(&private, label)?;
    }
    let public = fs::read_to_string(public_half(&private))?.trim().to_owned();
    let parsed = ssh_key::PublicKey::from_openssh(&public).map_err(key_error)?;
    Ok(DeviceKey {
        fingerprint: parsed.fingerprint(HashAlg::Sha256).to_string(),
        public,
    })
}

fn device_key_path() -> Option<PathBuf> {
    KEY_FOLDER.get().map(|folder| folder.join(DEVICE_KEY_FILE))
}

fn public_half(private: &Path) -> PathBuf {
    let mut name = private.as_os_str().to_owned();
    name.push(".pub");
    PathBuf::from(name)
}

fn make_key(private: &Path, label: &str) -> SyncResult<()> {
    let mut key =
        PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).map_err(key_error)?;
    key.set_comment(label);
    let secret = key.to_openssh(LineEnding::LF).map_err(key_error)?;
    let public = key.public_key().to_openssh().map_err(key_error)?;
    if let Some(folder) = private.parent() {
        fs::create_dir_all(folder)?;
    }
    write_private(private, secret.as_bytes())?;
    fs::write(public_half(private), format!("{public}\n"))?;
    Ok(())
}

/// Writes a private key readable by this user only, as ssh insists.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

fn no_key_folder() -> SyncError {
    SyncError::Io(std::io::Error::other("no folder for this device's SSH key"))
}

fn key_error(error: ssh_key::Error) -> SyncError {
    SyncError::Io(std::io::Error::other(error.to_string()))
}

/// One key a connection can offer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Identity {
    Agent,
    Pair { private: PathBuf, public: PathBuf },
}

/// Every key a connection can offer, in the order they're offered.
fn identities() -> Vec<Identity> {
    let agent = std::env::var_os("SSH_AUTH_SOCK")
        .is_some_and(|socket| !socket.is_empty() && Path::new(&socket).exists());
    let home_keys = std::env::var_os("HOME")
        .map(|home| key_pairs(&Path::new(&home).join(".ssh")))
        .unwrap_or_default();
    let device = device_key_path()
        .filter(|private| private.exists())
        .map(|private| Identity::Pair {
            public: public_half(&private),
            private,
        });
    agent
        .then_some(Identity::Agent)
        .into_iter()
        .chain(home_keys)
        .chain(device)
        .collect()
}

/// The key pairs in `folder` that open without a passphrase: each
/// `name.pub` with `name` beside it, by name.
fn key_pairs(folder: &Path) -> Vec<Identity> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut pairs: Vec<Identity> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "pub"))
        .filter_map(|public| {
            let private = public.with_extension("");
            let usable = fs::read_to_string(&private).is_ok_and(|text| !needs_passphrase(&text));
            usable.then_some(Identity::Pair { private, public })
        })
        .collect();
    pairs.sort();
    pairs
}

/// Whether a private key file is locked with a passphrase. Offering one
/// makes libssh2 give up on the whole connection, instead of trying the
/// next key.
fn needs_passphrase(text: &str) -> bool {
    if text.contains("ENCRYPTED") {
        return true;
    }
    openssh_cipher(text).is_some_and(|cipher| cipher != "none")
}

/// The cipher an OpenSSH-format private key is locked with ("none" when
/// it isn't), read from the header that follows the `openssh-key-v1` magic.
fn openssh_cipher(text: &str) -> Option<String> {
    let body: String = text
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let bytes = STANDARD.decode(body.trim()).ok()?;
    let rest = bytes.strip_prefix(b"openssh-key-v1\0")?;
    let length = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
    let name = rest.get(4..4 + length)?;
    Some(String::from_utf8_lossy(name).into_owned())
}

/// Answers libgit2's requests for SSH credentials, one key per request.
/// libgit2 asks again after a key is refused, and the request after the
/// last key fails, so a connection never loops.
pub(crate) struct SshSignIn {
    identities: Vec<Identity>,
    next: usize,
}

impl SshSignIn {
    pub(crate) fn new() -> Self {
        Self {
            identities: identities(),
            next: 0,
        }
    }

    pub(crate) fn username(username: Option<&str>) -> Result<Cred, git2::Error> {
        Cred::username(username.unwrap_or(SSH_USER))
    }

    pub(crate) fn next_key(&mut self, username: Option<&str>) -> Result<Cred, git2::Error> {
        let username = username.unwrap_or(SSH_USER);
        let Some(identity) = self.identities.get(self.next).cloned() else {
            return Err(git2::Error::new(
                ErrorCode::Auth,
                ErrorClass::Ssh,
                "the server didn’t accept any of this device’s SSH keys",
            ));
        };
        self.next += 1;
        match identity {
            Identity::Agent => Cred::ssh_key_from_agent(username),
            Identity::Pair { private, public } => {
                Cred::ssh_key(username, Some(&public), &private, None)
            }
        }
    }
}

/// Checks a server's identity. GitHub must show one of its published host
/// keys; every other SSH server and every HTTPS certificate is left to
/// libgit2's own checks.
pub(crate) fn check_host(
    cert: &git2::cert::Cert<'_>,
    host: &str,
) -> Result<CertificateCheckStatus, git2::Error> {
    let fingerprint = cert
        .as_hostkey()
        .and_then(|key| key.hash_sha256())
        .map(|hash| STANDARD_NO_PAD.encode(hash));
    let Some(fingerprint) = fingerprint.filter(|_| GITHUB_HOSTS.contains(&host)) else {
        return Ok(CertificateCheckStatus::CertificatePassthrough);
    };
    if GITHUB_HOST_KEYS.contains(&fingerprint.as_str()) {
        return Ok(CertificateCheckStatus::CertificateOk);
    }
    Err(git2::Error::new(
        ErrorCode::Certificate,
        ErrorClass::Ssh,
        not_github(host),
    ))
}

/// How [`check_host`] says a GitHub name answered with someone else's key.
pub(crate) fn not_github(host: &str) -> String {
    format!("{host} answered with an SSH host key that isn’t GitHub’s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_key_is_made_once_and_reads_back() {
        let folder = tempfile::tempdir().unwrap();
        let private = folder.path().join(DEVICE_KEY_FILE);
        make_key(&private, "Gasp on a test machine").unwrap();
        let public = fs::read_to_string(public_half(&private)).unwrap();
        assert!(public.starts_with("ssh-ed25519 "), "{public}");
        assert!(
            public.trim_end().ends_with("Gasp on a test machine"),
            "{public}"
        );
        let secret = fs::read_to_string(&private).unwrap();
        assert!(!needs_passphrase(&secret));
        assert!(
            make_key(&private, "again").is_err(),
            "an existing key is kept"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&private).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn a_key_with_a_passphrase_is_skipped() {
        let folder = tempfile::tempdir().unwrap();
        let mut key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        key.set_comment("locked");
        let locked = key.encrypt(&mut rand_core::OsRng, "secret").unwrap();
        let open = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        for (name, key) in [("id_locked", &locked), ("id_open", &open)] {
            let path = folder.path().join(name);
            let secret: &PrivateKey = key;
            fs::write(&path, secret.to_openssh(LineEnding::LF).unwrap().as_bytes()).unwrap();
            fs::write(
                public_half(&path),
                secret.public_key().to_openssh().unwrap(),
            )
            .unwrap();
        }
        fs::write(folder.path().join("known_hosts"), "").unwrap();
        let pairs = key_pairs(folder.path());
        assert_eq!(
            pairs,
            vec![Identity::Pair {
                private: folder.path().join("id_open"),
                public: folder.path().join("id_open.pub"),
            }]
        );
    }

    #[test]
    fn old_pem_keys_say_when_they_are_locked() {
        let locked = "-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,00\n\nAAAA\n-----END RSA PRIVATE KEY-----\n";
        assert!(needs_passphrase(locked));
        let open =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAK\n-----END RSA PRIVATE KEY-----\n";
        assert!(!needs_passphrase(open));
    }

    /// Run by hand against a repository you can reach over SSH:
    /// `GASP_SSH_PROBE=git@github.com:you/notes.git cargo test -p gasp-sync -- --ignored over_ssh`
    #[test]
    #[ignore = "talks to a real server over SSH"]
    fn a_real_remote_answers_over_ssh() {
        let url = std::env::var("GASP_SSH_PROBE").expect("GASP_SSH_PROBE names a repository");
        let mut remote = git2::Remote::create_detached(url.as_str()).unwrap();
        let callbacks = crate::credentials::remote_callbacks(None);
        let connection = remote.connect_auth(git2::Direction::Fetch, Some(callbacks), None);
        if let Err(error) = &connection {
            panic!("this device's own SSH keys didn't get in: {error}");
        }
        assert!(!connection.unwrap().list().unwrap().is_empty());
    }

    /// The same, with only a key the server has never seen.
    #[test]
    #[ignore = "talks to a real server over SSH"]
    fn a_key_the_server_doesnt_know_is_a_sign_in_problem_over_ssh() {
        let url = std::env::var("GASP_SSH_PROBE").expect("GASP_SSH_PROBE names a repository");
        let folder = tempfile::tempdir().unwrap();
        let private = folder.path().join(DEVICE_KEY_FILE);
        make_key(&private, "a key no server knows").unwrap();
        let mut sign_in = SshSignIn {
            identities: vec![Identity::Pair {
                public: public_half(&private),
                private,
            }],
            next: 0,
        };
        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(move |_, username, allowed| {
            if allowed.contains(git2::CredentialType::USERNAME) {
                return SshSignIn::username(username);
            }
            sign_in.next_key(username)
        });
        callbacks.certificate_check(check_host);
        let mut remote = git2::Remote::create_detached(url.as_str()).unwrap();
        let Err(error) = remote.connect_auth(git2::Direction::Fetch, Some(callbacks), None) else {
            panic!("a key the server doesn't know got in");
        };
        let error = SyncError::from_transport(error, &url);
        assert!(matches!(error, SyncError::Auth(_)), "{error:?}");
    }

    #[test]
    fn keys_run_out_with_an_auth_error() {
        let mut sign_in = SshSignIn {
            identities: Vec::new(),
            next: 0,
        };
        let Err(error) = sign_in.next_key(None) else {
            panic!("no key should be left");
        };
        assert_eq!(error.code(), ErrorCode::Auth);
    }
}
