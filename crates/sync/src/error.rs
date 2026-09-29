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
    /// A path that is not inside the vault.
    #[error("path outside the vault: {0}")]
    OutsideVault(PathBuf),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type SyncResult<T> = Result<T, SyncError>;

impl SyncError {
    /// Classifies a libgit2 error raised while talking to the remote.
    pub(crate) fn from_transport(error: git2::Error) -> Self {
        use git2::{ErrorClass, ErrorCode};
        if matches!(error.code(), ErrorCode::Auth | ErrorCode::Certificate) {
            return Self::Auth(error.message().to_owned());
        }
        if error.code() == ErrorCode::NotFastForward {
            return Self::PushRejected(error.message().to_owned());
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
        if transport_classes.contains(&error.class()) {
            return Self::Offline(error.message().to_owned());
        }
        Self::Git(error)
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
