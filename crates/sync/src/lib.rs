//! Git sync engine, merge policy and conflict model.
//!
//! [`Vault`] wraps a git clone of a notes vault and does the sync steps:
//! commit, fetch, merge (with the vault merge policy) and push. Real
//! conflicts come back as [`ConflictedFile`]s whose hunks can be resolved
//! one by one. A conflict never pauses git: the merge finishes, the file
//! waits with both versions on disk, and every other file keeps syncing.
//! [`Scheduler`] is the pure timing state machine that decides when each
//! step runs.

mod conflict;
mod credentials;
mod device_files;
mod error;
mod line_merge;
mod message;
mod parked;
pub mod phase;
mod policy;
mod runner;
mod scheduler;
mod vault;

pub use conflict::{ConflictHunk, ConflictedFile, MarkedHunk, MarkedText, Resolution, Segment};
#[cfg(all(
    feature = "keychain",
    any(target_os = "macos", target_os = "ios", target_os = "windows")
))]
pub use credentials::KeychainStore;
pub use credentials::{CredentialStore, InMemoryCredentialStore, KEYCHAIN_SERVICE, Token};
pub use device_files::{DEFAULT_DEVICE_ONLY_GLOBS, DeviceOnlyFiles};
pub use error::{SyncError, SyncResult};
pub use line_merge::{LineMerge, merge_lines};
pub use message::commit_message;
pub use policy::{FileKind, classify};
pub use runner::{drive, run_step};
pub use scheduler::{
    FailureKind, MergeReport, Scheduler, SchedulerConfig, StepFailure, StepReport, SyncEvent,
    SyncEventKind, SyncStatus, SyncStep,
};
pub use vault::{Author, MergeOutcome, RepoProbe, Vault, VaultConfig, probe, set_remote_url};
