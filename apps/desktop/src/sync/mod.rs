//! Git sync in the desktop app: a [`SyncService`] per vault window runs
//! `editor-sync`'s scheduler, with every git step on the background
//! executor. It shows as the [`SyncIndicator`] in the status bar, is set
//! up on the settings screen's Sync page, and hands conflicts to the
//! [`ConflictResolver`].
//!
//! Sync only shows for a vault that is a git clone with a remote. It only
//! writes to the clone once it's on the branch the settings name, so a
//! vault that another tool still syncs on `main` is left alone.

pub mod credentials;
pub mod engine;
pub mod indicator;
pub mod resolver;
pub mod service;
pub mod state;

use std::sync::Arc;

use editor_sync::CredentialStore;
use gpui::{App, Global};

pub use indicator::{SyncIndicator, SyncIndicatorEvent};
pub use resolver::{Choice, ConflictResolver};
pub use service::SyncService;
pub use state::{SetupProblem, SyncPhase, SyncRun};

struct StoreGlobal(Arc<dyn CredentialStore>);

impl Global for StoreGlobal {}

/// Where tokens are kept: the platform's store, unless a test set another.
pub fn credential_store(cx: &mut App) -> Arc<dyn CredentialStore> {
    if let Some(store) = cx.try_global::<StoreGlobal>() {
        return store.0.clone();
    }
    let store = credentials::default_store();
    cx.set_global(StoreGlobal(store.clone()));
    store
}

/// Keeps tokens in `store` from now on, such as an in-memory one in tests.
pub fn set_credential_store(store: Arc<dyn CredentialStore>, cx: &mut App) {
    cx.set_global(StoreGlobal(store));
}
