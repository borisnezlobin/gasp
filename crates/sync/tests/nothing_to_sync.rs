//! The shortcuts sync takes when nothing changed still notice every change.

mod common;

use std::path::Path;
use std::time::Duration;

use common::{World, author, numbered_note, replace_line, sync, write};
use gasp_sync::{MergeOutcome, Vault, VaultConfig};
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

#[test]
fn reopening_a_vault_leaves_its_settings_files_alone() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    let git_dir = laptop.root().join(".git");
    let modified = |name: &str| {
        let metadata = std::fs::metadata(git_dir.join(name)).unwrap();
        metadata.modified().unwrap()
    };
    let before = (modified("config"), modified("info/exclude"));
    std::thread::sleep(Duration::from_millis(20));

    Vault::open(laptop.root(), VaultConfig::default()).unwrap();
    assert_eq!((modified("config"), modified("info/exclude")), before);
    let config = Repository::open(laptop.root()).unwrap().config().unwrap();
    let local = config.open_level(git2::ConfigLevel::Local).unwrap();
    assert!(!local.get_bool("core.autocrlf").unwrap());
    assert_eq!(local.get_string("core.eol").unwrap(), "lf");
    assert_eq!(local.get_i64("pack.deltaCacheSize").unwrap(), 512 * 1024);
}

#[test]
fn a_fast_forward_writes_every_changed_file_and_keeps_local_edits() {
    let world = World::seeded(&[
        ("kept.md", b"kept\n"),
        ("edited.md", b"one\n"),
        ("Old folder/gone.md", b"gone\n"),
        ("moved.md", b"moving\n"),
        ("becomes a folder", b"a file for now\n"),
    ]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    write(&phone, "edited.md", b"one, then two\n");
    write(
        &phone,
        "New folder/Pasted image [1] *.png",
        b"\x89PNG pixels",
    );
    std::fs::remove_file(phone.root().join("Old folder/gone.md")).unwrap();
    std::fs::rename(phone.root().join("moved.md"), phone.root().join("here.md")).unwrap();
    std::fs::remove_file(phone.root().join("becomes a folder")).unwrap();
    write(&phone, "becomes a folder/inside.md", b"inside\n");
    sync(&phone, "phone");
    write(&laptop, "kept.md", b"kept, edited on the laptop\n");
    write(&laptop, "draft.md", b"not committed yet\n");

    laptop.fetch().unwrap();
    let outcome = laptop.merge(&author("laptop")).unwrap();
    assert_eq!(outcome, MergeOutcome::FastForward);
    let on_disk = |path: &str| std::fs::read(laptop.root().join(path)).ok();
    assert_eq!(on_disk("edited.md").unwrap(), b"one, then two\n");
    assert_eq!(
        on_disk("New folder/Pasted image [1] *.png").unwrap(),
        b"\x89PNG pixels"
    );
    assert!(!laptop.root().join("Old folder").exists());
    assert_eq!(on_disk("moved.md"), None);
    assert_eq!(on_disk("here.md").unwrap(), b"moving\n");
    assert_eq!(on_disk("becomes a folder/inside.md").unwrap(), b"inside\n");
    assert_eq!(on_disk("kept.md").unwrap(), b"kept, edited on the laptop\n");
    assert_eq!(on_disk("draft.md").unwrap(), b"not committed yet\n");
    assert_eq!(laptop.unpushed_changes().unwrap(), 2);
}

#[test]
fn a_merge_writes_renames_and_edits_from_both_sides() {
    let note = numbered_note(12);
    let world = World::seeded(&[("plan.md", note.as_bytes()), ("old.md", b"old\n")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    let renamed = replace_line(&note, 1, "Line 1, edited on the laptop.");
    std::fs::remove_file(laptop.root().join("plan.md")).unwrap();
    write(&laptop, "Plans/plan.md", renamed.as_bytes());
    sync(&laptop, "laptop");
    let edited = replace_line(&note, 12, "Line 12, edited on the phone.");
    write(&phone, "plan.md", edited.as_bytes());
    std::fs::remove_file(phone.root().join("old.md")).unwrap();
    write(&phone, "New/new.md", b"new\n");

    let outcome = sync(&phone, "phone");
    assert!(
        matches!(outcome, MergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    let on_disk = |path: &str| std::fs::read_to_string(phone.root().join(path)).ok();
    let merged = on_disk("Plans/plan.md").unwrap();
    assert!(
        merged.starts_with("Line 1, edited on the laptop.\n"),
        "{merged}"
    );
    assert!(
        merged.ends_with("Line 12, edited on the phone.\n"),
        "{merged}"
    );
    assert_eq!(on_disk("plan.md"), None);
    assert_eq!(on_disk("old.md"), None);
    assert_eq!(on_disk("New/new.md").unwrap(), "new\n");
    assert_eq!(phone.unpushed_changes().unwrap(), 0);
}
