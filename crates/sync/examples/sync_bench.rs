//! Times the sync steps the apps run, on a vault shaped like the owner's:
//! about 570 files and 250 MB, with the corpus's 204 notes, a hundred-odd
//! photos, two big plugin binaries, `.obsidian` and `.gasp` config, and a
//! second clone standing in for the phone. Everything lives in a scratch
//! folder that's removed at the end.
//!
//! `cargo run --release -p gasp-sync --example sync_bench [-- --quick]`
//!
//! `--quick` (or `GASP_SYNC_BENCH_QUICK=1`) keeps every file but makes the
//! binaries a tenth of their size, for a fast look while working. The
//! budgets are about three times what an M-series Mac measures at full
//! size, for slower CI machines.

use std::fs;
use std::path::Path;
use std::time::Duration;

use gasp_bench::corpus::{ScratchDir, copy_corpus, write_binary_file};
use gasp_bench::{AllocStats, CountingAllocator, Report, Samples};
use gasp_sync::{
    Author, MergeOutcome, Scheduler, StepReport, SyncStep, Vault, VaultConfig, run_step,
};
use git2::{Repository, RepositoryInitOptions};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const DEVICE: &str = "mac";
const PHONE: &str = "phone";
const NEW_PHOTOS: usize = 118;
const SMALLEST_PHOTO: usize = 50 * 1024;
const LARGEST_PHOTO: usize = 3 * 1024 * 1024;
const MCP_SERVER_BYTES: usize = 58 * 1024 * 1024;
const HARPER_BYTES: usize = 19 * 1024 * 1024;
const PASTED_PHOTO_BYTES: usize = 2 * 1024 * 1024;
const EDITED_NOTE: &str = "Habit Ideas.md";
const PHONE_NOTE: &str = "Derivation Ideas.md";
const HISTORY_NOTE: &str = "Daily Notes/Log.md";
const HISTORY_COMMITS: usize = 500;

/// How big the vault is and how often each step runs.
#[derive(Clone, Copy)]
struct Shape {
    /// Binaries are this fraction of their real size.
    byte_scale: f64,
    runs: usize,
    rounds: usize,
}

impl Shape {
    fn from_environment() -> Self {
        let quick = std::env::args().any(|argument| argument == "--quick")
            || std::env::var("GASP_SYNC_BENCH_QUICK").is_ok_and(|value| value == "1");
        Self {
            byte_scale: if quick { 0.1 } else { 1. },
            runs: 15,
            rounds: 8,
        }
    }

    fn bytes(&self, real: usize) -> usize {
        (real as f64 * self.byte_scale) as usize
    }
}

/// The owner's Mac and phone, each with a clone, and the bare "GitHub" repo.
struct Devices {
    mac: Vault,
    phone: Vault,
    mac_author: Author,
    phone_author: Author,
}

fn main() {
    let shape = Shape::from_environment();
    let scratch = ScratchDir::new("sync");
    let mut report = Report::new("Sync on a vault shaped like the owner's");
    let devices = set_up(scratch.path(), shape, &mut report);
    measure_quiet_vault(&devices, shape, &mut report);
    measure_local_edits(&devices, shape, &mut report);
    measure_remote_edits(&devices, shape, &mut report);
    measure_diverged_merges(&devices, shape, &mut report);
    measure_pasted_photos(&devices, shape, &mut report);
    drop(devices);
    drop(scratch);
    report.finish();
}

fn vault_config() -> VaultConfig {
    VaultConfig::default()
}

fn set_up(root: &Path, shape: Shape, report: &mut Report) -> Devices {
    let remote = init_bare_remote(&root.join("remote.git"));
    let mac_root = root.join(DEVICE);
    let mac = Vault::init(&mac_root, &remote, vault_config()).expect("the Mac's vault starts");
    write_vault(&mac_root, shape);
    let mac_author = Author::new("Mac", "mac@devices.invalid");
    let mut first_commit = Samples::new();
    first_commit.time(|| mac.commit_changes(&mac_author, DEVICE).expect("commit"));
    backdate_first_commit(&mac_root);
    let mut first_push = Samples::new();
    first_push.time(|| mac.push().expect("first push"));
    start_legacy_branch(&remote);
    write_history(&mac);
    let (files, bytes) = committed_size(&mac_root);
    let mut clone = Samples::new();
    let phone = clone.time(|| {
        Vault::clone_remote(&remote, root.join(PHONE), vault_config(), None).expect("clone")
    });
    report.note_count("files in the vault", files as f64);
    report.note_bytes("bytes in the vault", bytes as f64);
    report.note_time("first commit of the whole vault", first_commit.median());
    report.time(
        "first push of the whole vault",
        first_push.median(),
        Duration::from_secs(60),
    );
    report.note_time("clone onto the phone", clone.median());
    report.note_count(
        "commits since the legacy branch last moved",
        HISTORY_COMMITS as f64,
    );
    Devices {
        mac,
        phone,
        mac_author,
        phone_author: Author::new("Phone", "phone@devices.invalid"),
    }
}

/// Months of syncing since the old tools last pushed to the legacy branch:
/// a commit a minute for each of a few hundred edits, the last a minute
/// ago, then pushed. Git's history walks stop by commit date, so the dates
/// have to advance as real ones do.
fn write_history(mac: &Vault) {
    let repo = Repository::open(mac.root()).expect("the vault opens");
    let mut index = repo.index().expect("the index reads");
    for edit in 0..HISTORY_COMMITS {
        append_line(mac.root(), HISTORY_NOTE, &format!("Entry {edit}."));
        index.add_path(Path::new(HISTORY_NOTE)).expect("staged");
        let tree = repo.find_tree(index.write_tree().expect("a tree"));
        let parent = repo.head().and_then(|head| head.peel_to_commit());
        let signature = minutes_ago(HISTORY_COMMITS - edit);
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "mac: Log.md",
            &tree.expect("the tree"),
            &[&parent.expect("a parent")],
        )
        .expect("committed");
    }
    index.write().expect("the index writes");
    mac.push().expect("push");
}

/// Dates the first commit before the history that follows it.
fn backdate_first_commit(root: &Path) {
    let repo = Repository::open(root).expect("the vault opens");
    let first = repo.head().and_then(|head| head.peel_to_commit());
    let signature = minutes_ago(HISTORY_COMMITS + 1);
    let when = Some(&signature);
    first
        .and_then(|first| first.amend(Some("HEAD"), when, when, None, None, None))
        .expect("the first commit is dated");
}

fn minutes_ago(minutes: usize) -> git2::Signature<'static> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs() as i64;
    let when = git2::Time::new(now - minutes as i64 * 60, 0);
    git2::Signature::new("Mac", "mac@devices.invalid", &when).expect("a signature")
}

fn init_bare_remote(path: &Path) -> String {
    let mut options = RepositoryInitOptions::new();
    options.bare(true).initial_head(&vault_config().branch);
    Repository::init_opts(path, &options).expect("the bare remote starts");
    path.to_str().expect("a UTF-8 scratch path").to_owned()
}

/// The owner's remote still has the branch the old sync tools push to.
fn start_legacy_branch(remote: &str) {
    let config = vault_config();
    let legacy = config.legacy_branch.expect("a legacy branch");
    let repo = Repository::open_bare(remote).expect("the bare remote opens");
    let head = repo
        .refname_to_id(&format!("refs/heads/{}", config.branch))
        .expect("the synced branch exists");
    repo.reference(&format!("refs/heads/{legacy}"), head, true, "legacy")
        .expect("the legacy branch starts");
}

fn write_vault(root: &Path, shape: Shape) {
    copy_corpus(root, 1);
    write_config(root, shape);
    write_photos(root, shape);
    write_device_only_files(root);
}

fn write_config(root: &Path, shape: Shape) {
    let small_files = [
        (
            ".gitignore",
            ".DS_Store\n.trash/\n.obsidian/workspace*.json\n",
        ),
        (
            ".gasp/settings.toml",
            "[sync]\nauto = true\nbranch = \"master\"\n",
        ),
        (".gasp/stats/mac.json", "{\"edited_seconds\": 3600}\n"),
        (
            ".obsidian/app.json",
            "{\"attachmentFolderPath\": \"./images\"}\n",
        ),
        (".obsidian/appearance.json", "{\"theme\": \"obsidian\"}\n"),
        (
            ".obsidian/core-plugins.json",
            "[\"file-explorer\", \"switcher\"]\n",
        ),
        (
            ".obsidian/community-plugins.json",
            "[\"mcp-tools\", \"harper\"]\n",
        ),
        (
            ".obsidian/plugins/mcp-tools/manifest.json",
            "{\"id\": \"mcp-tools\"}\n",
        ),
        (".obsidian/plugins/mcp-tools/data.json", "{}\n"),
        (
            ".obsidian/plugins/harper/manifest.json",
            "{\"id\": \"harper\"}\n",
        ),
        (".obsidian/plugins/harper/data.json", "{}\n"),
    ];
    for (path, text) in small_files {
        write_text(&root.join(path), text);
    }
    let plugins = root.join(".obsidian/plugins");
    let mcp_server = plugins.join("mcp-tools/bin/mcp-server");
    write_binary_file(&mcp_server, shape.bytes(MCP_SERVER_BYTES), 1);
    write_binary_file(
        &plugins.join("harper/main.js"),
        shape.bytes(HARPER_BYTES),
        2,
    );
}

/// Photos next to the notes that embed them, 50 KB to 3 MB.
fn write_photos(root: &Path, shape: Shape) {
    let folders = [
        "images",
        "Course Notes/images",
        "Daily Notes/images",
        "Essays/images",
    ];
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    for index in 0..NEW_PHOTOS {
        state = next_random(state);
        let span = (LARGEST_PHOTO - SMALLEST_PHOTO) as u64;
        let size = SMALLEST_PHOTO + (state % span) as usize;
        let folder = folders[index % folders.len()];
        let path = root.join(format!("{folder}/Photo {index:03}.jpg"));
        write_binary_file(&path, shape.bytes(size), index as u64 + 10);
    }
}

fn next_random(mut state: u64) -> u64 {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    state
}

/// Files a Mac keeps that never sync: ignored, or device-only.
fn write_device_only_files(root: &Path) {
    write_text(&root.join(".gasp/device.toml"), "name = \"mac\"\n");
    write_text(&root.join(".obsidian/workspace.json"), "{\"main\": {}}\n");
    for folder in ["", "Essays/", "Daily Notes/", "images/"] {
        write_text(&root.join(format!("{folder}.DS_Store")), "finder");
    }
    for index in 0..20 {
        let path = root.join(format!(".trash/Deleted note {index}.md"));
        write_text(&path, "An old note.\n");
    }
}

fn write_text(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("folders are created");
    fs::write(path, text).expect("the file writes");
}

/// How many files the vault's head commit holds, and their total size.
fn committed_size(root: &Path) -> (usize, u64) {
    let repo = Repository::open(root).expect("the vault opens");
    let tree = repo.head().and_then(|head| head.peel_to_tree());
    let tree = tree.expect("a head commit");
    let (mut files, mut bytes) = (0, 0);
    tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
        let blob = entry
            .to_object(&repo)
            .and_then(|object| object.peel_to_blob());
        if let Ok(blob) = blob {
            files += 1;
            bytes += blob.size() as u64;
        }
        git2::TreeWalkResult::Ok
    })
    .expect("the tree walks");
    (files, bytes)
}

/// The sync steps as the apps run them: each step, then the head and
/// tracking commits, the paths a merge or push changed, and the waiting
/// conflicts, as `apps/desktop/src/sync/engine.rs` does.
fn app_step(vault: &Vault, step: SyncStep, author: &Author) -> StepReport {
    let before = match step {
        SyncStep::Merge => vault.head_commit().ok().flatten(),
        SyncStep::Push => vault.tracking_commit(),
        SyncStep::Commit | SyncStep::Fetch => None,
    };
    let report = run_step(vault, step, author, DEVICE);
    assert!(
        !matches!(report, StepReport::Failed(_)),
        "{step:?} failed: {report:?}"
    );
    if matches!(step, SyncStep::Merge | SyncStep::Push) {
        let after = vault.head_commit().ok().flatten();
        std::hint::black_box(vault.changed_paths(before, after).expect("changed paths"));
    }
    if matches!(step, SyncStep::Commit | SyncStep::Merge) {
        std::hint::black_box(vault.conflicts().expect("conflicts"));
    }
    report
}

/// One whole sync as the app runs it when asked: commit, fetch, merge, push.
fn app_sync(vault: &Vault, author: &Author) {
    let mut scheduler = Scheduler::default();
    scheduler.request_sync();
    let mut next = scheduler.poll(Duration::ZERO);
    while let Some(step) = next {
        let report = app_step(vault, step, author);
        next = scheduler.report(Duration::ZERO, report);
    }
}

/// Nothing changed anywhere: what every foreground and idle poll costs.
fn measure_quiet_vault(devices: &Devices, shape: Shape, report: &mut Report) {
    let Devices {
        mac, mac_author, ..
    } = devices;
    app_sync(mac, mac_author);
    let root = mac.root().to_path_buf();
    let open = Samples::collect(shape.runs, || {
        Vault::open(&root, vault_config()).expect("the vault opens")
    });
    let status = Samples::collect(shape.runs, || mac.unpushed_changes().expect("status"));
    let commit = Samples::collect(shape.runs, || {
        let commit = mac.commit_changes(mac_author, DEVICE).expect("commit");
        assert_eq!(commit, None, "nothing to commit");
    });
    let fetch = Samples::collect(shape.runs, || mac.fetch().expect("fetch"));
    let merge = Samples::collect(shape.runs, || {
        let outcome = mac.merge(mac_author).expect("merge");
        assert_eq!(outcome, MergeOutcome::UpToDate);
    });
    let push = Samples::collect(shape.runs, || mac.push().expect("push"));
    let cycle = Samples::collect(shape.runs, || app_sync(mac, mac_author));
    let allocations = cycle_allocations(mac, mac_author);
    report.time("open the vault", open.median(), ms(1));
    report.time("status, nothing changed", status.median(), ms(8));
    report.time("commit, nothing changed", commit.median(), ms(8));
    report.time("fetch, nothing new", fetch.median(), ms(3));
    report.time("merge, up to date", merge.median(), ms(1));
    report.time("push, nothing new", push.median(), ms(8));
    report.time("sync with nothing to do", cycle.median(), ms(10));
    report.note_time("sync with nothing to do, p95", cycle.p95());
    let (per_cycle, bytes_per_cycle) = allocations.per_operation(ALLOCATION_RUNS);
    report.count("allocations per sync with nothing to do", per_cycle, 200.);
    report.note_bytes(
        "bytes allocated per sync with nothing to do",
        bytes_per_cycle,
    );
}

const ALLOCATION_RUNS: usize = 5;

fn cycle_allocations(vault: &Vault, author: &Author) -> AllocStats {
    let ((), stats) = CountingAllocator::measure(|| {
        for _ in 0..ALLOCATION_RUNS {
            app_sync(vault, author);
        }
    });
    stats
}

/// One note edited on the Mac: the status, the commit and the push.
fn measure_local_edits(devices: &Devices, shape: Shape, report: &mut Report) {
    let Devices {
        mac, mac_author, ..
    } = devices;
    let mut status = Samples::new();
    let mut commit = Samples::new();
    let mut push = Samples::new();
    let mut cycle = Samples::new();
    for run in 0..shape.runs {
        append_line(mac.root(), EDITED_NOTE, &format!("Edit {run} on the Mac."));
        let waiting = status.time(|| mac.unpushed_changes().expect("status"));
        assert_eq!(waiting, 1, "one note waits");
        let made = commit.time(|| mac.commit_changes(mac_author, DEVICE).expect("commit"));
        assert!(made.is_some(), "the edit is committed");
        push.time(|| mac.push().expect("push"));
        append_line(
            mac.root(),
            EDITED_NOTE,
            &format!("Edit {run} synced at once."),
        );
        cycle.time(|| app_sync(mac, mac_author));
    }
    report.time("status, one note edited", status.median(), ms(8));
    report.time("commit one edited note", commit.median(), ms(16));
    report.time("push one commit", push.median(), ms(20));
    report.time("sync one edited note", cycle.median(), ms(45));
}

/// The phone pushed a note: the Mac fetches it and fast-forwards.
fn measure_remote_edits(devices: &Devices, shape: Shape, report: &mut Report) {
    let Devices {
        mac,
        phone,
        mac_author,
        phone_author,
        ..
    } = devices;
    catch_up(phone, phone_author);
    let mut fetch = Samples::new();
    let mut merge = Samples::new();
    for round in 0..shape.rounds {
        append_line(phone.root(), PHONE_NOTE, &format!("Phone edit {round}."));
        send(phone, phone_author);
        fetch.time(|| mac.fetch().expect("fetch"));
        let outcome = merge.time(|| mac.merge(mac_author).expect("merge"));
        assert_eq!(outcome, MergeOutcome::FastForward);
    }
    report.time("fetch one new commit", fetch.median(), ms(18));
    report.time("merge, fast-forward of one note", merge.median(), ms(12));
}

/// Both devices edited different notes: a real merge commit.
fn measure_diverged_merges(devices: &Devices, shape: Shape, report: &mut Report) {
    let Devices {
        mac,
        phone,
        mac_author,
        phone_author,
        ..
    } = devices;
    catch_up(phone, phone_author);
    let mut merge = Samples::new();
    let mut push = Samples::new();
    for round in 0..shape.rounds {
        append_line(phone.root(), PHONE_NOTE, &format!("Phone line {round}."));
        send(phone, phone_author);
        append_line(mac.root(), EDITED_NOTE, &format!("Mac line {round}."));
        mac.commit_changes(mac_author, DEVICE).expect("commit");
        mac.fetch().expect("fetch");
        let outcome = merge.time(|| mac.merge(mac_author).expect("merge"));
        assert!(
            matches!(outcome, MergeOutcome::Merged { .. }),
            "{outcome:?}"
        );
        push.time(|| mac.push().expect("push"));
        catch_up(phone, phone_author);
    }
    report.time("merge, both devices edited", merge.median(), ms(25));
    report.time("push a merge commit", push.median(), ms(25));
}

/// A 2 MB photo pasted into a note on the Mac, then brought to the phone.
fn measure_pasted_photos(devices: &Devices, shape: Shape, report: &mut Report) {
    let Devices {
        mac,
        phone,
        mac_author,
        phone_author,
        ..
    } = devices;
    let rounds = shape.rounds / 2;
    let mut commit = Samples::new();
    let mut push = Samples::new();
    let mut merge = Samples::new();
    for round in 0..rounds {
        let photo = format!("images/Pasted image {round}.png");
        let bytes = shape.bytes(PASTED_PHOTO_BYTES);
        write_binary_file(&mac.root().join(&photo), bytes, 1000 + round as u64);
        append_line(mac.root(), EDITED_NOTE, &format!("![[{photo}]]"));
        commit.time(|| mac.commit_changes(mac_author, DEVICE).expect("commit"));
        push.time(|| mac.push().expect("push"));
        phone.fetch().expect("fetch");
        merge.time(|| phone.merge(phone_author).expect("merge"));
    }
    report.time("commit a pasted 2 MB photo", commit.median(), ms(150));
    report.time("push a pasted 2 MB photo", push.median(), ms(250));
    report.time(
        "fast-forward bringing in a 2 MB photo",
        merge.median(),
        ms(25),
    );
}

/// Fetches and merges whatever the other device sent, untimed.
fn catch_up(vault: &Vault, author: &Author) {
    vault.fetch().expect("fetch");
    vault.merge(author).expect("merge");
}

/// Commits and pushes a device's edits, untimed.
fn send(vault: &Vault, author: &Author) {
    vault.commit_changes(author, PHONE).expect("commit");
    vault.push().expect("push");
}

fn append_line(root: &Path, note: &str, line: &str) {
    let path = root.join(note);
    let mut text = fs::read_to_string(&path).unwrap_or_default();
    text.push_str(line);
    text.push('\n');
    fs::write(&path, text).expect("the note writes");
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}
