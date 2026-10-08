//! Turning a vault folder that isn't a git clone into one, in place.
//!
//! The clone is built in a hidden folder beside the notes and only
//! becomes the vault's `.git` once everything worked, so a failure at any
//! step (an address that can't be reached, a token the remote refuses, a
//! push it rejects) leaves the folder exactly as it was, with no `.git`.
//!
//! The notes already in the folder are committed as this device's, then
//! merged with what the remote's branch holds. The two share no history,
//! so a note on both sides merges with [`merge_unrelated`]: lines only one
//! side has are kept, and a place where the two read differently waits for
//! a person, parked as any sync conflict is. The merge is pushed before
//! anything in the folder changes; only then are the remote's notes
//! written in.

use std::fs;
use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{
    FetchOptions, Index, IndexAddOption, IndexEntry, IndexTime, ObjectType, Oid, PushOptions,
    Repository, RepositoryInitOptions, Signature, Tree, TreeWalkMode, TreeWalkResult,
};

use crate::conflict::ConflictedFile;
use crate::credentials::{Token, remote_callbacks};
use crate::error::{SyncError, SyncResult};
use crate::line_merge::{LineMerge, merge_unrelated};
use crate::message::commit_message;
use crate::parked::{BranchKeeps, ParkedConflict, ParkedConflicts};
use crate::policy::{FileKind, classify};
use crate::vault::{Author, VaultConfig, pin_settings};

/// Where the clone is built before it becomes the vault's `.git`. It's
/// hidden, so the app and its file tree never show it.
pub const STAGING_FOLDER: &str = ".git-setup";

/// Everything setting up needs.
pub struct InPlaceSetup<'a> {
    /// The vault folder, which mustn't be a clone yet.
    pub root: &'a Path,
    /// The address git fetches from, as [`repository_url`] makes it.
    pub url: &'a str,
    pub config: VaultConfig,
    pub token: Option<Token>,
    pub author: Author,
    /// Names this device in the first commit's message.
    pub device: &'a str,
}

/// What setting up did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupReport {
    /// The remote's branch had no commits, so the notes started it.
    pub remote_was_empty: bool,
    /// Files the remote had that were written into the folder.
    pub brought_in: Vec<PathBuf>,
    /// Files this device sent: its own, and notes the merge changed.
    pub sent: Vec<PathBuf>,
    /// Notes that differ on both sides and wait for a person.
    pub waiting: Vec<PathBuf>,
}

/// The address git uses for what a person typed: GitHub shorthand
/// (`you/notes`, `github.com/you/notes`) becomes an HTTPS address, an
/// address with a scheme or an absolute folder path stays as it is.
pub fn repository_url(typed: &str) -> Option<String> {
    let typed = typed.trim().trim_end_matches('/');
    if typed.is_empty() || typed.chars().any(char::is_whitespace) {
        return None;
    }
    if typed.contains("://") || typed.starts_with('/') {
        return Some(typed.to_owned());
    }
    let path = typed
        .strip_prefix("github.com/")
        .or_else(|| typed.strip_prefix("www.github.com/"))
        .unwrap_or(typed);
    let parts: Vec<&str> = path.split('/').collect();
    let is_owner_and_name = parts.len() == 2 && parts.iter().all(|part| !part.is_empty());
    is_owner_and_name.then(|| format!("https://github.com/{path}"))
}

/// Whether the remote signs in with a token: GitHub over HTTPS does, a
/// folder on this device doesn't.
pub fn url_takes_token(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Whether the address is a folder on this device rather than a server.
pub fn url_is_local(url: &str) -> bool {
    url.starts_with("file://") || url.starts_with('/')
}

/// Why setting up stopped, in words that say what to do next.
pub fn setup_problem(error: &SyncError, url: &str) -> String {
    let refused_token = "GitHub didn’t accept this token. Check that it can read and write the repository’s contents, then paste it again.";
    match error {
        SyncError::Auth(_) => refused_token.to_owned(),
        SyncError::Offline(message) if message.contains("401") || message.contains("403") => {
            refused_token.to_owned()
        }
        SyncError::Offline(_) => {
            format!("Couldn’t reach {url}. Check the address and your connection, then try again.")
        }
        SyncError::PushRejected(_) => {
            "The repository didn’t take the notes. Check that the token can write to it, then try again.".to_owned()
        }
        SyncError::AlreadyAClone => {
            "This vault is already a git repository, so it’s set up on the Sync page.".to_owned()
        }
        other => format!(
            "Setting up stopped. Git said: {}",
            crate::phase::plain_git_message(&other.to_string())
        ),
    }
}

/// Who sync's commits are from: this device, at an address no account
/// owns, rather than the person's own git identity. Commits under their
/// own address would fill their GitHub contribution graph with autosaves.
pub fn sync_author(device: &str) -> Author {
    Author::new(device, crate::vault::SYNC_AUTHOR_EMAIL)
}

/// Makes the vault at `setup.root` a clone of `setup.url` on the configured
/// branch, keeping every note it has. On failure nothing in the folder has
/// changed and it has no `.git`.
pub fn set_up_in_place(setup: &InPlaceSetup<'_>) -> SyncResult<SetupReport> {
    let git_dir = setup.root.join(".git");
    if git_dir.exists() {
        return Err(SyncError::AlreadyAClone);
    }
    let staging = setup.root.join(STAGING_FOLDER);
    remove_folder(&staging)?;
    let built = Builder::start(setup, &staging).and_then(|builder| builder.finish());
    let report = match built {
        Ok(report) => report,
        Err(error) => {
            let _ = remove_folder(&staging);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&staging, &git_dir) {
        let _ = remove_folder(&staging);
        return Err(error.into());
    }
    Ok(report)
}

fn remove_folder(folder: &Path) -> SyncResult<()> {
    match fs::remove_dir_all(folder) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// The clone being built in the staging folder, with the vault as its
/// work tree in memory only.
struct Builder<'a> {
    setup: &'a InPlaceSetup<'a>,
    repo: Repository,
}

/// A note both sides have that differs, and waits for a person.
struct Waiting {
    path: PathBuf,
    this_device: Oid,
    other_device: Oid,
    file: ConflictedFile,
}

/// The branch's new tip and what it took to get there.
struct Outcome {
    head: Option<Oid>,
    remote_tip: Option<Oid>,
    local_tree: Oid,
    waiting: Vec<Waiting>,
}

impl<'a> Builder<'a> {
    fn start(setup: &'a InPlaceSetup<'a>, staging: &Path) -> SyncResult<Builder<'a>> {
        let mut options = RepositoryInitOptions::new();
        options.bare(true).initial_head(&setup.config.branch);
        let repo = Repository::init_opts(staging, &options)?;
        // Only in memory: nothing is written into the vault to say so.
        repo.set_workdir(setup.root, false)?;
        pin_settings(&repo)?;
        setup.config.device_only.write_exclude(repo.path())?;
        repo.remote(&setup.config.remote, setup.url)?;
        Ok(Builder { setup, repo })
    }

    fn config(&self) -> &VaultConfig {
        &self.setup.config
    }

    fn finish(self) -> SyncResult<SetupReport> {
        self.fetch()?;
        let outcome = self.combine()?;
        if let Some(head) = outcome.head {
            self.repo
                .reference(&self.local_ref(), head, true, "sync: set up")?;
            if outcome.remote_tip != Some(head) {
                self.push()?;
            }
            let tracking = format!(
                "refs/remotes/{}/{}",
                self.config().remote,
                self.config().branch
            );
            self.repo.reference(&tracking, head, true, "sync: set up")?;
        }
        let report = self.report(&outcome)?;
        self.write_work_tree(&outcome)?;
        self.save_waiting(&outcome.waiting)?;
        let mut config = self.repo.config()?;
        config.set_bool("core.bare", false)?;
        Ok(report)
    }

    fn local_ref(&self) -> String {
        format!("refs/heads/{}", self.config().branch)
    }

    fn fetch(&self) -> SyncResult<()> {
        let mut remote = self.repo.find_remote(&self.config().remote)?;
        let mut options = FetchOptions::new();
        options
            .remote_callbacks(remote_callbacks(self.setup.token.as_ref()))
            .update_fetchhead(false);
        remote
            .fetch(&[self.refspec()], Some(&mut options), None)
            .map_err(SyncError::from_transport)
    }

    fn tracking_ref(&self) -> String {
        format!(
            "refs/remotes/{}/{}",
            self.config().remote,
            self.config().branch
        )
    }

    fn refspec(&self) -> String {
        format!(
            "+refs/heads/{}:{}",
            self.config().branch,
            self.tracking_ref()
        )
    }

    /// The remote's branch, where the notes start.
    fn remote_tip(&self) -> Option<Oid> {
        self.repo.refname_to_id(&self.tracking_ref()).ok()
    }

    /// Commits the folder's notes and merges them with the remote's.
    fn combine(&self) -> SyncResult<Outcome> {
        let remote_tip = self.remote_tip();
        let local_tree = self.stage_folder()?;
        let local_commit = self.commit_folder(local_tree)?;
        let mut outcome = Outcome {
            head: local_commit.or(remote_tip),
            remote_tip,
            local_tree,
            waiting: Vec::new(),
        };
        let (Some(local), Some(remote)) = (local_commit, remote_tip) else {
            return Ok(outcome);
        };
        let remote_tree = self.repo.find_commit(remote)?.tree()?;
        let (merged_tree, waiting) = self.merge_trees(local_tree, &remote_tree)?;
        outcome.waiting = waiting;
        outcome.head = Some(if merged_tree == remote_tree.id() {
            remote
        } else {
            self.commit_merge(merged_tree, local, remote)?
        });
        Ok(outcome)
    }

    /// Stages every file in the folder that syncs, honouring `.gitignore`
    /// and the device-only list, and returns their tree.
    fn stage_folder(&self) -> SyncResult<Oid> {
        let mut index = self.repo.index()?;
        let device_only = &self.config().device_only;
        let mut skip = |path: &Path, _: &[u8]| -> i32 {
            let skipped = path.starts_with(STAGING_FOLDER) || device_only.matches(path);
            i32::from(skipped)
        };
        index.add_all(["*"], IndexAddOption::DEFAULT, Some(&mut skip))?;
        index.write()?;
        Ok(index.write_tree()?)
    }

    /// This device's notes as a commit with no parent, or none for an
    /// empty folder.
    fn commit_folder(&self, tree: Oid) -> SyncResult<Option<Oid>> {
        let tree = self.repo.find_tree(tree)?;
        if tree.is_empty() {
            return Ok(None);
        }
        let paths: Vec<PathBuf> = tree_files(&tree)?
            .into_iter()
            .map(|file| file.path)
            .collect();
        let message = commit_message(self.setup.device, &paths);
        let signature = self.signature()?;
        let oid = self
            .repo
            .commit(None, &signature, &signature, &message, &tree, &[])?;
        Ok(Some(oid))
    }

    fn signature(&self) -> SyncResult<Signature<'static>> {
        let author = &self.setup.author;
        Ok(Signature::now(&author.name, &author.email)?)
    }

    fn commit_merge(&self, tree: Oid, local: Oid, remote: Oid) -> SyncResult<Oid> {
        let tree = self.repo.find_tree(tree)?;
        let parents = [
            &self.repo.find_commit(local)?,
            &self.repo.find_commit(remote)?,
        ];
        let message = format!("Merge {}/{}", self.config().remote, self.config().branch);
        let signature = self.signature()?;
        Ok(self
            .repo
            .commit(None, &signature, &signature, &message, &tree, &parents)?)
    }

    /// The remote's tree with this device's notes merged into it. A note
    /// that waits for a person keeps the remote's version in the tree.
    fn merge_trees(&self, local: Oid, remote: &Tree<'_>) -> SyncResult<(Oid, Vec<Waiting>)> {
        let mut index = Index::new()?;
        index.read_tree(remote)?;
        let mut waiting = Vec::new();
        for file in tree_files(&self.repo.find_tree(local)?)? {
            let theirs = remote.get_path(&file.path).ok().map(|entry| entry.id());
            match theirs {
                Some(theirs) if theirs == file.id => {}
                Some(theirs) => {
                    if let Some(parked) = self.merge_file(&mut index, &file, theirs)? {
                        waiting.push(parked);
                    }
                }
                None => index.add(&file.entry())?,
            }
        }
        Ok((index.write_tree_to(&self.repo)?, waiting))
    }

    /// Merges a note both sides have: a binary keeps this device's copy,
    /// text merges line by line or waits for a person.
    fn merge_file(
        &self,
        index: &mut Index,
        file: &TreeFile,
        theirs: Oid,
    ) -> SyncResult<Option<Waiting>> {
        let mine = self.repo.find_blob(file.id)?.content().to_vec();
        let other = self.repo.find_blob(theirs)?.content().to_vec();
        if classify(&file.path, &[&mine, &other]) == FileKind::Binary {
            index.add(&file.entry())?;
            return Ok(None);
        }
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
        match merge_unrelated(&text(&mine), &text(&other)) {
            LineMerge::Clean(merged) => {
                let id = self.repo.blob(merged.as_bytes())?;
                index.add(&TreeFile { id, ..file.clone() }.entry())?;
                Ok(None)
            }
            LineMerge::Conflicted(segments) => Ok(Some(Waiting {
                path: file.path.clone(),
                this_device: file.id,
                other_device: theirs,
                file: ConflictedFile {
                    path: file.path.clone(),
                    segments,
                },
            })),
        }
    }

    fn push(&self) -> SyncResult<()> {
        let mut remote = self.repo.find_remote(&self.config().remote)?;
        let refspec = format!("{0}:{0}", self.local_ref());
        let mut rejection = None;
        {
            let mut callbacks = remote_callbacks(self.setup.token.as_ref());
            callbacks.push_update_reference(|refname, status| {
                if let Some(status) = status {
                    rejection = Some(format!("{refname}: {status}"));
                }
                Ok(())
            });
            let mut options = PushOptions::new();
            options.remote_callbacks(callbacks);
            remote
                .push(&[&refspec], Some(&mut options))
                .map_err(SyncError::from_transport)?;
        }
        match rejection {
            Some(message) => Err(SyncError::PushRejected(message)),
            None => Ok(()),
        }
    }

    fn report(&self, outcome: &Outcome) -> SyncResult<SetupReport> {
        let tree_of = |commit: Option<Oid>| -> SyncResult<Option<Tree<'_>>> {
            commit
                .map(|oid| Ok(self.repo.find_commit(oid)?.tree()?))
                .transpose()
        };
        let head = tree_of(outcome.head)?;
        let remote = tree_of(outcome.remote_tip)?;
        let local = self.repo.find_tree(outcome.local_tree)?;
        let waiting: Vec<PathBuf> = outcome.waiting.iter().map(|w| w.path.clone()).collect();
        let mut brought_in = self.differing_files(Some(&local), head.as_ref())?;
        brought_in.retain(|path| !waiting.contains(path));
        Ok(SetupReport {
            remote_was_empty: outcome.remote_tip.is_none(),
            brought_in,
            sent: self.differing_files(remote.as_ref(), head.as_ref())?,
            waiting,
        })
    }

    /// Files whose entry in `new` differs from `old`, leaving out removals,
    /// which setting up never makes.
    fn differing_files(
        &self,
        old: Option<&Tree<'_>>,
        new: Option<&Tree<'_>>,
    ) -> SyncResult<Vec<PathBuf>> {
        let diff = self.repo.diff_tree_to_tree(old, new, None)?;
        Ok(diff
            .deltas()
            .filter(|delta| delta.new_file().exists())
            .filter_map(|delta| delta.new_file().path().map(Path::to_owned))
            .collect())
    }

    /// Writes the new tip's files the folder lacks or has older copies of,
    /// and the notes that wait with both versions between markers. A file
    /// that changed on disk while setting up ran is left as it is now; the
    /// next sync sends it.
    fn write_work_tree(&self, outcome: &Outcome) -> SyncResult<()> {
        let Some(head) = outcome.head else {
            return Ok(());
        };
        let head_tree = self.repo.find_commit(head)?.tree()?;
        let local = self.repo.find_tree(outcome.local_tree)?;
        let unchanged_on_disk = |path: &Path| -> bool {
            let before = local.get_path(path).ok().map(|entry| entry.id());
            let now = Oid::hash_file(ObjectType::Blob, self.setup.root.join(path)).ok();
            before == now
        };
        let paths: Vec<PathBuf> = self
            .differing_files(Some(&local), Some(&head_tree))?
            .into_iter()
            .filter(|path| !outcome.waiting.iter().any(|waiting| waiting.path == *path))
            .filter(|path| unchanged_on_disk(path))
            .collect();
        if !paths.is_empty() {
            let mut checkout = CheckoutBuilder::new();
            checkout.force().disable_pathspec_match(true);
            for path in &paths {
                checkout.path(path);
            }
            self.repo
                .checkout_tree(head_tree.as_object(), Some(&mut checkout))?;
        }
        for waiting in &outcome.waiting {
            if unchanged_on_disk(&waiting.path) {
                let text = waiting.file.marked_text().text;
                fs::write(self.setup.root.join(&waiting.path), text)?;
            }
        }
        let mut index = self.repo.index()?;
        index.read_tree(&head_tree)?;
        index.write()?;
        Ok(())
    }

    /// Records the notes waiting for a person, as a merge that parks them does.
    fn save_waiting(&self, waiting: &[Waiting]) -> SyncResult<()> {
        let mut parked = ParkedConflicts::default();
        for file in waiting {
            parked.insert(ParkedConflict {
                path: file.path.clone(),
                keeps: BranchKeeps::OtherDevice,
                base: None,
                this_device: Some(file.this_device),
                other_device: Some(file.other_device),
            });
        }
        parked.save(self.repo.path())
    }
}

/// A file in a tree: where it is, its blob and its mode.
#[derive(Clone)]
struct TreeFile {
    path: PathBuf,
    id: Oid,
    mode: u32,
}

impl TreeFile {
    fn entry(&self) -> IndexEntry {
        let time = IndexTime::new(0, 0);
        IndexEntry {
            ctime: time,
            mtime: time,
            dev: 0,
            ino: 0,
            mode: self.mode,
            uid: 0,
            gid: 0,
            file_size: 0,
            id: self.id,
            flags: 0,
            flags_extended: 0,
            path: self.path.to_string_lossy().replace('\\', "/").into_bytes(),
        }
    }
}

/// Every file (blob or link) in `tree`, at any depth.
fn tree_files(tree: &Tree<'_>) -> SyncResult<Vec<TreeFile>> {
    let mut files = Vec::new();
    tree.walk(TreeWalkMode::PreOrder, |folder, entry| {
        if entry.kind() == Some(ObjectType::Blob) {
            let name = entry.name().unwrap_or_default();
            files.push(TreeFile {
                path: Path::new(folder).join(name),
                id: entry.id(),
                mode: u32::try_from(entry.filemode()).unwrap_or(0o100_644),
            });
        }
        TreeWalkResult::Ok
    })?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_repositories_become_addresses() {
        let url = |typed: &str| repository_url(typed);
        assert_eq!(
            url("you/notes").as_deref(),
            Some("https://github.com/you/notes")
        );
        assert_eq!(
            url(" github.com/you/notes/ ").as_deref(),
            Some("https://github.com/you/notes")
        );
        assert_eq!(
            url("file:///tmp/notes.git").as_deref(),
            Some("file:///tmp/notes.git")
        );
        assert_eq!(url("/tmp/notes.git").as_deref(), Some("/tmp/notes.git"));
        assert_eq!(url("notes"), None);
        assert_eq!(url("you/my notes"), None);
        assert!(url_takes_token("https://github.com/you/notes"));
        assert!(!url_takes_token("/tmp/notes.git"));
        assert!(url_is_local("/tmp/notes.git"));
        assert!(!url_is_local("https://github.com/you/notes"));
    }
}
