//! Times reading notes as prose the way the apps do: building the grammar
//! checker (Harper's dictionary and rules), checking a whole long note,
//! re-checking after a one-character edit with flags cached per paragraph,
//! and finding every sentence's length across a note, which the phone does
//! on every keystroke.
//!
//! `cargo run --release -p gasp-prose --example prose_bench`
//!
//! Set `GASP_BENCH_ENFORCE=1` to fail when a line is over its budget.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::process::Command;
use std::time::{Duration, Instant};

use gasp_bench::corpus::{corpus_notes, long_note};
use gasp_bench::{CountingAllocator, Report, Samples};
use gasp_core::syntax::{SyntaxTree, parse};
use gasp_prose::{
    CheckOptions, Checker, English, Flag, Purpose, SentenceLengthCache, Thresholds, Unit,
    sentence_lengths, units,
};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const LONG_NOTE_BYTES: usize = 200 * 1024;
const STARTUP_RUNS: usize = 3;
const STARTUP_CHILD: &str = "--startup-child";
/// About a screen of text around the cursor, as the apps check.
const SCREEN_BYTES: usize = 3000;
const EDITS: usize = 40;
const TINT_RUNS: usize = 20;

fn ms(millis: f64) -> Duration {
    Duration::from_secs_f64(millis / 1000.)
}

fn options() -> CheckOptions {
    CheckOptions {
        spelling: true,
        english: English::American,
    }
}

fn main() {
    if std::env::args().any(|arg| arg == STARTUP_CHILD) {
        startup_child();
        return;
    }
    let mut report = Report::new("gasp-prose");
    startup(&mut report);
    let mut checker = Checker::new(options());
    let long = long_note(LONG_NOTE_BYTES);
    let typical = typical_note();
    whole_note(&mut report, &mut checker, &long);
    editing(&mut report, &mut checker, &long);
    sentence_tints(&mut report, "long note", &long, ms(5.));
    sentence_tints(&mut report, "typical note", &typical, ms(0.06));
    report.finish();
}

/// The corpus note of median length.
fn typical_note() -> String {
    let mut notes: Vec<String> = corpus_notes().into_iter().map(|(_, text)| text).collect();
    notes.sort_by_key(String::len);
    notes.swap_remove(notes.len() / 2)
}

/// Run in a fresh process: the first checker loads Harper's dictionary and
/// builds its rules.
fn startup_child() {
    let started = Instant::now();
    let (checker, stats) = CountingAllocator::measure(|| Checker::new(options()));
    let first = started.elapsed();
    let started = Instant::now();
    let second = Checker::new(options());
    let second_time = started.elapsed();
    drop((checker, second));
    println!(
        "{} {} {}",
        first.as_nanos(),
        second_time.as_nanos(),
        stats.retained_bytes
    );
}

fn startup(report: &mut Report) {
    let exe = std::env::current_exe().expect("the bench knows where it is");
    let mut first = Samples::new();
    let mut second = Samples::new();
    let mut retained = 0.;
    for _ in 0..STARTUP_RUNS {
        let output = Command::new(&exe)
            .arg(STARTUP_CHILD)
            .output()
            .expect("the startup run starts");
        let text = String::from_utf8_lossy(&output.stdout);
        let numbers: Vec<f64> = text
            .split_whitespace()
            .filter_map(|word| word.parse().ok())
            .collect();
        let [first_ns, second_ns, bytes] = numbers[..] else {
            panic!("the startup run printed {text:?}");
        };
        first.push(Duration::from_nanos(first_ns as u64));
        second.push(Duration::from_nanos(second_ns as u64));
        retained = bytes;
    }
    report.time(
        "first checker in a fresh process (median)",
        first.median(),
        ms(1500.),
    );
    report.time(
        "another checker, dictionary loaded (median)",
        second.median(),
        ms(10.),
    );
    report.bytes("memory kept by the first checker", retained, 150e6);
}

fn whole_note(report: &mut Report, checker: &mut Checker, text: &str) {
    let tree = parse(text);
    let mut paragraphs = Samples::new();
    let started = Instant::now();
    let grammar_units = units(&tree, 0..text.len(), Purpose::Grammar);
    let mut flags = 0;
    let (_, stats) = CountingAllocator::measure(|| {
        for unit in &grammar_units {
            flags += paragraphs.time(|| checker.check(text, unit)).len();
        }
    });
    let whole = started.elapsed();
    report.note_count("long note paragraphs", grammar_units.len() as f64);
    report.note_count("long note flags", flags as f64);
    report.time("long note checked whole", whole, ms(6000.));
    report.time("paragraph check (median)", paragraphs.median(), ms(1.5));
    report.time("paragraph check (p95)", paragraphs.p95(), ms(8.));
    let (allocations, _) = stats.per_operation(grammar_units.len());
    report.note_count("allocations per paragraph check", allocations);
    report.note_bytes(
        "memory kept by checking the long note",
        stats.retained_bytes as f64,
    );
    on_another_thread(report, text, &grammar_units);
}

/// A new checker on a new thread checking the long note again, as the
/// phone does from whichever thread is free: what a thread costs.
fn on_another_thread(report: &mut Report, text: &str, grammar_units: &[Unit]) {
    let (elapsed, stats) = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let mut checker = Checker::new(options());
                let started = Instant::now();
                let (_, stats) = CountingAllocator::measure(|| {
                    for unit in grammar_units {
                        checker.check(text, unit);
                    }
                });
                (started.elapsed(), stats)
            })
            .join()
            .expect("the thread checks")
    });
    report.time(
        "long note checked again on another thread",
        elapsed,
        ms(3500.),
    );
    report.bytes(
        "memory kept by checking on another thread",
        stats.retained_bytes as f64,
        20e6,
    );
}

/// Flags cached by paragraph text, as both apps keep them.
#[derive(Default)]
struct FlagCache {
    flags: HashMap<u64, Vec<Flag>>,
}

impl FlagCache {
    /// Checks the paragraphs overlapping `visible` that aren't cached.
    fn check_screen(
        &mut self,
        checker: &mut Checker,
        text: &str,
        tree: &SyntaxTree,
        visible: Range<usize>,
    ) {
        for unit in units(tree, visible, Purpose::Grammar) {
            let slice = &text[unit.range.clone()];
            let mut hasher = DefaultHasher::new();
            slice.hash(&mut hasher);
            self.flags
                .entry(hasher.finish())
                .or_insert_with(|| checker.check(text, &unit));
        }
    }
}

/// One character typed in a paragraph near the middle of the note, then
/// the screen around it checked with every other paragraph cached.
fn editing(report: &mut Report, checker: &mut Checker, long: &str) {
    let tree = parse(long);
    let grammar_units = units(&tree, 0..long.len(), Purpose::Grammar);
    let middle = grammar_units.len() / 2;
    let mut cache = FlagCache::default();
    let mut samples = Samples::new();
    for unit in grammar_units.iter().skip(middle).take(EDITS) {
        let visible = screen_around(long, unit.range.start);
        cache.check_screen(checker, long, &tree, visible.clone());
        let edited = typed_into(long, unit);
        let tree = parse(&edited);
        samples.time(|| cache.check_screen(checker, &edited, &tree, visible));
    }
    report.time(
        "screen re-checked after a keystroke (median)",
        samples.median(),
        ms(0.5),
    );
    report.time(
        "screen re-checked after a keystroke (p95)",
        samples.p95(),
        ms(3.),
    );
}

fn screen_around(text: &str, at: usize) -> Range<usize> {
    let mut start = at.saturating_sub(SCREEN_BYTES / 2);
    let mut end = (at + SCREEN_BYTES / 2).min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    while !text.is_char_boundary(end) {
        end += 1;
    }
    start..end
}

/// The note with a letter typed at the end of the unit's first text piece.
fn typed_into(text: &str, unit: &Unit) -> String {
    let at = unit
        .pieces
        .first()
        .map_or(unit.range.end, |piece| piece.range.end);
    let mut edited = String::with_capacity(text.len() + 1);
    edited.push_str(&text[..at]);
    edited.push('s');
    edited.push_str(&text[at..]);
    edited
}

/// Every sentence's length across the whole note, as the phone asks for
/// on each keystroke.
fn sentence_tints(report: &mut Report, name: &str, text: &str, budget: Duration) {
    let tree = parse(text);
    let all = Samples::collect(TINT_RUNS, || tint_pass(&tree, text));
    let unit_only = Samples::collect(TINT_RUNS, || units(&tree, 0..text.len(), Purpose::Rhythm));
    let (tints, stats) = CountingAllocator::measure(|| tint_pass(&tree, text));
    report.note_count(format!("{name} sentences"), tints as f64);
    report.time(
        format!("{name} sentence tints (median)"),
        all.median(),
        budget,
    );
    report.note_time(format!("{name} units only (median)"), unit_only.median());
    report.note_count(
        format!("{name} allocations per tint pass"),
        stats.allocations as f64,
    );
    let keystrokes = cached_tints_while_typing(&tree, text);
    report.time(
        format!("{name} sentence tints after a keystroke, cached (median)"),
        keystrokes.median(),
        budget / 4,
    );
}

/// What the phone does on each keystroke with a [`SentenceLengthCache`]:
/// every sentence of the note found again, the edited paragraph segmented.
fn cached_tints_while_typing(tree: &SyntaxTree, text: &str) -> Samples {
    let mut cache = SentenceLengthCache::new();
    cache.sentence_lengths(text, tree, 0..text.len(), Thresholds::default());
    let rhythm_units = units(tree, 0..text.len(), Purpose::Rhythm);
    let mut samples = Samples::new();
    for unit in rhythm_units
        .iter()
        .step_by((rhythm_units.len() / EDITS).max(1))
    {
        let edited = typed_into(text, unit);
        let tree = parse(&edited);
        samples.time(|| {
            cache.sentence_lengths(&edited, &tree, 0..edited.len(), Thresholds::default())
        });
    }
    samples
}

fn tint_pass(tree: &SyntaxTree, text: &str) -> usize {
    units(tree, 0..text.len(), Purpose::Rhythm)
        .iter()
        .flat_map(|unit| sentence_lengths(text, unit, Thresholds::default()))
        .count()
}
