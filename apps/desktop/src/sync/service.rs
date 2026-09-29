//! One vault window's sync: the scheduler, the clone it drives on the
//! background executor, and what the indicator, popover, settings page and
//! resolver read. Git work never runs on the UI thread.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gasp_config::settings::SyncSettings;
use gasp_sync::{
    ConflictedFile, CredentialStore, FailureKind, Resolution, Scheduler, SchedulerConfig,
    StepReport, SyncStatus, SyncStep, Token,
};
use gpui::{AppContext, BackgroundExecutor, Context, Task};

use super::engine::{self, Engine, Opened, StepOutcome};
use super::state::{SetupProblem, SyncPhase, SyncRun, Times};

/// Quiet time after the last edit before committing, as `vault-sync` does.
pub const EDIT_DEBOUNCE: Duration = Duration::from_secs(60);
/// Wait before retrying a sync that failed.
pub const RETRY_AFTER: Duration = Duration::from_secs(60);
/// Coming back to the window syncs, unless a sync started this recently.
pub const FOCUS_SYNC_GAP: Duration = Duration::from_secs(60);
/// Changes the watcher reports this soon after a merge wrote them are the
/// merge's own, not edits.
const ECHO_WINDOW: Duration = Duration::from_secs(5);
/// Syncs that changed something, kept for the popover.
const RECENT_RUNS: usize = 5;

/// Where the vault stands with sync before the scheduler has a say.
enum Presence {
    /// Looking at the clone.
    Starting,
    NotSynced,
    Problem(SetupProblem),
    Ready(Arc<Engine>),
}

/// Sync for one vault.
pub struct SyncService {
    root: PathBuf,
    settings: SyncSettings,
    store: Arc<dyn CredentialStore>,
    presence: Presence,
    remote_url: Option<String>,
    signed_in: bool,
    scheduler: Scheduler,
    executor: BackgroundExecutor,
    start: Instant,
    current: Option<SyncRun>,
    recent: VecDeque<SyncRun>,
    last_run: Option<SyncRun>,
    last_started: Option<Duration>,
    /// When the last sync ended, however it ended.
    last_ended: Option<Duration>,
    failure: Option<(FailureKind, String)>,
    conflicts: Vec<ConflictedFile>,
    /// What the last merge wrote, and when, to tell its echoes from edits.
    echo: Option<(Duration, Vec<PathBuf>)>,
    step_task: Option<Task<()>>,
    resolve_task: Option<Task<()>>,
    timer: Option<Task<()>>,
    setup_task: Option<Task<()>>,
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

impl SyncService {
    /// Sync for the vault at `root`. It looks at the clone in the
    /// background and, when it can sync, starts with a sync as the app opens.
    pub fn new(
        root: &Path,
        settings: SyncSettings,
        store: Arc<dyn CredentialStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let executor = cx.background_executor().clone();
        let mut service = SyncService {
            root: root.to_path_buf(),
            scheduler: Scheduler::new(scheduler_config(&settings)),
            settings,
            store,
            presence: Presence::Starting,
            remote_url: None,
            signed_in: false,
            start: executor.now(),
            executor,
            current: None,
            recent: VecDeque::new(),
            last_run: None,
            last_started: None,
            last_ended: None,
            failure: None,
            conflicts: Vec::new(),
            echo: None,
            step_task: None,
            resolve_task: None,
            timer: None,
            setup_task: None,
        };
        // Looking at the clone runs git, which can wait until the window
        // is on screen.
        if crate::first_frame::is_waiting() {
            let this = cx.weak_entity();
            crate::first_frame::defer(move |cx| {
                this.update(cx, |this, cx| this.open(cx)).ok();
            });
        } else {
            service.open(cx);
        }
        service
    }

    /// Time on this service's clock, which tests can move by hand.
    pub fn now(&self) -> Duration {
        self.executor.now().saturating_duration_since(self.start)
    }

    fn open(&mut self, cx: &mut Context<Self>) {
        self.presence = Presence::Starting;
        let root = self.root.clone();
        let settings = self.settings.clone();
        let store = self.store.clone();
        let opening = cx.background_spawn(async move { engine::open(&root, &settings, &*store) });
        self.setup_task = Some(cx.spawn(async move |this, cx| {
            let opened = opening.await;
            this.update(cx, |this, cx| this.opened(opened, cx)).ok();
        }));
    }

    fn opened(&mut self, opened: Opened, cx: &mut Context<Self>) {
        self.setup_task = None;
        match opened {
            Opened::NotSynced => {
                self.presence = Presence::NotSynced;
                self.remote_url = None;
            }
            Opened::Problem {
                problem,
                remote_url,
            } => {
                self.presence = Presence::Problem(problem);
                self.remote_url = Some(remote_url);
            }
            Opened::Ready { engine, signed_in } => {
                self.remote_url = Some(engine.remote_url().to_owned());
                self.signed_in = signed_in;
                self.presence = Presence::Ready(engine);
                if self.settings.auto {
                    self.scheduler.request_sync();
                }
                self.tick(cx);
            }
        }
        cx.notify();
    }

    // ---- What the UI reads ----

    /// What the indicator shows now.
    pub fn phase(&self) -> SyncPhase {
        match &self.presence {
            Presence::Starting => return SyncPhase::Starting,
            Presence::NotSynced => return SyncPhase::Hidden,
            Presence::Problem(problem) => return SyncPhase::Setup(problem.clone()),
            Presence::Ready(_) => {}
        }
        if let Some(step) = self.scheduler.in_flight() {
            return SyncPhase::Syncing(step);
        }
        self.phase_for_status()
    }

    fn phase_for_status(&self) -> SyncPhase {
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

    /// Whether the vault is a git clone with a remote, so sync shows.
    pub fn is_present(&self) -> bool {
        !matches!(self.presence, Presence::NotSynced | Presence::Starting)
    }

    /// Whether the clone is open and syncing as set up.
    pub fn is_ready(&self) -> bool {
        matches!(self.presence, Presence::Ready(_))
    }

    /// How long ago the last sync finished, if one did this session.
    pub fn synced_ago(&self) -> Option<Duration> {
        let at = self.scheduler.last_synced_at()?;
        Some(self.now().saturating_sub(at))
    }

    /// How long ago the last sync, and the last attempt, ended.
    pub fn times(&self) -> Times {
        let now = self.now();
        Times {
            synced: self.synced_ago(),
            tried: self.last_ended.map(|at| now.saturating_sub(at)),
        }
    }

    /// Syncs that changed something, newest first.
    pub fn recent_runs(&self) -> impl Iterator<Item = &SyncRun> {
        self.recent.iter()
    }

    /// The last sync that finished, whether or not it changed anything.
    pub fn last_run(&self) -> Option<&SyncRun> {
        self.last_run.as_ref()
    }

    /// The files a paused merge is waiting on.
    pub fn conflicts(&self) -> &[ConflictedFile] {
        &self.conflicts
    }

    /// Whether `path` (absolute or vault-relative) is one of them.
    pub fn is_conflicted(&self, path: &Path) -> bool {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        self.conflicts.iter().any(|file| file.path == relative)
    }

    pub fn remote_url(&self) -> Option<&str> {
        self.remote_url.as_deref()
    }

    pub fn is_signed_in(&self) -> bool {
        self.signed_in
    }

    pub fn settings(&self) -> &SyncSettings {
        &self.settings
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    // ---- What starts a sync ----

    /// `sync.now`: syncs as soon as nothing else is running.
    pub fn sync_now(&mut self, cx: &mut Context<Self>) {
        self.scheduler.request_sync();
        self.tick(cx);
    }

    /// Files changed on disk: a sync follows once edits stop, unless the
    /// changes are what the last merge wrote.
    pub fn files_changed(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if !self.is_ready() || !self.settings.auto {
            return;
        }
        let now = self.now();
        let merging = self.scheduler.in_flight() == Some(SyncStep::Merge);
        let edits = paths.iter().any(|path| !self.is_echo(path, now));
        if merging || !edits {
            return;
        }
        self.scheduler.edited(now);
        self.reschedule(cx);
    }

    /// Something other than a note changed, such as a setting.
    pub fn edited(&mut self, cx: &mut Context<Self>) {
        if self.is_ready() && self.settings.auto {
            self.scheduler.edited(self.now());
            self.reschedule(cx);
        }
    }

    fn is_echo(&self, path: &Path, now: Duration) -> bool {
        let Some((at, written)) = &self.echo else {
            return false;
        };
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        now.saturating_sub(*at) <= ECHO_WINDOW && written.iter().any(|w| w == relative)
    }

    /// The window came to the front: sync, unless one started recently.
    pub fn window_activated(&mut self, cx: &mut Context<Self>) {
        if !self.is_ready() || !self.settings.auto {
            return;
        }
        let now = self.now();
        let recent = self
            .last_started
            .is_some_and(|started| now.saturating_sub(started) < FOCUS_SYNC_GAP);
        if !recent {
            self.sync_now(cx);
        }
    }

    // ---- Running steps ----

    fn tick(&mut self, cx: &mut Context<Self>) {
        let Presence::Ready(_) = &self.presence else {
            return;
        };
        if self.step_task.is_some() {
            return;
        }
        let now = self.now();
        match self.scheduler.poll(now) {
            Some(step) => {
                self.last_started = Some(now);
                self.current = Some(SyncRun {
                    started_at: now,
                    ..SyncRun::default()
                });
                self.run(step, cx);
            }
            None => self.reschedule(cx),
        }
        cx.notify();
    }

    fn run(&mut self, step: SyncStep, cx: &mut Context<Self>) {
        let Presence::Ready(engine) = &self.presence else {
            return;
        };
        self.timer = None;
        let engine = engine.clone();
        let work = cx.background_spawn(async move { engine.run(step) });
        self.step_task = Some(cx.spawn(async move |this, cx| {
            let outcome = work.await;
            this.update(cx, |this, cx| this.step_done(step, outcome, cx))
                .ok();
        }));
    }

    fn step_done(&mut self, step: SyncStep, outcome: StepOutcome, cx: &mut Context<Self>) {
        self.step_task = None;
        let now = self.now();
        self.note_outcome(step, &outcome, now);
        if let StepReport::Failed(failure) = &outcome.report {
            self.failure = Some((failure.kind, failure.message.clone()));
            if failure.kind == FailureKind::SignIn {
                self.signed_in = false;
            }
        }
        if let Some(conflicts) = outcome.conflicts {
            self.conflicts = conflicts;
        }
        match self.scheduler.report(now, outcome.report) {
            Some(next) => self.run(next, cx),
            None => self.finish_run(now, cx),
        }
        cx.notify();
    }

    fn note_outcome(&mut self, step: SyncStep, outcome: &StepOutcome, now: Duration) {
        let Some(run) = self.current.as_mut() else {
            return;
        };
        run.steps.push((step, outcome.took));
        match step {
            SyncStep::Merge if !outcome.changed.is_empty() => {
                run.received.extend(outcome.changed.iter().cloned());
                self.echo = Some((now, outcome.changed.clone()));
            }
            SyncStep::Push => run.sent.extend(outcome.changed.iter().cloned()),
            _ => {}
        }
    }

    fn finish_run(&mut self, now: Duration, cx: &mut Context<Self>) {
        self.last_ended = Some(now);
        if let Some(mut run) = self.current.take() {
            run.finished_at = Some(now);
            if run.changed_anything() {
                self.recent.push_front(run.clone());
                self.recent.truncate(RECENT_RUNS);
            }
            self.last_run = Some(run);
        }
        if self.scheduler.status() == SyncStatus::Synced {
            self.failure = None;
        }
        self.reschedule(cx);
    }

    /// Wakes up when the scheduler next has something due.
    fn reschedule(&mut self, cx: &mut Context<Self>) {
        self.timer = None;
        let Some(wake) = self.scheduler.next_wake() else {
            return;
        };
        let delay = wake.saturating_sub(self.now());
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |this, cx| {
                this.timer = None;
                this.tick(cx);
            })
            .ok();
        }));
    }

    // ---- Conflicts ----

    /// Settles each file with one resolution per hunk, then syncs what was
    /// settled. Every other note has kept syncing all along, so a step may
    /// be running; the engine takes its turn after it.
    pub fn resolve(
        &mut self,
        choices: Vec<(ConflictedFile, Vec<Resolution>)>,
        cx: &mut Context<Self>,
    ) {
        let Presence::Ready(engine) = &self.presence else {
            return;
        };
        let engine = engine.clone();
        let written: Vec<PathBuf> = choices.iter().map(|(file, _)| file.path.clone()).collect();
        let work = cx.background_spawn(async move { engine.resolve(&choices) });
        self.resolve_task = Some(cx.spawn(async move |this, cx| {
            let (waiting, result) = work.await;
            this.update(cx, |this, cx| this.resolved(waiting, result, written, cx))
                .ok();
        }));
    }

    fn resolved(
        &mut self,
        waiting: Vec<ConflictedFile>,
        result: Result<(), String>,
        written: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.resolve_task = None;
        let now = self.now();
        self.conflicts = waiting;
        self.echo = Some((now, written));
        if let Err(message) = result {
            self.failure = Some((FailureKind::Other, message));
        }
        self.scheduler.conflicts_resolved(now);
        self.tick(cx);
        cx.notify();
    }

    // ---- Settings ----

    /// Follows new sync settings; a new branch or file list reopens the clone.
    pub fn apply_settings(&mut self, settings: SyncSettings, cx: &mut Context<Self>) {
        if settings == self.settings {
            return;
        }
        let reopen = settings.branch != self.settings.branch
            || settings.legacy_branch != self.settings.legacy_branch
            || settings.device_only != self.settings.device_only;
        self.scheduler.set_config(scheduler_config(&settings));
        self.settings = settings;
        if reopen && self.step_task.is_none() {
            self.open(cx);
        } else {
            self.reschedule(cx);
        }
        cx.notify();
    }

    /// Points the vault's remote at `url` and starts over with it.
    pub fn set_remote_url(&mut self, url: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let url = url.trim();
        if url.is_empty() {
            return Err("Enter the address of your notes repository.".to_owned());
        }
        if self.remote_url.as_deref() == Some(url) {
            return Ok(());
        }
        gasp_sync::set_remote_url(&self.root, engine::REMOTE, url).map_err(|_| {
            "This vault isn’t a git repository yet. Clone your notes repository into it first."
                .to_owned()
        })?;
        self.open(cx);
        Ok(())
    }

    /// Keeps `token` for this vault's remote and syncs with it.
    pub fn sign_in(&mut self, token: Token, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(url) = self.remote_url.clone() else {
            return Err("Add the repository’s address first.".to_owned());
        };
        self.store
            .save(&url, &token)
            .map_err(|error| format!("The token couldn’t be saved: {error}"))?;
        self.signed_in = true;
        if let Presence::Ready(engine) = &self.presence {
            engine.set_token(Some(token));
        }
        self.sync_now(cx);
        Ok(())
    }

    /// Forgets this vault's token.
    pub fn sign_out(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some(url) = &self.remote_url {
            self.store
                .delete(url)
                .map_err(|error| format!("The token couldn’t be removed: {error}"))?;
        }
        if let Presence::Ready(engine) = &self.presence {
            engine.set_token(None);
        }
        self.signed_in = false;
        cx.notify();
        Ok(())
    }
}
