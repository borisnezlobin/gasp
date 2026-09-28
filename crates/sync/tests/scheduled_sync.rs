mod common;

use std::time::Duration;

use common::{World, author, write};
use editor_sync::{Scheduler, SyncEventKind, SyncStatus, SyncStep, drive, run_step};

const DEVICE: &str = "laptop";

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

#[test]
fn scheduler_commits_a_minute_after_the_last_edit_then_pushes() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    let mut scheduler = Scheduler::default();
    let who = author("laptop");

    write(&laptop, "note.md", b"hello, first draft\n");
    scheduler.edited(secs(0));
    write(&laptop, "note.md", b"hello, second draft\n");
    scheduler.edited(secs(40));

    assert_eq!(
        drive(&laptop, &mut scheduler, secs(60), &who, DEVICE),
        SyncStatus::Synced
    );
    assert_eq!(world.remote_file("master", "note.md").unwrap(), b"hello\n");

    assert_eq!(
        drive(&laptop, &mut scheduler, secs(100), &who, DEVICE),
        SyncStatus::Synced
    );
    assert_eq!(
        world.remote_file("master", "note.md").unwrap(),
        b"hello, second draft\n"
    );
    assert_eq!(
        common::remote_head_message(&world, "master"),
        "laptop: note.md"
    );
    let kinds: Vec<_> = scheduler.log().map(|event| event.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            SyncEventKind::Started,
            SyncEventKind::Committed,
            SyncEventKind::Fetched,
            SyncEventKind::UpToDate,
            SyncEventKind::Pushed
        ]
    );
}

#[test]
fn push_failure_keeps_commits_local_and_reports_offline() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    let who = author("laptop");
    let real_url = laptop.remote_url().unwrap();

    write(&laptop, "note.md", b"written on a plane\n");
    write(&laptop, "second.md", b"also on a plane\n");
    let commit = laptop.commit_all(&who, DEVICE).unwrap().unwrap();
    laptop.fetch().unwrap();
    laptop.merge(&who).unwrap();
    laptop
        .set_remote_url(&world.path("unreachable.git").to_string_lossy())
        .unwrap();

    let error = laptop.push().unwrap_err();
    assert!(error.is_offline(), "expected offline, got {error:?}");
    assert_eq!(laptop.head_commit().unwrap(), Some(commit));
    assert_eq!(laptop.unpushed_changes().unwrap(), 2);
    assert_eq!(world.remote_file("master", "note.md").unwrap(), b"hello\n");

    let report = run_step(&laptop, SyncStep::Push, &who, DEVICE);
    let mut scheduler = Scheduler::default();
    scheduler.request_sync();
    scheduler.poll(secs(0));
    for step_report in [
        editor_sync::StepReport::Committed { new_commit: false },
        editor_sync::StepReport::Fetched,
        editor_sync::StepReport::Merged(editor_sync::MergeReport::UpToDate),
        report,
    ] {
        scheduler.report(secs(1), step_report);
    }
    assert_eq!(scheduler.status(), SyncStatus::Offline { waiting: 2 });

    laptop.set_remote_url(&real_url).unwrap();
    assert_eq!(
        drive(&laptop, &mut scheduler, secs(30), &who, DEVICE),
        SyncStatus::Offline { waiting: 2 }
    );
    assert_eq!(
        drive(&laptop, &mut scheduler, secs(61), &who, DEVICE),
        SyncStatus::Synced
    );
    assert_eq!(
        world.remote_file("master", "note.md").unwrap(),
        b"written on a plane\n"
    );
    assert_eq!(laptop.unpushed_changes().unwrap(), 0);
}

#[test]
fn unreachable_remote_during_a_scheduled_sync_reports_offline() {
    let world = World::seeded(&[("note.md", b"hello\n")]);
    let laptop = world.device("laptop");
    let who = author("laptop");
    laptop
        .set_remote_url(&world.path("missing.git").to_string_lossy())
        .unwrap();
    let mut scheduler = Scheduler::default();
    write(&laptop, "note.md", b"offline edit\n");
    scheduler.edited(secs(0));
    let status = drive(&laptop, &mut scheduler, secs(60), &who, DEVICE);
    assert_eq!(status, SyncStatus::Offline { waiting: 1 });
    assert!(laptop.head_commit().unwrap().is_some());
    assert!(
        scheduler
            .log()
            .any(|event| matches!(event.kind, SyncEventKind::Offline { .. }))
    );
}

#[test]
fn scheduled_sync_keeps_syncing_other_notes_while_a_conflict_waits() {
    let world = World::seeded(&[("note.md", b"shared line\n"), ("other.md", b"other\n")]);
    let laptop = world.device("laptop");
    let phone = world.device("phone");
    write(&laptop, "note.md", b"laptop line\n");
    common::sync(&laptop, "laptop");

    let who = author("phone");
    let mut scheduler = Scheduler::default();
    write(&phone, "note.md", b"phone line\n");
    scheduler.edited(secs(0));
    let status = drive(&phone, &mut scheduler, secs(60), &who, "phone");
    assert_eq!(status, SyncStatus::Conflict { files: 1 });
    assert!(!phone.is_merging());

    write(&phone, "other.md", b"other, edited offline\n");
    scheduler.edited(secs(70));
    let status = drive(&phone, &mut scheduler, secs(130), &who, "phone");
    assert_eq!(status, SyncStatus::Conflict { files: 1 });
    assert_eq!(
        world.remote_file("master", "other.md").unwrap(),
        b"other, edited offline\n"
    );
    assert_eq!(
        common::remote_head_message(&world, "master"),
        "phone: other.md"
    );

    let files = phone.conflicts().unwrap();
    phone
        .resolve(&files[0], &[editor_sync::Resolution::Both])
        .unwrap();
    scheduler.conflicts_resolved(secs(140));
    assert_eq!(
        drive(&phone, &mut scheduler, secs(140), &who, "phone"),
        SyncStatus::Synced
    );
    assert_eq!(
        world.remote_file("master", "note.md").unwrap(),
        b"phone line\nlaptop line\n"
    );
    assert_eq!(
        common::remote_head_message(&world, "master"),
        "phone: note.md"
    );
}
