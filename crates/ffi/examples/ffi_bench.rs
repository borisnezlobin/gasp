//! Times what the iPhone asks of the core on every keystroke and cursor
//! move: `update` with the text view's whole text, then `plan`,
//! `sentence_tints` and `heading_folds`, each result lowered into the
//! buffer UniFFI hands to Swift. Runs on a typical corpus note and on a
//! long one, with sentence-length highlighting on.
//!
//! `cargo run --release -p gasp-ffi --example ffi_bench`
//!
//! With `GASP_BENCH_ENFORCE=1`, a result over its budget fails the run.

use std::sync::Arc;
use std::time::Duration;

use gasp_bench::corpus::{ScratchDir, corpus_notes, long_note};
use gasp_bench::{CountingAllocator, Report, Samples, Stopwatch};
use gasp_ffi::{NoteDocument, TextRange, UniFfiTag, VaultFolder};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const KEYS: usize = 200;
const TYPED: &str = "the quick brown fox and a lazy dog ";

fn main() {
    let scratch = ScratchDir::new("ffi");
    let vault = VaultFolder::open(scratch.path().to_string_lossy().into_owned())
        .expect("the scratch vault opens");
    vault
        .toggle_sentence_highlighting()
        .expect("the setting writes");
    let mut report = Report::new("gasp-ffi, what the phone asks per keystroke");
    let mut notes = corpus_notes();
    notes.sort_by_key(|(_, text)| text.len());
    let typical = notes[notes.len() / 2].1.clone();
    let longest = notes[notes.len() - 1].1.clone();
    phone_benches(
        "typical note",
        &typical,
        &vault,
        &mut report,
        Budgets::TYPICAL,
    );
    phone_benches(
        "longest corpus note",
        &longest,
        &vault,
        &mut report,
        Budgets::LONGEST_IN_CORPUS,
    );
    let long = long_note(200 * 1024);
    phone_benches("200 KB note", &long, &vault, &mut report, Budgets::LONG);
    report.finish();
}

#[derive(Clone, Copy)]
struct Budgets {
    keystroke: Duration,
    cursor_move: Duration,
    open: Duration,
}

impl Budgets {
    const TYPICAL: Budgets = Budgets {
        keystroke: Duration::from_micros(300),
        cursor_move: Duration::from_micros(100),
        open: Duration::from_micros(300),
    };
    const LONGEST_IN_CORPUS: Budgets = Budgets {
        keystroke: Duration::from_micros(1200),
        cursor_move: Duration::from_micros(400),
        open: Duration::from_micros(1200),
    };
    const LONG: Budgets = Budgets {
        keystroke: Duration::from_millis(15),
        cursor_move: Duration::from_millis(4),
        open: Duration::from_millis(15),
    };
}

fn phone_benches(
    label: &str,
    text: &str,
    vault: &VaultFolder,
    report: &mut Report,
    budgets: Budgets,
) {
    let open = Samples::collect(10, || vault.document(text.to_owned()));
    report.time(
        format!("{label}: open (parse), median"),
        open.median(),
        budgets.open,
    );
    let middle = text[..text.len() / 2].rfind('\n').unwrap_or(0);
    let cursor = utf16_len(&text[..middle]);
    let at_cursor = TextRange {
        start: cursor,
        end: cursor,
    };
    let (document, kept) = CountingAllocator::measure(|| {
        let document = vault.document(text.to_owned());
        drop(document.plan(at_cursor));
        document
    });
    report.note_bytes(
        format!("{label}: memory an open, drawn note keeps"),
        kept.retained_bytes as f64,
    );
    let mut typing = Typing::new(text, middle, cursor);
    let mut keystrokes = Samples::new();
    let mut phases = PhaseTimes::default();
    let (_, stats) = CountingAllocator::measure(|| {
        for key in TYPED.chars().cycle().take(KEYS) {
            let new_text = typing.press(key);
            let started = Stopwatch::start();
            phases.update.time(|| document.update(new_text));
            let selection = typing.selection();
            ask_for_drawing(&document, selection, &mut phases);
            keystrokes.push(started.elapsed());
        }
    });
    report.time(
        format!("{label}: keystroke, median"),
        keystrokes.median(),
        budgets.keystroke,
    );
    phases.report(label, report);
    report.note_count(
        format!("{label}: allocations per keystroke"),
        stats.allocations as f64 / KEYS as f64,
    );
    report.note_bytes(
        format!("{label}: bytes allocated per keystroke"),
        stats.allocated_bytes as f64 / KEYS as f64,
    );
    let mut move_phases = PhaseTimes::default();
    let moves = Samples::collect(50, || {
        ask_for_drawing(&document, typing.selection(), &mut move_phases)
    });
    report.time(
        format!("{label}: cursor move, median"),
        moves.median(),
        budgets.cursor_move,
    );
    move_phases.report(&format!("{label}, cursor move"), report);
}

/// What the phone's `restyle` asks for, lowered as UniFFI returns it.
fn ask_for_drawing(document: &Arc<NoteDocument>, selection: TextRange, phases: &mut PhaseTimes) {
    let plan = phases.plan.time(|| document.plan(selection));
    let plan = phases.plan_lowering.time(|| lowered(plan));
    let tints = phases.tints.time(|| lowered(document.sentence_tints()));
    let folds = phases.folds.time(|| lowered(document.heading_folds()));
    phases.buffer_bytes = plan + tints + folds;
}

/// The size of the buffer UniFFI would hand to Swift for `value`.
fn lowered<T: uniffi::Lower<UniFfiTag>>(value: T) -> usize {
    let buffer = T::lower_into_rust_buffer(value);
    buffer.destroy_into_vec().len()
}

#[derive(Default)]
struct PhaseTimes {
    update: Samples,
    plan: Samples,
    plan_lowering: Samples,
    tints: Samples,
    folds: Samples,
    buffer_bytes: usize,
}

impl PhaseTimes {
    fn report(&self, label: &str, report: &mut Report) {
        for (name, samples) in [
            ("update", &self.update),
            ("plan", &self.plan),
            ("sentence tints", &self.tints),
            ("heading folds", &self.folds),
            ("lowering the plan", &self.plan_lowering),
        ] {
            report.note_time(format!("{label}: {name}, median"), samples.median());
        }
        report.note_bytes(
            format!("{label}: buffers handed to Swift per keystroke"),
            self.buffer_bytes as f64,
        );
    }
}

/// The text view's text as keys go in at one place.
struct Typing {
    text: String,
    at: usize,
    cursor_utf16: u32,
}

impl Typing {
    fn new(text: &str, at: usize, cursor_utf16: u32) -> Self {
        Self {
            text: text.to_owned(),
            at,
            cursor_utf16,
        }
    }

    fn press(&mut self, key: char) -> String {
        self.text.insert(self.at, key);
        self.at += key.len_utf8();
        self.cursor_utf16 += key.len_utf16() as u32;
        self.text.clone()
    }

    fn selection(&self) -> TextRange {
        TextRange {
            start: self.cursor_utf16,
            end: self.cursor_utf16,
        }
    }
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}
