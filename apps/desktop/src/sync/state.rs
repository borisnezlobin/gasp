//! What sync is doing, in words: the phase the indicator shows, the
//! sentence under it, when it last happened and what changed. Pure data,
//! so every state can be tested without a window.

use std::path::{Path, PathBuf};
use std::time::Duration;

use editor_sync::{FailureKind, SyncStep};

/// Why a git clone can't sync as the settings describe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupProblem {
    /// The clone is on another branch than the one sync uses.
    WrongBranch {
        expected: String,
        actual: Option<String>,
    },
    /// The clone couldn't be opened for sync.
    Broken(String),
}

/// What the indicator shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncPhase {
    /// The vault isn't a git clone with a remote, so there's nothing to show.
    Hidden,
    /// Looking at the clone, before the first sync can start.
    Starting,
    Setup(SetupProblem),
    Synced,
    Syncing(SyncStep),
    Offline {
        waiting: usize,
    },
    /// The remote wants a token: none is kept, or it refused the one that is.
    SignIn {
        has_token: bool,
    },
    Conflict {
        files: usize,
    },
    Failed {
        kind: FailureKind,
        message: String,
    },
}

impl SyncPhase {
    /// Whether the phase asks the person to do something.
    pub fn needs_attention(&self) -> bool {
        matches!(
            self,
            SyncPhase::Setup(_)
                | SyncPhase::SignIn { .. }
                | SyncPhase::Conflict { .. }
                | SyncPhase::Failed { .. }
        )
    }
}

/// One sync from its first step to its last, for the popover.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncRun {
    pub started_at: Duration,
    pub finished_at: Option<Duration>,
    /// Files other devices changed, brought in by the merge.
    pub received: Vec<PathBuf>,
    /// Files this device sent.
    pub sent: Vec<PathBuf>,
    /// How long each step took.
    pub steps: Vec<(SyncStep, Duration)>,
}

impl SyncRun {
    pub fn changed_anything(&self) -> bool {
        !self.received.is_empty() || !self.sent.is_empty()
    }

    /// Wall time spent in steps.
    pub fn total(&self) -> Duration {
        self.steps.iter().map(|(_, took)| *took).sum()
    }
}

/// `elapsed` as people say it: "just now", "2 minutes ago", "yesterday".
pub fn ago(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    match (seconds, minutes, hours, days) {
        (0..45, ..) => "just now".to_owned(),
        (_, 0..2, ..) => "a minute ago".to_owned(),
        (_, 2..60, ..) => format!("{minutes} minutes ago"),
        (_, _, 0..2, _) => "an hour ago".to_owned(),
        (_, _, 2..24, _) => format!("{hours} hours ago"),
        (.., 0..2) => "yesterday".to_owned(),
        _ => format!("{days} days ago"),
    }
}

/// "1 file" or "3 files".
pub fn count(number: usize, noun: &str) -> String {
    if number == 1 {
        format!("1 {noun}")
    } else {
        format!("{number} {noun}s")
    }
}

/// What a step is doing, as the indicator says it.
pub fn step_label(step: SyncStep) -> &'static str {
    match step {
        SyncStep::Commit => "Saving your changes for sync",
        SyncStep::Fetch => "Checking other devices for changes",
        SyncStep::Merge => "Bringing in changes",
        SyncStep::Push => "Sending your changes",
    }
}

/// The popover's first line for a phase. `synced_ago` is how long ago the
/// last sync finished, if one did.
pub fn headline(phase: &SyncPhase, synced_ago: Option<Duration>) -> String {
    match phase {
        SyncPhase::Hidden => String::new(),
        SyncPhase::Starting => "Getting sync ready".to_owned(),
        SyncPhase::Setup(_) => "Sync needs setting up".to_owned(),
        SyncPhase::Synced => match synced_ago {
            Some(elapsed) => format!("Synced {}", ago(elapsed)),
            None => "Not synced yet".to_owned(),
        },
        SyncPhase::Syncing(step) => step_label(*step).to_owned(),
        SyncPhase::Offline { .. } => "Offline".to_owned(),
        SyncPhase::SignIn { .. } => "Sign in to sync".to_owned(),
        SyncPhase::Conflict { files } => {
            format!("{} changed on two devices", count(*files, "note"))
        }
        SyncPhase::Failed { .. } => "Sync stopped".to_owned(),
    }
}

/// What happened and what to do about it, in a sentence or two.
pub fn explanation(phase: &SyncPhase) -> Option<String> {
    let text = match phase {
        SyncPhase::Setup(problem) => setup_explanation(problem),
        SyncPhase::Offline { waiting: 0 } => {
            "The notes repository can't be reached right now. Sync picks up again on its own once it can.".to_owned()
        }
        SyncPhase::Offline { waiting } => format!(
            "The notes repository can't be reached right now. {} will sync once it can.",
            count(*waiting, "changed file")
        ),
        SyncPhase::SignIn { has_token: true } => {
            "GitHub didn't accept this device's token. It may have expired. Paste a new one in sync settings.".to_owned()
        }
        SyncPhase::SignIn { has_token: false } => {
            "Your notes repository needs a GitHub token to sync. Paste one in sync settings.".to_owned()
        }
        SyncPhase::Conflict { .. } => {
            "Both devices changed the same lines. Pick which version to keep, and sync carries on.".to_owned()
        }
        SyncPhase::Failed { kind, message } => failure_explanation(*kind, message),
        _ => return None,
    };
    Some(text)
}

fn setup_explanation(problem: &SetupProblem) -> String {
    match problem {
        SetupProblem::WrongBranch {
            expected,
            actual: Some(actual),
        } => format!(
            "This vault is on the branch {actual}, but sync uses {expected}, so it doesn't sync here yet. Switch the vault to {expected} to move it to this app, or change the branch in sync settings."
        ),
        SetupProblem::WrongBranch {
            expected,
            actual: None,
        } => format!("This vault isn't on a branch. Check out {expected} to sync it."),
        SetupProblem::Broken(message) => format!(
            "Sync couldn't open this vault's git repository. Git said: {}",
            plain_git_message(message)
        ),
    }
}

fn failure_explanation(kind: FailureKind, message: &str) -> String {
    match kind {
        FailureKind::Rejected => {
            "Another device sent changes at the same moment. Sync tries again in a minute."
                .to_owned()
        }
        _ if message.contains("index is locked") => {
            "Another git program is working in this vault, so sync waits for it. If none is running, delete .git/index.lock in the vault. Sync tries again in a minute.".to_owned()
        }
        _ => format!(
            "Git said: {} Sync tries again in a minute.",
            plain_git_message(message)
        ),
    }
}

/// A libgit2 message as a sentence: without its `; class=… code=…`
/// suffix, capitalized, ending in a full stop.
pub fn plain_git_message(message: &str) -> String {
    let core = message.split("; class=").next().unwrap_or(message).trim();
    let core = core.trim_end_matches('.');
    let mut chars = core.chars();
    let capitalized: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => return String::new(),
    };
    format!("{capitalized}.")
}

/// How long ago things happened, for sentences that say when.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Times {
    /// Since the last sync that finished cleanly.
    pub synced: Option<Duration>,
    /// Since the last sync ended, however it ended.
    pub tried: Option<Duration>,
}

/// The tooltip: what's happening, and when it happened.
pub fn tooltip(phase: &SyncPhase, times: Times) -> String {
    let headline = headline(phase, times.synced);
    let when = match phase {
        SyncPhase::Synced | SyncPhase::Syncing(_) | SyncPhase::Hidden | SyncPhase::Starting => None,
        SyncPhase::Offline { waiting } if *waiting > 0 => {
            Some(format!("{} waiting", count(*waiting, "change")))
        }
        _ => None,
    };
    let since = match (times.synced, times.tried) {
        (_, None) => None,
        (Some(synced), _) if !matches!(phase, SyncPhase::Synced | SyncPhase::Syncing(_)) => {
            Some(format!("last synced {}", ago(synced)))
        }
        (None, Some(tried))
            if phase.needs_attention() || matches!(phase, SyncPhase::Offline { .. }) =>
        {
            Some(format!("tried {}", ago(tried)))
        }
        _ => None,
    };
    let details: Vec<String> = when.into_iter().chain(since).collect();
    if details.is_empty() {
        headline
    } else {
        format!("{headline} — {}", details.join(", "))
    }
}

/// How a changed file reads in a list: a note's title, else its name.
pub fn file_label(path: &Path) -> String {
    let is_note = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
    let name = if is_note {
        path.file_stem()
    } else {
        path.file_name()
    };
    name.map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Up to `limit` file labels, then how many more there are.
pub fn file_list(paths: &[PathBuf], limit: usize) -> (Vec<String>, usize) {
    let shown = paths
        .iter()
        .take(limit)
        .map(|path| file_label(path))
        .collect();
    (shown, paths.len().saturating_sub(limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    #[test]
    fn times_read_the_way_people_say_them() {
        let cases = [
            (0, "just now"),
            (44, "just now"),
            (60, "a minute ago"),
            (150, "2 minutes ago"),
            (59 * 60, "59 minutes ago"),
            (90 * 60, "an hour ago"),
            (5 * 3600, "5 hours ago"),
            (30 * 3600, "yesterday"),
            (72 * 3600, "3 days ago"),
        ];
        for (seconds, expected) in cases {
            assert_eq!(ago(secs(seconds)), expected, "{seconds}s");
        }
    }

    #[test]
    fn every_phase_has_a_headline_and_the_tooltip_says_when() {
        let synced = SyncPhase::Synced;
        let times = |synced| Times {
            synced,
            tried: synced,
        };
        assert_eq!(
            tooltip(&synced, times(Some(secs(130)))),
            "Synced 2 minutes ago"
        );
        assert_eq!(tooltip(&synced, times(None)), "Not synced yet");
        let offline = SyncPhase::Offline { waiting: 3 };
        let times = Times {
            synced: Some(secs(7200)),
            tried: Some(secs(10)),
        };
        assert_eq!(
            tooltip(&offline, times),
            "Offline — 3 changes waiting, last synced 2 hours ago"
        );
        let never = Times {
            synced: None,
            tried: Some(secs(130)),
        };
        assert_eq!(
            tooltip(&SyncPhase::SignIn { has_token: false }, never),
            "Sign in to sync — tried 2 minutes ago"
        );
        assert_eq!(
            explanation(&offline).unwrap(),
            "The notes repository can't be reached right now. 3 changed files will sync once it can."
        );
        let conflict = SyncPhase::Conflict { files: 1 };
        assert_eq!(headline(&conflict, None), "1 note changed on two devices");
        assert!(
            explanation(&SyncPhase::SignIn { has_token: true })
                .unwrap()
                .contains("expired")
        );
        assert!(explanation(&SyncPhase::Synced).is_none());
    }

    #[test]
    fn git_messages_read_as_sentences() {
        assert_eq!(
            plain_git_message("failed to resolve path; class=Os (2); code=NotFound (-3)"),
            "Failed to resolve path."
        );
        let locked = SyncPhase::Failed {
            kind: FailureKind::Other,
            message: "the index is locked; this might be due to a concurrent or crashed process; class=Index (10)".into(),
        };
        assert!(explanation(&locked).unwrap().contains(".git/index.lock"));
    }

    #[test]
    fn a_wrong_branch_names_both_branches_and_the_fix() {
        let phase = SyncPhase::Setup(SetupProblem::WrongBranch {
            expected: "master".into(),
            actual: Some("main".into()),
        });
        let text = explanation(&phase).unwrap();
        assert!(
            text.contains("on the branch main, but sync uses master"),
            "{text}"
        );
        assert!(phase.needs_attention());
        assert!(!SyncPhase::Offline { waiting: 1 }.needs_attention());
    }

    #[test]
    fn notes_list_by_title_and_long_lists_are_cut() {
        let paths: Vec<PathBuf> = ["a/Wave Packets.md", "images/plot.png", "b.md"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let (shown, more) = file_list(&paths, 2);
        assert_eq!(shown, ["Wave Packets", "plot.png"]);
        assert_eq!(more, 1);
    }
}
