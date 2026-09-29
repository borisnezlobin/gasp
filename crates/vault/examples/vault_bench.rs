//! Times the vault's link index the way the apps use it: listing and
//! reading a vault into the index, what the index keeps in memory,
//! following one file the watcher reports, answering backlinks, outgoing
//! links, tags and unlinked mentions, and renaming a note and an
//! attachment with every link to them rewritten, on disk as the iPhone
//! and the MCP server do and in memory as the desktop does. It runs on
//! the synthetic corpus and on a vault of five copies of it, which has
//! over a thousand images.
//!
//! Every time is the best of several runs, so other work on the machine
//! doesn't make it noisy.
//!
//! `cargo run --release -p gasp-vault --example vault_bench`
//!
//! Set `GASP_BENCH_ENFORCE=1` to fail when a line is over its budget.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gasp_bench::corpus::{ScratchDir, copy_corpus};
use gasp_bench::{CountingAllocator, Report, Samples};
use gasp_vault::build::{LinkChange, build_index, read_changes, scan};
use gasp_vault::index::LinkIndex;
use gasp_vault::link_update::{LinkUpdater, expand_folder_move};
use gasp_vault::mentions::MentionSearch;
use gasp_vault::ops::{rename, vault_files};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// The larger vault is this many copies of the corpus.
const COPIES: usize = 5;
/// Runs of each measured operation; the best one counts.
const RUNS: usize = 10;
/// The note the watcher sees change.
const EDITED_NOTE: &str = "Lemma.md";
/// A note other notes link to, renamed with its links.
const LINKED_NOTE: &str = "Daily Notes/2024-01-13.md";
/// An image notes embed, renamed with its embeds.
const EMBEDDED_IMAGE: &str = "images/quote-27.png";
/// A note whose title other notes mention without linking.
const MENTIONED_NOTE: &str = "Wave.md";

/// A vault on disk and its link index.
struct Vault {
    scratch: ScratchDir,
    index: LinkIndex,
    label: &'static str,
}

impl Vault {
    fn new(label: &'static str, copies: usize) -> Vault {
        let scratch = ScratchDir::new(&format!("vault-{label}"));
        copy_corpus(scratch.path(), copies);
        let index = build_index(scratch.path());
        Vault {
            scratch,
            index,
            label,
        }
    }

    fn root(&self) -> &Path {
        self.scratch.path()
    }
}

fn ms(millis: f64) -> Duration {
    Duration::from_secs_f64(millis / 1000.)
}

fn best_of<T>(operation: impl FnMut() -> T) -> Duration {
    Samples::collect(RUNS, operation).min()
}

/// Listing the vault, building the index and what the index keeps.
fn report_loading(report: &mut Report, vault: &Vault, budgets: [f64; 3]) {
    let root = vault.root();
    let label = vault.label;
    report.time(
        format!("list files ({label})"),
        best_of(|| scan(root, root)),
        ms(budgets[0]),
    );
    report.time(
        format!("build the link index ({label})"),
        best_of(|| build_index(root)),
        ms(budgets[1]),
    );
    let (index, alloc) = CountingAllocator::measure(|| build_index(root));
    report.bytes(
        format!("memory kept by the link index ({label})"),
        alloc.retained_bytes as f64,
        budgets[2],
    );
    report.note_count(format!("  notes ({label})"), index.note_count() as f64);
    report.note_count(
        format!("  files ({label})"),
        index.file_paths().len() as f64,
    );
}

/// What the desktop does when the watcher reports one file: read it, and
/// hand the index what it found.
fn watcher_event(index: &mut LinkIndex, root: &Path, path: &Path) {
    for change in read_changes(root, &[path.to_path_buf()]) {
        match change {
            LinkChange::Note(path, text, parsed) => index.set_parsed_note(&path, text, parsed),
            LinkChange::File(path) => index.add_file(&path),
        }
    }
}

fn report_watcher(report: &mut Report, vault: &mut Vault, budgets: [f64; 3]) {
    let root = vault.root().to_path_buf();
    let label = vault.label;
    let edited = root.join(EDITED_NOTE);
    let original = std::fs::read_to_string(&edited).expect("the note reads");
    let mut samples = Samples::new();
    for round in 0..RUNS {
        let text =
            format!("{original}\nEdit {round}: see [[Wave]] and ![[quote-27.png]] #edited\n");
        std::fs::write(&edited, text).expect("the note writes");
        samples.time(|| watcher_event(&mut vault.index, &root, &edited));
    }
    std::fs::write(&edited, &original).expect("the note writes back");
    watcher_event(&mut vault.index, &root, &edited);
    report.time(
        format!("watcher: a note changed ({label})"),
        samples.min(),
        ms(budgets[0]),
    );
    let image = root.join("images/new-image.png");
    std::fs::copy(root.join(EMBEDDED_IMAGE), &image).expect("the image copies");
    let mut added = Samples::new();
    let mut removed = Samples::new();
    for _ in 0..RUNS {
        added.time(|| watcher_event(&mut vault.index, &root, &image));
        removed.time(|| vault.index.remove("images/new-image.png"));
    }
    std::fs::remove_file(&image).expect("the image goes");
    report.time(
        format!("watcher: an image added ({label})"),
        added.min(),
        ms(budgets[1]),
    );
    report.time(
        format!("watcher: a file removed ({label})"),
        removed.min(),
        ms(budgets[2]),
    );
}

/// Backlinks and outgoing links of every file, as the sidebar asks for
/// them when a note opens, then the vault-wide lists.
fn report_queries(report: &mut Report, vault: &Vault, budgets: [f64; 3]) {
    let index = &vault.index;
    let label = vault.label;
    let mut files = index.file_paths();
    files.sort();
    let backlinks = best_of(|| {
        files
            .iter()
            .map(|file| index.backlinks(file).len())
            .sum::<usize>()
    });
    report.time(
        format!("backlinks of every file ({label})"),
        backlinks,
        ms(budgets[0]),
    );
    let outgoing = best_of(|| {
        files
            .iter()
            .filter_map(|file| index.note(file))
            .map(|entry| entry.resolved.iter().flatten().count())
            .sum::<usize>()
    });
    report.note_time(format!("outgoing links of every note ({label})"), outgoing);
    let lists = best_of(|| (index.unresolved().len(), index.tags().len()));
    report.time(
        format!("unresolved links and tags ({label})"),
        lists,
        ms(budgets[1]),
    );
    let tagged = best_of(|| index.notes_tagged("lecture").len());
    report.time(
        format!("notes with a tag ({label})"),
        tagged,
        ms(budgets[2]),
    );
}

fn report_mentions(report: &mut Report, vault: &Vault, budget: f64) {
    let label = vault.label;
    let mentions = best_of(|| MentionSearch::new(&vault.index, MENTIONED_NOTE).run(200));
    report.time(
        format!("unlinked mentions of a note ({label})"),
        mentions,
        ms(budget),
    );
}

/// The desktop's rename: the files from the index, the notes that link to
/// what moved, rewritten in memory, and the index following the move.
fn desktop_rename(index: &mut LinkIndex, from: &str, to: &str) -> usize {
    let files = index.file_paths();
    let mut moves = expand_folder_move(&files, from, to);
    moves.push((from.to_string(), to.to_string()));
    let updater = LinkUpdater::new(&files, &moves);
    let rewritten = index
        .linking_to(from)
        .iter()
        .filter_map(|source| updater.rewrite(source, &index.note(source)?.text))
        .count();
    index.rename(from, to);
    rewritten
}

/// The best of several desktop renames, each moved back untimed.
fn best_desktop_rename(index: &mut LinkIndex, from: &str, to: &str) -> Duration {
    let mut samples = Samples::new();
    for _ in 0..RUNS {
        samples.time(|| desktop_rename(index, from, to));
        index.rename(to, from);
    }
    samples.min()
}

fn report_desktop_rename(report: &mut Report, vault: &mut Vault, budget: f64) {
    let label = vault.label;
    let index = &mut vault.index;
    let note = best_desktop_rename(index, LINKED_NOTE, "Daily Notes/Renamed.md");
    report.time(
        format!("rename a note in memory ({label})"),
        note,
        ms(budget),
    );
    let image = best_desktop_rename(index, EMBEDDED_IMAGE, "images/renamed.png");
    report.note_time(format!("rename an image in memory ({label})"), image);
}

/// The iPhone's and the MCP server's rename: on disk, every note checked
/// for links. Each run moves the file there and back.
fn disk_rename(root: &Path, from: &str, to: &str) -> Duration {
    let (from, to) = (PathBuf::from(from), PathBuf::from(to));
    let mut samples = Samples::new();
    for _ in 0..RUNS / 2 {
        samples.time(|| rename(root, &from, &to, true).expect("the rename works"));
        samples.time(|| rename(root, &to, &from, true).expect("the rename back works"));
    }
    samples.min()
}

fn report_disk_rename(report: &mut Report, vault: &Vault, budgets: [f64; 2]) {
    let root = vault.root();
    let label = vault.label;
    report.time(
        format!("rename a note on disk ({label})"),
        disk_rename(root, LINKED_NOTE, "Daily Notes/Renamed.md"),
        ms(budgets[0]),
    );
    report.time(
        format!("rename an image on disk ({label})"),
        disk_rename(root, EMBEDDED_IMAGE, "images/renamed.png"),
        ms(budgets[1]),
    );
    report.note_time(
        format!("  of which listing the vault ({label})"),
        best_of(|| vault_files(root)),
    );
}

fn main() {
    let mut small = Vault::new("1x", 1);
    let mut large = Vault::new("5x", COPIES);
    let mut report = Report::new(format!(
        "vault: {} notes (1x), {} notes ({COPIES}x)",
        small.index.note_count(),
        large.index.note_count()
    ));
    report_loading(&mut report, &small, [2., 10., 1.5e6]);
    report_loading(&mut report, &large, [10., 55., 6e6]);
    report_watcher(&mut report, &mut small, [0.1, 0.015, 0.01]);
    report_watcher(&mut report, &mut large, [0.1, 0.015, 0.025]);
    report_queries(&mut report, &small, [0.15, 0.025, 0.005]);
    report_queries(&mut report, &large, [0.55, 0.03, 0.02]);
    report_mentions(&mut report, &small, 3.);
    report_mentions(&mut report, &large, 4.);
    report_desktop_rename(&mut report, &mut small, 0.7);
    report_desktop_rename(&mut report, &mut large, 3.5);
    report_disk_rename(&mut report, &small, [25., 20.]);
    report_disk_rename(&mut report, &large, [135., 90.]);
    // The scratch vaults go before a budget failure exits the process.
    drop((small, large));
    report.finish();
}
