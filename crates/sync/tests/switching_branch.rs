//! Changing the branch sync uses, as the Branch setting does.

mod common;

use common::{World, read, sync, write};
use gasp_sync::{Vault, VaultConfig, switch_branch};

const NOTE: &str = "notes/plan.md";

fn on_branch(branch: &str) -> VaultConfig {
    VaultConfig {
        branch: branch.to_owned(),
        ..VaultConfig::default()
    }
}

#[test]
fn a_new_branch_starts_where_the_vault_is_and_takes_the_next_sync() {
    let world = World::seeded(&[(NOTE, b"first\n")]);
    let laptop = world.device("laptop");
    write(&laptop, NOTE, b"first\nunsaved edit\n");
    drop(laptop);

    switch_branch(&world.path("laptop"), "notes").unwrap();
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
fn switching_back_to_a_branch_at_the_same_commit_works() {
    let world = World::seeded(&[(NOTE, b"first\n")]);
    drop(world.device("laptop"));
    let root = world.path("laptop");
    switch_branch(&root, "notes").unwrap();
    switch_branch(&root, "master").unwrap();
    assert!(Vault::open(root, VaultConfig::default()).is_ok());
}

#[test]
fn a_branch_that_would_change_the_notes_is_refused() {
    let world = World::seeded(&[(NOTE, b"first\n")]);
    let laptop = world.device("laptop");
    let root = world.path("laptop");
    drop(laptop);
    switch_branch(&root, "notes").unwrap();
    let on_notes = Vault::open(root.clone(), on_branch("notes")).unwrap();
    write(&on_notes, NOTE, b"moved on\n");
    sync(&on_notes, "laptop");
    drop(on_notes);
    switch_branch(&root, "master").unwrap_err();
}
