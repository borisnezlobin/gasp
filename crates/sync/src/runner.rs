use std::time::Duration;

use crate::error::SyncError;
use crate::scheduler::{MergeReport, Scheduler, StepFailure, StepReport, SyncStatus, SyncStep};
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
        SyncStep::Merge => vault.merge(author).map(merge_report),
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

fn merge_report(outcome: MergeOutcome) -> StepReport {
    let report = match outcome {
        MergeOutcome::NothingToMerge | MergeOutcome::UpToDate => MergeReport::UpToDate,
        MergeOutcome::FastForward | MergeOutcome::Merged { .. } => MergeReport::Merged,
        MergeOutcome::Conflicts(files) => MergeReport::Conflicts { files: files.len() },
    };
    StepReport::Merged(report)
}

fn failure_report(vault: &Vault, error: SyncError) -> StepReport {
    if let SyncError::UnresolvedConflicts(files) = error {
        return StepReport::Merged(MergeReport::Conflicts { files });
    }
    StepReport::Failed(StepFailure {
        offline: error.is_offline(),
        waiting: vault.unpushed_changes().unwrap_or(0),
        message: error.to_string(),
    })
}
