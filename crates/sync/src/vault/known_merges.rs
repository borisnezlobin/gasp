//! Which fetched branch tips the local branch already holds.
//!
//! Git works out that a fetched tip is already merged by walking back from
//! the local branch until it meets the tip. The legacy branch stops moving
//! once every device runs the app, so that walk went through every commit
//! made since, on every merge. The record here names the commit each tip
//! was last seen merged into; while the branch only moves forward from
//! there, the short walk back to that commit is all it takes.

use std::fs;
use std::path::Path;

use git2::Oid;

use super::Vault;
use crate::error::SyncResult;

const RECORD_FILE: &str = "gasp-sync-merged";

/// `tip` of `branch` is part of the local branch as of `head`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KnownMerge {
    branch: String,
    tip: Oid,
    head: Oid,
}

impl Vault {
    /// Whether `tip` of `branch` is known to be in the local branch.
    pub(super) fn known_merged(&self, branch: &str, tip: Oid) -> bool {
        let Ok(Some(head)) = self.head_commit() else {
            return false;
        };
        let known = load(self.repo.path());
        let Some(known) = known.iter().find(|known| known.branch == branch) else {
            return false;
        };
        known.tip == tip
            && (known.head == head
                || self
                    .repo
                    .graph_descendant_of(head, known.head)
                    .unwrap_or(false))
    }

    /// Records that `tip` of `branch` is in the local branch as it is now.
    pub(super) fn remember_merged(&self, branch: &str, tip: Oid) -> SyncResult<()> {
        let Some(head) = self.head_commit()? else {
            return Ok(());
        };
        let mut known = load(self.repo.path());
        let entry = KnownMerge {
            branch: branch.to_owned(),
            tip,
            head,
        };
        if known.contains(&entry) {
            return Ok(());
        }
        known.retain(|known| known.branch != branch);
        known.push(entry);
        let text: String = known
            .iter()
            .map(|known| format!("{} {} {}\n", known.branch, known.tip, known.head))
            .collect();
        fs::write(self.repo.path().join(RECORD_FILE), text)?;
        Ok(())
    }
}

/// The record, or nothing when it's missing or unreadable: then git's
/// own walk decides, as it always could.
fn load(git_dir: &Path) -> Vec<KnownMerge> {
    let Ok(text) = fs::read_to_string(git_dir.join(RECORD_FILE)) else {
        return Vec::new();
    };
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<KnownMerge> {
    let mut fields = line.split_whitespace();
    let branch = fields.next()?.to_owned();
    let tip = Oid::from_str(fields.next()?).ok()?;
    let head = Oid::from_str(fields.next()?).ok()?;
    fields
        .next()
        .is_none()
        .then_some(KnownMerge { branch, tip, head })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_parse_and_garbage_is_skipped() {
        let tip = "a".repeat(40);
        let head = "b".repeat(40);
        let parsed = parse_line(&format!("main {tip} {head}")).unwrap();
        assert_eq!(parsed.branch, "main");
        assert_eq!(parsed.tip, Oid::from_str(&tip).unwrap());
        assert_eq!(parse_line("main nope"), None);
        assert_eq!(parse_line(&format!("main {tip} {head} extra")), None);
    }
}
