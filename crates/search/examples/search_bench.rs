//! Times vault search the way the apps run it: building the notes the
//! search reads, keeping them current after one note is edited, and every
//! keystroke of queries typed letter by letter (words, phrases, prefixes,
//! rare and very common terms, `tag:` and `#tag`). It also counts the
//! memory the notes keep and the allocations each keystroke makes. It runs
//! on the synthetic corpus and on a vault of five copies of it.
//!
//! Every time is the best of several runs, so other work on the machine
//! doesn't make it noisy.
//!
//! `cargo run --release -p gasp-search --example search_bench`
//!
//! Set `GASP_BENCH_ENFORCE=1` to fail when a line is over its budget.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use gasp_bench::corpus::{ScratchDir, copy_corpus};
use gasp_bench::{AllocStats, CountingAllocator, Report, Samples};
use gasp_search::engine::{Note, NoteCache, NoteResult, load_vault, search};
use gasp_search::tags::{search_tagged, tag_query};
use gasp_vault::build::build_index;
use gasp_vault::index::LinkIndex;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// The larger vault is this many copies of the corpus.
const COPIES: usize = 5;
/// How many times each query is typed; every keystroke is one sample.
const TYPING_ROUNDS: usize = 5;
/// Runs of each whole-vault operation.
const BUILD_RUNS: usize = 10;
/// The note the incremental update edits.
const EDITED_NOTE: &str = "Lemma.md";

/// A query typed letter by letter, and what kind of query it is.
struct TypedQuery {
    kind: &'static str,
    text: &'static str,
}

const QUERIES: [TypedQuery; 9] = [
    TypedQuery {
        kind: "word",
        text: "derivation",
    },
    TypedQuery {
        kind: "phrase",
        text: "office hour",
    },
    TypedQuery {
        kind: "phrase",
        text: "falls apart",
    },
    TypedQuery {
        kind: "common",
        text: "the ",
    },
    TypedQuery {
        kind: "rare",
        text: "lindqvist",
    },
    TypedQuery {
        kind: "prefix",
        text: "converg",
    },
    TypedQuery {
        kind: "missing",
        text: "quaternion",
    },
    TypedQuery {
        kind: "tag",
        text: "tag:lecture/week",
    },
    TypedQuery {
        kind: "tag",
        text: "#review",
    },
];

/// One vault on disk with what the apps keep in memory for it.
struct Vault {
    scratch: ScratchDir,
    notes: Vec<Note>,
    links: LinkIndex,
}

impl Vault {
    fn new(name: &str, copies: usize) -> Vault {
        let scratch = ScratchDir::new(name);
        copy_corpus(scratch.path(), copies);
        let notes = load_vault(scratch.path());
        let links = build_index(scratch.path());
        Vault {
            scratch,
            notes,
            links,
        }
    }

    fn root(&self) -> &Path {
        self.scratch.path()
    }

    fn texts(&self) -> Vec<(PathBuf, String)> {
        self.notes
            .iter()
            .map(|note| (note.path.clone(), note.text.to_string()))
            .collect()
    }
}

/// What the desktop's search panel does on a keystroke: a `tag:` query
/// asks the link index which notes carry the tag, anything else searches
/// every note.
fn keystroke(notes: &[Note], links: &LinkIndex, query: &str) -> Vec<NoteResult> {
    let generation = AtomicUsize::new(0);
    match tag_query(query) {
        Some(tag) => {
            let tagged: HashSet<String> = links.notes_tagged(tag);
            search_tagged(notes, tag, &tagged, &generation, 0)
        }
        None => search(notes, query, &generation, 0),
    }
}

/// Every prefix of `text`, as typed one letter at a time.
fn prefixes(text: &str) -> impl Iterator<Item = &str> {
    text.char_indices()
        .map(move |(at, ch)| &text[..at + ch.len_utf8()])
}

/// Timing and allocations of typing every query. Each keystroke's time is
/// its best of [`TYPING_ROUNDS`], which keeps other work on the machine
/// out of the numbers.
#[derive(Default)]
struct Typing {
    all: Samples,
    by_kind: Vec<(&'static str, Samples)>,
    alloc: AllocStats,
    keystrokes: usize,
}

impl Typing {
    fn samples_for(&mut self, kind: &'static str) -> &mut Samples {
        let at = match self.by_kind.iter().position(|(known, _)| *known == kind) {
            Some(at) => at,
            None => {
                self.by_kind.push((kind, Samples::new()));
                self.by_kind.len() - 1
            }
        };
        &mut self.by_kind[at].1
    }
}

/// Every keystroke of every query, with its kind.
fn keystrokes() -> Vec<(&'static str, &'static str)> {
    QUERIES
        .iter()
        .flat_map(|query| prefixes(query.text).map(|typed| (query.kind, typed)))
        .collect()
}

fn type_queries(notes: &[Note], links: &LinkIndex) -> Typing {
    let typed = keystrokes();
    let mut rounds = vec![Samples::new(); typed.len()];
    let ((), alloc) = CountingAllocator::measure(|| {
        for _ in 0..TYPING_ROUNDS {
            for ((_, query), samples) in typed.iter().zip(&mut rounds) {
                samples.time(|| keystroke(notes, links, query));
            }
        }
    });
    let mut typing = Typing {
        alloc,
        keystrokes: typed.len() * TYPING_ROUNDS,
        ..Typing::default()
    };
    for ((kind, _), samples) in typed.iter().zip(&rounds) {
        typing.all.push(samples.min());
        typing.samples_for(kind).push(samples.min());
    }
    typing
}

/// A hash of every keystroke's results, which must not change when the
/// search gets faster.
fn results_digest(notes: &[Note], links: &LinkIndex) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (_, typed) in keystrokes() {
        format!("{typed}{:?}", keystroke(notes, links, typed)).hash(&mut hasher);
    }
    hasher.finish()
}

fn report_typing(report: &mut Report, label: &str, typing: &Typing, budgets: [Duration; 2]) {
    report.time(
        format!("keystroke median ({label})"),
        typing.all.median(),
        budgets[0],
    );
    report.time(
        format!("keystroke p95 ({label})"),
        typing.all.p95(),
        budgets[1],
    );
    report.note_time(format!("keystroke max ({label})"), typing.all.max());
    for (kind, samples) in &typing.by_kind {
        report.note_time(format!("  {kind} p95 ({label})"), samples.p95());
    }
}

fn report_allocations(report: &mut Report, label: &str, typing: &Typing, budget: f64) {
    let (allocations, bytes) = typing.alloc.per_operation(typing.keystrokes);
    report.count(
        format!("allocations per keystroke ({label})"),
        allocations,
        budget,
    );
    report.note_bytes(format!("bytes allocated per keystroke ({label})"), bytes);
}

/// Builds every note from its text, as a load does after reading them.
fn build_notes(texts: &[(PathBuf, String)]) -> Vec<Note> {
    texts
        .iter()
        .map(|(path, text)| Note::new(path.clone(), text.clone()))
        .collect()
}

fn report_build(report: &mut Report, label: &str, vault: &Vault, budgets: (Duration, f64)) {
    let texts = vault.texts();
    let text_bytes: usize = texts.iter().map(|(_, text)| text.len()).sum();
    let build = Samples::collect(BUILD_RUNS, || build_notes(&texts));
    report.time(
        format!("build notes from texts ({label})"),
        build.min(),
        budgets.0,
    );
    let (notes, alloc) = CountingAllocator::measure(|| build_notes(&texts));
    // The texts are kept by the notes, so they count towards what the
    // search holds; the clones handed to `Note::new` are part of that.
    report.bytes(
        format!("memory kept by the notes ({label})"),
        alloc.retained_bytes as f64,
        budgets.1,
    );
    report.note_bytes(format!("  of which note text ({label})"), text_bytes as f64);
    report.note_bytes(
        format!("  peak while building ({label})"),
        alloc.peak_extra_bytes as f64,
    );
    drop(notes);
}

fn report_loading(report: &mut Report, vault: &Vault) {
    let root = vault.root();
    let cold = Samples::collect(BUILD_RUNS, || load_vault(root));
    report.time("load every note from disk (1x)", cold.min(), ms(30));
    let mut cache = NoteCache::default();
    cache.refresh(root);
    let unchanged = Samples::collect(BUILD_RUNS, || {
        cache.refresh(root);
        cache.notes()
    });
    report.time("refresh with nothing changed (1x)", unchanged.min(), ms(5));
    let edited = PathBuf::from(EDITED_NOTE);
    let original = std::fs::read_to_string(root.join(&edited)).expect("the note reads");
    let mut after_edit = Samples::new();
    for round in 0..BUILD_RUNS {
        let text = format!("{original}\nAn edit, number {round}, about derivations.\n");
        std::fs::write(root.join(&edited), text).expect("the note writes");
        after_edit.time(|| {
            cache.reload(root, std::slice::from_ref(&edited));
            cache.notes()
        });
    }
    std::fs::write(root.join(&edited), original).expect("the note writes back");
    report.time("reload one edited note (1x)", after_edit.min(), ms(2));
}

/// The iPhone's keystroke: bring the note cache up to date, then search.
fn report_phone_keystroke(report: &mut Report, vault: &Vault) {
    let root = vault.root();
    let query = "derivation";
    let cold = Samples::collect(BUILD_RUNS, || {
        let mut cache = NoteCache::default();
        cache.refresh(root);
        keystroke(&cache.notes(), &vault.links, query)
    });
    report.note_time("keystroke reading every note first (1x)", cold.min());
    let mut cache = NoteCache::default();
    cache.refresh(root);
    let kept = Samples::collect(BUILD_RUNS, || {
        cache.refresh(root);
        keystroke(&cache.notes(), &vault.links, query)
    });
    report.time(
        "keystroke refreshing a kept note cache (1x)",
        kept.min(),
        ms(10),
    );
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn main() {
    let small = Vault::new("search-1x", 1);
    let large = Vault::new("search-5x", COPIES);
    let mut report = Report::new(format!(
        "search: {} notes (1x), {} notes ({COPIES}x)",
        small.notes.len(),
        large.notes.len()
    ));
    report_build(&mut report, "1x", &small, (ms(20), 8e6));
    report_build(&mut report, "5x", &large, (ms(100), 40e6));
    report_loading(&mut report, &small);
    let typing = type_queries(&small.notes, &small.links);
    report_typing(&mut report, "1x", &typing, [ms(8), ms(16)]);
    report_allocations(&mut report, "1x", &typing, 50_000.);
    let large_typing = type_queries(&large.notes, &large.links);
    report_typing(&mut report, "5x", &large_typing, [ms(40), ms(80)]);
    report_allocations(&mut report, "5x", &large_typing, 250_000.);
    report_phone_keystroke(&mut report, &small);
    println!(
        "results digest: {:016x} (1x), {:016x} ({COPIES}x)",
        results_digest(&small.notes, &small.links),
        results_digest(&large.notes, &large.links)
    );
    // The scratch vaults go before a budget failure exits the process.
    drop((small, large));
    report.finish();
}
