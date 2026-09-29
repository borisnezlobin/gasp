//! The phone's sync against local bare repositories: setting up, sending
//! and receiving notes, the legacy branch, conflicts and going offline.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use editor_sync::{Author, InMemoryCredentialStore, Vault, VaultConfig};
use git2::{Repository, RepositoryInitOptions};

use crate::sync::{SyncPhaseKind, VaultSync};
use crate::sync_conflicts::PlaceChoice;
use crate::sync_setup::{SyncSetup, set_up_with};

struct World {
    dir: tempfile::TempDir,
    store: Arc<InMemoryCredentialStore>,
}

impl World {
    /// A bare remote whose `main` holds `Note.md`, as the old sync tool left it.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut options = RepositoryInitOptions::new();
        options.bare(true).initial_head("main");
        Repository::init_opts(dir.path().join("remote.git"), &options).unwrap();
        let world = World {
            dir,
            store: Arc::default(),
        };
        let laptop = world.laptop_on("main");
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

    /// Another device that syncs `branch` with no legacy branch.
    fn laptop_on(&self, branch: &str) -> Vault {
        let config = VaultConfig {
            branch: branch.to_owned(),
            legacy_branch: None,
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
fn setup_clones_master_from_main_and_keeps_the_token() {
    let world = World::new();
    let phone = world.phone();
    let overview = phone.overview();
    assert_eq!(overview.phase, SyncPhaseKind::Synced);
    assert_eq!(overview.headline, "Not synced yet");
    assert_eq!(overview.branch, "master");
    assert_eq!(overview.legacy_branch, "main");
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
fn a_commit_on_the_legacy_branch_merges_into_master() {
    let world = World::new();
    let phone = world.phone();
    phone.sync_now();
    let old_tool = world.laptop_on("main");
    write(old_tool.root(), "one\ntwo\nthree\nfrom the old tool\n");
    old_tool.commit_all(&author(), "laptop: Note.md").unwrap();
    old_tool.push().unwrap();
    phone.sync_now();
    assert_eq!(
        world.remote_note("master"),
        "one\ntwo\nthree\nfrom the old tool\n"
    );
    assert_eq!(
        world.remote_note("main"),
        "one\ntwo\nthree\nfrom the old tool\n"
    );
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
