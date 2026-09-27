//! Times the input pipeline per keystroke with the built-in snippets and
//! replacements installed: typing math at the end of a `$$` block's first
//! line in a long note, and typing prose in its middle. The whole pipeline
//! runs, as in the editor, but not layout or paint.
//!
//! `cargo run --release -p editor-core --example typing_bench -- NOTE.md`

use std::sync::Arc;
use std::time::{Duration, Instant};

use editor_core::document::{Document, Selection};
use editor_core::history::EditorState;
use editor_core::pipeline::{EditRequest, Pipeline, TabStops, follow_stops, step_names};
use editor_core::steps::install_typing_steps;
use editor_core::syntax;
use editor_core::transaction::{Origin, Transaction};
use editor_snippets::{Replacements, SnippetEngine, SnippetFile};

const MATH_TEXT: &str = "x2 + ab - cd ";
const PROSE_TEXT: &str = "the quick brown fox -- and ... ";

fn main() {
    let path = std::env::args().nth(1).expect("a note to type into");
    let text = std::fs::read_to_string(path).expect("the note reads");
    let engine = SnippetEngine::from_file(&SnippetFile::builtin()).expect("snippets compile");
    println!("{} snippets", engine.len());
    let mut pipeline = Pipeline::builtin();
    install_typing_steps(
        &mut pipeline,
        Arc::new(engine),
        Arc::new(Replacements::builtin()),
    )
    .expect("the slots exist");
    let math_line = text.find("\n$$").map(|at| {
        let first = at + 3;
        first + text[first + 1..].find('\n').unwrap_or(0) + 1
    });
    if let Some(at) = math_line {
        report("in math", &type_at(&pipeline, &text, at, MATH_TEXT));
    }
    let middle = text[..text.len() / 2].rfind('\n').unwrap_or(0);
    report("in prose", &type_at(&pipeline, &text, middle, PROSE_TEXT));
    // The same again without snippets, replacements and the math helpers,
    // to see what they add.
    for step in [
        step_names::SNIPPETS,
        step_names::REPLACEMENTS,
        step_names::MATH,
    ] {
        pipeline.set_enabled(step, false).expect("the step exists");
    }
    if let Some(at) = math_line {
        report(
            "in math, without them",
            &type_at(&pipeline, &text, at, MATH_TEXT),
        );
    }
    report(
        "in prose, without them",
        &type_at(&pipeline, &text, middle, PROSE_TEXT),
    );
}

/// Types 300 keys at `at` and returns how long each took in the
/// pipeline. The syntax tree is re-parsed between keys, outside the
/// timing, as the editor updates it incrementally.
fn type_at(pipeline: &Pipeline, text: &str, at: usize, typed: &str) -> Vec<Duration> {
    let mut state = EditorState::new(Document::from(text));
    let select = Transaction::select(Selection::cursor(at), Origin::Input, 0);
    state.apply(select).expect("the cursor goes there");
    let mut stops: Option<TabStops> = None;
    let mut times = Vec::new();
    for (step, key) in typed.chars().cycle().take(300).enumerate() {
        let tree = syntax::parse(&state.doc().to_string());
        let started = Instant::now();
        let output = pipeline.run_input(
            EditRequest::InsertText(key.to_string()),
            state.doc(),
            state.selection(),
            &tree,
            step as u64 * 1000,
            stops.as_ref(),
        );
        times.push(started.elapsed());
        for transaction in output.transactions.clone() {
            state.apply(transaction).expect("the edit applies");
        }
        stops = follow_stops(stops, &output);
    }
    times
}

fn report(name: &str, times: &[Duration]) {
    let mut sorted = times.to_vec();
    sorted.sort();
    let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q) as usize].as_secs_f64() * 1e6;
    println!(
        "{name}: {} keys, median {:.1} µs, p95 {:.1} µs, max {:.1} µs",
        sorted.len(),
        at(0.5),
        at(0.95),
        at(1.)
    );
}
