use std::path::PathBuf;

/// Everything that can go wrong while syncing a vault.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// The remote could not be reached (no connection, DNS, timeout, missing remote).
    #[error("remote unreachable: {0}")]
    Offline(String),
    /// The remote refused the credentials.
    #[error("authentication failed: {0}")]
    Auth(String),
    /// The server's identity couldn't be confirmed: an SSH host key this
    /// device doesn't know, or an HTTPS certificate that doesn't check out.
    #[error("{0}")]
    Untrusted(String),
    /// The server answered, but the fetch or push failed anyway, such as
    /// for an address git can't use. The message is git's own.
    #[error("{0}")]
    Remote(String),
    /// The remote rejected the push, usually because it moved on since the last fetch.
    #[error("push rejected: {0}")]
    PushRejected(String),
    /// A merge commit was attempted with conflicts still in the index.
    #[error("unresolved conflicts in {0} file(s)")]
    UnresolvedConflicts(usize),
    /// A resolution did not fit the conflict it was applied to.
    #[error("invalid resolution: {0}")]
    InvalidResolution(String),
    /// A glob in the device-only list could not be parsed.
    #[error("invalid device-only glob `{glob}`: {message}")]
    InvalidGlob { glob: String, message: String },
    /// The clone's HEAD is on a different branch than the one the vault syncs.
    #[error("the vault is on branch `{actual}` but syncs `{expected}`")]
    WrongBranch { expected: String, actual: String },
    /// Moving onto `branch` would lose edits: these notes changed on this
    /// device and changed differently on the branch.
    #[error("{} changed on this device and changed differently on {branch}", note_list(.notes))]
    BranchBlocked { branch: String, notes: Vec<PathBuf> },
    /// Setting up sync in place found the folder already a git clone.
    #[error("the vault is already a git repository")]
    AlreadyAClone,
    /// A path that is not inside the vault.
    #[error("path outside the vault: {0}")]
    OutsideVault(PathBuf),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type SyncResult<T> = Result<T, SyncError>;

/// What to do about a server whose identity couldn't be confirmed.
fn untrusted(url: &str, message: &str) -> String {
    let host = crate::address::host_and_port(url).map_or_else(|| url.to_owned(), |(host, _)| host);
    if !crate::address::url_uses_ssh(url) {
        let reason = crate::phase::plain_git_message(message);
        return format!("The certificate of {host} couldn’t be checked. Git said: {reason}");
    }
    if message == crate::ssh::not_github(&host) {
        return format!(
            "{host} answered with an SSH host key that isn’t GitHub’s, so sync didn’t trust it. The network may be intercepting the connection. Try again on another network."
        );
    }
    format!(
        "This device doesn’t know the SSH host key of {host} yet. Connect to it once with ssh {host} in a terminal and accept its key."
    )
}

/// "plan.md" or "plan.md, ideas.md and 3 more".
fn note_list(notes: &[PathBuf]) -> String {
    const SHOWN: usize = 3;
    let names: Vec<String> = notes
        .iter()
        .take(SHOWN)
        .map(|path| path.display().to_string())
        .collect();
    match notes.len().saturating_sub(SHOWN) {
        0 => names.join(", "),
        more => format!("{} and {more} more", names.join(", ")),
    }
}

impl SyncError {
    /// Classifies a libgit2 error raised while talking to the remote at
    /// `url`. A network error only counts as offline when the server
    /// really can't be reached; one that answered gets its message shown.
    pub(crate) fn from_transport(error: git2::Error, url: &str) -> Self {
        use git2::{ErrorClass, ErrorCode};
        let message = error.message().to_owned();
        match error.code() {
            ErrorCode::Auth => return Self::Auth(message),
            ErrorCode::Certificate => return Self::Untrusted(untrusted(url, &message)),
            ErrorCode::NotFastForward => return Self::PushRejected(message),
            _ => {}
        }
        let transport_classes = [
            ErrorClass::Net,
            ErrorClass::Http,
            ErrorClass::Ssl,
            ErrorClass::Ssh,
            ErrorClass::Os,
            ErrorClass::Repository,
            ErrorClass::Callback,
        ];
        if !transport_classes.contains(&error.class()) {
            return Self::Git(error);
        }
        if crate::address::is_reachable(url) {
            Self::Remote(message)
        } else {
            Self::Offline(message)
        }
    }

    /// True when nothing was found where asked, such as a branch the
    /// remote doesn't have.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Git(error) if error.code() == git2::ErrorCode::NotFound)
    }

    /// True when the failure means "try again later when the network is back".
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline(_))
    }
}
