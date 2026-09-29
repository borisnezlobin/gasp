//! Times the core's hot paths on the synthetic corpus: a full parse of a
//! long note, the incremental reparse, input pipeline and viewport plan of
//! every keystroke (with the owner's Latex Suite snippets and replacements
//! migrated from `reference/obsidian`), planning math-, table- and
//! footnote-heavy notes, and the allocations each keystroke makes.
//!
//! `cargo run --release -p gasp-core --example core_bench`
//!
//! With `GASP_BENCH_ENFORCE=1`, a result over its budget fails the run.

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use gasp_bench::corpus::{corpus_notes, long_note};
use gasp_bench::{CountingAllocator, Report, Samples, Stopwatch};
use gasp_core::document::{Document, Selection};
use gasp_core::history::EditorState;
use gasp_core::pipeline::{EditRequest, Pipeline, TabStops, follow_stops};
use gasp_core::render::folds::Folds;
use gasp_core::render::{RenderInput, RevealSettings, plan, plan_lines, reveal_settings};
use gasp_core::steps::install_typing_steps;
use gasp_core::syntax::{self, Edit, SyntaxTree};
use gasp_core::transaction::{Origin, Transaction};
use gasp_snippets::{Replacements, SnippetEngine, SnippetFile};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const LONG_NOTE_BYTES: usize = 200 * 1024;
const VIEWPORT_LINES: usize = 60;
const KEYS_PER_RUN: usize = 300;
const PROSE_TYPED: &str = "the quick brown fox -- and ... ";
const MATH_TYPED: &str = "x2 + ab - cd ";

type Section = fn(&str, &mut Report);

const SECTIONS: [(&str, Section); 3] = [
    ("parse", parse_benches),
    ("plan", plan_benches),
    ("keys", keystroke_benches),
];

/// Runs every section, or only the one named as the first argument (for a
/// profiler), `repeat` times as the second.
fn main() {
    let only = std::env::args().nth(1);
    let repeat: usize = std::env::args()
        .nth(2)
        .and_then(|count| count.parse().ok())
        .unwrap_or(1);
    let long = long_note(LONG_NOTE_BYTES);
    let mut report = Report::new(format!(
        "gasp-core on a {} KB note built from the corpus",
        long.len() / 1024
    ));
    let chosen = SECTIONS
        .iter()
        .filter(|(name, _)| only.as_deref().is_none_or(|only| only == *name));
    for (_, section) in chosen {
        for _ in 0..repeat {
            section(&long, &mut report);
        }
    }
    report.finish();
}

fn parse_benches(long: &str, report: &mut Report) {
    let full = Samples::collect(15, || syntax::parse(long));
    report.time("full parse, median", full.median(), ms(12));
    let notes = corpus_notes();
    let corpus = Samples::collect(15, || {
        for (_, text) in &notes {
            std::hint::black_box(syntax::parse(text));
        }
    });
    report.time(
        "parse all 204 corpus notes, median",
        corpus.median(),
        ms(30),
    );
    let (tree, parsed) = CountingAllocator::measure(|| syntax::parse(long));
    report.note_count("tree nodes", tree.nodes().len() as f64);
    report.note_bytes("tree size in memory", parsed.retained_bytes as f64);
    report.note_count("allocations in a full parse", parsed.allocations as f64);
}

fn plan_benches(long: &str, report: &mut Report) {
    let tree = syntax::parse(long);
    let settings = default_reveal();
    let middle_line = tree.lines().line_count() / 2;
    let cursor = tree.lines().line_start(middle_line + 5);
    let viewport = middle_line..middle_line + VIEWPORT_LINES;
    let cursor_range = cursor..cursor;
    let selections = [cursor_range];
    let input = RenderInput {
        text: long,
        tree: &tree,
        selections: &selections,
        settings: &settings,
    };
    let samples = Samples::collect(200, || plan_lines(&input, viewport.clone()));
    report.time("plan a 60-line viewport, median", samples.median(), us(60));
    let whole = Samples::collect(30, || plan(&input));
    report.time("plan the whole note, median", whole.median(), ms(4));
    let folds = Folds::default();
    let mut plans = plan_lines(&input, viewport.clone()).lines;
    let applied = Samples::collect(200, || folds.apply(&mut plans, &tree, &selections));
    report.time(
        "apply folds to a viewport, median",
        applied.median(),
        us(10),
    );
    let mut folded = Folds::default();
    folded.fold_all_headings(&tree);
    let applied = Samples::collect(200, || {
        let mut plans = plan_lines(&input, viewport.clone()).lines;
        folded.apply(&mut plans, &tree, &selections);
        folded.mark_folded_headings(&mut plans, long, &tree, &selections);
        plans
    });
    report.time(
        "plan a viewport with every heading folded, median",
        applied.median(),
        us(400),
    );
    for (label, note) in heavy_notes() {
        let tree = syntax::parse(&note);
        let input = RenderInput {
            text: &note,
            tree: &tree,
            selections: &[Range::default()],
            settings: &settings,
        };
        let samples = Samples::collect(50, || plan(&input));
        report.time(
            format!("plan the {label} note in full, median"),
            samples.median(),
            us(400),
        );
    }
}

/// The corpus notes with the most math, table rows and footnotes.
fn heavy_notes() -> Vec<(&'static str, String)> {
    let notes = corpus_notes();
    let heaviest = |count: fn(&str) -> usize| {
        let (_, text) = notes
            .iter()
            .max_by_key(|(_, text)| count(text))
            .expect("the corpus has notes");
        text.clone()
    };
    vec![
        ("most math", heaviest(|text| text.matches('$').count())),
        (
            "most table rows",
            heaviest(|text| text.lines().filter(|l| l.starts_with('|')).count()),
        ),
        (
            "most footnotes",
            heaviest(|text| text.matches("[^").count()),
        ),
    ]
}

fn keystroke_benches(long: &str, report: &mut Report) {
    let pipeline = owners_pipeline();
    let middle = long[..long.len() / 2].rfind('\n').expect("a line break");
    let prose = Typist::new(long, middle, &pipeline).type_keys(PROSE_TYPED);
    prose.report("typing prose mid-note", report);
    if let Some(at) = first_math_block_line(long) {
        let math = Typist::new(long, at, &pipeline).type_keys(MATH_TYPED);
        math.report("typing in a math block", report);
    }
    let start = long.find("\n\n").expect("a paragraph break") + 1;
    let top = Typist::new(long, start, &pipeline).type_keys(PROSE_TYPED);
    top.report("typing prose near the top", report);
}

fn owners_pipeline() -> Pipeline {
    let obsidian = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/obsidian");
    let migration = gasp_migrate::migrate_obsidian(&obsidian).expect("the settings migrate");
    let snippets = migration.latex_suite.map(|suite| suite.file);
    let snippets = snippets.unwrap_or_else(SnippetFile::builtin);
    let engine = SnippetEngine::from_file(&snippets).expect("the snippets compile");
    let replacements = migration.replacements.map(|found| found.table);
    let replacements = replacements.unwrap_or_else(Replacements::builtin);
    let mut pipeline = Pipeline::builtin();
    install_typing_steps(&mut pipeline, Arc::new(engine), Arc::new(replacements))
        .expect("the slots exist");
    pipeline
}

fn first_math_block_line(text: &str) -> Option<usize> {
    let open = text.find("\n$$\n")? + 4;
    Some(open + text[open..].find('\n')?)
}

/// An editor without a window: the text, its tree and history, fed keys
/// through the pipeline, with each phase of a keystroke timed.
struct Typist<'a> {
    state: EditorState,
    text: String,
    tree: SyntaxTree,
    pipeline: &'a Pipeline,
    stops: Option<TabStops>,
    settings: RevealSettings,
    timings: KeyTimings,
}

/// One phase of a keystroke: how long it took each time, and what it
/// allocated over all of them.
#[derive(Default)]
struct Phase {
    samples: Samples,
    allocations: usize,
    allocated_bytes: usize,
}

impl Phase {
    fn run<T>(&mut self, operation: impl FnOnce() -> T) -> T {
        let started = Stopwatch::start();
        let (result, stats) = CountingAllocator::measure(operation);
        self.samples.push(started.elapsed());
        self.allocations += stats.allocations;
        self.allocated_bytes += stats.allocated_bytes;
        result
    }
}

#[derive(Default)]
struct KeyTimings {
    pipeline: Phase,
    history: Phase,
    reparse: Phase,
    plan: Phase,
    whole: Phase,
    keys: usize,
}

impl<'a> Typist<'a> {
    fn new(text: &str, cursor: usize, pipeline: &'a Pipeline) -> Self {
        let mut state = EditorState::new(Document::from(text));
        let select = Transaction::select(Selection::cursor(cursor), Origin::Input, 0);
        state.apply(select).expect("the cursor goes there");
        Self {
            state,
            text: text.to_owned(),
            tree: syntax::parse(text),
            pipeline,
            stops: None,
            settings: default_reveal(),
            timings: KeyTimings::default(),
        }
    }

    fn type_keys(mut self, typed: &str) -> KeyTimings {
        for (step, key) in typed.chars().cycle().take(KEYS_PER_RUN).enumerate() {
            let mut whole = std::mem::take(&mut self.timings.whole);
            whole.run(|| self.press(key, step as u64 * 1000));
            self.timings.whole = whole;
            self.timings.keys += 1;
        }
        self.timings
    }

    fn press(&mut self, key: char, now: u64) {
        let output = self.timings.pipeline.run(|| {
            self.pipeline.run_input(
                EditRequest::InsertText(key.to_string()),
                self.state.doc(),
                self.state.selection(),
                &self.tree,
                now,
                self.stops.as_ref(),
            )
        });
        for transaction in output.transactions.clone() {
            self.apply(transaction);
        }
        self.stops = follow_stops(self.stops.take(), &output);
        let head = self.state.selection().primary().head;
        let mut plan = std::mem::take(&mut self.timings.plan);
        plan.run(|| self.plan_viewport(head));
        self.timings.plan = plan;
    }

    fn apply(&mut self, transaction: Transaction) {
        let edits = transaction.changes.edits().to_vec();
        let state = &mut self.state;
        self.timings
            .history
            .run(|| state.apply(transaction).expect("the edit applies"));
        for edit in edits.iter().rev() {
            let change = Edit {
                old: edit.range.clone(),
                new_len: edit.insert.len(),
            };
            let (tree, text) = (&mut self.tree, &mut self.text);
            self.timings.reparse.run(|| {
                text.replace_range(edit.range.clone(), &edit.insert);
                tree.edit(text, &change)
            });
        }
    }

    fn plan_viewport(&self, head: usize) -> usize {
        let line = self.tree.lines().line_of(head);
        let first = line.saturating_sub(VIEWPORT_LINES / 2);
        let cursor = head..head;
        let selections = [cursor];
        let input = RenderInput {
            text: &self.text,
            tree: &self.tree,
            selections: &selections,
            settings: &self.settings,
        };
        plan_lines(&input, viewport(first)).lines.len()
    }
}

fn viewport(first: usize) -> Range<usize> {
    first..first + VIEWPORT_LINES
}

impl KeyTimings {
    fn report(&self, label: &str, report: &mut Report) {
        let phases = [
            ("pipeline", &self.pipeline, us(40)),
            ("history", &self.history, us(15)),
            ("reparse", &self.reparse, us(250)),
            ("viewport plan", &self.plan, us(150)),
        ];
        for (name, phase, budget) in phases {
            report.time(
                format!("{label}: {name}, median"),
                phase.samples.median(),
                budget,
            );
        }
        report.time(
            format!("{label}: whole keystroke, p95"),
            self.whole.samples.p95(),
            us(1500),
        );
        report.note_time(format!("{label}: reparse, p95"), self.reparse.samples.p95());
        report.note_time(
            format!("{label}: reparse, slowest"),
            self.reparse.samples.max(),
        );
        let keys = self.keys.max(1) as f64;
        report.count(
            format!("{label}: allocations per key"),
            self.whole.allocations as f64 / keys,
            1500.,
        );
        let phases = [
            ("pipeline", &self.pipeline),
            ("history", &self.history),
            ("reparse", &self.reparse),
            ("viewport plan", &self.plan),
            ("whole key", &self.whole),
        ];
        for (name, phase) in phases {
            report.note_bytes(
                format!("{label}: bytes allocated per key, {name}"),
                phase.allocated_bytes as f64 / keys,
            );
        }
    }
}

fn default_reveal() -> RevealSettings {
    reveal_settings(&gasp_config::settings::SymbolSettings::default())
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn us(micros: u64) -> Duration {
    Duration::from_micros(micros)
}
