use std::fs;
use std::path::{Component, Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{
    AnnotatedCommit, Commit, DiffOptions, FetchOptions, IndexAddOption, IndexConflict,
    MergeOptions, Oid, PushOptions, Repository, RepositoryInitOptions, Signature,
};

use crate::conflict::ConflictedFile;
use crate::credentials::{CredentialStore, Token, remote_callbacks};
use crate::device_files::DeviceOnlyFiles;
use crate::error::{SyncError, SyncResult};
use crate::line_merge::{LineMerge, merge_lines};
use crate::policy::{FileKind, classify};

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
    /// The merge is paused until these files are resolved.
    Conflicts(Vec<ConflictedFile>),
}

/// A git clone of a vault and the sync operations on it.
pub struct Vault {
    repo: Repository,
    config: VaultConfig,
    token: Option<Token>,
}

/// What the merge policy does with one conflicting path.
enum Settlement {
    Write(Vec<u8>),
    KeepLocalBinary(Vec<u8>),
    Delete,
    NeedsPerson(ConflictedFile),
}

/// The three versions of a conflicting path, read from the index.
struct ConflictVersions {
    path: PathBuf,
    base: Option<Vec<u8>>,
    this_device: Option<Vec<u8>>,
    other_device: Option<Vec<u8>>,
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

/// Notes must sync byte for byte on every device. Without this, a machine
/// whose global git config sets `core.autocrlf` (the default on Windows)
/// would rewrite line endings on checkout and merge.
fn keep_bytes_as_committed(repo: &Repository) -> SyncResult<()> {
    let mut config = repo.config()?;
    config.set_bool("core.autocrlf", false)?;
    config.set_str("core.eol", "lf")?;
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
        keep_bytes_as_committed(&repo)?;
        repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
        Self::from_repo(repo, config, token)
    }

    fn from_repo(repo: Repository, config: VaultConfig, token: Option<Token>) -> SyncResult<Self> {
        if repo.is_bare() {
            return Err(SyncError::Git(git2::Error::from_str(
                "a vault needs a work tree",
            )));
        }
        keep_bytes_as_committed(&repo)?;
        config.device_only.write_exclude(repo.path())?;
        let vault = Self {
            repo,
            config,
            token,
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

    /// Stages every change except device-only files and commits it.
    ///
    /// Returns `None` when there was nothing to commit.
    pub fn commit_all(&self, author: &Author, message: &str) -> SyncResult<Option<Oid>> {
        if self.is_merging() {
            return Err(SyncError::UnresolvedConflicts(self.conflicts()?.len()));
        }
        let mut index = self.repo.index()?;
        let device_only = &self.config.device_only;
        let mut skip_device_only = |path: &Path, _: &[u8]| i32::from(device_only.matches(path));
        index.add_all(["*"], IndexAddOption::DEFAULT, Some(&mut skip_device_only))?;
        index.update_all(["*"], Some(&mut skip_device_only))?;
        index.write()?;
        let tree = self.repo.find_tree(index.write_tree()?)?;
        let parent = self.local_head()?;
        let unchanged = match &parent {
            Some(parent) => parent.tree_id() == tree.id(),
            None => tree.is_empty(),
        };
        if unchanged {
            return Ok(None);
        }
        let signature = author.signature()?;
        let parents: Vec<&Commit> = parent.iter().collect();
        let oid = self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )?;
        Ok(Some(oid))
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
        options.remote_callbacks(remote_callbacks(self.token.as_ref()));
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
        remote
            .fetch(&refspecs, Some(&mut options), None)
            .map_err(SyncError::from_transport)
    }

    /// Pushes the local branch. A rejected push leaves everything local.
    pub fn push(&self) -> SyncResult<()> {
        let Some(head) = self.head_commit()? else {
            return Ok(());
        };
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
        if let Some(message) = rejection {
            return Err(SyncError::PushRejected(message));
        }
        self.repo
            .reference(&self.config.tracking_ref(), head, true, "push")?;
        Ok(())
    }

    /// Merges the fetched remote branch into the local one using the vault merge policy.
    /// Then merges the legacy branch the same way, if one is configured.
    pub fn merge(&self, author: &Author) -> SyncResult<MergeOutcome> {
        if self.is_merging() {
            return Ok(MergeOutcome::Conflicts(self.conflicts()?));
        }
        let own = self.merge_tracking(&self.config.tracking_ref(), author)?;
        let Some(legacy) = self.config.active_legacy() else {
            return Ok(own);
        };
        if matches!(own, MergeOutcome::Conflicts(_)) {
            return Ok(own);
        }
        let legacy = self.merge_tracking(&self.config.tracking_ref_for(legacy), author)?;
        Ok(combine_outcomes(own, legacy))
    }

    fn merge_tracking(&self, tracking_ref: &str, author: &Author) -> SyncResult<MergeOutcome> {
        let Ok(tracking) = self.repo.find_reference(tracking_ref) else {
            return Ok(MergeOutcome::NothingToMerge);
        };
        let theirs = self.repo.reference_to_annotated_commit(&tracking)?;
        let (analysis, _) = self.repo.merge_analysis(&[&theirs])?;
        if analysis.is_up_to_date() {
            return Ok(MergeOutcome::UpToDate);
        }
        if analysis.is_unborn() || analysis.is_fast_forward() {
            self.fast_forward(theirs.id())?;
            return Ok(MergeOutcome::FastForward);
        }
        self.merge_diverged(&theirs, author)
    }

    fn fast_forward(&self, target: Oid) -> SyncResult<()> {
        let commit = self.repo.find_commit(target)?;
        self.repo
            .checkout_tree(commit.as_object(), Some(CheckoutBuilder::new().safe()))?;
        let local_ref = self.config.local_ref();
        self.repo
            .reference(&local_ref, target, true, "sync: fast-forward")?;
        self.repo.set_head(&local_ref)?;
        Ok(())
    }

    fn merge_diverged(
        &self,
        theirs: &AnnotatedCommit,
        author: &Author,
    ) -> SyncResult<MergeOutcome> {
        let mut checkout = CheckoutBuilder::new();
        checkout.safe().allow_conflicts(true);
        self.repo.merge(
            &[theirs],
            Some(&mut MergeOptions::new()),
            Some(&mut checkout),
        )?;
        let (kept_local, needs_person) = self.settle_conflicts()?;
        if !needs_person.is_empty() {
            return Ok(MergeOutcome::Conflicts(needs_person));
        }
        let commit = self.commit_merge(author)?;
        Ok(MergeOutcome::Merged { commit, kept_local })
    }

    /// Applies the merge policy to every index conflict. Returns the binaries
    /// that kept the local copy and the text files that need a person.
    fn settle_conflicts(&self) -> SyncResult<(Vec<PathBuf>, Vec<ConflictedFile>)> {
        let mut index = self.repo.index()?;
        let mut kept_local = Vec::new();
        let mut needs_person = Vec::new();
        for versions in self.conflict_versions(&index)? {
            let path = versions.path.clone();
            match versions.settle() {
                Settlement::Write(bytes) => self.stage_bytes(&mut index, &path, &bytes)?,
                Settlement::KeepLocalBinary(bytes) => {
                    self.stage_bytes(&mut index, &path, &bytes)?;
                    kept_local.push(path);
                }
                Settlement::Delete => self.stage_deletion(&mut index, &path)?,
                Settlement::NeedsPerson(file) => {
                    fs::write(self.root().join(&path), file.marked_text().text)?;
                    needs_person.push(file);
                }
            }
        }
        index.write()?;
        Ok((kept_local, needs_person))
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
        let read = |entry: &Option<git2::IndexEntry>| -> SyncResult<Option<Vec<u8>>> {
            let Some(entry) = entry else { return Ok(None) };
            Ok(Some(self.repo.find_blob(entry.id)?.content().to_vec()))
        };
        Ok(ConflictVersions {
            path,
            base: read(&conflict.ancestor)?,
            this_device: read(&conflict.our)?,
            other_device: read(&conflict.their)?,
        })
    }

    fn stage_bytes(&self, index: &mut git2::Index, path: &Path, bytes: &[u8]) -> SyncResult<()> {
        let full = self.root().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(full, bytes)?;
        index.add_path(path)?;
        Ok(())
    }

    fn stage_deletion(&self, index: &mut git2::Index, path: &Path) -> SyncResult<()> {
        match fs::remove_file(self.root().join(path)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        index.remove_path(path)?;
        Ok(())
    }

    /// The text conflicts of the paused merge, recomputed from the index.
    pub fn conflicts(&self) -> SyncResult<Vec<ConflictedFile>> {
        let index = self.repo.index()?;
        let files = self
            .conflict_versions(&index)?
            .into_iter()
            .filter_map(|versions| match versions.settle() {
                Settlement::NeedsPerson(file) => Some(file),
                _ => None,
            })
            .collect();
        Ok(files)
    }

    /// Applies one resolution per hunk to `file`, then stages it.
    ///
    /// Returns the merge commit once the last conflicted file is resolved.
    pub fn resolve(
        &self,
        file: &ConflictedFile,
        resolutions: &[crate::conflict::Resolution],
        author: &Author,
    ) -> SyncResult<Option<Oid>> {
        let text = file.resolve(resolutions)?;
        self.resolve_with_text(&file.path, &text, author)
    }

    /// Replaces a conflicted file with `text` the person settled on, then stages it.
    pub fn resolve_with_text(
        &self,
        path: &Path,
        text: &str,
        author: &Author,
    ) -> SyncResult<Option<Oid>> {
        if !is_inside_vault(path) {
            return Err(SyncError::OutsideVault(path.to_owned()));
        }
        let mut index = self.repo.index()?;
        self.stage_bytes(&mut index, path, text.as_bytes())?;
        index.write()?;
        if index.has_conflicts() {
            return Ok(None);
        }
        Ok(Some(self.commit_merge(author)?))
    }

    fn commit_merge(&self, author: &Author) -> SyncResult<Oid> {
        let mut index = self.repo.index()?;
        if index.has_conflicts() {
            return Err(SyncError::UnresolvedConflicts(self.conflicts()?.len()));
        }
        let tree = self.repo.find_tree(index.write_tree()?)?;
        let mut parents = vec![self.repo.head()?.peel_to_commit()?];
        for oid in self.merge_heads()? {
            parents.push(self.repo.find_commit(oid)?);
        }
        let signature = author.signature()?;
        let message = format!("Merge {}/{}", self.config.remote, self.config.branch);
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

    /// How many files differ from the last known remote state, committed or not.
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
        let count = diff
            .deltas()
            .filter_map(|delta| {
                delta
                    .new_file()
                    .path()
                    .or(delta.old_file().path())
                    .map(Path::to_owned)
            })
            .filter(|path| !device_only.matches(path))
            .count();
        Ok(count)
    }
}

impl ConflictVersions {
    fn settle(self) -> Settlement {
        match (self.this_device, self.other_device) {
            (None, None) => Settlement::Delete,
            (Some(kept), None) | (None, Some(kept)) => Settlement::Write(kept),
            (Some(this_device), Some(other_device)) => {
                settle_both(self.path, self.base, this_device, other_device)
            }
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
        let diff = self
            .repo
            .diff_tree_to_tree(old.as_ref(), new.as_ref(), None)?;
        let device_only = &self.config.device_only;
        let paths = diff
            .deltas()
            .filter_map(|delta| delta.new_file().path().or(delta.old_file().path()))
            .filter(|path| !device_only.matches(path))
            .map(Path::to_owned)
            .collect();
        Ok(paths)
    }
}

fn is_inside_vault(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        && path.components().next().is_some()
}
