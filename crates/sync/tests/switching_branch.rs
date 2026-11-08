//! Changing the branch sync uses, as the Branch setting and the
//! indicator's "Switch to …" button do.

mod common;

use std::fs;

use common::{World, read, sync, write};
use gasp_sync::{SyncError, Vault, VaultConfig, switch_branch};

const NOTE: &str = "notes/plan.md";
const IDEA: &str = "notes/idea.md";
const REMOTE: &str = "origin";

fn on_branch(branch: &str) -> VaultConfig {
    VaultConfig {
        branch: branch.to_owned(),
        ..VaultConfig::default()
    }
}

fn head_branch(world: &World, device: &str) -> String {
    let repo = git2::Repository::open(world.path(device)).unwrap();
    repo.head().unwrap().shorthand().unwrap().to_owned()
}

/// A remote whose `old` branch stayed at the first commit while `master`
/// moved on, and a laptop that cloned only `old`, the way a repository's
/// default branch gets cloned when it isn't the one the notes are on.
fn laptop_on_an_old_branch(phone_note: &[u8]) -> World {
    let world = World::seeded(&[(NOTE, b"first\nsecond\nthird\n")]);
    drop(world.device("desk"));
    switch_branch(&world.path("desk"), REMOTE, "old", None).unwrap();
    sync(
        &Vault::open(world.path("desk"), on_branch("old")).unwrap(),
        "desk",
    );
    drop(world.device_with("laptop", on_branch("old")));
    let phone = world.device("phone");
    write(&phone, NOTE, phone_note);
    write(&phone, "notes/new.md", b"from the phone\n");
    sync(&phone, "phone");
    world
}

#[test]
fn a_branch_the_remote_has_brings_its_notes() {
    let world = laptop_on_an_old_branch(b"first\nsecond\nthird\nfrom the phone\n");
    let laptop = world.path("laptop");
    switch_branch(&laptop, REMOTE, "master", None).unwrap();

    assert_eq!(head_branch(&world, "laptop"), "master");
    let vault = Vault::open(&laptop, VaultConfig::default()).unwrap();
    assert_eq!(read(&vault, NOTE), "first\nsecond\nthird\nfrom the phone\n");
    assert_eq!(read(&vault, "notes/new.md"), "from the phone\n");
    assert_eq!(
        world.remote_file("old", NOTE).unwrap(),
        b"first\nsecond\nthird\n"
    );
}

#[test]
fn unsynced_edits_come_along_and_merge() {
    let world = laptop_on_an_old_branch(b"first\nsecond\nthird\nfrom the phone\n");
    let laptop = world.path("laptop");
    let old = Vault::open(&laptop, on_branch("old")).unwrap();
    write(&old, NOTE, b"first from the laptop\nsecond\nthird\n");
    write(&old, IDEA, b"a new idea\n");
    drop(old);

    switch_branch(&laptop, REMOTE, "master", None).unwrap();
    let vault = Vault::open(&laptop, VaultConfig::default()).unwrap();
    assert_eq!(
        read(&vault, NOTE),
        "first from the laptop\nsecond\nthird\nfrom the phone\n"
    );
    assert_eq!(read(&vault, IDEA), "a new idea\n");
    sync(&vault, "laptop");
    assert_eq!(world.remote_file("master", IDEA).unwrap(), b"a new idea\n");
}

#[test]
fn an_edit_that_clashes_with_the_branch_stops_the_move_and_keeps_everything() {
    let world = laptop_on_an_old_branch(b"first from the phone\nsecond\nthird\n");
    let laptop = world.path("laptop");
    let old = Vault::open(&laptop, on_branch("old")).unwrap();
    write(&old, NOTE, b"first from the laptop\nsecond\nthird\n");
    drop(old);

    let error = switch_branch(&laptop, REMOTE, "master", None).unwrap_err();
    let SyncError::BranchBlocked { branch, notes } = &error else {
        panic!("expected a blocked move, got {error:?}");
    };
    assert_eq!(branch, "master");
    assert_eq!(notes, &[std::path::PathBuf::from(NOTE)]);
    assert_eq!(head_branch(&world, "laptop"), "old");
    assert_eq!(
        fs::read_to_string(laptop.join(NOTE)).unwrap(),
        "first from the laptop\nsecond\nthird\n"
    );
}

#[test]
fn a_branch_the_remote_lacks_starts_where_the_vault_is_and_takes_the_next_sync() {
    let world = World::seeded(&[(NOTE, b"first\n")]);
    let laptop = world.device("laptop");
    write(&laptop, NOTE, b"first\nunsaved edit\n");
    drop(laptop);

    switch_branch(&world.path("laptop"), REMOTE, "notes", None).unwrap();
    let laptop = Vault::open(world.path("laptop"), on_branch("notes")).unwrap();
    assert_eq!(
        read(&laptop, NOTE),
        "first\nunsaved edit\n",
        "edits stay on disk"
    );
    sync(&laptop, "laptop");

    assert_eq!(
        world.remote_file("notes", NOTE).unwrap(),
        b"first\nunsaved edit\n"
    );
    assert_eq!(world.remote_file("master", NOTE).unwrap(), b"first\n");
}

#[test]
fn switching_back_and_forth_at_the_same_commit_works() {
    let world = World::seeded(&[(NOTE, b"first\n")]);
    drop(world.device("laptop"));
    let root = world.path("laptop");
    switch_branch(&root, REMOTE, "notes", None).unwrap();
    switch_branch(&root, REMOTE, "master", None).unwrap();
    assert!(Vault::open(root, VaultConfig::default()).is_ok());
}

#[test]
fn an_unreachable_remote_leaves_the_vault_where_it_was() {
    let world = World::seeded(&[(NOTE, b"first\n")]);
    let laptop = world.device("laptop");
    laptop
        .set_remote_url(world.path("gone.git").to_str().unwrap())
        .unwrap();
    drop(laptop);
    let error = switch_branch(&world.path("laptop"), REMOTE, "notes", None).unwrap_err();
    assert!(error.is_offline(), "{error:?}");
    assert_eq!(head_branch(&world, "laptop"), "master");
}
