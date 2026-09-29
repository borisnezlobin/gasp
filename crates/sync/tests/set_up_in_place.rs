//! Making a vault folder that isn't a clone into one, in place, against a
//! bare repository in a temp dir.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{World, author, read, write};
use gasp_sync::{
    InPlaceSetup, STAGING_FOLDER, SetupReport, SyncError, Vault, VaultConfig, set_up_in_place,
};
use git2::{Repository, RepositoryInitOptions};

/// A folder of notes that has never been a clone.
fn folder_with(world: &World, name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = world.path(name);
    for (path, text) in files {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    root
}

fn set_up(root: &Path, url: &str) -> Result<SetupReport, SyncError> {
    set_up_in_place(&InPlaceSetup {
        root,
        url,
        config: VaultConfig::default(),
        token: None,
        author: author("laptop"),
        device: "laptop",
    })
}

/// An empty bare repository, as a new GitHub repository is.
fn empty_world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("remote.git");
    let mut options = RepositoryInitOptions::new();
    options.bare(true).initial_head("master");
    Repository::init_opts(&bare, &options).unwrap();
    let remote_url = bare.to_str().unwrap().to_owned();
    World { dir, remote_url }
}

fn remote_text(world: &World, branch: &str, path: &str) -> Option<String> {
    world
        .remote_file(branch, path)
        .map(|bytes| String::from_utf8(bytes).unwrap())
}

/// Every file under `root`, with its bytes, leaving out nothing.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let relative = path.strip_prefix(root).unwrap().to_path_buf();
                files.push((relative, fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn an_empty_remote_gets_the_folders_notes() {
    let world = empty_world();
    let root = folder_with(
        &world,
        "laptop",
        &[("Plan.md", "# Plan\n"), ("Daily/Monday.md", "coffee\n")],
    );
    let report = set_up(&root, &world.remote_url).unwrap();
    assert!(report.remote_was_empty);
    assert_eq!(report.sent.len(), 2);
    assert!(report.brought_in.is_empty());
    assert_eq!(
        remote_text(&world, "master", "Plan.md").as_deref(),
        Some("# Plan\n")
    );
    assert_eq!(
        remote_text(&world, "master", "Daily/Monday.md").as_deref(),
        Some("coffee\n")
    );
    let vault = Vault::open(&root, VaultConfig::default()).unwrap();
    assert_eq!(vault.unpushed_changes().unwrap(), 0);
    assert!(!root.join(STAGING_FOLDER).exists());
}

#[test]
fn disjoint_notes_meet_on_both_sides() {
    let world = World::seeded(&[("Remote.md", b"from the repository\n")]);
    let root = folder_with(&world, "laptop", &[("Local.md", "from the laptop\n")]);
    let report = set_up(&root, &world.remote_url).unwrap();
    assert!(!report.remote_was_empty);
    assert_eq!(report.brought_in, [PathBuf::from("Remote.md")]);
    assert_eq!(report.sent, [PathBuf::from("Local.md")]);
    assert_eq!(
        fs::read_to_string(root.join("Remote.md")).unwrap(),
        "from the repository\n"
    );
    assert_eq!(
        remote_text(&world, "master", "Local.md").as_deref(),
        Some("from the laptop\n")
    );

    // It syncs as any clone does from here: another device's edit comes in.
    let desktop = world.device("desktop");
    write(
        &desktop,
        "Remote.md",
        b"from the repository\nand the desktop\n",
    );
    desktop.commit_all(&author("desktop"), "Edit").unwrap();
    desktop.fetch().unwrap();
    desktop.merge(&author("desktop")).unwrap();
    desktop.push().unwrap();
    let laptop = Vault::open(&root, VaultConfig::default()).unwrap();
    laptop.fetch().unwrap();
    laptop.merge(&author("laptop")).unwrap();
    assert_eq!(
        read(&laptop, "Remote.md"),
        "from the repository\nand the desktop\n"
    );
}

#[test]
fn a_note_changed_on_both_sides_is_never_lost() {
    let world = World::seeded(&[
        ("Plan.md", b"# Plan\nnew goal\nshared\n"),
        ("Grown.md", b"one\ntwo\nthree\n"),
    ]);
    let root = folder_with(
        &world,
        "laptop",
        &[
            ("Plan.md", "# Plan\nold goal\nshared\n"),
            ("Grown.md", "one\ntwo\n"),
        ],
    );
    let report = set_up(&root, &world.remote_url).unwrap();
    assert_eq!(report.waiting, [PathBuf::from("Plan.md")]);

    // Lines only one side had merge: the remote's longer copy wins nothing
    // from the laptop, and loses nothing either.
    assert_eq!(
        fs::read_to_string(root.join("Grown.md")).unwrap(),
        "one\ntwo\nthree\n"
    );

    // The clash waits with both versions on disk and the remote's in the branch.
    let on_disk = fs::read_to_string(root.join("Plan.md")).unwrap();
    assert!(on_disk.contains("old goal") && on_disk.contains("new goal"));
    assert_eq!(
        remote_text(&world, "master", "Plan.md").as_deref(),
        Some("# Plan\nnew goal\nshared\n")
    );
    let vault = Vault::open(&root, VaultConfig::default()).unwrap();
    let conflicts = vault.conflicts().unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].hunk_count(), 1);

    // Keeping the laptop's line settles it, and the next commit sends it.
    vault
        .resolve(&conflicts[0], &[gasp_sync::Resolution::ThisDevice])
        .unwrap();
    vault.commit_all(&author("laptop"), "Resolve").unwrap();
    vault.fetch().unwrap();
    vault.merge(&author("laptop")).unwrap();
    vault.push().unwrap();
    assert_eq!(
        remote_text(&world, "master", "Plan.md").as_deref(),
        Some("# Plan\nold goal\nshared\n")
    );
}

#[test]
fn device_only_files_stay_on_the_device() {
    let world = empty_world();
    let config_dir = gasp_config::CONFIG_DIR;
    let device_file = format!("{config_dir}/device.toml");
    let settings_file = format!("{config_dir}/settings.toml");
    let root = folder_with(
        &world,
        "laptop",
        &[
            ("Note.md", "text\n"),
            (&device_file, "open-tabs = []\n"),
            (&settings_file, "[sync]\nauto = true\n"),
            (".trash/Old.md", "gone\n"),
            (".DS_Store", "finder\n"),
        ],
    );
    set_up(&root, &world.remote_url).unwrap();
    assert!(remote_text(&world, "master", "Note.md").is_some());
    assert!(remote_text(&world, "master", &settings_file).is_some());
    assert!(remote_text(&world, "master", &device_file).is_none());
    assert!(remote_text(&world, "master", ".trash/Old.md").is_none());
    assert!(remote_text(&world, "master", ".DS_Store").is_none());
    assert!(root.join(&device_file).exists(), "and they stay on disk");
}

#[test]
fn the_legacy_branch_starts_the_branch_when_it_is_all_there_is() {
    let legacy = VaultConfig {
        branch: "main".to_owned(),
        legacy_branch: None,
        ..VaultConfig::default()
    };
    let world = World::seeded_on(legacy, &[("Old.md", b"from the old tool\n")]);
    let root = folder_with(&world, "laptop", &[("New.md", "laptop\n")]);
    set_up(&root, &world.remote_url).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("Old.md")).unwrap(),
        "from the old tool\n"
    );
    assert!(remote_text(&world, "master", "Old.md").is_some());
    assert!(remote_text(&world, "master", "New.md").is_some());
    assert!(
        remote_text(&world, "main", "New.md").is_none(),
        "the legacy branch is never pushed to"
    );
}

#[test]
fn an_address_that_cant_be_reached_leaves_the_folder_as_it_was() {
    let world = empty_world();
    let root = folder_with(&world, "laptop", &[("Plan.md", "# Plan\n")]);
    let before = snapshot(&root);
    let missing = world.path("nowhere.git");
    let error = set_up(&root, missing.to_str().unwrap()).unwrap_err();
    assert!(error.is_offline(), "{error:?}");
    assert_eq!(snapshot(&root), before);
    assert!(!root.join(".git").exists());
}

#[test]
fn a_refused_push_leaves_the_folder_as_it_was() {
    let world = World::seeded(&[("Remote.md", b"from the repository\n")]);
    let root = folder_with(&world, "laptop", &[("Local.md", "from the laptop\n")]);
    let before = snapshot(&root);
    // A repository that can be read but not written, as with a token
    // that only reads.
    let remote = Path::new(&world.remote_url);
    set_folders_read_only(remote, true);
    let result = set_up(&root, &world.remote_url);
    set_folders_read_only(remote, false);
    assert!(result.is_err(), "the push can't write");
    assert_eq!(snapshot(&root), before, "Remote.md isn't written in");
    assert!(!root.join(".git").exists());
    assert!(!root.join(STAGING_FOLDER).exists());
}

/// Makes every folder under `root` read-only, or writable again.
fn set_folders_read_only(root: &Path, read_only: bool) {
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        if !read_only {
            set_read_only(&folder, false);
        }
        for entry in fs::read_dir(&folder).unwrap().flatten() {
            if entry.path().is_dir() {
                folders.push(entry.path());
            }
        }
        if read_only {
            set_read_only(&folder, true);
        }
    }
}

fn set_read_only(folder: &Path, read_only: bool) {
    let mut permissions = fs::metadata(folder).unwrap().permissions();
    permissions.set_readonly(read_only);
    fs::set_permissions(folder, permissions).unwrap();
}

#[test]
fn a_clone_is_not_set_up_again() {
    let world = World::seeded(&[("Remote.md", b"text\n")]);
    let clone = world.device("laptop");
    let error = set_up(clone.root(), &world.remote_url).unwrap_err();
    assert!(matches!(error, SyncError::AlreadyAClone));
}
