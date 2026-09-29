//! The git side of sync, run on the background executor: opening the
//! clone, running one scheduler step and noting what it changed, and
//! applying conflict resolutions. Nothing here touches the UI.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use gasp_config::settings::SyncSettings;
use gasp_sync::{
    Author, ConflictedFile, CredentialStore, DeviceOnlyFiles, Resolution, StepReport, SyncStep,
    Token, Vault, VaultConfig, run_step,
};

use super::state::SetupProblem;
use crate::edit_time::device_name;

/// The remote sync pushes to.
pub const REMOTE: &str = "origin";

/// An open clone, shared between the steps that run on it one at a time.
pub struct Engine {
    vault: Mutex<Vault>,
    author: Author,
    /// Names this device in commit messages.
    device: String,
    remote_url: String,
}

/// What opening a vault for sync found.
pub enum Opened {
    /// Not a git clone, or one with no remote: sync stays out of sight.
    NotSynced,
    /// A clone with a remote that can't sync as set up.
    Problem {
        problem: SetupProblem,
        remote_url: String,
    },
    Ready {
        engine: Arc<Engine>,
        signed_in: bool,
        /// Notes already waiting for a person, such as those setting up
        /// sync parked.
        conflicts: Vec<ConflictedFile>,
    },
}

/// What one step did.
#[derive(Debug)]
pub struct StepOutcome {
    pub report: StepReport,
    pub took: Duration,
    /// Files the step brought in (merge) or sent (push).
    pub changed: Vec<PathBuf>,
    /// The files waiting for a person after a commit or merge step, which
    /// can settle some or add more; `None` after the other steps.
    pub conflicts: Option<Vec<ConflictedFile>>,
}

/// The vault settings the engine uses.
pub fn vault_config(settings: &SyncSettings) -> Result<VaultConfig, String> {
    let device_only = DeviceOnlyFiles::new(&settings.device_only).map_err(|e| e.to_string())?;
    let legacy = settings.legacy_branch.trim();
    Ok(VaultConfig {
        remote: REMOTE.to_owned(),
        branch: settings.branch.trim().to_owned(),
        legacy_branch: (!legacy.is_empty()).then(|| legacy.to_owned()),
        device_only,
    })
}

/// Opens the clone at `root` for sync, if it is one. The clone is only
/// written to (line-ending settings, device-only excludes) once it's on
/// the branch sync uses, so a vault another tool syncs is left alone.
pub fn open(root: &Path, settings: &SyncSettings, store: &dyn CredentialStore) -> Opened {
    let Some(probe) = gasp_sync::probe(root, REMOTE) else {
        return Opened::NotSynced;
    };
    let Some(remote_url) = probe.remote_url else {
        return Opened::NotSynced;
    };
    let problem = |problem| Opened::Problem {
        problem,
        remote_url: remote_url.clone(),
    };
    let config = match vault_config(settings) {
        Ok(config) => config,
        Err(message) => return problem(SetupProblem::Broken(message)),
    };
    if probe.branch.as_deref() != Some(config.branch.as_str()) {
        return problem(SetupProblem::WrongBranch {
            expected: config.branch,
            actual: probe.branch,
        });
    }
    let mut vault = match Vault::open(root, config) {
        Ok(vault) => vault,
        Err(error) => return problem(SetupProblem::Broken(error.to_string())),
    };
    // A token that can't be read is the same as none: sync asks to sign in.
    let token = store.load(&remote_url).ok().flatten();
    let conflicts = vault.conflicts().unwrap_or_default();
    let signed_in = token.is_some();
    vault.set_token(token);
    let device = device_name();
    let author = probe
        .author
        .unwrap_or_else(|| Author::new(device.clone(), gasp_sync::FALLBACK_AUTHOR_EMAIL));
    let engine = Engine {
        vault: Mutex::new(vault),
        author,
        device,
        remote_url,
    };
    Opened::Ready {
        engine: Arc::new(engine),
        signed_in,
        conflicts,
    }
}

impl Engine {
    fn vault(&self) -> MutexGuard<'_, Vault> {
        self.vault
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn remote_url(&self) -> &str {
        &self.remote_url
    }

    /// Uses `token` for every step from now on.
    pub fn set_token(&self, token: Option<Token>) {
        self.vault().set_token(token);
    }

    /// Runs one step and notes what it changed.
    pub fn run(&self, step: SyncStep) -> StepOutcome {
        let vault = self.vault();
        let started = Instant::now();
        let before = match step {
            SyncStep::Merge => vault.head_commit().ok().flatten(),
            SyncStep::Push => vault.tracking_commit(),
            SyncStep::Commit | SyncStep::Fetch => None,
        };
        let report = run_step(&vault, step, &self.author, &self.device);
        let changed = match (step, &report) {
            (_, StepReport::Failed(_)) | (SyncStep::Commit | SyncStep::Fetch, _) => Vec::new(),
            (SyncStep::Merge | SyncStep::Push, _) => vault
                .changed_paths(before, vault.head_commit().ok().flatten())
                .unwrap_or_default(),
        };
        let conflicts = match step {
            SyncStep::Commit | SyncStep::Merge => vault.conflicts().ok(),
            SyncStep::Fetch | SyncStep::Push => None,
        };
        StepOutcome {
            report,
            took: started.elapsed(),
            changed,
            conflicts,
        }
    }

    /// Applies one resolution per hunk to each file; the next sync's commit
    /// sends them. Returns the files still waiting, which a refused
    /// resolution leaves among them.
    pub fn resolve(
        &self,
        choices: &[(ConflictedFile, Vec<Resolution>)],
    ) -> (Vec<ConflictedFile>, Result<(), String>) {
        let vault = self.vault();
        let mut result = Ok(());
        for (file, resolutions) in choices {
            if let Err(error) = vault.resolve(file, resolutions) {
                result = Err(error.to_string());
            }
        }
        (vault.conflicts().unwrap_or_default(), result)
    }
}
