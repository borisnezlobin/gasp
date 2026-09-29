//! Times the sync steps on a synthetic vault of about 200 notes.
//! Run with `cargo test -p gasp-sync --release --test timing -- --nocapture`.

mod common;

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use common::{World, author, read, write};
use gasp_sync::{MergeOutcome, Vault, VaultConfig};

const NOTE_COUNT: usize = 200;
const EDITED_PER_DEVICE: usize = 20;

fn synthetic_note(index: usize) -> String {
    let mut text = format!("# Synthetic note {index}\n\n");
    for paragraph in 0..12 {
        let _ = writeln!(
            text,
            "Paragraph {paragraph} of note {index} talks about topic {} with $x_{paragraph}^2$ math.\n",
            (index * 7 + paragraph) % 31
        );
    }
    text.push_str("- [ ] a task\n- [x] a finished task\n");
    text
}

fn note_path(index: usize) -> String {
    format!("folder-{}/note-{index:03}.md", index % 10)
}

fn timed<T>(label: &str, timings: &mut Vec<(String, Duration)>, work: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = work();
    timings.push((label.to_owned(), start.elapsed()));
    result
}

/// Rewrites the first line of `edited` notes, starting at `first`.
fn edit_first_lines(vault: &Vault, first: usize, device: &str) {
    for index in first..first + EDITED_PER_DEVICE {
        let path = note_path(index);
        let text = read(vault, &path).replacen("# Synthetic", &format!("# Edited on {device}:"), 1);
        write(vault, &path, text.as_bytes());
    }
}

/// Appends a line to the end of `shared`, so both devices touch the same note in different places.
fn append_line(vault: &Vault, path: &str, line: &str) {
    let mut text = read(vault, path);
    text.push_str(line);
    write(vault, path, text.as_bytes());
}

#[test]
fn timing_report_for_two_hundred_notes() {
    let notes: Vec<(String, Vec<u8>)> = (0..NOTE_COUNT)
        .map(|index| (note_path(index), synthetic_note(index).into_bytes()))
        .collect();
    let files: Vec<(&str, &[u8])> = notes
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_slice()))
        .collect();
    let mut timings = Vec::new();

    let world = timed("seed: init + commit + push 200 notes", &mut timings, || {
        World::seeded(&files)
    });
    let laptop = timed("clone (laptop)", &mut timings, || world.device("laptop"));
    let phone = timed("clone (phone)", &mut timings, || world.device("phone"));

    edit_first_lines(&laptop, 0, "laptop");
    edit_first_lines(&phone, 100, "phone");
    append_line(&phone, &note_path(0), "Appended on the phone.\n");
    let laptop_author = author("laptop");
    let phone_author = author("phone");
    timed("commit 20 edited notes", &mut timings, || {
        laptop.commit_all(&laptop_author, "laptop edits").unwrap()
    });
    timed("push (fast-forward)", &mut timings, || {
        laptop.push().unwrap()
    });
    timed(
        "commit 20 edited notes (other device)",
        &mut timings,
        || phone.commit_all(&phone_author, "phone edits").unwrap(),
    );
    timed("fetch", &mut timings, || phone.fetch().unwrap());
    let outcome = timed(
        "merge (diverged, 40 notes, one shared)",
        &mut timings,
        || phone.merge(&phone_author).unwrap(),
    );
    assert!(
        matches!(outcome, MergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    timed("push (merge commit)", &mut timings, || {
        phone.push().unwrap()
    });
    timed("commit with nothing changed", &mut timings, || {
        phone.commit_all(&phone_author, "noop").unwrap()
    });
    timed("fetch + fast-forward (laptop)", &mut timings, || {
        laptop.fetch().unwrap();
        laptop.merge(&laptop_author).unwrap()
    });
    let opened = timed("open existing clone", &mut timings, || {
        Vault::open(laptop.root(), VaultConfig::default()).unwrap()
    });

    let shared = read(&opened, &note_path(0));
    assert!(shared.starts_with("# Edited on laptop:"));
    assert!(shared.ends_with("Appended on the phone.\n"));

    println!("\nSync timings, {NOTE_COUNT} synthetic notes:");
    for (label, elapsed) in &timings {
        println!("  {label:<42} {:>8.2} ms", elapsed.as_secs_f64() * 1000.0);
    }
}
