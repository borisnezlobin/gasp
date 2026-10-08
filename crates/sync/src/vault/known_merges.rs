//! Which fetched branch tips the local branch already holds.
//!
//! Git works out that a fetched tip is already merged by walking back from
//! the local branch until it meets the tip, which can take a walk through
//! every commit made since that tip. The record here names the commit each
//! tip was last seen merged into; while the branch only moves forward from
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
    /// Whether `tip` of `branch` is known to be in the local branch. When
    /// it is and the branch has moved on since, the record moves with it,
    /// so the next check walks back only as far as this one.
    pub(super) fn known_merged(&self, branch: &str, tip: Oid) -> SyncResult<bool> {
        let Some(head) = self.head_commit()? else {
            return Ok(false);
        };
        let mut known = load(self.repo.path());
        let Some(entry) = known.iter_mut().find(|known| known.branch == branch) else {
            return Ok(false);
        };
        if entry.tip != tip {
            return Ok(false);
        }
        if entry.head == head {
            return Ok(true);
        }
        let descends = self.repo.graph_descendant_of(head, entry.head);
        if !descends.unwrap_or(false) {
            return Ok(false);
        }
        entry.head = head;
        save(self.repo.path(), &known)?;
        Ok(true)
    }

    /// Records that `tip` of `branch` is in the local branch as it is now.
    pub(super) fn remember_merged(&self, branch: &str, tip: Oid) -> SyncResult<()> {
        let Some(head) = self.head_commit()? else {
            return Ok(());
        };
        let mut known = load(self.repo.path());
        known.retain(|known| known.branch != branch);
        known.push(KnownMerge {
            branch: branch.to_owned(),
            tip,
            head,
        });
        save(self.repo.path(), &known)
    }
}

fn save(git_dir: &Path, known: &[KnownMerge]) -> SyncResult<()> {
    let text: String = known
        .iter()
        .map(|known| format!("{} {} {}\n", known.branch, known.tip, known.head))
        .collect();
    fs::write(git_dir.join(RECORD_FILE), text)?;
    Ok(())
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
