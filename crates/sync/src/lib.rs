//! Git sync engine, merge policy and conflict model.
//!
//! [`Vault`] wraps a git clone of a notes vault and does the sync steps:
//! commit, fetch, merge (with the vault merge policy) and push. Real
//! conflicts come back as [`ConflictedFile`]s whose hunks can be resolved
//! one by one. A conflict never pauses git: the merge finishes, the file
//! waits with both versions on disk, and every other file keeps syncing.
//! [`Scheduler`] is the pure timing state machine that decides when each
//! step runs.
//!
//! [`github`] signs in with GitHub and finds or makes the repository, and
//! [`icloud`] keeps a vault in iCloud Drive instead of a repository.

mod conflict;
mod credentials;
mod device_files;
mod error;
pub mod github;
pub mod icloud;
mod line_merge;
mod message;
mod parked;
pub mod phase;
mod policy;
mod runner;
mod scheduler;
mod setup;
mod vault;

pub use conflict::{ConflictHunk, ConflictedFile, MarkedHunk, MarkedText, Resolution, Segment};
#[cfg(all(
    feature = "keychain",
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "linux"
    )
))]
pub use credentials::KeychainStore;
pub use credentials::{
    CredentialStore, InMemoryCredentialStore, KEYCHAIN_SERVICE, LEGACY_KEYCHAIN_SERVICE,
    MigratingStore, Token,
};
pub use device_files::{DEFAULT_DEVICE_ONLY_GLOBS, DeviceOnlyFiles};
pub use error::{SyncError, SyncResult};
pub use line_merge::{LineMerge, merge_lines, merge_unrelated};
pub use message::commit_message;
pub use policy::{FileKind, classify};
pub use runner::{drive, run_step};
pub use scheduler::{
    FailureKind, MergeReport, Scheduler, SchedulerConfig, StepFailure, StepReport, SyncEvent,
    SyncEventKind, SyncStatus, SyncStep,
};
pub use setup::{
    InPlaceSetup, STAGING_FOLDER, SetupReport, repository_url, set_up_in_place, setup_problem,
    sync_author, url_is_local, url_takes_token,
};
pub use vault::{
    Author, MergeOutcome, RepoProbe, SYNC_AUTHOR_EMAIL, Vault, VaultConfig, follow_branch, probe,
    set_remote_url, switch_branch,
};
