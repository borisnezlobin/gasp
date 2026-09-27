//! Phase 0 math spike: converts and renders a set of equations and reports
//! success rates, failure reasons and timings.
//!
//! Run with `cargo run --release -p editor-math --example math_spike [DIR…]`.
//! Equations come from the built-in synthetic list, from
//! `fixtures/corpus/**/*.md` when it exists, and from any directories given.
//! Pass `--failures` to print every failing equation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use editor_math::{MathCache, MathSnippet, find_math, latex_to_typst, render_latex, warm_up};

const FONT_SIZE: f64 = 16.0;
const BUILT_IN: &str = include_str!("../tests/data/equations.md");

struct Source {
    name: String,
    snippets: Vec<MathSnippet>,
}

#[derive(Default)]
struct Outcome {
    converted: usize,
    rendered: usize,
    convert_times: Vec<Duration>,
    new_render_times: Vec<Duration>,
    cached_times: Vec<Duration>,
    failures: BTreeMap<String, Vec<String>>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_failures = args.iter().any(|arg| arg == "--failures");
    report_first_render();
    for source in collect_sources(&args) {
        let outcome = measure(&source.snippets);
        print_outcome(&source, &outcome, show_failures);
    }
}

/// The first render pays for font loading, the standard library and
/// evaluating the mitex scope; later new equations reuse all of that.
fn report_first_render() {
    let cold = Instant::now();
    warm_up();
    let cold = cold.elapsed();
    let second = Instant::now();
    let _ = render_latex(r"\frac{1}{2}", false, FONT_SIZE);
    println!("one-time setup (first render, incl. fonts, library, mitex scope): {cold:.2?}");
    println!("next new equation: {:.2?}", second.elapsed());
}

fn collect_sources(args: &[String]) -> Vec<Source> {
    let mut sources = vec![Source {
        name: "built-in synthetic list".to_owned(),
        snippets: find_math(BUILT_IN),
    }];
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus");
    let extra = args
        .iter()
        .filter(|arg| !arg.starts_with("--"))
        .map(PathBuf::from);
    for dir in std::iter::once(corpus).chain(extra) {
        match markdown_math(&dir) {
            Some(snippets) => sources.push(Source {
                name: dir.display().to_string(),
                snippets,
            }),
            None => println!("\n(no Markdown found in {})", dir.display()),
        }
    }
    sources
}

fn markdown_math(dir: &Path) -> Option<Vec<MathSnippet>> {
    let mut files = Vec::new();
    collect_markdown_files(dir, &mut files);
    if files.is_empty() {
        return None;
    }
    files.sort();
    let snippets = files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| find_math(&text))
        .collect();
    Some(snippets)
}

fn collect_markdown_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            collect_markdown_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }
}

fn measure(snippets: &[MathSnippet]) -> Outcome {
    let mut outcome = Outcome::default();
    let mut cache = MathCache::new();
    for snippet in snippets {
        measure_convert(snippet, &mut outcome);
        if cache.contains(&snippet.source, snippet.display, FONT_SIZE) {
            continue;
        }
        let start = Instant::now();
        let result = cache.render(&snippet.source, snippet.display, FONT_SIZE);
        outcome.new_render_times.push(start.elapsed());
        match result {
            Ok(_) => outcome.rendered += 1,
            Err(error) => record_failure(&mut outcome, &error.to_string(), snippet),
        }
    }
    for snippet in snippets {
        let start = Instant::now();
        let _ = cache.render(&snippet.source, snippet.display, FONT_SIZE);
        outcome.cached_times.push(start.elapsed());
    }
    outcome
}

fn measure_convert(snippet: &MathSnippet, outcome: &mut Outcome) {
    let start = Instant::now();
    let converted = latex_to_typst(&snippet.source, snippet.display);
    outcome.convert_times.push(start.elapsed());
    if converted.is_ok() {
        outcome.converted += 1;
    }
}

fn record_failure(outcome: &mut Outcome, message: &str, snippet: &MathSnippet) {
    let key = message.lines().next().unwrap_or(message).to_owned();
    let delimiter = if snippet.display { "$$" } else { "$" };
    let example = format!("{delimiter}{}{delimiter}", snippet.source);
    outcome.failures.entry(key).or_default().push(example);
}

fn print_outcome(source: &Source, outcome: &Outcome, show_failures: bool) {
    let total = source.snippets.len();
    let unique = outcome.new_render_times.len();
    let failed: usize = outcome.failures.values().map(Vec::len).sum();
    println!("\n== {} ==", source.name);
    println!("equations: {total} ({unique} unique)");
    println!("converted: {} / {total}", outcome.converted);
    println!(
        "rendered:  {} / {unique} unique ({failed} failed)",
        outcome.rendered
    );
    print_times("convert only", &outcome.convert_times);
    print_times(
        "new render (convert + layout + SVG)",
        &outcome.new_render_times,
    );
    print_times("cached render", &outcome.cached_times);
    print_failures(&outcome.failures, show_failures);
}

fn print_times(label: &str, times: &[Duration]) {
    if times.is_empty() {
        return;
    }
    let mut sorted = times.to_vec();
    sorted.sort();
    let at = |fraction: f64| sorted[((sorted.len() - 1) as f64 * fraction).round() as usize];
    println!(
        "{label}: median {:.3?}, p95 {:.3?}, max {:.3?}",
        at(0.5),
        at(0.95),
        at(1.0)
    );
}

fn print_failures(failures: &BTreeMap<String, Vec<String>>, show_all: bool) {
    let mut groups: Vec<_> = failures.iter().collect();
    groups.sort_by_key(|(_, examples)| std::cmp::Reverse(examples.len()));
    for (reason, examples) in groups {
        println!("  {:>4} × {reason}   e.g. {}", examples.len(), examples[0]);
        if show_all {
            examples
                .iter()
                .skip(1)
                .for_each(|example| println!("         {example}"));
        }
    }
}
