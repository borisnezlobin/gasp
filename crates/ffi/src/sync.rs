//! Sync for one vault on the phone: the scheduler that decides when, the
//! clone it drives, and what the indicator, its details, the settings and
//! the resolver read. It runs the same scheduler, steps and merge policy
//! as the desktop's `SyncService`. The app calls the git work from a
//! background queue; reading the overview never waits on git.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gasp_config::ConfigLoader;
use gasp_config::settings::SyncSettings;
use gasp_sync::phase::{self, SetupProblem, SyncPhase, SyncRun, file_label};
use gasp_sync::{
    Author, CredentialStore, DeviceOnlyFiles, FailureKind, Scheduler, SchedulerConfig, StepReport,
    SyncStatus, SyncStep, Token, Vault, VaultConfig, run_step,
};

use crate::sync_setup::{repository_url, takes_token, token_store};
use crate::vault::VaultError;

/// The remote sync pulls from and pushes to.
pub(crate) const REMOTE: &str = "origin";
/// Quiet time after the last edit before committing, as on the desktop.
const EDIT_DEBOUNCE: Duration = Duration::from_secs(60);
/// Wait before retrying a sync that failed.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// Syncs that changed something, kept for the details sheet.
const RECENT_RUNS: usize = 5;

/// The vault settings sync uses.
pub(crate) fn vault_config(settings: &SyncSettings) -> Result<VaultConfig, String> {
    let device_only = DeviceOnlyFiles::new(&settings.device_only).map_err(|e| e.to_string())?;
    Ok(VaultConfig {
        remote: REMOTE.to_owned(),
        branch: settings.branch.trim().to_owned(),
        device_only,
    })
}

fn scheduler_config(settings: &SyncSettings) -> SchedulerConfig {
    let minutes = u64::from(settings.interval_minutes);
    SchedulerConfig {
        debounce: EDIT_DEBOUNCE,
        retry_after: RETRY_AFTER,
        poll_every: (settings.auto && minutes > 0).then(|| Duration::from_secs(minutes * 60)),
        ..SchedulerConfig::default()
    }
}

fn load_sync_settings(root: &Path) -> SyncSettings {
    let mut loader = ConfigLoader::for_vault(root);
    loader.load_all();
    loader.config().settings.sync.clone()
}

/// What the indicator shows, as the desktop's phases.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SyncPhaseKind {
    /// The vault isn't a git clone with a remote.
    Hidden,
    /// The clone can't sync as the settings describe.
    NeedsSetup,
    Synced,
    Syncing,
    Offline {
        waiting: u32,
    },
    SignIn {
        has_token: bool,
    },
    Conflict {
        files: u32,
    },
    Failed,
}

/// One sync that changed something: the notes it brought in and sent.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SyncRunSummary {
    pub seconds_ago: f64,
    pub received: Vec<String>,
    pub sent: Vec<String>,
}

/// Everything the indicator, its details and the settings show.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SyncOverview {
    pub phase: SyncPhaseKind,
    pub headline: String,
    pub explanation: Option<String>,
    pub repository: Option<String>,
    pub branch: String,
    pub interval_minutes: u32,
    pub auto_sync: bool,
    pub signed_in: bool,
    /// Whether the repository signs in with a token (GitHub does, a folder doesn't).
    pub takes_token: bool,
    pub seconds_since_sync: Option<f64>,
    /// Newest first.
    pub recent: Vec<SyncRunSummary>,
}

/// What a call that may have synced did.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct SyncOutcome {
    /// Whether a sync ran.
    pub ran: bool,
    /// Notes the merge changed on disk, which open notes reload.
    pub received: Vec<String>,
}

/// An open clone and who commits in it.
pub(crate) struct OpenClone {
    pub(crate) vault: Vault,
    author: Author,
}

/// What one step did.
struct StepDone {
    report: StepReport,
    took: Duration,
    changed: Vec<PathBuf>,
}

impl OpenClone {
    fn run(&self, step: SyncStep, device: &str) -> StepDone {
        let started = Instant::now();
        let before = match step {
            SyncStep::Merge => self.vault.head_commit().ok().flatten(),
            SyncStep::Push => self.vault.tracking_commit(),
            SyncStep::Commit | SyncStep::Fetch => None,
        };
        let report = run_step(&self.vault, step, &self.author, device);
        let moved = matches!(step, SyncStep::Merge | SyncStep::Push)
            && !matches!(report, StepReport::Failed(_));
        let changed = if moved {
            let after = self.vault.head_commit().ok().flatten();
            self.vault.changed_paths(before, after).unwrap_or_default()
        } else {
            Vec::new()
        };
        StepDone {
            report,
            took: started.elapsed(),
            changed,
        }
    }
}

/// What opening the vault for sync found.
enum Opened {
    NotSynced,
    Problem {
        problem: SetupProblem,
        remote_url: String,
    },
    Ready {
        clone: OpenClone,
        remote_url: String,
        signed_in: bool,
    },
}

/// Opens the clone at `root`, if it is one. It's only written to once
/// it's on the branch sync uses, as on the desktop.
fn open_clone(
    root: &Path,
    settings: &SyncSettings,
    store: &dyn CredentialStore,
    device: &str,
) -> Opened {
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
    let token = store.load(&remote_url).ok().flatten();
    let signed_in = token.is_some();
    vault.set_token(token);
    let author = gasp_sync::sync_author(device);
    Opened::Ready {
        clone: OpenClone { vault, author },
        remote_url,
        signed_in,
    }
}

enum Presence {
    NotSynced,
    Problem(SetupProblem),
    Ready,
}

/// What sync knows between calls.
struct SyncState {
    presence: Presence,
    remote_url: Option<String>,
    signed_in: bool,
    settings: SyncSettings,
    scheduler: Scheduler,
    failure: Option<(FailureKind, String)>,
    current: Option<SyncRun>,
    recent: VecDeque<SyncRun>,
}

impl SyncState {
    fn new(settings: SyncSettings) -> Self {
        SyncState {
            presence: Presence::NotSynced,
            remote_url: None,
            signed_in: false,
            scheduler: Scheduler::new(scheduler_config(&settings)),
            settings,
            failure: None,
            current: None,
            recent: VecDeque::new(),
        }
    }

    fn is_ready(&self) -> bool {
        matches!(self.presence, Presence::Ready)
    }

    /// Takes what opening found, and hands back the clone to keep.
    fn opened(&mut self, opened: Opened) -> Option<OpenClone> {
        match opened {
            Opened::NotSynced => {
                self.presence = Presence::NotSynced;
                self.remote_url = None;
                None
            }
            Opened::Problem {
                problem,
                remote_url,
            } => {
                self.presence = Presence::Problem(problem);
                self.remote_url = Some(remote_url);
                None
            }
            Opened::Ready {
                clone,
                remote_url,
                signed_in,
            } => {
                self.presence = Presence::Ready;
                self.remote_url = Some(remote_url);
                self.signed_in = signed_in;
                Some(clone)
            }
        }
    }

    fn phase(&self) -> SyncPhase {
        match &self.presence {
            Presence::NotSynced => return SyncPhase::Hidden,
            Presence::Problem(problem) => return SyncPhase::Setup(problem.clone()),
            Presence::Ready => {}
        }
        if let Some(step) = self.scheduler.in_flight() {
            return SyncPhase::Syncing(step);
        }
        match self.scheduler.status() {
            SyncStatus::Synced => SyncPhase::Synced,
            SyncStatus::Syncing => SyncPhase::Syncing(SyncStep::Commit),
            SyncStatus::Offline { waiting } => SyncPhase::Offline { waiting },
            SyncStatus::SignInNeeded { .. } => SyncPhase::SignIn {
                has_token: self.signed_in,
            },
            SyncStatus::Conflict { files } => SyncPhase::Conflict { files },
            SyncStatus::Failed { .. } => {
                let (kind, message) = self
                    .failure
                    .clone()
                    .unwrap_or((FailureKind::Other, "unknown error".to_owned()));
                SyncPhase::Failed { kind, message }
            }
        }
    }

    /// The first step of a sync, when one is due.
    fn start_run(&mut self, now: Duration) -> Option<SyncStep> {
        if !self.is_ready() {
            return None;
        }
        let step = self.scheduler.poll(now)?;
        self.current = Some(SyncRun {
            started_at: now,
            ..SyncRun::default()
        });
        Some(step)
    }

    /// Takes the result of `step` and returns the next one, if any.
    fn step_done(&mut self, now: Duration, step: SyncStep, done: StepDone) -> Option<SyncStep> {
        if let Some(run) = self.current.as_mut() {
            run.steps.push((step, done.took));
            match step {
                SyncStep::Merge => run.received.extend(done.changed),
                SyncStep::Push => run.sent.extend(done.changed),
                SyncStep::Commit | SyncStep::Fetch => {}
            }
        }
        if let StepReport::Failed(failure) = &done.report {
            self.failure = Some((failure.kind, failure.message.clone()));
            self.signed_in &= failure.kind != FailureKind::SignIn;
        }
        let next = self.scheduler.report(now, done.report);
        if next.is_none() {
            self.finish_run(now);
        }
        next
    }

    fn finish_run(&mut self, now: Duration) {
        if let Some(mut run) = self.current.take() {
            run.finished_at = Some(now);
            if run.changed_anything() {
                self.recent.push_front(run);
                self.recent.truncate(RECENT_RUNS);
            }
        }
        if self.scheduler.status() == SyncStatus::Synced {
            self.failure = None;
        }
    }

    fn received_last_run(&self, started_at: Duration) -> Vec<String> {
        self.recent
            .front()
            .filter(|run| run.started_at == started_at)
            .map(|run| run.received.iter().map(|path| slash_path(path)).collect())
            .unwrap_or_default()
    }

    fn overview(&self, now: Duration) -> SyncOverview {
        let phase = self.phase();
        let synced_ago = self
            .scheduler
            .last_synced_at()
            .map(|at| now.saturating_sub(at));
        SyncOverview {
            phase: phase_kind(&phase),
            headline: phase::headline(&phase, synced_ago),
            explanation: phase::explanation(&phase),
            repository: self.remote_url.clone(),
            branch: self.settings.branch.clone(),
            interval_minutes: self.settings.interval_minutes,
            auto_sync: self.settings.auto,
            signed_in: self.signed_in,
            takes_token: self.remote_url.as_deref().is_some_and(takes_token),
            seconds_since_sync: synced_ago.map(|elapsed| elapsed.as_secs_f64()),
            recent: self.recent.iter().map(|run| summary(run, now)).collect(),
        }
    }
}

fn phase_kind(phase: &SyncPhase) -> SyncPhaseKind {
    let count = |number: usize| u32::try_from(number).unwrap_or(u32::MAX);
    match phase {
        SyncPhase::Hidden => SyncPhaseKind::Hidden,
        SyncPhase::Starting | SyncPhase::Setup(_) => SyncPhaseKind::NeedsSetup,
        SyncPhase::Synced => SyncPhaseKind::Synced,
        SyncPhase::Syncing(_) => SyncPhaseKind::Syncing,
        SyncPhase::Offline { waiting } => SyncPhaseKind::Offline {
            waiting: count(*waiting),
        },
        SyncPhase::SignIn { has_token } => SyncPhaseKind::SignIn {
            has_token: *has_token,
        },
        SyncPhase::Conflict { files } => SyncPhaseKind::Conflict {
            files: count(*files),
        },
        SyncPhase::Failed { .. } => SyncPhaseKind::Failed,
    }
}

fn summary(run: &SyncRun, now: Duration) -> SyncRunSummary {
    let labels = |paths: &[PathBuf]| paths.iter().map(|path| file_label(path)).collect();
    SyncRunSummary {
        seconds_ago: now
            .saturating_sub(run.finished_at.unwrap_or(run.started_at))
            .as_secs_f64(),
        received: labels(&run.received),
        sent: labels(&run.sent),
    }
}

pub(crate) fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Sync for one vault folder.
#[derive(uniffi::Object)]
pub struct VaultSync {
    root: PathBuf,
    device: String,
    store: Arc<dyn CredentialStore>,
    started: Instant,
    state: Mutex<SyncState>,
    clone: Mutex<Option<OpenClone>>,
}

impl VaultSync {
    pub(crate) fn open_with(
        root: PathBuf,
        device: String,
        store: Arc<dyn CredentialStore>,
    ) -> Arc<Self> {
        let settings = load_sync_settings(&root);
        let sync = VaultSync {
            state: Mutex::new(SyncState::new(settings)),
            clone: Mutex::new(None),
            started: Instant::now(),
            root,
            device,
            store,
        };
        sync.reopen();
        Arc::new(sync)
    }

    fn state(&self) -> MutexGuard<'_, SyncState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn open_clone(&self) -> MutexGuard<'_, Option<OpenClone>> {
        self.clone.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> Duration {
        self.started.elapsed()
    }

    /// Opens the clone again with the settings on disk, after the
    /// repository, the branch or the files that stay on the device changed.
    fn reopen(&self) {
        let mut clone = self.open_clone();
        let settings = load_sync_settings(&self.root);
        let opened = open_clone(&self.root, &settings, &*self.store, &self.device);
        let mut state = self.state();
        state.scheduler.set_config(scheduler_config(&settings));
        state.settings = settings;
        *clone = state.opened(opened);
    }

    /// Runs every step the scheduler hands out, one at a time, while the
    /// overview stays readable.
    pub(crate) fn drive(&self) -> SyncOutcome {
        let guard = self.open_clone();
        let Some(clone) = guard.as_ref() else {
            return SyncOutcome::default();
        };
        let started_at = self.now();
        let mut next = self.state().start_run(started_at);
        let ran = next.is_some();
        while let Some(step) = next {
            let done = clone.run(step, &self.device);
            next = self.state().step_done(self.now(), step, done);
        }
        SyncOutcome {
            ran,
            received: self.state().received_last_run(started_at),
        }
    }

    pub(crate) fn remote_url(&self) -> Option<String> {
        self.state().remote_url.clone()
    }

    pub(crate) fn conflicts_resolved(&self) {
        let now = self.now();
        self.state().scheduler.conflicts_resolved(now);
    }
}

fn refused(message: impl Into<String>) -> VaultError {
    VaultError::Refused {
        message: message.into(),
    }
}

#[uniffi::export]
impl VaultSync {
    /// Sync for the vault at `folder`. `device` names this phone in
    /// commit messages.
    #[uniffi::constructor]
    pub fn open(folder: String, device: String) -> Arc<Self> {
        Self::open_with(PathBuf::from(folder), device, token_store())
    }

    pub fn overview(&self) -> SyncOverview {
        let now = self.now();
        self.state().overview(now)
    }

    /// Syncs now: the app opened or came back, or the person asked.
    pub fn sync_now(&self) -> SyncOutcome {
        self.state().scheduler.request_sync();
        self.drive()
    }

    /// Syncs if the scheduler says it's time: edits have stopped, the
    /// interval passed, or a failed sync is due another try.
    pub fn sync_if_due(&self) -> SyncOutcome {
        self.drive()
    }

    /// A note changed on this phone; a sync follows once edits stop.
    pub fn edited(&self) {
        let now = self.now();
        let mut state = self.state();
        if state.is_ready() && state.settings.auto {
            state.scheduler.edited(now);
        }
    }

    /// How long until `sync_if_due` has something to do, if anything is
    /// waiting at all.
    pub fn seconds_until_due(&self) -> Option<f64> {
        let now = self.now();
        let state = self.state();
        if !state.is_ready() {
            return None;
        }
        let wake = state.scheduler.next_wake()?;
        Some(wake.saturating_sub(now).as_secs_f64())
    }

    /// Keeps `token` for this vault's repository in the Keychain and syncs
    /// with it from now on.
    pub fn sign_in(&self, token: String) -> Result<(), VaultError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(refused("Paste a GitHub token first."));
        }
        let url = self
            .remote_url()
            .ok_or_else(|| refused("Add the repository's address first."))?;
        let token = Token::new(token);
        self.store.save(&url, &token).map_err(|error| {
            refused(format!(
                "The token couldn't be kept in the Keychain: {error}"
            ))
        })?;
        if let Some(clone) = self.open_clone().as_mut() {
            clone.vault.set_token(Some(token));
        }
        self.state().signed_in = true;
        Ok(())
    }

    /// Forgets this vault's token. The notes stay on the phone and sync
    /// again after signing in.
    pub fn sign_out(&self) -> Result<(), VaultError> {
        if let Some(url) = self.remote_url() {
            self.store
                .delete(&url)
                .map_err(|error| refused(format!("The token couldn't be removed: {error}")))?;
        }
        if let Some(clone) = self.open_clone().as_mut() {
            clone.vault.set_token(None);
        }
        self.state().signed_in = false;
        Ok(())
    }

    /// Points the vault at another repository, keeping the token.
    pub fn set_repository(&self, typed: String) -> Result<(), VaultError> {
        let url = repository_url(typed)
            .ok_or_else(|| refused("Enter your notes repository, such as github.com/you/notes."))?;
        let old = self.remote_url();
        if old.as_deref() == Some(url.as_str()) {
            return Ok(());
        }
        gasp_sync::set_remote_url(&self.root, REMOTE, &url)
            .map_err(|error| refused(error.to_string()))?;
        let kept = old.and_then(|old| self.store.load(&old).ok().flatten());
        if let Some(token) = kept {
            let _ = self.store.save(&url, &token);
        }
        self.reopen();
        Ok(())
    }

    /// Follows the sync settings after they changed in `.editor/settings.toml`.
    pub fn reload_settings(&self) {
        let settings = load_sync_settings(&self.root);
        let reopen = {
            let mut state = self.state();
            let old = &state.settings;
            let reopen = settings.branch != old.branch || settings.device_only != old.device_only;
            state.scheduler.set_config(scheduler_config(&settings));
            state.settings = settings;
            reopen
        };
        if reopen {
            self.reopen();
        }
    }
}
