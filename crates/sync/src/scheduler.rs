use std::collections::VecDeque;
use std::time::Duration;

/// Timing knobs for automatic sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Quiet time after the last edit before committing.
    pub debounce: Duration,
    /// How long to wait before retrying after a failed sync.
    pub retry_after: Duration,
    /// Fetch from the remote this often while idle, if set.
    pub poll_every: Option<Duration>,
    /// Oldest log entries are dropped past this many.
    pub log_capacity: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_secs(60),
            retry_after: Duration::from_secs(60),
            poll_every: Some(Duration::from_secs(300)),
            log_capacity: 500,
        }
    }
}

/// What the sync indicator shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Synced,
    Syncing,
    /// The remote could not be reached; `waiting` changed files are not on it yet.
    Offline {
        waiting: usize,
    },
    /// The remote refused the credentials; syncing waits for a new token.
    SignInNeeded {
        waiting: usize,
    },
    /// A step failed for another reason; it is retried after a while.
    Failed {
        waiting: usize,
    },
    /// A merge is paused on `files` conflicted files.
    Conflict {
        files: usize,
    },
}

/// One step of a sync, run by the caller in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStep {
    Commit,
    Fetch,
    Merge,
    Push,
}

/// How a merge step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeReport {
    UpToDate,
    Merged,
    Conflicts { files: usize },
}

/// What kind of trouble stopped a step, which decides what the status
/// shows and whether the scheduler retries on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The remote could not be reached. Retried after a while.
    Offline,
    /// The remote refused the credentials. Retrying with the same token
    /// can't help, so it waits for a sync request.
    SignIn,
    /// The remote moved on since the fetch. Retried after a while.
    Rejected,
    Other,
}

/// Why a step failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepFailure {
    pub kind: FailureKind,
    /// Changed files not yet on the remote.
    pub waiting: usize,
    pub message: String,
}

/// The caller's report of the step it just ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepReport {
    Committed { new_commit: bool },
    Fetched,
    Merged(MergeReport),
    Pushed,
    Failed(StepFailure),
}

/// What happened, for the sync log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncEventKind {
    Started,
    Committed,
    NothingToCommit,
    Fetched,
    Merged,
    UpToDate,
    Conflict { files: usize },
    ConflictsResolved,
    Pushed,
    Offline { waiting: usize, message: String },
    SignInNeeded { message: String },
    Failed { message: String },
}

/// One sync log entry, stamped with the caller's clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEvent {
    pub at: Duration,
    pub kind: SyncEventKind,
}

/// Decides when to sync and tracks status. Pure logic: the caller supplies
/// the time (any monotonic clock, as a `Duration` since some start) and
/// runs the steps it hands out.
#[derive(Debug, Clone)]
pub struct Scheduler {
    config: SchedulerConfig,
    status: SyncStatus,
    last_edit: Option<Duration>,
    dirty: bool,
    sync_requested: bool,
    in_flight: Option<SyncStep>,
    retry_at: Option<Duration>,
    last_synced_at: Option<Duration>,
    log: VecDeque<SyncEvent>,
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            status: SyncStatus::Synced,
            last_edit: None,
            dirty: false,
            sync_requested: false,
            in_flight: None,
            retry_at: None,
            last_synced_at: None,
            log: VecDeque::new(),
        }
    }

    pub fn status(&self) -> SyncStatus {
        self.status
    }

    pub fn log(&self) -> impl Iterator<Item = &SyncEvent> {
        self.log.iter()
    }

    pub fn in_flight(&self) -> Option<SyncStep> {
        self.in_flight
    }

    /// When the last sync finished pushing, on the caller's clock.
    pub fn last_synced_at(&self) -> Option<Duration> {
        self.last_synced_at
    }

    /// Changes the timing knobs, such as after the settings changed.
    pub fn set_config(&mut self, config: SchedulerConfig) {
        self.config = config;
    }

    /// Records an edit; the commit waits until edits stop for the debounce time.
    pub fn edited(&mut self, now: Duration) {
        self.last_edit = Some(now);
        self.dirty = true;
    }

    /// Asks for a sync at the next poll (sync.now, app open, foreground or background).
    pub fn request_sync(&mut self) {
        self.sync_requested = true;
    }

    /// Call after the person resolved every conflict; the next poll pushes the merge.
    pub fn conflicts_resolved(&mut self, now: Duration) {
        if matches!(self.status, SyncStatus::Conflict { .. }) {
            self.record(now, SyncEventKind::ConflictsResolved);
            self.status = SyncStatus::Syncing;
            self.sync_requested = true;
        }
    }

    /// Returns the first step of a sync when one is due.
    pub fn poll(&mut self, now: Duration) -> Option<SyncStep> {
        if self.in_flight.is_some() || matches!(self.status, SyncStatus::Conflict { .. }) {
            return None;
        }
        if !self.is_due(now) {
            return None;
        }
        self.dirty = false;
        self.sync_requested = false;
        self.retry_at = None;
        self.status = SyncStatus::Syncing;
        self.record(now, SyncEventKind::Started);
        self.start(SyncStep::Commit)
    }

    /// The earliest time a poll could start a sync, if nothing else happens.
    pub fn next_wake(&self) -> Option<Duration> {
        if self.in_flight.is_some() || matches!(self.status, SyncStatus::Conflict { .. }) {
            return None;
        }
        [
            self.debounce_deadline(),
            self.retry_at,
            self.poll_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Takes the result of the step in flight and returns the next step, if any.
    pub fn report(&mut self, now: Duration, report: StepReport) -> Option<SyncStep> {
        self.in_flight.take()?;
        match report {
            StepReport::Committed { new_commit } => {
                let kind = if new_commit {
                    SyncEventKind::Committed
                } else {
                    SyncEventKind::NothingToCommit
                };
                self.record(now, kind);
                self.start(SyncStep::Fetch)
            }
            StepReport::Fetched => {
                self.record(now, SyncEventKind::Fetched);
                self.start(SyncStep::Merge)
            }
            StepReport::Merged(merge) => self.after_merge(now, merge),
            StepReport::Pushed => {
                self.record(now, SyncEventKind::Pushed);
                self.status = SyncStatus::Synced;
                self.last_synced_at = Some(now);
                None
            }
            StepReport::Failed(failure) => {
                self.fail(now, failure);
                None
            }
        }
    }

    fn after_merge(&mut self, now: Duration, merge: MergeReport) -> Option<SyncStep> {
        match merge {
            MergeReport::UpToDate => self.record(now, SyncEventKind::UpToDate),
            MergeReport::Merged => self.record(now, SyncEventKind::Merged),
            MergeReport::Conflicts { files } => {
                self.record(now, SyncEventKind::Conflict { files });
                self.status = SyncStatus::Conflict { files };
                return None;
            }
        }
        self.start(SyncStep::Push)
    }

    fn fail(&mut self, now: Duration, failure: StepFailure) {
        let StepFailure {
            kind,
            waiting,
            message,
        } = failure;
        let (event, status) = match kind {
            FailureKind::Offline => (
                SyncEventKind::Offline { waiting, message },
                SyncStatus::Offline { waiting },
            ),
            FailureKind::SignIn => (
                SyncEventKind::SignInNeeded { message },
                SyncStatus::SignInNeeded { waiting },
            ),
            FailureKind::Rejected | FailureKind::Other => (
                SyncEventKind::Failed { message },
                SyncStatus::Failed { waiting },
            ),
        };
        self.record(now, event);
        self.status = status;
        self.retry_at = (kind != FailureKind::SignIn).then(|| now + self.config.retry_after);
    }

    fn start(&mut self, step: SyncStep) -> Option<SyncStep> {
        self.in_flight = Some(step);
        Some(step)
    }

    fn is_due(&self, now: Duration) -> bool {
        self.sync_requested || self.next_wake().is_some_and(|wake| now >= wake)
    }

    fn debounce_deadline(&self) -> Option<Duration> {
        let last_edit = self.last_edit.filter(|_| self.dirty)?;
        Some(last_edit + self.config.debounce)
    }

    fn poll_deadline(&self) -> Option<Duration> {
        if self.status != SyncStatus::Synced {
            return None;
        }
        Some(self.last_synced_at? + self.config.poll_every?)
    }

    fn record(&mut self, at: Duration, kind: SyncEventKind) {
        if self.log.len() == self.config.log_capacity {
            self.log.pop_front();
        }
        self.log.push_back(SyncEvent { at, kind });
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(SchedulerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    fn run_clean_sync(scheduler: &mut Scheduler, now: Duration) {
        assert_eq!(scheduler.poll(now), Some(SyncStep::Commit));
        let reports = [
            StepReport::Committed { new_commit: true },
            StepReport::Fetched,
            StepReport::Merged(MergeReport::UpToDate),
        ];
        for report in reports {
            assert!(scheduler.report(now, report).is_some());
        }
        assert_eq!(scheduler.report(now, StepReport::Pushed), None);
    }

    fn offline(waiting: usize) -> StepReport {
        StepReport::Failed(StepFailure {
            kind: FailureKind::Offline,
            waiting,
            message: "unreachable".into(),
        })
    }

    #[test]
    fn debounces_until_a_minute_after_the_last_edit() {
        let mut scheduler = Scheduler::default();
        scheduler.edited(secs(0));
        scheduler.edited(secs(30));
        assert_eq!(scheduler.poll(secs(60)), None);
        assert_eq!(scheduler.next_wake(), Some(secs(90)));
        scheduler.edited(secs(80));
        assert_eq!(scheduler.poll(secs(90)), None);
        assert_eq!(scheduler.poll(secs(139)), None);
        assert_eq!(scheduler.poll(secs(140)), Some(SyncStep::Commit));
        assert_eq!(scheduler.status(), SyncStatus::Syncing);
    }

    #[test]
    fn steps_run_in_order_and_end_synced() {
        let mut scheduler = Scheduler::default();
        scheduler.edited(secs(0));
        assert_eq!(scheduler.poll(secs(60)), Some(SyncStep::Commit));
        assert_eq!(
            scheduler.report(secs(61), StepReport::Committed { new_commit: true }),
            Some(SyncStep::Fetch)
        );
        assert_eq!(
            scheduler.report(secs(62), StepReport::Fetched),
            Some(SyncStep::Merge)
        );
        assert_eq!(
            scheduler.report(secs(63), StepReport::Merged(MergeReport::Merged)),
            Some(SyncStep::Push)
        );
        assert_eq!(scheduler.report(secs(64), StepReport::Pushed), None);
        assert_eq!(scheduler.status(), SyncStatus::Synced);
        let kinds: Vec<_> = scheduler.log().map(|event| event.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                SyncEventKind::Started,
                SyncEventKind::Committed,
                SyncEventKind::Fetched,
                SyncEventKind::Merged,
                SyncEventKind::Pushed
            ]
        );
    }

    #[test]
    fn nothing_starts_while_a_step_is_in_flight() {
        let mut scheduler = Scheduler::default();
        scheduler.request_sync();
        assert_eq!(scheduler.poll(secs(0)), Some(SyncStep::Commit));
        scheduler.request_sync();
        assert_eq!(scheduler.poll(secs(1)), None);
        assert_eq!(scheduler.next_wake(), None);
    }

    #[test]
    fn edits_during_a_sync_trigger_another_after_debounce() {
        let mut scheduler = Scheduler::default();
        scheduler.edited(secs(0));
        assert_eq!(scheduler.poll(secs(60)), Some(SyncStep::Commit));
        scheduler.edited(secs(61));
        for report in [
            StepReport::Committed { new_commit: true },
            StepReport::Fetched,
            StepReport::Merged(MergeReport::UpToDate),
            StepReport::Pushed,
        ] {
            scheduler.report(secs(62), report);
        }
        assert_eq!(scheduler.next_wake(), Some(secs(121)));
        assert_eq!(scheduler.poll(secs(121)), Some(SyncStep::Commit));
    }

    #[test]
    fn offline_failure_reports_waiting_changes_and_retries() {
        let mut scheduler = Scheduler::default();
        scheduler.request_sync();
        scheduler.poll(secs(0));
        scheduler.report(secs(0), StepReport::Committed { new_commit: true });
        scheduler.report(secs(1), StepReport::Fetched);
        scheduler.report(secs(1), StepReport::Merged(MergeReport::UpToDate));
        assert_eq!(scheduler.report(secs(2), offline(3)), None);
        assert_eq!(scheduler.status(), SyncStatus::Offline { waiting: 3 });
        assert_eq!(scheduler.poll(secs(30)), None);
        assert_eq!(scheduler.poll(secs(62)), Some(SyncStep::Commit));
    }

    #[test]
    fn refused_credentials_wait_for_a_request_instead_of_retrying() {
        let mut scheduler = Scheduler::default();
        scheduler.request_sync();
        scheduler.poll(secs(0));
        let refused = StepReport::Failed(StepFailure {
            kind: FailureKind::SignIn,
            waiting: 1,
            message: "authentication failed".into(),
        });
        scheduler.report(secs(1), StepReport::Committed { new_commit: true });
        scheduler.report(secs(1), StepReport::Fetched);
        assert_eq!(scheduler.report(secs(2), refused), None);
        assert_eq!(scheduler.status(), SyncStatus::SignInNeeded { waiting: 1 });
        assert_eq!(scheduler.next_wake(), None);
        assert_eq!(scheduler.poll(secs(500)), None);
        scheduler.request_sync();
        assert_eq!(scheduler.poll(secs(501)), Some(SyncStep::Commit));
    }

    #[test]
    fn other_failures_show_as_failed_and_retry() {
        let mut scheduler = Scheduler::default();
        scheduler.request_sync();
        scheduler.poll(secs(0));
        let failed = StepReport::Failed(StepFailure {
            kind: FailureKind::Other,
            waiting: 0,
            message: "disk full".into(),
        });
        assert_eq!(scheduler.report(secs(5), failed), None);
        assert_eq!(scheduler.status(), SyncStatus::Failed { waiting: 0 });
        assert_eq!(scheduler.next_wake(), Some(secs(65)));
    }

    #[test]
    fn conflict_pauses_until_resolved() {
        let mut scheduler = Scheduler::default();
        scheduler.request_sync();
        scheduler.poll(secs(0));
        scheduler.report(secs(0), StepReport::Committed { new_commit: false });
        scheduler.report(secs(0), StepReport::Fetched);
        let merge = StepReport::Merged(MergeReport::Conflicts { files: 2 });
        assert_eq!(scheduler.report(secs(0), merge), None);
        assert_eq!(scheduler.status(), SyncStatus::Conflict { files: 2 });
        scheduler.edited(secs(1));
        assert_eq!(scheduler.poll(secs(500)), None);
        scheduler.conflicts_resolved(secs(501));
        assert_eq!(scheduler.poll(secs(501)), Some(SyncStep::Commit));
    }

    #[test]
    fn idle_vault_fetches_periodically() {
        let mut scheduler = Scheduler::default();
        scheduler.request_sync();
        run_clean_sync(&mut scheduler, secs(10));
        assert_eq!(scheduler.next_wake(), Some(secs(310)));
        assert_eq!(scheduler.poll(secs(309)), None);
        assert_eq!(scheduler.poll(secs(310)), Some(SyncStep::Commit));
    }

    #[test]
    fn log_is_capped() {
        let config = SchedulerConfig {
            log_capacity: 3,
            ..SchedulerConfig::default()
        };
        let mut scheduler = Scheduler::new(config);
        scheduler.request_sync();
        run_clean_sync(&mut scheduler, secs(0));
        let kinds: Vec<_> = scheduler.log().map(|event| event.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                SyncEventKind::Fetched,
                SyncEventKind::UpToDate,
                SyncEventKind::Pushed
            ]
        );
    }

    #[test]
    fn stray_reports_are_ignored() {
        let mut scheduler = Scheduler::default();
        assert_eq!(scheduler.report(secs(0), StepReport::Pushed), None);
        assert_eq!(scheduler.status(), SyncStatus::Synced);
        assert_eq!(scheduler.log().count(), 0);
    }
}
