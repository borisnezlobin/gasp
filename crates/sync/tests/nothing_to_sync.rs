//! The shortcuts sync takes when nothing changed still notice every change.

mod common;

use std::path::Path;
use std::time::Duration;

use common::{World, author, sync, write};
use git2::Repository;

fn remote_tip(world: &World) -> Option<git2::Oid> {
    let repo = Repository::open_bare(&world.remote_url).unwrap();
    repo.refname_to_id("refs/heads/master").ok()
}

#[test]
fn a_note_saved_without_changes_is_not_committed() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    std::thread::sleep(Duration::from_millis(20));
    write(&laptop, "note.md", b"hello\n");
    assert_eq!(laptop.commit_all(&author("laptop"), "same").unwrap(), None);
    assert_eq!(laptop.unpushed_changes().unwrap(), 0);
    write(&laptop, "note.md", b"hello again\n");
    assert!(
        laptop
            .commit_all(&author("laptop"), "edit")
            .unwrap()
            .is_some()
    );
}

#[test]
fn changes_another_git_program_staged_are_committed() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    write(&laptop, "staged.md", b"staged by another program\n");
    let repo = Repository::open(laptop.root()).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("staged.md")).unwrap();
    index.write().unwrap();

    sync(&laptop, "laptop");
    assert_eq!(
        world.remote_file("master", "staged.md"),
        Some(b"staged by another program\n".to_vec())
    );
}

#[test]
fn a_push_after_a_fetch_brings_back_a_deleted_remote_branch() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    let tip = remote_tip(&world);
    let remote = Repository::open_bare(&world.remote_url).unwrap();
    remote
        .find_reference("refs/heads/master")
        .unwrap()
        .delete()
        .unwrap();

    sync(&laptop, "laptop");
    assert_eq!(remote_tip(&world), tip);
}

#[test]
fn a_push_without_a_fresh_fetch_always_asks_the_remote() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    write(&laptop, "note.md", b"edited\n");
    sync(&laptop, "laptop");
    sync(&laptop, "laptop");
    let edited = remote_tip(&world).unwrap();
    let remote = Repository::open_bare(&world.remote_url).unwrap();
    let parent = remote.find_commit(edited).unwrap().parent_id(0).unwrap();
    remote
        .reference("refs/heads/master", parent, true, "rewound")
        .unwrap();

    laptop.push().unwrap();
    assert_eq!(remote_tip(&world), Some(edited));
}
