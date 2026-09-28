mod common;

use std::path::{Path, PathBuf};

use common::{World, author, numbered_note, read, read_bytes, replace_line, sync, write};
use editor_sync::{
    ConflictedFile, DeviceOnlyFiles, MergeOutcome, Resolution, SyncError, Vault, VaultConfig,
};
use git2::Repository;

const NOTE: &str = "notes/plan.md";

fn two_devices_with_note(lines: usize) -> (World, Vault, Vault, String) {
    let base = numbered_note(lines);
    let world = World::seeded(&[(NOTE, base.as_bytes())]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    (world, laptop, phone, base)
}

fn head_parent_count(vault: &Vault) -> usize {
    let repo = Repository::open(vault.root()).unwrap();
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent_count()
}

#[test]
fn offline_edits_to_different_parts_merge_cleanly() {
    let (world, laptop, phone, base) = two_devices_with_note(10);
    write(
        &laptop,
        NOTE,
        replace_line(&base, 2, "Laptop wrote this.").as_bytes(),
    );
    write(
        &phone,
        NOTE,
        replace_line(&base, 9, "Phone wrote this.").as_bytes(),
    );
    laptop
        .commit_all(&author("laptop"), "offline edit")
        .unwrap();
    phone.commit_all(&author("phone"), "offline edit").unwrap();

    assert_eq!(sync(&laptop, "laptop"), MergeOutcome::UpToDate);
    assert!(matches!(sync(&phone, "phone"), MergeOutcome::Merged { .. }));
    assert_eq!(head_parent_count(&phone), 2);
    assert_eq!(sync(&laptop, "laptop"), MergeOutcome::FastForward);

    let both = replace_line(
        &replace_line(&base, 2, "Laptop wrote this."),
        9,
        "Phone wrote this.",
    );
    assert_eq!(read(&laptop, NOTE), both);
    assert_eq!(read(&phone, NOTE), both);
    assert_eq!(world.remote_file("master", NOTE).unwrap(), both.as_bytes());
    assert!(!phone.is_merging());
}

#[test]
fn edits_to_adjacent_lines_merge_without_a_conflict() {
    let (_world, laptop, phone, base) = two_devices_with_note(6);
    write(
        &laptop,
        NOTE,
        replace_line(&base, 3, "Laptop line three.").as_bytes(),
    );
    write(
        &phone,
        NOTE,
        replace_line(&base, 4, "Phone line four.").as_bytes(),
    );
    sync(&laptop, "laptop");
    assert!(matches!(sync(&phone, "phone"), MergeOutcome::Merged { .. }));
    let expected = replace_line(
        &replace_line(&base, 3, "Laptop line three."),
        4,
        "Phone line four.",
    );
    assert_eq!(read(&phone, NOTE), expected);
}

#[test]
fn appends_at_the_end_on_both_devices_keep_both_lines() {
    let (world, laptop, phone, base) = two_devices_with_note(4);
    write(
        &laptop,
        NOTE,
        format!("{base}Laptop appended.\n").as_bytes(),
    );
    write(&phone, NOTE, format!("{base}Phone appended.\n").as_bytes());
    sync(&laptop, "laptop");
    assert!(matches!(sync(&phone, "phone"), MergeOutcome::Merged { .. }));
    sync(&laptop, "laptop");

    let phone_view = format!("{base}Phone appended.\nLaptop appended.\n");
    assert_eq!(read(&phone, NOTE), phone_view);
    assert_eq!(read(&laptop, NOTE), phone_view);
    assert_eq!(
        world.remote_file("master", NOTE).unwrap(),
        phone_view.as_bytes()
    );
}

#[test]
fn edits_to_different_notes_and_new_notes_merge() {
    let world = World::seeded(&[("a.md", b"a\n"), ("b.md", b"b\n")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    write(&laptop, "a.md", b"a from laptop\n");
    write(&laptop, "new/laptop.md", b"new\n");
    write(&phone, "b.md", b"b from phone\n");
    sync(&laptop, "laptop");
    assert!(matches!(sync(&phone, "phone"), MergeOutcome::Merged { .. }));
    sync(&laptop, "laptop");
    for vault in [&laptop, &phone] {
        assert_eq!(read(vault, "a.md"), "a from laptop\n");
        assert_eq!(read(vault, "b.md"), "b from phone\n");
        assert_eq!(read(vault, "new/laptop.md"), "new\n");
    }
}

/// Laptop and phone both rewrite line 3; the laptop pushes first.
fn conflicted_phone() -> (World, Vault, Vault, String) {
    let (world, laptop, phone, base) = two_devices_with_note(5);
    write(
        &laptop,
        NOTE,
        replace_line(&base, 3, "Laptop version.").as_bytes(),
    );
    let phone_text = replace_line(&base, 3, "Phone version.");
    let phone_text = replace_line(&phone_text, 1, "Phone edited line one too.");
    write(&phone, NOTE, phone_text.as_bytes());
    sync(&laptop, "laptop");
    (world, laptop, phone, base)
}

fn phone_conflict() -> (World, Vault, String, Vec<ConflictedFile>) {
    let (world, _laptop, phone, base) = conflicted_phone();
    let MergeOutcome::Conflicts(files) = sync(&phone, "phone") else {
        panic!("expected a conflict");
    };
    (world, phone, base, files)
}

#[test]
fn overlapping_edits_produce_correct_hunks() {
    let (_world, _phone, _base, files) = phone_conflict();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, PathBuf::from(NOTE));
    let hunks: Vec<_> = files[0].hunks().collect();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].base, "Line 3 of the note.\n");
    assert_eq!(hunks[0].this_device, "Phone version.\n");
    assert_eq!(hunks[0].other_device, "Laptop version.\n");
    assert_eq!(hunks[0].base_lines, 2..3);
    assert_eq!(hunks[0].this_device_lines, 2..3);
    assert_eq!(hunks[0].other_device_lines, 2..3);
}

#[test]
fn conflicted_note_shows_both_versions_on_disk() {
    let (_world, phone, base, files) = phone_conflict();
    let on_disk = read(&phone, NOTE);
    assert_eq!(on_disk, files[0].marked_text().text);
    let expected_start = "Phone edited line one too.\nLine 2 of the note.\n<<<<<<< this device\n";
    assert!(on_disk.starts_with(expected_start));
    assert!(on_disk.contains("Phone version.\n=======\nLaptop version.\n>>>>>>> other device\n"));
    assert!(on_disk.ends_with(&base[base.find("Line 4").unwrap()..]));
}

#[test]
fn paused_merge_blocks_commits_until_resolved() {
    let (_world, phone, _base, files) = phone_conflict();
    assert!(phone.is_merging());
    assert_eq!(phone.conflicts().unwrap(), files);
    let commit = phone.commit_all(&author("phone"), "should wait");
    assert!(matches!(commit, Err(SyncError::UnresolvedConflicts(1))));
    let merge = phone.merge(&author("phone")).unwrap();
    assert_eq!(merge, MergeOutcome::Conflicts(files));
}

#[test]
fn each_resolution_completes_the_merge_and_syncs() {
    let cases = [
        (Resolution::ThisDevice, "Phone version.\n"),
        (Resolution::OtherDevice, "Laptop version.\n"),
        (Resolution::Both, "Phone version.\nLaptop version.\n"),
        (
            Resolution::Merged("Both agreed.\n".into()),
            "Both agreed.\n",
        ),
    ];
    for (resolution, middle) in cases {
        let (world, laptop, phone, base) = conflicted_phone();
        let MergeOutcome::Conflicts(files) = sync(&phone, "phone") else {
            panic!("expected a conflict");
        };
        let commit = phone
            .resolve(&files[0], &[resolution], &author("phone"))
            .unwrap();
        assert!(commit.is_some());
        assert!(!phone.is_merging());
        assert_eq!(head_parent_count(&phone), 2);

        let prefix = "Phone edited line one too.\nLine 2 of the note.\n";
        let suffix = &base[base.find("Line 4").unwrap()..];
        let expected = format!("{prefix}{middle}{suffix}");
        assert_eq!(read(&phone, NOTE), expected);

        phone.push().unwrap();
        assert_eq!(
            world.remote_file("master", NOTE).unwrap(),
            expected.as_bytes()
        );
        assert_eq!(sync(&laptop, "laptop"), MergeOutcome::FastForward);
        assert_eq!(read(&laptop, NOTE), expected);
    }
}

#[test]
fn merge_commits_only_after_the_last_file_is_resolved() {
    let world = World::seeded(&[("a.md", b"shared\n"), ("b.md", b"shared\n")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    for path in ["a.md", "b.md"] {
        write(&laptop, path, b"laptop\n");
        write(&phone, path, b"phone\n");
    }
    sync(&laptop, "laptop");
    let MergeOutcome::Conflicts(files) = sync(&phone, "phone") else {
        panic!("expected conflicts");
    };
    assert_eq!(files.len(), 2);
    let first = phone.resolve(&files[0], &[Resolution::ThisDevice], &author("phone"));
    assert_eq!(first.unwrap(), None);
    assert_eq!(phone.conflicts().unwrap().len(), 1);
    let text_result = phone.resolve_with_text(&files[1].path, "typed by hand\n", &author("phone"));
    assert!(text_result.unwrap().is_some());
    assert_eq!(
        read(&phone, files[1].path.to_str().unwrap()),
        "typed by hand\n"
    );
}

#[test]
fn resolution_paths_must_stay_inside_the_vault() {
    let (_world, _laptop, phone, _) = conflicted_phone();
    sync(&phone, "phone");
    let outside = phone.resolve_with_text(Path::new("../escape.md"), "x", &author("phone"));
    assert!(matches!(outside, Err(SyncError::OutsideVault(_))));
}

#[test]
fn binary_conflict_keeps_the_local_copy() {
    let image = "attachments/diagram.png";
    let world = World::seeded(&[(image, b"\x89PNG\r\n\x1a\n base")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    write(&laptop, image, b"\x89PNG\r\n\x1a\n laptop pixels");
    write(&phone, image, b"\x89PNG\r\n\x1a\n phone pixels");
    sync(&laptop, "laptop");
    let outcome = sync(&phone, "phone");
    let MergeOutcome::Merged { kept_local, .. } = outcome else {
        panic!("expected an automatic merge, got {outcome:?}");
    };
    assert_eq!(kept_local, vec![PathBuf::from(image)]);
    assert_eq!(read_bytes(&phone, image), b"\x89PNG\r\n\x1a\n phone pixels");
    assert_eq!(
        world.remote_file("master", image).unwrap(),
        b"\x89PNG\r\n\x1a\n phone pixels"
    );
    sync(&laptop, "laptop");
    assert_eq!(
        read_bytes(&laptop, image),
        b"\x89PNG\r\n\x1a\n phone pixels"
    );
}

#[test]
fn edit_beats_delete_on_either_side() {
    let world = World::seeded(&[("kept.md", b"one\n"), ("revived.md", b"two\n")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    std::fs::remove_file(laptop.root().join("kept.md")).unwrap();
    write(&laptop, "revived.md", b"two, edited on laptop\n");
    write(&phone, "kept.md", b"one, edited on phone\n");
    std::fs::remove_file(phone.root().join("revived.md")).unwrap();
    sync(&laptop, "laptop");
    assert!(matches!(sync(&phone, "phone"), MergeOutcome::Merged { .. }));
    assert_eq!(read(&phone, "kept.md"), "one, edited on phone\n");
    assert_eq!(read(&phone, "revived.md"), "two, edited on laptop\n");
}

#[test]
fn device_only_files_never_get_committed() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    let device_only = [
        ".editor/device.toml",
        ".obsidian/workspace.json",
        ".obsidian/workspace-mobile.json",
        ".DS_Store",
        "notes/.DS_Store",
        ".trash/deleted.md",
    ];
    for path in device_only {
        write(&laptop, path, b"only on the laptop\n");
    }
    write(&laptop, ".editor/stats/laptop.json", b"{\"seconds\": 10}\n");
    write(&phone, ".editor/stats/phone.json", b"{\"seconds\": 20}\n");
    write(&laptop, ".obsidian/app.json", b"{}\n");
    sync(&laptop, "laptop");
    sync(&phone, "phone");
    sync(&laptop, "laptop");

    for path in device_only {
        assert_eq!(
            world.remote_file("master", path),
            None,
            "{path} was committed"
        );
        assert!(
            !phone.root().join(path).exists(),
            "{path} reached the phone"
        );
    }
    for path in [
        ".editor/stats/laptop.json",
        ".editor/stats/phone.json",
        ".obsidian/app.json",
    ] {
        assert!(
            world.remote_file("master", path).is_some(),
            "{path} did not sync"
        );
        assert!(laptop.root().join(path).exists());
        assert!(phone.root().join(path).exists());
    }
    let exclude = std::fs::read_to_string(laptop.root().join(".git/info/exclude")).unwrap();
    for line in [
        "/.editor/device.toml",
        "/.obsidian/workspace*.json",
        "\n.DS_Store\n",
        "/.trash/**",
    ] {
        assert!(exclude.contains(line), "exclude lacks {line}");
    }
}

#[test]
fn custom_device_only_globs_apply_even_to_unignored_paths() {
    let config = VaultConfig {
        device_only: DeviceOnlyFiles::new(&["private/**"]).unwrap(),
        ..VaultConfig::default()
    };
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device_with("laptop", config);
    std::fs::remove_file(laptop.root().join(".git/info/exclude")).unwrap();
    write(&laptop, "private/draft.md", b"secret-ish synthetic text\n");
    write(&laptop, "public.md", b"shared\n");
    sync(&laptop, "laptop");
    assert!(world.remote_file("master", "public.md").is_some());
    assert_eq!(world.remote_file("master", "private/draft.md"), None);
}

#[test]
fn nothing_to_commit_returns_none() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    assert_eq!(laptop.commit_all(&author("laptop"), "empty").unwrap(), None);
    assert_eq!(laptop.unpushed_changes().unwrap(), 0);
    write(&laptop, "note.md", b"changed\n");
    assert_eq!(laptop.unpushed_changes().unwrap(), 1);
}

#[test]
fn a_custom_branch_is_used_end_to_end() {
    let config = VaultConfig {
        branch: "trunk".into(),
        ..VaultConfig::default()
    };
    let world = World::seeded_on(config.clone(), &[("note.md", b"hello\n")]);
    let laptop = world.device_with("laptop", config.clone());
    write(&laptop, "note.md", b"hello from trunk\n");
    sync(&laptop, "laptop");
    assert_eq!(
        world.remote_file("trunk", "note.md").unwrap(),
        b"hello from trunk\n"
    );
    assert_eq!(world.remote_file("master", "note.md"), None);
}

#[test]
fn opening_a_clone_on_another_branch_is_refused() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    let root = laptop.root().to_owned();
    drop(laptop);
    let config = VaultConfig {
        branch: "other".into(),
        ..VaultConfig::default()
    };
    assert!(matches!(
        Vault::open(&root, config),
        Err(SyncError::WrongBranch { .. })
    ));
    assert!(Vault::open(&root, VaultConfig::default()).is_ok());
}

#[test]
fn a_new_vault_adopts_the_remote_history() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let fresh = Vault::init(
        world.path("fresh"),
        &world.remote_url,
        VaultConfig::default(),
    )
    .unwrap();
    fresh.fetch().unwrap();
    assert_eq!(
        fresh.merge(&author("fresh")).unwrap(),
        MergeOutcome::FastForward
    );
    assert_eq!(read(&fresh, "note.md"), "hello\n");
}

#[test]
fn line_endings_sync_byte_for_byte() {
    let crlf_note = b"first line\r\nsecond line\r\n";
    let lf_note = b"first line\nsecond line\n";
    let world = World::seeded(&[("windows.md", crlf_note), ("unix.md", lf_note)]);
    let device = world.device("laptop");

    let config = Repository::open(device.root()).unwrap().config().unwrap();
    let local = config.open_level(git2::ConfigLevel::Local).unwrap();
    assert!(!local.get_bool("core.autocrlf").unwrap());
    assert_eq!(read_bytes(&device, "windows.md"), crlf_note);
    assert_eq!(read_bytes(&device, "unix.md"), lf_note);
}
