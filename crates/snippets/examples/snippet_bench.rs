//! Times compiling the built-in snippets (or a snippets file given as the
//! first argument) and looking for an expansion and a replacement on every
//! keystroke of a long line, the work the editor does per key.
//!
//! `cargo run --release -p editor-snippets --example snippet_bench [snippets.txt]`

use std::time::{Duration, Instant};

use editor_snippets::{
    DEFAULT_SNIPPETS, InputContext, Replacements, Request, SnippetEngine, SnippetFile, TriggerKey,
};

const LINE: &str = "Let x_{n+1} = \\frac{a}{b} + \\sum_{i=1}^{N} \\alpha_i x_i^2 \\cdot \\exp(-t/\\tau) \
                    where the constants are chosen so that the series converges for every n.";

fn main() {
    let text = std::env::args()
        .nth(1)
        .map(|path| std::fs::read_to_string(path).expect("the snippets file reads"))
        .unwrap_or_else(|| DEFAULT_SNIPPETS.to_string());
    let started = Instant::now();
    let file = SnippetFile::parse(&text).expect("the snippets parse");
    let engine = SnippetEngine::from_file(&file).expect("the snippets compile");
    println!(
        "{} snippets parsed and compiled in {:.2} ms",
        engine.len(),
        started.elapsed().as_secs_f64() * 1000.
    );
    let replacements = Replacements::builtin();
    let mut times: Vec<Duration> = Vec::new();
    for _ in 0..20 {
        for (end, typed) in LINE.char_indices() {
            let before = &LINE[..end + typed.len_utf8()];
            for context in [InputContext::Math, InputContext::Text] {
                let started = Instant::now();
                let request = Request::new(before, context, TriggerKey::Char(typed));
                std::hint::black_box(engine.expand(&request));
                std::hint::black_box(replacements.find(before, typed, context));
                times.push(started.elapsed());
            }
        }
    }
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize].as_secs_f64() * 1e6;
    println!(
        "per keystroke over {} keys: median {:.1} µs, p95 {:.1} µs, max {:.1} µs",
        times.len(),
        at(0.5),
        at(0.95),
        at(1.)
    );
}
