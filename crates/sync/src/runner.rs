use std::time::Duration;

use crate::error::{SyncError, SyncResult};
use crate::scheduler::{
    FailureKind, MergeReport, Scheduler, StepFailure, StepReport, SyncStatus, SyncStep,
};
use crate::vault::{Author, MergeOutcome, Vault};

/// Runs one scheduler step against `vault` and reports how it went.
pub fn run_step(
    vault: &Vault,
    step: SyncStep,
    author: &Author,
    commit_message: &str,
) -> StepReport {
    let result = match step {
        SyncStep::Commit => {
            vault
                .commit_all(author, commit_message)
                .map(|commit| StepReport::Committed {
                    new_commit: commit.is_some(),
                })
        }
        SyncStep::Fetch => vault.fetch().map(|()| StepReport::Fetched),
        SyncStep::Merge => vault
            .merge(author)
            .and_then(|outcome| merge_report(vault, &outcome)),
        SyncStep::Push => vault.push().map(|()| StepReport::Pushed),
    };
    result.unwrap_or_else(|error| failure_report(vault, error))
}

/// Polls `scheduler` at `now` and runs every step it hands out, in order.
pub fn drive(
    vault: &Vault,
    scheduler: &mut Scheduler,
    now: Duration,
    author: &Author,
    commit_message: &str,
) -> SyncStatus {
    let mut next = scheduler.poll(now);
    while let Some(step) = next {
        let report = run_step(vault, step, author, commit_message);
        next = scheduler.report(now, report);
    }
    scheduler.status()
}

/// Files waiting for a person outrank what the merge itself did: the
/// sync still goes on to push, but the status keeps showing them.
fn merge_report(vault: &Vault, outcome: &MergeOutcome) -> SyncResult<StepReport> {
    let waiting = match outcome {
        MergeOutcome::Conflicts(files) => files.len(),
        _ => vault.conflicts()?.len(),
    };
    let report = match outcome {
        _ if waiting > 0 => MergeReport::Conflicts { files: waiting },
        MergeOutcome::NothingToMerge | MergeOutcome::UpToDate => MergeReport::UpToDate,
        _ => MergeReport::Merged,
    };
    Ok(StepReport::Merged(report))
}

fn failure_report(vault: &Vault, error: SyncError) -> StepReport {
    StepReport::Failed(StepFailure {
        kind: failure_kind(&error),
        waiting: vault.unpushed_changes().unwrap_or(0),
        message: error.to_string(),
    })
}

fn failure_kind(error: &SyncError) -> FailureKind {
    match error {
        SyncError::Offline(_) => FailureKind::Offline,
        SyncError::Auth(_) => FailureKind::SignIn,
        SyncError::PushRejected(_) => FailureKind::Rejected,
        _ => FailureKind::Other,
    }
}
