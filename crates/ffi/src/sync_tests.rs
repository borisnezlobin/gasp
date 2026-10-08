//! The phone's sync against local bare repositories: setting up, sending
//! and receiving notes, conflicts and going offline.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gasp_sync::{
    Author, CredentialStore, InMemoryCredentialStore, SyncError, SyncResult, Token, Vault,
    VaultConfig,
};
use git2::{Repository, RepositoryInitOptions};

use crate::sync::{SyncPhaseKind, VaultSync};
use crate::sync_conflicts::PlaceChoice;
use crate::sync_setup::{InPlaceSyncSetup, SyncSetup, set_up_in_place_with, set_up_with};

struct World {
    dir: tempfile::TempDir,
    store: Arc<InMemoryCredentialStore>,
}

impl World {
    /// A bare remote whose `master` holds `Note.md`.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut options = RepositoryInitOptions::new();
        options.bare(true).initial_head("master");
        Repository::init_opts(dir.path().join("remote.git"), &options).unwrap();
        let world = World {
            dir,
            store: Arc::default(),
        };
        let laptop = world.laptop_on("master");
        std::fs::write(laptop.root().join("Note.md"), "one\ntwo\nthree\n").unwrap();
        laptop.commit_all(&author(), "laptop: Note.md").unwrap();
        laptop.push().unwrap();
        world
    }

    fn url(&self) -> String {
        format!("file://{}", self.dir.path().join("remote.git").display())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Another device that syncs `branch`.
    fn laptop_on(&self, branch: &str) -> Vault {
        let config = VaultConfig {
            branch: branch.to_owned(),
            ..VaultConfig::default()
        };
        let folder = self.path(&format!("laptop-{branch}"));
        if folder.exists() {
            let vault = Vault::open(&folder, config).unwrap();
            vault.fetch().unwrap();
            vault.merge(&author()).unwrap();
            return vault;
        }
        let url = self.url();
        Vault::clone_remote(&url, &folder, config.clone(), None)
            .or_else(|_| {
                let _ = std::fs::remove_dir_all(&folder);
                Vault::init(&folder, &url, config)
            })
            .unwrap()
    }

    fn phone(&self) -> Arc<VaultSync> {
        let setup = SyncSetup {
            repository: self.url(),
            branch: String::new(),
            token: "synthetic-token".into(),
            folder: self.path("phone").to_string_lossy().into_owned(),
        };
        set_up_with(&setup, &*self.store).unwrap();
        VaultSync::open_with(self.path("phone"), "iphone".into(), self.store.clone())
    }

    fn remote_note(&self, branch: &str) -> String {
        let repo = Repository::open_bare(self.path("remote.git")).unwrap();
        let tree = repo
            .find_reference(&format!("refs/heads/{branch}"))
            .unwrap()
            .peel_to_tree()
            .unwrap();
        let blob = repo
            .find_blob(tree.get_path(Path::new("Note.md")).unwrap().id())
            .unwrap();
        String::from_utf8(blob.content().to_vec()).unwrap()
    }

    fn remote_message(&self, branch: &str) -> String {
        let repo = Repository::open_bare(self.path("remote.git")).unwrap();
        let commit = repo
            .find_reference(&format!("refs/heads/{branch}"))
            .unwrap()
            .peel_to_commit()
            .unwrap();
        commit.message().unwrap_or_default().to_owned()
    }
}

fn author() -> Author {
    Author::new("laptop", "laptop@devices.invalid")
}

fn write(root: &Path, text: &str) {
    std::fs::write(root.join("Note.md"), text).unwrap();
}

#[test]
fn setup_clones_master_and_keeps_the_token() {
    let world = World::new();
    let phone = world.phone();
    let overview = phone.overview();
    assert_eq!(overview.phase, SyncPhaseKind::Synced);
    assert_eq!(overview.headline, "Not synced yet");
    assert_eq!(overview.branch, "master");
    assert!(overview.signed_in);
    assert!(!overview.takes_token);
    assert!(phone.sync_now().ran);
    assert_eq!(world.remote_note("master"), "one\ntwo\nthree\n");
}

#[test]
fn an_edit_on_the_phone_is_pushed_and_another_devices_edit_comes_in() {
    let world = World::new();
    let phone = world.phone();
    phone.sync_now();
    write(&world.path("phone"), "one\ntwo\nthree\nfrom the phone\n");
    phone.edited();
    let due = phone.seconds_until_due().unwrap();
    assert!(due > 50. && due <= 60., "{due}");
    phone.sync_now();
    assert_eq!(
        world.remote_note("master"),
        "one\ntwo\nthree\nfrom the phone\n"
    );
    assert_eq!(world.remote_message("master").trim(), "iphone: Note.md");

    let laptop = world.laptop_on("master");
    write(laptop.root(), "zero\none\ntwo\nthree\nfrom the phone\n");
    laptop.commit_all(&author(), "laptop: Note.md").unwrap();
    laptop.push().unwrap();
    let outcome = phone.sync_now();
    assert_eq!(outcome.received, ["Note.md"]);
    let on_phone = std::fs::read_to_string(world.path("phone/Note.md")).unwrap();
    assert!(on_phone.starts_with("zero\n"), "{on_phone}");
    assert_eq!(phone.overview().recent[0].received, ["Note"]);
}

#[test]
fn a_conflict_waits_and_resolves_with_an_edit() {
    let world = World::new();
    let phone = world.phone();
    phone.sync_now();
    let laptop = world.laptop_on("master");
    write(laptop.root(), "one\nlaptop\nthree\n");
    laptop.commit_all(&author(), "laptop: Note.md").unwrap();
    laptop.push().unwrap();
    write(&world.path("phone"), "one\nphone\nthree\n");
    phone.sync_now();
    assert_eq!(phone.overview().phase, SyncPhaseKind::Conflict { files: 1 });
    let conflicts = phone.conflicts();
    let note = &conflicts[0];
    assert_eq!(note.places[0].this_device, "phone\n");
    assert_eq!(note.places[0].other_device, "laptop\n");
    assert_eq!(note.places[0].context, "one");

    let stale = phone.resolve(note.path.clone(), "old".into(), vec![PlaceChoice::Both]);
    assert!(stale.is_err());
    let edited = PlaceChoice::Edited {
        text: "phone and laptop".into(),
    };
    phone
        .resolve(note.path.clone(), note.version.clone(), vec![edited])
        .unwrap();
    assert_eq!(phone.overview().phase, SyncPhaseKind::Synced);
    assert!(phone.conflicts().is_empty());
    assert_eq!(
        world.remote_note("master"),
        "one\nphone and laptop\nthree\n"
    );
}

#[test]
fn offline_changes_wait_and_go_out_once_the_remote_is_back() {
    let world = World::new();
    let phone = world.phone();
    phone.sync_now();
    let remote = world.path("remote.git");
    let away = world.path("remote-away.git");
    std::fs::rename(&remote, &away).unwrap();
    write(&world.path("phone"), "one\ntwo\nthree\noffline\n");
    phone.sync_now();
    assert_eq!(
        phone.overview().phase,
        SyncPhaseKind::Offline { waiting: 1 }
    );
    std::fs::rename(&away, &remote).unwrap();
    phone.sync_now();
    assert_eq!(phone.overview().phase, SyncPhaseKind::Synced);
    assert_eq!(world.remote_note("master"), "one\ntwo\nthree\noffline\n");
}

#[test]
fn signing_out_forgets_the_token() {
    let world = World::new();
    let phone = world.phone();
    phone.sign_out().unwrap();
    assert!(!phone.overview().signed_in);
    assert!(phone.sign_in("  ".into()).is_err());
    phone.sign_in("another-synthetic-token".into()).unwrap();
    assert!(phone.overview().signed_in);
}

/// A Keychain that refuses to keep anything, as the simulator's does for
/// an unsigned app.
struct RefusingStore;

impl CredentialStore for RefusingStore {
    fn load(&self, _: &str) -> SyncResult<Option<Token>> {
        Ok(None)
    }

    fn save(&self, _: &str, _: &Token) -> SyncResult<()> {
        Err(SyncError::Auth("no entitlement".into()))
    }

    fn delete(&self, _: &str) -> SyncResult<()> {
        Ok(())
    }
}

#[test]
fn a_token_the_keychain_refuses_leaves_no_clone_behind() {
    let world = World::new();
    let folder = world.path("phone");
    let setup = SyncSetup {
        repository: world.url(),
        branch: String::new(),
        token: "synthetic-token".into(),
        folder: folder.to_string_lossy().into_owned(),
    };
    assert!(set_up_with(&setup, &RefusingStore).is_err());
    assert!(!folder.exists());
    set_up_with(&setup, &*world.store).unwrap();
    assert!(folder.join("Note.md").is_file());
}

fn in_place(url: String, folder: &Path) -> InPlaceSyncSetup {
    InPlaceSyncSetup {
        url,
        branch: String::new(),
        token: "synthetic-token".into(),
        folder: folder.to_string_lossy().into_owned(),
        device: "iphone".into(),
    }
}

#[test]
fn signing_in_with_github_sets_up_a_new_empty_repository_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let mut options = RepositoryInitOptions::new();
    options.bare(true);
    Repository::init_opts(dir.path().join("notes.git"), &options).unwrap();
    let url = format!("file://{}", dir.path().join("notes.git").display());
    let phone = dir.path().join("Notes");
    std::fs::create_dir(&phone).unwrap();
    std::fs::write(phone.join("Plan.md"), "# Plan\n").unwrap();
    let store = InMemoryCredentialStore::default();

    let summary = set_up_in_place_with(&in_place(url.clone(), &phone), &store).unwrap();

    assert_eq!(summary.waiting, 0);
    assert!(phone.join(".git").is_dir());
    assert_eq!(
        store.load(&url).unwrap(),
        Some(Token::new("synthetic-token"))
    );
    let remote = Repository::open_bare(dir.path().join("notes.git")).unwrap();
    let tree = remote
        .find_reference("refs/heads/master")
        .unwrap()
        .peel_to_tree()
        .unwrap();
    assert!(tree.get_path(Path::new("Plan.md")).is_ok());
}

#[test]
fn signing_in_with_github_brings_an_existing_repository_into_a_new_folder() {
    let world = World::new();
    let folder = world.path("Synced/notes");

    let summary = set_up_in_place_with(&in_place(world.url(), &folder), &*world.store).unwrap();

    assert_eq!(summary.brought_in, 1);
    assert_eq!(
        std::fs::read_to_string(folder.join("Note.md")).unwrap(),
        "one\ntwo\nthree\n"
    );
}
