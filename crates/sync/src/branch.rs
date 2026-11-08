//! Moving a vault's clone onto another branch, as the Branch setting and
//! the indicator's "Switch to …" button do.
//!
//! When the remote has the branch, the vault takes it, and the notes on
//! disk become that branch's notes. Edits this device hasn't synced yet
//! come along. Where the branch changed the same note, the two are merged
//! line by line. If an edit can't be merged, nothing changes and the
//! error names the notes. When the remote has no such branch, the branch
//! starts at the commit the vault is on and reaches the remote with the
//! next push.

use std::fs;
use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{Commit, DiffOptions, FetchOptions, FetchPrune, Oid, Repository, Tree};

use crate::credentials::{Token, remote_callbacks};
use crate::error::{SyncError, SyncResult};
use crate::line_merge::{LineMerge, merge_lines};
use crate::vault::probe;

/// Moves the clone at `path` from the branch sync used, `from`, onto `to`,
/// after the branch setting changed, but only when it's on `from`: a clone
/// on some other branch belongs to another tool and is left alone.
pub fn follow_branch(
    path: &Path,
    remote: &str,
    from: &str,
    to: &str,
    token: Option<&Token>,
) -> SyncResult<()> {
    let on_old_branch =
        probe(path, remote).is_some_and(|found| found.branch.as_deref() == Some(from.trim()));
    if !on_old_branch {
        return Ok(());
    }
    switch_branch(path, remote, to.trim(), token)
}

/// Moves the clone at `path` onto `branch`, taking the remote's branch
/// when it has one. See the module's documentation for what happens to
/// the notes on disk.
pub fn switch_branch(
    path: &Path,
    remote: &str,
    branch: &str,
    token: Option<&Token>,
) -> SyncResult<()> {
    let repo = Repository::open(path)?;
    let head = repo.head()?;
    if head.shorthand() == Some(branch) {
        return Ok(());
    }
    let current = head.peel_to_commit()?;
    let remote_tip = fetch_branch(&repo, remote, branch, token)?;
    let target = target_commit(&repo, branch, current.id(), remote_tip)?;
    let carried = if target == current.id() {
        Vec::new()
    } else {
        let target_tree = repo.find_commit(target)?.tree()?;
        let carried = carried_edits(&repo, &current, &target_tree, branch)?;
        repo.checkout_tree(
            target_tree.as_object(),
            Some(CheckoutBuilder::new().force()),
        )?;
        carried
    };
    let local_ref = format!("refs/heads/{branch}");
    repo.reference(&local_ref, target, true, "sync: move to branch")?;
    repo.set_head(&local_ref)?;
    write_carried(path, &carried)
}

/// Why a move onto another branch stopped, as a sentence or two that say
/// what to do. The vault is where it was before the move.
pub fn branch_move_problem(error: &SyncError) -> String {
    match error {
        SyncError::BranchBlocked { .. } => format!(
            "{error}. Switching now would lose one of the two versions. Make them match or undo this device’s edit, then switch again."
        ),
        SyncError::Offline(_) => {
            "The notes repository can’t be reached right now. Switch again once this device is online.".to_owned()
        }
        SyncError::Auth(_) => {
            "The notes repository didn’t accept this device’s sign-in. Check it in sync settings, then switch again.".to_owned()
        }
        SyncError::Untrusted(message) => message.clone(),
        other => format!(
            "Git said: {}",
            crate::phase::plain_git_message(&other.to_string())
        ),
    }
}

/// Fetches `branch` into its remote-tracking ref and returns where the
/// remote has it, or `None` when it doesn't have it or there's no remote.
fn fetch_branch(
    repo: &Repository,
    remote: &str,
    branch: &str,
    token: Option<&Token>,
) -> SyncResult<Option<Oid>> {
    let Ok(mut found) = repo.find_remote(remote) else {
        return Ok(None);
    };
    let url = found.url().unwrap_or_default().to_owned();
    let tracking = format!("refs/remotes/{remote}/{branch}");
    let refspec = format!("+refs/heads/{branch}:{tracking}");
    let mut options = FetchOptions::new();
    options
        .remote_callbacks(remote_callbacks(token))
        .update_fetchhead(false)
        .prune(FetchPrune::On);
    found
        .fetch(&[refspec], Some(&mut options), None)
        .map_err(|error| SyncError::from_transport(error, &url))?;
    Ok(repo.refname_to_id(&tracking).ok())
}

/// The commit the branch should be at: the remote's tip when this device
/// has no such branch or is behind it, else this device's own branch,
/// whose differences the next sync merges. A branch neither side has
/// starts at `current`.
fn target_commit(
    repo: &Repository,
    branch: &str,
    current: Oid,
    remote_tip: Option<Oid>,
) -> SyncResult<Oid> {
    let local = repo
        .find_branch(branch, git2::BranchType::Local)
        .ok()
        .and_then(|found| found.get().target());
    let target = match (local, remote_tip) {
        (Some(local), Some(remote)) if repo.graph_descendant_of(remote, local)? => remote,
        (Some(local), _) => local,
        (None, Some(remote)) => remote,
        (None, None) => current,
    };
    Ok(target)
}

/// A file as it should be once the branch is checked out: its new
/// contents, or `None` to delete it.
struct Carried {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

/// What this device changed since `current`, settled against `target`.
/// Fails without touching anything when an edit can't be merged.
fn carried_edits(
    repo: &Repository,
    current: &Commit<'_>,
    target: &Tree<'_>,
    branch: &str,
) -> SyncResult<Vec<Carried>> {
    let base_tree = current.tree()?;
    let mut options = DiffOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let diff = repo.diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut options))?;
    let root = repo.workdir().unwrap_or_else(|| repo.path());
    let mut carried = Vec::new();
    let mut blocked = Vec::new();
    for delta in diff.deltas() {
        let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) else {
            continue;
        };
        let versions = Versions {
            base: blob_at(repo, &base_tree, path),
            local: fs::read(root.join(path)).ok(),
            target: blob_at(repo, target, path),
        };
        match versions.settle() {
            Settle::Keep => {}
            Settle::Write(contents) => carried.push(Carried {
                path: path.to_path_buf(),
                contents,
            }),
            Settle::Blocked => blocked.push(path.to_path_buf()),
        }
    }
    if blocked.is_empty() {
        Ok(carried)
    } else {
        Err(SyncError::BranchBlocked {
            branch: branch.to_owned(),
            notes: blocked,
        })
    }
}

fn blob_at(repo: &Repository, tree: &Tree<'_>, path: &Path) -> Option<Vec<u8>> {
    let entry = tree.get_path(path).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    Some(blob.content().to_vec())
}

/// One file's three versions: as the vault's commit has it, as it is on
/// disk, and as the branch has it. `None` is a file that isn't there.
struct Versions {
    base: Option<Vec<u8>>,
    local: Option<Vec<u8>>,
    target: Option<Vec<u8>>,
}

/// What to do with one changed file once the branch is checked out.
enum Settle {
    /// The branch already has it as it is on disk.
    Keep,
    Write(Option<Vec<u8>>),
    /// Both sides changed it in a way that can't be merged.
    Blocked,
}

impl Versions {
    fn settle(self) -> Settle {
        if self.target == self.local {
            return Settle::Keep;
        }
        if self.target == self.base {
            return Settle::Write(self.local);
        }
        let (Some(local), Some(target)) = (&self.local, &self.target) else {
            return Settle::Blocked;
        };
        let texts = (
            mergeable_text(self.base.as_deref().unwrap_or_default()),
            mergeable_text(local),
            mergeable_text(target),
        );
        let (Some(base), Some(local), Some(target)) = texts else {
            return Settle::Blocked;
        };
        match merge_lines(base, local, target) {
            LineMerge::Clean(merged) => Settle::Write(Some(merged.into_bytes())),
            LineMerge::Conflicted(_) => Settle::Blocked,
        }
    }
}

/// The text of a file that merges by line: UTF-8, and not an encrypted
/// note, whose lines are ciphertext.
fn mergeable_text(bytes: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    (!gasp_crypt::is_encrypted(text)).then_some(text)
}

fn write_carried(root: &Path, carried: &[Carried]) -> SyncResult<()> {
    for file in carried {
        let path = root.join(&file.path);
        match &file.contents {
            Some(bytes) => {
                if let Some(folder) = path.parent() {
                    fs::create_dir_all(folder)?;
                }
                fs::write(&path, bytes)?;
            }
            None if path.exists() => fs::remove_file(&path)?,
            None => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versions(base: &str, local: &str, target: &str) -> Versions {
        let some = |text: &str| Some(text.as_bytes().to_vec());
        Versions {
            base: some(base),
            local: some(local),
            target: some(target),
        }
    }

    #[test]
    fn an_edit_the_branch_never_touched_comes_along_as_it_is() {
        let settled = versions("a\n", "a\nmine\n", "a\n").settle();
        assert!(matches!(settled, Settle::Write(Some(bytes)) if bytes == b"a\nmine\n"));
    }

    #[test]
    fn edits_to_different_lines_merge() {
        let settled = versions("a\nb\nc\n", "a\nmine\nc\n", "a\nb\nc\ntheirs\n").settle();
        assert!(matches!(settled, Settle::Write(Some(bytes)) if bytes == b"a\nmine\nc\ntheirs\n"));
    }

    #[test]
    fn the_same_change_on_both_sides_needs_nothing() {
        assert!(matches!(
            versions("a\n", "b\n", "b\n").settle(),
            Settle::Keep
        ));
    }

    #[test]
    fn edits_to_the_same_line_block_the_move() {
        assert!(matches!(
            versions("a\n", "mine\n", "theirs\n").settle(),
            Settle::Blocked
        ));
    }

    #[test]
    fn an_edit_to_a_note_the_branch_deleted_blocks_the_move() {
        let settled = Versions {
            base: Some(b"a\n".to_vec()),
            local: Some(b"a\nmine\n".to_vec()),
            target: None,
        }
        .settle();
        assert!(matches!(settled, Settle::Blocked));
    }
}
