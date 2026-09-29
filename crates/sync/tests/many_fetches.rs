//! A clone that fetches for months keeps a handful of packs and all of
//! its history.

mod common;

use std::path::Path;

use common::{World, read, sync, write};
use git2::{Repository, TreeWalkMode, TreeWalkResult};

const ROUNDS: usize = 60;

fn pack_count(root: &Path) -> usize {
    std::fs::read_dir(root.join(".git/objects/pack"))
        .unwrap()
        .filter(|entry| {
            let path = entry.as_ref().unwrap().path();
            path.extension().is_some_and(|extension| extension == "idx")
        })
        .count()
}

/// Reads every commit, tree and blob reachable from HEAD.
fn read_all_history(root: &Path) -> usize {
    let repo = Repository::open(root).unwrap();
    let mut walk = repo.revwalk().unwrap();
    walk.push_head().unwrap();
    let mut blobs = 0;
    for commit in walk {
        let tree = repo.find_commit(commit.unwrap()).unwrap().tree().unwrap();
        tree.walk(TreeWalkMode::PreOrder, |_, entry| {
            if let Ok(blob) = entry.to_object(&repo).unwrap().into_blob() {
                assert!(blob.size() > 0);
                blobs += 1;
            }
            TreeWalkResult::Ok
        })
        .unwrap();
    }
    blobs
}

#[test]
fn packs_from_many_fetches_are_combined_without_losing_anything() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    for round in 0..ROUNDS {
        write(
            &phone,
            &format!("phone/{round}.md"),
            format!("{round}\n").as_bytes(),
        );
        sync(&phone, "phone");
        write(&laptop, "note.md", format!("laptop {round}\n").as_bytes());
        sync(&laptop, "laptop");
        sync(&phone, "phone");
    }

    assert!(
        pack_count(laptop.root()) < 24,
        "{}",
        pack_count(laptop.root())
    );
    assert!(
        pack_count(phone.root()) < 24,
        "{}",
        pack_count(phone.root())
    );
    for vault in [&laptop, &phone] {
        assert!(read_all_history(vault.root()) >= ROUNDS * ROUNDS / 2);
        assert_eq!(read(vault, "phone/0.md"), "0\n");
        assert_eq!(read(vault, "note.md"), format!("laptop {}\n", ROUNDS - 1));
    }
    write(&phone, "last.md", b"still syncs\n");
    sync(&phone, "phone");
    sync(&laptop, "laptop");
    assert_eq!(read(&laptop, "last.md"), "still syncs\n");
}
