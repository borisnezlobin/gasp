//! A conflict never stops sync: the merge finishes, the conflicted note
//! waits for a person with both versions on disk, and every other note
//! keeps syncing between devices.

mod common;

use common::{World, author, read, sync, write};
use editor_sync::{MergeOutcome, Resolution, Vault, VaultConfig};
use git2::Repository;

const NOTE: &str = "Lemma.md";
const OTHER: &str = "Habit Ideas.md";
const BASE: &str = "# Lemma\nstatement\nproof\n";

fn laptop_text() -> String {
    BASE.replace("proof", "proof by the laptop")
}

fn phone_text() -> String {
    BASE.replace("proof", "proof by the phone")
}

/// Three devices; the laptop and the phone rewrite the same line of
/// `NOTE`, the laptop syncs first and the phone's sync parks the note.
struct Trio {
    world: World,
    laptop: Vault,
    phone: Vault,
    desktop: Vault,
}

fn phone_waiting_on_a_conflict() -> Trio {
    let world = World::seeded(&[(NOTE, BASE.as_bytes()), (OTHER, b"ideas\n")]);
    let trio = Trio {
        laptop: world.device("laptop"),
        phone: world.device("phone"),
        desktop: world.device("desktop"),
        world,
    };
    write(&trio.laptop, NOTE, laptop_text().as_bytes());
    sync(&trio.laptop, "laptop");
    write(&trio.phone, NOTE, phone_text().as_bytes());
    let outcome = sync(&trio.phone, "phone");
    assert!(
        matches!(outcome, MergeOutcome::Conflicts(ref files) if files.len() == 1),
        "{outcome:?}"
    );
    trio
}

fn merge_state_files(vault: &Vault) -> bool {
    let git = vault.root().join(".git");
    git.join("MERGE_HEAD").exists() || git.join("MERGE_MSG").exists()
}

#[test]
fn a_conflict_in_one_note_does_not_block_an_edit_to_another() {
    let trio = phone_waiting_on_a_conflict();
    assert!(!trio.phone.is_merging());
    assert!(!merge_state_files(&trio.phone));

    write(&trio.phone, OTHER, b"ideas\nwritten offline on the phone\n");
    sync(&trio.phone, "phone");
    assert_eq!(trio.phone.conflicts().unwrap().len(), 1, "still waiting");

    for device in [&trio.laptop, &trio.desktop] {
        sync(device, "other");
        assert_eq!(read(device, OTHER), "ideas\nwritten offline on the phone\n");
        assert_eq!(
            read(device, NOTE),
            laptop_text(),
            "no markers leave the phone"
        );
        assert!(device.conflicts().unwrap().is_empty());
    }
}

#[test]
fn merges_that_diverge_leave_the_waiting_note_alone() {
    let trio = phone_waiting_on_a_conflict();
    let waiting = read(&trio.phone, NOTE);
    sync(&trio.desktop, "desktop");
    write(&trio.desktop, OTHER, b"ideas\nfrom the desktop\n");
    sync(&trio.desktop, "desktop");
    write(&trio.phone, "New.md", b"new on the phone\n");

    let outcome = sync(&trio.phone, "phone");
    assert!(
        matches!(outcome, MergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    assert_eq!(read(&trio.phone, NOTE), waiting);
    assert_eq!(read(&trio.phone, OTHER), "ideas\nfrom the desktop\n");
    assert_eq!(
        trio.world.remote_file("master", "New.md").unwrap(),
        b"new on the phone\n"
    );
    assert_eq!(trio.phone.conflicts().unwrap().len(), 1);
}

#[test]
fn the_other_devices_text_stays_on_the_remote_until_resolved() {
    let trio = phone_waiting_on_a_conflict();
    assert_eq!(
        trio.world.remote_file("master", NOTE).unwrap(),
        laptop_text().as_bytes()
    );
    let on_disk = read(&trio.phone, NOTE);
    assert!(on_disk.contains("proof by the phone"));
    assert!(on_disk.contains("proof by the laptop"));
    // The phone's version is in the pushed history, too.
    let repo = Repository::open(trio.phone.root()).unwrap();
    let merge = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(merge.parent_count(), 2);
    let phone_commit = merge.parent(0).unwrap();
    let blob = phone_commit
        .tree()
        .unwrap()
        .get_path(NOTE.as_ref())
        .unwrap();
    let phone_version = repo.find_blob(blob.id()).unwrap().content().to_vec();
    assert_eq!(phone_version, phone_text().as_bytes());
}

#[test]
fn the_conflict_waits_across_reopening_the_vault_and_resolves() {
    let trio = phone_waiting_on_a_conflict();
    let files = trio.phone.conflicts().unwrap();
    let root = trio.phone.root().to_owned();
    drop(trio.phone);
    let phone = Vault::open(&root, VaultConfig::default()).unwrap();
    assert_eq!(phone.conflicts().unwrap(), files);

    phone.resolve(&files[0], &[Resolution::Both]).unwrap();
    sync(&phone, "phone");
    let both = BASE.replace("proof", "proof by the phone\nproof by the laptop");
    assert_eq!(
        trio.world.remote_file("master", NOTE).unwrap(),
        both.as_bytes()
    );
    sync(&trio.desktop, "desktop");
    assert_eq!(read(&trio.desktop, NOTE), both);
}

#[test]
fn editing_away_the_markers_settles_the_note() {
    let trio = phone_waiting_on_a_conflict();
    write(
        &trio.phone,
        NOTE,
        b"# Lemma\nstatement\nproof, merged by hand\n",
    );
    sync(&trio.phone, "phone");
    assert!(trio.phone.conflicts().unwrap().is_empty());
    assert_eq!(
        trio.world.remote_file("master", NOTE).unwrap(),
        b"# Lemma\nstatement\nproof, merged by hand\n"
    );
}

#[test]
fn edits_to_a_waiting_note_are_kept_when_it_is_resolved() {
    let trio = phone_waiting_on_a_conflict();
    let edited = format!("{}a line added while it waited\n", read(&trio.phone, NOTE));
    write(&trio.phone, NOTE, edited.as_bytes());
    sync(&trio.phone, "phone");
    let files = trio.phone.conflicts().unwrap();
    assert_eq!(files.len(), 1);
    let hunk = files[0].hunks().next().unwrap();
    assert_eq!(hunk.base, "proof\n", "the base survives the edit");

    let stale = files[0].clone();
    trio.phone
        .resolve(&files[0], &[Resolution::ThisDevice])
        .unwrap();
    let expected = format!("{}a line added while it waited\n", phone_text());
    assert_eq!(read(&trio.phone, NOTE), expected);
    let again = trio.phone.resolve(&stale, &[Resolution::OtherDevice]);
    assert!(again.is_err(), "a stale resolution is refused");
    assert_eq!(read(&trio.phone, NOTE), expected);
}

#[test]
fn more_changes_from_the_other_device_fold_into_the_waiting_note() {
    let trio = phone_waiting_on_a_conflict();
    sync(&trio.desktop, "desktop");
    let desktop_text = format!("{}a remark from the desktop\n", laptop_text());
    write(&trio.desktop, NOTE, desktop_text.as_bytes());
    sync(&trio.desktop, "desktop");

    let outcome = sync(&trio.phone, "phone");
    assert_eq!(outcome, MergeOutcome::FastForward);
    assert!(read(&trio.phone, NOTE).ends_with("a remark from the desktop\n"));
    assert_eq!(
        trio.world.remote_file("master", NOTE).unwrap(),
        desktop_text.as_bytes()
    );
    let files = trio.phone.conflicts().unwrap();
    assert_eq!(files.len(), 1);
    trio.phone
        .resolve(&files[0], &[Resolution::ThisDevice])
        .unwrap();
    sync(&trio.phone, "phone");
    let expected = format!("{}a remark from the desktop\n", phone_text());
    assert_eq!(
        trio.world.remote_file("master", NOTE).unwrap(),
        expected.as_bytes()
    );
}

#[test]
fn a_conflict_with_the_legacy_branch_keeps_masters_version_meanwhile() {
    let old_tool_config = VaultConfig {
        branch: "main".to_owned(),
        legacy_branch: None,
        ..VaultConfig::default()
    };
    let world = World::seeded_on(old_tool_config.clone(), &[(NOTE, BASE.as_bytes())]);
    let app = world.device("laptop");
    let old_tool = world.device_with("phone", old_tool_config);
    write(&app, NOTE, laptop_text().as_bytes());
    sync(&app, "laptop");
    write(&old_tool, NOTE, phone_text().as_bytes());
    sync(&old_tool, "phone");

    let outcome = sync(&app, "laptop");
    assert!(matches!(outcome, MergeOutcome::Conflicts(_)), "{outcome:?}");
    assert_eq!(
        world.remote_file("master", NOTE).unwrap(),
        laptop_text().as_bytes()
    );
    let files = app.conflicts().unwrap();
    let hunk = files[0].hunks().next().unwrap();
    assert_eq!(hunk.this_device, "proof by the laptop\n");
    assert_eq!(hunk.other_device, "proof by the phone\n");

    // The phone rewrites the same line again before anyone resolves it.
    let again = BASE.replace("proof", "proof by the phone, again");
    write(&old_tool, NOTE, again.as_bytes());
    sync(&old_tool, "phone");
    sync(&app, "laptop");
    assert_eq!(
        world.remote_file("master", NOTE).unwrap(),
        laptop_text().as_bytes()
    );
    let files = app.conflicts().unwrap();
    assert_eq!(files.len(), 1);
    let hunk = files[0].hunks().next().unwrap();
    assert_eq!(hunk.this_device, "proof by the laptop\n");
    assert_eq!(hunk.other_device, "proof by the phone, again\n");

    app.resolve(&files[0], &[Resolution::OtherDevice]).unwrap();
    sync(&app, "laptop");
    assert_eq!(world.remote_file("master", NOTE).unwrap(), again.as_bytes());
    assert_eq!(world.remote_file("main", NOTE).unwrap(), again.as_bytes());
    assert_eq!(sync(&app, "laptop"), MergeOutcome::UpToDate);
}

#[test]
fn a_merge_an_older_version_left_paused_is_finished_and_parked() {
    let world = World::seeded(&[(NOTE, BASE.as_bytes()), (OTHER, b"ideas\n")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    write(&laptop, NOTE, laptop_text().as_bytes());
    sync(&laptop, "laptop");
    write(&phone, NOTE, phone_text().as_bytes());
    phone.commit_all(&author("phone"), "phone edit").unwrap();
    phone.fetch().unwrap();
    pause_merge_as_the_old_version_did(&phone);
    assert!(phone.is_merging());
    write(&phone, OTHER, b"ideas\nfrom the phone\n");

    sync(&phone, "phone");
    assert!(!phone.is_merging());
    assert_eq!(phone.conflicts().unwrap().len(), 1);
    assert_eq!(
        world.remote_file("master", OTHER).unwrap(),
        b"ideas\nfrom the phone\n"
    );
    assert_eq!(
        world.remote_file("master", NOTE).unwrap(),
        laptop_text().as_bytes()
    );
}

/// What the app used to do on a conflict: merge with git, write the note
/// with its own markers, and stop there with `MERGE_HEAD` in place.
fn pause_merge_as_the_old_version_did(vault: &Vault) {
    let repo = Repository::open(vault.root()).unwrap();
    let theirs = repo
        .find_reference("refs/remotes/origin/master")
        .and_then(|reference| repo.reference_to_annotated_commit(&reference))
        .unwrap();
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.allow_conflicts(true);
    repo.merge(&[&theirs], None, Some(&mut checkout)).unwrap();
    let marked = BASE.replace(
        "proof\n",
        "<<<<<<< this device\nproof by the phone\n=======\nproof by the laptop\n>>>>>>> other device\n",
    );
    write(vault, NOTE, marked.as_bytes());
}
