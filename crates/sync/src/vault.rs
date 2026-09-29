use std::cell::Cell;
use std::fs;
use std::path::{Component, Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{
    AnnotatedCommit, Commit, ConfigLevel, DiffOptions, FetchOptions, IndexConflict, MergeOptions,
    Oid, PushOptions, Repository, RepositoryInitOptions, Signature,
};

use crate::conflict::ConflictedFile;
use crate::credentials::{CredentialStore, Token, remote_callbacks};
use crate::device_files::DeviceOnlyFiles;
use crate::error::{SyncError, SyncResult};
use crate::line_merge::{LineMerge, merge_lines};
use crate::message::commit_message;
use crate::parked::{BranchKeeps, ParkedConflicts};
use crate::policy::{FileKind, classify};

mod parking;

use parking::Parking;

/// The address commits carry when git has no author configured.
pub const FALLBACK_AUTHOR_EMAIL: &str = concat!(gasp_config::command_name!(), "@localhost");

/// Who commits on this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

impl Author {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
        }
    }

    fn signature(&self) -> SyncResult<Signature<'static>> {
        Ok(Signature::now(&self.name, &self.email)?)
    }
}

/// Which remote and branch a vault syncs, and what stays on this device.
#[derive(Debug, Clone)]
pub struct VaultConfig {
    pub remote: String,
    /// The branch the app commits to and pushes.
    pub branch: String,
    /// A branch older sync tools still push to. While it is set, every
    /// merge also pulls it into `branch`, one way, so nothing they push is
    /// lost; the app never pushes to it. A clone of a remote that has only
    /// this branch starts `branch` from its tip.
    pub legacy_branch: Option<String>,
    pub device_only: DeviceOnlyFiles,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            remote: "origin".to_owned(),
            branch: "master".to_owned(),
            legacy_branch: Some("main".to_owned()),
            device_only: DeviceOnlyFiles::default(),
        }
    }
}

impl VaultConfig {
    fn local_ref(&self) -> String {
        format!("refs/heads/{}", self.branch)
    }

    fn tracking_ref(&self) -> String {
        self.tracking_ref_for(&self.branch)
    }

    fn tracking_ref_for(&self, branch: &str) -> String {
        format!("refs/remotes/{}/{branch}", self.remote)
    }

    /// The legacy branch, unless it is the branch the app syncs.
    fn active_legacy(&self) -> Option<&str> {
        self.legacy_branch
            .as_deref()
            .filter(|legacy| *legacy != self.branch)
    }
}

/// What a merge of the remote branch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// The remote has no such branch yet.
    NothingToMerge,
    UpToDate,
    FastForward,
    /// A merge commit was made. `kept_local` lists binaries where this device's copy won.
    Merged {
        commit: Oid,
        kept_local: Vec<PathBuf>,
    },
    /// A merge commit was made, but some files need a person: these are
    /// every file now waiting. Each one keeps a single version in the
    /// branch, its version on the synced branch, and shows both
    /// between conflict markers on disk until it's resolved. Every other
    /// file merged and syncs as usual.
    Conflicts(Vec<ConflictedFile>),
}

/// A git clone of a vault and the sync operations on it.
pub struct Vault {
    repo: Repository,
    config: VaultConfig,
    token: Option<Token>,
    /// Where the remote's branch was when the last fetch looked, until a
    /// push uses it: a push of that very commit has nothing to send.
    fetched_remote_tip: Cell<Option<Oid>>,
}

/// What the merge policy does with one conflicting path.
enum Settlement {
    Write(Vec<u8>),
    KeepLocalBinary(Vec<u8>),
    Delete,
    NeedsPerson(ConflictedFile),
}

/// The three versions of a conflicting path, read from the index or from
/// a parked conflict.
struct ConflictVersions {
    path: PathBuf,
    base: Option<Version>,
    this_device: Option<Version>,
    other_device: Option<Version>,
}

/// One version of a file: its blob and what's in it.
#[derive(Clone)]
struct Version {
    id: Oid,
    bytes: Vec<u8>,
}

/// What the merge policy is settling: conflicts of a merge that just ran,
/// or of one an earlier version of the app left paused.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MergeRun {
    Fresh,
    /// The files needing a person were already written with markers and
    /// may have been edited since; one with no markers left was settled by hand.
    Interrupted,
}

/// What settling a merge's conflicts did.
struct Settled {
    kept_local: Vec<PathBuf>,
    parked: usize,
}

/// Clones only `branch`, without checking anything out yet.
fn clone_branch(
    url: &str,
    path: &Path,
    branch: &str,
    token: Option<&Token>,
) -> SyncResult<Repository> {
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(remote_callbacks(token));
    // Check out only after the line-ending settings are pinned, so the
    // first checkout already writes files byte for byte.
    let mut no_checkout = CheckoutBuilder::new();
    no_checkout.dry_run();
    RepoBuilder::new()
        .branch(branch)
        .fetch_options(fetch_options)
        .with_checkout(no_checkout)
        .clone(url, path)
        .map_err(SyncError::from_transport)
}

/// Creates `branch` at the current commit and switches HEAD to it.
fn start_branch_at_head(repo: &Repository, branch: &str) -> SyncResult<()> {
    let head = repo.head()?.peel_to_commit()?;
    repo.branch(branch, &head, false)?;
    repo.set_head(&format!("refs/heads/{branch}"))?;
    Ok(())
}

/// The result of merging the synced branch and then the legacy one: a
/// conflict wins, then whichever merge actually changed something.
fn combine_outcomes(own: MergeOutcome, legacy: MergeOutcome) -> MergeOutcome {
    let changed = |outcome: &MergeOutcome| {
        !matches!(
            outcome,
            MergeOutcome::UpToDate | MergeOutcome::NothingToMerge
        )
    };
    if matches!(legacy, MergeOutcome::Conflicts(_)) || !changed(&own) && changed(&legacy) {
        return legacy;
    }
    own
}

/// The largest object a push tries to store as a delta against another.
/// Photos and plugin binaries above it are sent whole: a delta between two
/// different photos never pays off, and looking for one took most of the
/// time a push of new photos spent.
///
/// libgit2 reads its big-file threshold from `pack.deltaCacheSize` (the
/// key is misspelt in its source), which also caps the delta cache; deltas
/// between note versions are small and stay cached under this cap.
const LARGEST_DELTA_CANDIDATE: i64 = 512 * 1024;

/// Pins the clone's own settings sync depends on, writing each only when
/// it differs, so opening a vault normally leaves its config alone.
///
/// Notes must sync byte for byte on every device. Without the line-ending
/// settings, a machine whose global git config sets `core.autocrlf` (the
/// default on Windows) would rewrite line endings on checkout and merge.
fn pin_settings(repo: &Repository) -> SyncResult<()> {
    let mut local = repo.config()?.open_level(ConfigLevel::Local)?;
    if local.get_bool("core.autocrlf").ok() != Some(false) {
        local.set_bool("core.autocrlf", false)?;
    }
    if local.get_string("core.eol").ok().as_deref() != Some("lf") {
        local.set_str("core.eol", "lf")?;
    }
    if local.get_i64("pack.deltaCacheSize").ok() != Some(LARGEST_DELTA_CANDIDATE) {
        local.set_i64("pack.deltaCacheSize", LARGEST_DELTA_CANDIDATE)?;
    }
    Ok(())
}

impl Vault {
    /// Opens an existing clone whose HEAD is on the configured branch.
    pub fn open(path: impl AsRef<Path>, config: VaultConfig) -> SyncResult<Self> {
        let repo = Repository::open(path)?;
        Self::from_repo(repo, config, None)
    }

    /// Starts a new vault repo at `path` that will sync with `remote_url`.
    pub fn init(path: impl AsRef<Path>, remote_url: &str, config: VaultConfig) -> SyncResult<Self> {
        let mut options = RepositoryInitOptions::new();
        options.initial_head(&config.branch);
        let repo = Repository::init_opts(path, &options)?;
        repo.remote(&config.remote, remote_url)?;
        Self::from_repo(repo, config, None)
    }

    /// Clones `url` into `path`, checking out the configured branch.
    pub fn clone_remote(
        url: &str,
        path: impl AsRef<Path>,
        config: VaultConfig,
        token: Option<Token>,
    ) -> SyncResult<Self> {
        let path = path.as_ref();
        let repo = match clone_branch(url, path, &config.branch, token.as_ref()) {
            Ok(repo) => repo,
            Err(error) => {
                let Some(legacy) = config.active_legacy() else {
                    return Err(error);
                };
                let repo = clone_branch(url, path, legacy, token.as_ref())?;
                start_branch_at_head(&repo, &config.branch)?;
                repo
            }
        };
        pin_settings(&repo)?;
        repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
        Self::from_repo(repo, config, token)
    }

    fn from_repo(repo: Repository, config: VaultConfig, token: Option<Token>) -> SyncResult<Self> {
        if repo.is_bare() {
            return Err(SyncError::Git(git2::Error::from_str(
                "a vault needs a work tree",
            )));
        }
        pin_settings(&repo)?;
        config.device_only.write_exclude(repo.path())?;
        let vault = Self {
            repo,
            config,
            token,
            fetched_remote_tip: Cell::new(None),
        };
        vault.check_branch()?;
        Ok(vault)
    }

    fn check_branch(&self) -> SyncResult<()> {
        let head = self.repo.find_reference("HEAD")?;
        let actual = head.symbolic_target().unwrap_or("(detached)").to_owned();
        if actual == self.config.local_ref() {
            return Ok(());
        }
        Err(SyncError::WrongBranch {
            expected: self.config.branch.clone(),
            actual,
        })
    }

    pub fn set_token(&mut self, token: Option<Token>) {
        self.token = token;
    }

    /// Loads the token for this vault's remote from `store`.
    pub fn load_token(&mut self, store: &dyn CredentialStore) -> SyncResult<()> {
        let url = self.remote_url()?;
        self.token = store.load(&url)?;
        Ok(())
    }

    pub fn remote_url(&self) -> SyncResult<String> {
        let remote = self.repo.find_remote(&self.config.remote)?;
        Ok(remote.url().unwrap_or_default().to_owned())
    }

    /// Points the remote somewhere else (for a moved repo, or a test that goes offline).
    pub fn set_remote_url(&self, url: &str) -> SyncResult<()> {
        self.fetched_remote_tip.set(None);
        Ok(self.repo.remote_set_url(&self.config.remote, url)?)
    }

    pub fn root(&self) -> &Path {
        self.repo.workdir().expect("checked in from_repo")
    }

    pub fn config(&self) -> &VaultConfig {
        &self.config
    }

    /// The commit the local branch points to, if any.
    pub fn head_commit(&self) -> SyncResult<Option<Oid>> {
        Ok(self.repo.refname_to_id(&self.config.local_ref()).ok())
    }

    /// True while a merge is waiting for conflicts to be resolved.
    pub fn is_merging(&self) -> bool {
        self.repo.state() == git2::RepositoryState::Merge
    }

    /// Stages every change except device-only files and files waiting on a
    /// conflict, and commits it with `message`.
    ///
    /// Returns `None` when there was nothing to commit.
    pub fn commit_all(&self, author: &Author, message: &str) -> SyncResult<Option<Oid>> {
        self.commit_with(author, |_| message.to_owned())
    }

    /// Like [`Vault::commit_all`], with a message naming `device` and the
    /// files that changed, such as `mac: Lemma.md, Habit Ideas.md`.
    pub fn commit_changes(&self, author: &Author, device: &str) -> SyncResult<Option<Oid>> {
        self.commit_with(author, |paths| commit_message(device, paths))
    }

    fn commit_with(
        &self,
        author: &Author,
        message: impl FnOnce(&[PathBuf]) -> String,
    ) -> SyncResult<Option<Oid>> {
        self.finish_interrupted_merge(author)?;
        let tree_id = self.staged_tree()?;
        let parent = self.local_head()?;
        if parent.as_ref().map(Commit::tree_id) == Some(tree_id) {
            return Ok(None);
        }
        let tree = self.repo.find_tree(tree_id)?;
        let parent_tree = parent.as_ref().map(Commit::tree).transpose()?;
        let changed = self.paths_between(parent_tree.as_ref(), Some(&tree))?;
        if changed.is_empty() {
            return Ok(None);
        }
        let signature = author.signature()?;
        let parents: Vec<&Commit> = parent.iter().collect();
        let oid = self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &message(&changed),
            &tree,
            &parents,
        )?;
        Ok(Some(oid))
    }

    /// Stages what changed and returns the tree the next commit would have.
    /// The index is only saved when something was staged.
    fn staged_tree(&self) -> SyncResult<Oid> {
        let parked = self.release_settled_conflicts()?;
        let mut index = self.current_index()?;
        let staged = self.stage_work_tree(&mut index, &parked)?;
        let tree_id = index.write_tree()?;
        if staged {
            index.write()?;
        }
        Ok(tree_id)
    }

    /// Stages every file that changed on disk since the index last saw it,
    /// except device-only files and files waiting on a conflict, in one
    /// scan of the work tree. Returns whether anything was staged.
    ///
    /// The scan also refreshes the index's record of files that were
    /// touched without changing, so later scans needn't hash them again;
    /// libgit2 saves the index itself when it does.
    fn stage_work_tree(
        &self,
        index: &mut git2::Index,
        parked: &ParkedConflicts,
    ) -> SyncResult<bool> {
        let mut options = DiffOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_typechange(true)
            .update_index(true);
        let diff = self
            .repo
            .diff_index_to_workdir(Some(&*index), Some(&mut options))?;
        let device_only = &self.config.device_only;
        let changes: Vec<(PathBuf, bool)> = diff
            .deltas()
            .filter_map(|delta| {
                let path = delta.old_file().path()?;
                let skipped = device_only.matches(path) || parked.contains(path);
                (!skipped).then(|| (path.to_owned(), delta.new_file().exists()))
            })
            .collect();
        drop(diff);
        for (path, on_disk) in &changes {
            if *on_disk {
                index.add_path(path)?;
            } else {
                index.remove_path(path)?;
            }
        }
        Ok(!changes.is_empty())
    }

    /// The index as it is on disk, in case another git program changed it
    /// since this clone last read it.
    fn current_index(&self) -> SyncResult<git2::Index> {
        let mut index = self.repo.index()?;
        index.read(false)?;
        Ok(index)
    }

    fn local_head(&self) -> SyncResult<Option<Commit<'_>>> {
        let Some(oid) = self.head_commit()? else {
            return Ok(None);
        };
        Ok(Some(self.repo.find_commit(oid)?))
    }

    /// Fetches the configured branch into its remote-tracking ref.
    pub fn fetch(&self) -> SyncResult<()> {
        let mut remote = self.repo.find_remote(&self.config.remote)?;
        let mut options = FetchOptions::new();
        options
            .remote_callbacks(remote_callbacks(self.token.as_ref()))
            .update_fetchhead(false);
        let branches =
            std::iter::once(self.config.branch.as_str()).chain(self.config.active_legacy());
        let refspecs: Vec<String> = branches
            .map(|branch| {
                format!(
                    "+refs/heads/{branch}:{}",
                    self.config.tracking_ref_for(branch)
                )
            })
            .collect();
        self.fetched_remote_tip.set(None);
        remote
            .fetch(&refspecs, Some(&mut options), None)
            .map_err(SyncError::from_transport)?;
        let local_ref = self.config.local_ref();
        let advertised = remote.list()?.iter().find(|head| head.name() == local_ref);
        self.fetched_remote_tip
            .set(advertised.map(|head| head.oid()));
        Ok(())
    }

    /// Pushes the local branch. A rejected push leaves everything local.
    ///
    /// Right after a fetch that found the remote's branch already at this
    /// commit, there's nothing to send, so the remote isn't asked again.
    pub fn push(&self) -> SyncResult<()> {
        let Some(head) = self.head_commit()? else {
            return Ok(());
        };
        if self.fetched_remote_tip.take() != Some(head) {
            self.send()?;
        }
        if self.tracking_commit() != Some(head) {
            self.repo
                .reference(&self.config.tracking_ref(), head, true, "push")?;
        }
        Ok(())
    }

    fn send(&self) -> SyncResult<()> {
        let mut remote = self.repo.find_remote(&self.config.remote)?;
        let refspec = format!("{0}:{0}", self.config.local_ref());
        let mut rejection = None;
        {
            let mut callbacks = remote_callbacks(self.token.as_ref());
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

    /// Merges the fetched remote branch into the local one using the vault
    /// merge policy, then the legacy branch the same way, if one is set.
    ///
    /// A merge always finishes with a commit. Files that need a person keep
    /// their version on the synced branch and wait, with both
    /// versions on disk, while everything else syncs.
    pub fn merge(&self, author: &Author) -> SyncResult<MergeOutcome> {
        self.finish_interrupted_merge(author)?;
        let own = self.merge_tracking(&self.config.branch, BranchKeeps::OtherDevice, author)?;
        let Some(legacy) = self.config.active_legacy() else {
            return Ok(own);
        };
        let legacy = self.merge_tracking(legacy, BranchKeeps::ThisDevice, author)?;
        Ok(combine_outcomes(own, legacy))
    }

    fn merge_tracking(
        &self,
        branch: &str,
        keeps: BranchKeeps,
        author: &Author,
    ) -> SyncResult<MergeOutcome> {
        let Ok(tracking) = self
            .repo
            .find_reference(&self.config.tracking_ref_for(branch))
        else {
            return Ok(MergeOutcome::NothingToMerge);
        };
        let theirs = self.repo.reference_to_annotated_commit(&tracking)?;
        let (analysis, _) = self.repo.merge_analysis(&[&theirs])?;
        if analysis.is_up_to_date() {
            return Ok(MergeOutcome::UpToDate);
        }
        let lifted = self.lift_parked(theirs.id())?;
        let merged = if analysis.is_unborn() || analysis.is_fast_forward() {
            self.fast_forward(theirs.id())
                .map(|()| MergeOutcome::FastForward)
        } else {
            self.merge_diverged(&theirs, branch, keeps, author)
        };
        // Parked files go back on disk whether or not the merge worked.
        self.refold(lifted)?;
        merged
    }

    /// Moves the branch to `target`, writing only the files that differ
    /// between the two commits rather than checking the whole work tree.
    fn fast_forward(&self, target: Oid) -> SyncResult<()> {
        let commit = self.repo.find_commit(target)?;
        let mut checkout = CheckoutBuilder::new();
        checkout.safe();
        let differing = match self.local_head()? {
            Some(head) => Some(self.differing_paths(&head.tree()?, &commit.tree()?)?),
            None => None,
        };
        if let Some(paths) = &differing {
            checkout.disable_pathspec_match(true);
            for path in paths {
                checkout.path(path);
            }
        }
        if differing.is_none_or(|paths| !paths.is_empty()) {
            self.repo
                .checkout_tree(commit.as_object(), Some(&mut checkout))?;
        }
        let local_ref = self.config.local_ref();
        self.repo
            .reference(&local_ref, target, true, "sync: fast-forward")?;
        self.repo.set_head(&local_ref)?;
        Ok(())
    }

    fn merge_diverged(
        &self,
        theirs: &AnnotatedCommit,
        branch: &str,
        keeps: BranchKeeps,
        author: &Author,
    ) -> SyncResult<MergeOutcome> {
        let mut checkout = CheckoutBuilder::new();
        checkout.safe().allow_conflicts(true);
        self.repo.merge(
            &[theirs],
            Some(&mut MergeOptions::new()),
            Some(&mut checkout),
        )?;
        let settled = self.settle_conflicts(keeps, MergeRun::Fresh)?;
        let commit = self.commit_merge(author, branch)?;
        if settled.parked > 0 {
            return Ok(MergeOutcome::Conflicts(self.conflicts()?));
        }
        Ok(MergeOutcome::Merged {
            commit,
            kept_local: settled.kept_local,
        })
    }

    /// Finishes a merge an earlier version of the app left paused on
    /// conflicts, parking the files it was waiting on.
    fn finish_interrupted_merge(&self, author: &Author) -> SyncResult<()> {
        if !self.is_merging() {
            return Ok(());
        }
        let merging = self.merge_heads()?.first().copied();
        let legacy = self.config.active_legacy().filter(|legacy| {
            let tracking = self.config.tracking_ref_for(legacy);
            merging.is_some() && self.repo.refname_to_id(&tracking).ok() == merging
        });
        let (branch, keeps) = match legacy {
            Some(legacy) => (legacy.to_owned(), BranchKeeps::ThisDevice),
            None => (self.config.branch.clone(), BranchKeeps::OtherDevice),
        };
        self.settle_conflicts(keeps, MergeRun::Interrupted)?;
        self.commit_merge(author, &branch)?;
        Ok(())
    }

    /// Applies the merge policy to every index conflict, parking the text
    /// files that need a person.
    fn settle_conflicts(&self, keeps: BranchKeeps, run: MergeRun) -> SyncResult<Settled> {
        let mut index = self.current_index()?;
        let mut parked = self.parked()?;
        let mut settled = Settled {
            kept_local: Vec::new(),
            parked: 0,
        };
        for versions in self.conflict_versions(&index)? {
            let path = versions.path.clone();
            match versions.settle() {
                Settlement::Write(bytes) => self.stage_bytes(&mut index, &path, &bytes)?,
                Settlement::KeepLocalBinary(bytes) => {
                    self.stage_bytes(&mut index, &path, &bytes)?;
                    settled.kept_local.push(path);
                }
                Settlement::Delete => self.stage_deletion(&mut index, &path)?,
                Settlement::NeedsPerson(file) => {
                    let parking = Parking {
                        versions: &versions,
                        file: &file,
                        keeps,
                        run,
                    };
                    let waits = self.settle_by_person(&mut index, &mut parked, parking)?;
                    settled.parked += usize::from(waits);
                }
            }
        }
        index.write()?;
        self.save_parked(&parked)?;
        Ok(settled)
    }

    fn conflict_versions(&self, index: &git2::Index) -> SyncResult<Vec<ConflictVersions>> {
        let mut all = Vec::new();
        for conflict in index.conflicts()? {
            all.push(self.read_conflict(&conflict?)?);
        }
        Ok(all)
    }

    fn read_conflict(&self, conflict: &IndexConflict) -> SyncResult<ConflictVersions> {
        let entries = [&conflict.ancestor, &conflict.our, &conflict.their];
        let path = entries
            .iter()
            .find_map(|entry| entry.as_ref())
            .map(|entry| PathBuf::from(String::from_utf8_lossy(&entry.path).into_owned()))
            .unwrap_or_default();
        let read = |entry: &Option<git2::IndexEntry>| -> SyncResult<Option<Version>> {
            let Some(entry) = entry else { return Ok(None) };
            self.read_version(entry.id).map(Some)
        };
        Ok(ConflictVersions {
            path,
            base: read(&conflict.ancestor)?,
            this_device: read(&conflict.our)?,
            other_device: read(&conflict.their)?,
        })
    }

    fn read_version(&self, id: Oid) -> SyncResult<Version> {
        let bytes = self.repo.find_blob(id)?.content().to_vec();
        Ok(Version { id, bytes })
    }

    fn stage_bytes(&self, index: &mut git2::Index, path: &Path, bytes: &[u8]) -> SyncResult<()> {
        self.write_file(path, bytes)?;
        index.add_path(path)?;
        Ok(())
    }

    fn write_file(&self, path: &Path, bytes: &[u8]) -> SyncResult<()> {
        let full = self.root().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(full, bytes)?;
        Ok(())
    }

    fn stage_deletion(&self, index: &mut git2::Index, path: &Path) -> SyncResult<()> {
        self.remove_file(path)?;
        index.remove_path(path)?;
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> SyncResult<()> {
        match fs::remove_file(self.root().join(path)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    /// Commits the merge in progress, naming the branch it brought in.
    fn commit_merge(&self, author: &Author, branch: &str) -> SyncResult<Oid> {
        let mut index = self.current_index()?;
        if index.has_conflicts() {
            return Err(SyncError::UnresolvedConflicts(index.conflicts()?.count()));
        }
        let tree = self.repo.find_tree(index.write_tree()?)?;
        let mut parents = vec![self.repo.head()?.peel_to_commit()?];
        for oid in self.merge_heads()? {
            parents.push(self.repo.find_commit(oid)?);
        }
        let signature = author.signature()?;
        let message = format!("Merge {}/{branch}", self.config.remote);
        let parent_refs: Vec<&Commit> = parents.iter().collect();
        let oid = self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &message,
            &tree,
            &parent_refs,
        )?;
        self.repo.cleanup_state()?;
        Ok(oid)
    }

    /// The commits being merged in, from `MERGE_HEAD`.
    fn merge_heads(&self) -> SyncResult<Vec<Oid>> {
        let text = fs::read_to_string(self.repo.path().join("MERGE_HEAD"))?;
        let mut oids = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            oids.push(Oid::from_str(line.trim())?);
        }
        Ok(oids)
    }

    /// How many files differ from the last known remote state, committed or
    /// not. Files waiting on a conflict don't count: they wait for a person.
    pub fn unpushed_changes(&self) -> SyncResult<usize> {
        let remote_tree = match self.repo.find_reference(&self.config.tracking_ref()) {
            Ok(reference) => Some(reference.peel_to_tree()?),
            Err(_) => None,
        };
        let mut options = DiffOptions::new();
        options.include_untracked(true).recurse_untracked_dirs(true);
        let diff = self
            .repo
            .diff_tree_to_workdir_with_index(remote_tree.as_ref(), Some(&mut options))?;
        let device_only = &self.config.device_only;
        let parked = self.parked()?;
        let count = diff
            .deltas()
            .filter_map(|delta| {
                delta
                    .new_file()
                    .path()
                    .or(delta.old_file().path())
                    .map(Path::to_owned)
            })
            .filter(|path| !device_only.matches(path) && !parked.contains(path))
            .count();
        Ok(count)
    }
}

impl ConflictVersions {
    fn settle(&self) -> Settlement {
        let bytes =
            |version: &Option<Version>| version.as_ref().map(|version| version.bytes.clone());
        match (bytes(&self.this_device), bytes(&self.other_device)) {
            (None, None) => Settlement::Delete,
            (Some(kept), None) | (None, Some(kept)) => Settlement::Write(kept),
            (Some(this_device), Some(other_device)) => settle_both(
                self.path.clone(),
                bytes(&self.base),
                this_device,
                other_device,
            ),
        }
    }
}

/// Both devices changed the file: binaries keep the local copy, text merges by line.
fn settle_both(
    path: PathBuf,
    base: Option<Vec<u8>>,
    this_device: Vec<u8>,
    other_device: Vec<u8>,
) -> Settlement {
    let base = base.unwrap_or_default();
    if classify(&path, &[&base, &this_device, &other_device]) == FileKind::Binary {
        return Settlement::KeepLocalBinary(this_device);
    }
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    match merge_lines(&text(&base), &text(&this_device), &text(&other_device)) {
        LineMerge::Clean(merged) => Settlement::Write(merged.into_bytes()),
        LineMerge::Conflicted(segments) => {
            Settlement::NeedsPerson(ConflictedFile { path, segments })
        }
    }
}

/// What a folder's git clone looks like, read without changing anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoProbe {
    /// The branch HEAD is on, or `None` when it's detached.
    pub branch: Option<String>,
    /// The URL of the remote asked about, if it exists.
    pub remote_url: Option<String>,
    /// `user.name` and `user.email` from git's config, when both are set.
    pub author: Option<Author>,
}

/// Looks at the clone at `path` (not its parents) without writing to it.
/// Returns `None` when `path` isn't the root of a git work tree.
///
/// [`Vault::open`] pins line endings and writes the device-only excludes,
/// so this is how a caller checks a folder before letting sync touch it.
pub fn probe(path: &Path, remote: &str) -> Option<RepoProbe> {
    let repo = Repository::open(path).ok()?;
    if repo.is_bare() {
        return None;
    }
    let head = repo.find_reference("HEAD").ok()?;
    let branch = head
        .symbolic_target()
        .and_then(|target| target.strip_prefix("refs/heads/"))
        .map(str::to_owned);
    let remote_url = repo
        .find_remote(remote)
        .ok()
        .and_then(|remote| remote.url().map(str::to_owned))
        .filter(|url| !url.is_empty());
    let author = repo.config().ok().and_then(|config| {
        let name = config.get_string("user.name").ok()?;
        let email = config.get_string("user.email").ok()?;
        Some(Author::new(name, email))
    });
    Some(RepoProbe {
        branch,
        remote_url,
        author,
    })
}

/// Points `remote` of the clone at `path` to `url`, adding the remote when
/// it doesn't exist yet.
pub fn set_remote_url(path: &Path, remote: &str, url: &str) -> SyncResult<()> {
    let repo = Repository::open(path)?;
    if repo.find_remote(remote).is_ok() {
        repo.remote_set_url(remote, url)?;
    } else {
        repo.remote(remote, url)?;
    }
    Ok(())
}

impl Vault {
    /// The commit the remote-tracking branch points to: the remote as of
    /// the last fetch or push.
    pub fn tracking_commit(&self) -> Option<Oid> {
        self.repo.refname_to_id(&self.config.tracking_ref()).ok()
    }

    /// Paths that differ between two commits, where `None` is the empty
    /// tree. Device-only files are left out.
    pub fn changed_paths(&self, from: Option<Oid>, to: Option<Oid>) -> SyncResult<Vec<PathBuf>> {
        if from == to {
            return Ok(Vec::new());
        }
        let tree = |oid: Option<Oid>| -> SyncResult<Option<git2::Tree<'_>>> {
            let Some(oid) = oid else { return Ok(None) };
            Ok(Some(self.repo.find_commit(oid)?.tree()?))
        };
        let (old, new) = (tree(from)?, tree(to)?);
        self.paths_between(old.as_ref(), new.as_ref())
    }

    /// Paths that differ between two trees, leaving out device-only files.
    fn paths_between(
        &self,
        old: Option<&git2::Tree<'_>>,
        new: Option<&git2::Tree<'_>>,
    ) -> SyncResult<Vec<PathBuf>> {
        let diff = self.repo.diff_tree_to_tree(old, new, None)?;
        let device_only = &self.config.device_only;
        let paths = diff
            .deltas()
            .filter_map(|delta| delta.new_file().path().or(delta.old_file().path()))
            .filter(|path| !device_only.matches(path))
            .map(Path::to_owned)
            .collect();
        Ok(paths)
    }

    /// Every path whose entry differs between two trees, on either side.
    fn differing_paths(
        &self,
        old: &git2::Tree<'_>,
        new: &git2::Tree<'_>,
    ) -> SyncResult<Vec<PathBuf>> {
        let mut options = DiffOptions::new();
        options.include_typechange(true);
        let diff = self
            .repo
            .diff_tree_to_tree(Some(old), Some(new), Some(&mut options))?;
        let mut paths = Vec::new();
        for delta in diff.deltas() {
            let (old_path, new_path) = (delta.old_file().path(), delta.new_file().path());
            paths.extend(old_path.map(Path::to_owned));
            if new_path != old_path {
                paths.extend(new_path.map(Path::to_owned));
            }
        }
        Ok(paths)
    }
}

fn is_inside_vault(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        && path.components().next().is_some()
}
