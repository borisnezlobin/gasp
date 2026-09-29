//! Times math rendering the way the apps use it: the one-time setup paid by
//! the first equation after launch, a new equation, the whole path to
//! pixels, typing inside an equation (every keystroke is a new source), the
//! cache over an editing session, and the memory Typst keeps.
//!
//! "To pixels via SVG" is what the apps did before `rasterize_latex`:
//! render the SVG, parse it with usvg and rasterise it with resvg.
//!
//! `cargo run --release -p gasp-math --example math_bench`
//!
//! Set `GASP_BENCH_ENFORCE=1` to fail when a line is over its budget.

use std::collections::HashSet;
use std::process::Command;
use std::time::{Duration, Instant};

use gasp_bench::corpus::corpus_notes;
use gasp_bench::{CountingAllocator, Report, Samples};
use gasp_math::{
    MathCache, MathSnippet, RenderedMath, evict_layout_memory, fill_empty_arguments, find_math,
    latex_to_typst, rasterize_latex, render_latex,
};
use resvg::{tiny_skia, usvg};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const FONT_SIZE: f64 = 16.;
/// The phone's screen scale; the desktop draws at 2x and oversamples 2x.
const PIXELS_PER_POINT: f32 = 3.;
const STARTUP_RUNS: usize = 5;
const STARTUP_CHILD: &str = "--startup-child";
const FIRST_EQUATION: &str = r"\int_0^1 \frac{x^2}{1 + x^2} \, dx";
const TYPED_EQUATIONS: usize = 40;
const SESSION_FRAMES: usize = 60;

fn ms(millis: f64) -> Duration {
    Duration::from_secs_f64(millis / 1000.)
}

fn us(micros: u64) -> Duration {
    Duration::from_micros(micros)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == STARTUP_CHILD) {
        startup_child(args.iter().any(|arg| arg == "pixels"));
        return;
    }
    let equations = corpus_equations();
    let mut report = Report::new("gasp-math");
    report.note_count("corpus equations (unique)", equations.len() as f64);
    startup(&mut report, "svg");
    startup(&mut report, "pixels");
    new_equations(&mut report, &equations);
    to_pixels(&mut report, &equations);
    typing(&mut report, &equations);
    editing_session(&mut report, &equations);
    layout_memory(&mut report, &equations);
    report.finish();
}

/// Unique corpus equations, in corpus order.
fn corpus_equations() -> Vec<MathSnippet> {
    let mut seen = HashSet::new();
    corpus_notes()
        .iter()
        .flat_map(|(_, text)| find_math(text))
        .filter(|snippet| seen.insert((snippet.source.clone(), snippet.display)))
        .collect()
}

fn render_to_pixels(source: &str, display: bool, pixels: bool) -> bool {
    if pixels {
        rasterize_latex(source, display, FONT_SIZE, PIXELS_PER_POINT).is_ok()
    } else {
        render_latex(source, display, FONT_SIZE).is_ok()
    }
}

/// Run in a fresh process: the first equation pays for fonts, the standard
/// library and evaluating the mitex scope.
fn startup_child(pixels: bool) {
    let started = Instant::now();
    let (first, stats) =
        CountingAllocator::measure(|| render_to_pixels(FIRST_EQUATION, true, pixels));
    let cold = started.elapsed();
    assert!(first, "the first equation renders");
    let second = Instant::now();
    assert!(render_to_pixels(r"\sum_{k=1}^n k^2", false, pixels));
    println!(
        "{} {} {}",
        cold.as_nanos(),
        second.elapsed().as_nanos(),
        stats.retained_bytes
    );
}

fn startup(report: &mut Report, mode: &str) {
    let exe = std::env::current_exe().expect("the bench knows where it is");
    let mut cold = Samples::new();
    let mut second = Samples::new();
    let mut retained = 0.;
    for _ in 0..STARTUP_RUNS {
        let output = Command::new(&exe)
            .args([STARTUP_CHILD, mode])
            .output()
            .expect("the startup run starts");
        let text = String::from_utf8_lossy(&output.stdout);
        let numbers: Vec<f64> = text
            .split_whitespace()
            .filter_map(|word| word.parse().ok())
            .collect();
        let [cold_ns, second_ns, bytes] = numbers[..] else {
            panic!("the startup run printed {text:?}");
        };
        cold.push(Duration::from_nanos(cold_ns as u64));
        second.push(Duration::from_nanos(second_ns as u64));
        retained = bytes;
    }
    report.time(
        format!("first equation in a fresh process, {mode} (median)"),
        cold.median(),
        ms(40.),
    );
    report.time(
        format!("the equation after it, {mode} (median)"),
        second.median(),
        ms(1.),
    );
    report.bytes(
        format!("memory kept by the first equation, {mode}"),
        retained,
        10e6,
    );
}

fn new_equations(report: &mut Report, equations: &[MathSnippet]) {
    let _ = render_latex("x", false, FONT_SIZE);
    let convert = Samples::collect(1, || {
        for snippet in equations {
            let _ = latex_to_typst(&snippet.source, snippet.display);
        }
    });
    let mut renders = Samples::new();
    let mut failures = 0;
    let (_, stats) = CountingAllocator::measure(|| {
        for snippet in equations {
            let result = renders.time(|| render_latex(&snippet.source, snippet.display, FONT_SIZE));
            failures += usize::from(result.is_err());
        }
    });
    let (allocations, bytes) = stats.per_operation(equations.len());
    report.note_time(
        "convert only, per equation (mean)",
        convert.total() / equations.len() as u32,
    );
    report.time("new equation, SVG (median)", renders.median(), us(400));
    report.time("new equation, SVG (p95)", renders.p95(), ms(2.));
    report.note_time("new equation, SVG (max)", renders.max());
    report.note_count("allocations per new equation, SVG", allocations);
    report.note_bytes("bytes allocated per new equation, SVG", bytes);
    report.count("equations that fail", failures as f64, 0.);
}

/// The SVG parsed and rasterised to coverage, as the apps did with resvg.
fn svg_coverage(rendered: &RenderedMath) -> Vec<u8> {
    let tree = usvg::Tree::from_str(&rendered.svg, &usvg::Options::default()).expect("SVG parses");
    let width = (rendered.width as f32 * PIXELS_PER_POINT).ceil().max(1.);
    let height = (rendered.height as f32 * PIXELS_PER_POINT).ceil().max(1.);
    let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).expect("an area");
    let size = tree.size();
    let transform = tiny_skia::Transform::from_scale(width / size.width(), height / size.height());
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect()
}

fn svg_to_pixels(source: &str, display: bool) -> Option<Vec<u8>> {
    let rendered = render_latex(source, display, FONT_SIZE).ok()?;
    (rendered.width > 0. && rendered.height > 0.).then(|| svg_coverage(&rendered))
}

/// Every corpus equation to pixels, first as the apps did through SVG and
/// then directly, each after Typst's memory is cleared so both lay out
/// from scratch.
fn to_pixels(report: &mut Report, equations: &[MathSnippet]) {
    evict_layout_memory(0);
    let _ = render_latex("x", false, FONT_SIZE);
    let mut via_svg = Samples::new();
    for snippet in equations {
        via_svg.time(|| svg_to_pixels(&snippet.source, snippet.display));
    }
    evict_layout_memory(0);
    let _ = render_latex("x", false, FONT_SIZE);
    let mut direct = Samples::new();
    let (_, stats) = CountingAllocator::measure(|| {
        for snippet in equations {
            let _ = direct.time(|| {
                rasterize_latex(
                    &snippet.source,
                    snippet.display,
                    FONT_SIZE,
                    PIXELS_PER_POINT,
                )
            });
        }
    });
    report.note_time("new equation to pixels via SVG (median)", via_svg.median());
    report.note_time("new equation to pixels via SVG (p95)", via_svg.p95());
    report.time("new equation to pixels (median)", direct.median(), us(300));
    report.time("new equation to pixels (p95)", direct.p95(), ms(1.5));
    report.note_time("new equation to pixels (max)", direct.max());
    let (allocations, bytes) = stats.per_operation(equations.len());
    report.note_count("allocations per equation to pixels", allocations);
    report.note_bytes("bytes allocated per equation to pixels", bytes);
}

/// Equations of a typical length, spread through the corpus.
fn typed_equations(equations: &[MathSnippet]) -> Vec<&MathSnippet> {
    let typical: Vec<&MathSnippet> = equations
        .iter()
        .filter(|snippet| (20..=80).contains(&snippet.source.len()))
        .collect();
    let step = (typical.len() / TYPED_EQUATIONS).max(1);
    typical
        .into_iter()
        .step_by(step)
        .take(TYPED_EQUATIONS)
        .collect()
}

/// Each keystroke inside an equation renders its source so far to pixels,
/// with empty arguments boxed as the live preview shows them.
fn typing(report: &mut Report, equations: &[MathSnippet]) {
    let typed = typed_equations(equations);
    let keystrokes = |render: &mut dyn FnMut(&str, bool)| {
        let mut samples = Samples::new();
        for snippet in &typed {
            let source = &snippet.source;
            for (at, character) in source.char_indices() {
                let prefix = fill_empty_arguments(&source[..at + character.len_utf8()]);
                samples.time(|| render(&prefix, snippet.display));
            }
        }
        samples
    };
    let via_svg = keystrokes(&mut |source, display| {
        svg_to_pixels(source, display);
    });
    let direct = keystrokes(&mut |source, display| {
        let _ = rasterize_latex(source, display, FONT_SIZE, PIXELS_PER_POINT);
    });
    report.note_count("keystrokes typed inside equations", direct.len() as f64);
    report.note_time("keystroke to pixels via SVG (median)", via_svg.median());
    report.note_time("keystroke to pixels via SVG (p95)", via_svg.p95());
    report.time("keystroke to pixels (median)", direct.median(), us(200));
    report.time("keystroke to pixels (p95)", direct.p95(), ms(1.));
}

/// The note with the most equations, rendered through a [`MathCache`]:
/// opened, scrolled while equations elsewhere are typed, and then redrawn
/// frame after frame.
fn editing_session(report: &mut Report, equations: &[MathSnippet]) {
    let notes = corpus_notes();
    let note = notes
        .iter()
        .map(|(_, text)| find_math(text))
        .max_by_key(Vec::len)
        .unwrap_or_default();
    let typed = typed_equations(equations);
    let mut cache = MathCache::new();
    let (hits, lookups) = run_session(&mut cache, &note, &typed);
    report.note_count("session lookups", lookups as f64);
    report.note_count("session hit rate (%)", (100 * hits / lookups.max(1)) as f64);
    let mut samples = Samples::new();
    let (_, stats) = CountingAllocator::measure(|| {
        for _ in 0..SESSION_FRAMES {
            for snippet in &note {
                let _ = samples.time(|| cache.render(&snippet.source, snippet.display, FONT_SIZE));
            }
        }
    });
    let (allocations, _) = stats.per_operation(samples.len());
    report.time("cache hit (median)", samples.median(), us(2));
    report.count("allocations per cache hit", allocations, 0.5);
    let entries = cache.len();
    let (_, freed) = CountingAllocator::measure(move || drop(cache));
    report.note_count("cache entries after the session", entries as f64);
    report.bytes(
        "cache memory after the session",
        -freed.retained_bytes as f64,
        24e6,
    );
}

/// Returns the hits and lookups of the session.
fn run_session(
    cache: &mut MathCache,
    note: &[MathSnippet],
    typed: &[&MathSnippet],
) -> (usize, usize) {
    let mut hits = 0;
    let mut lookups = 0;
    let mut look_up = |cache: &mut MathCache, source: &str, display: bool| {
        lookups += 1;
        hits += usize::from(cache.contains(source, display, FONT_SIZE));
        let _ = cache.render(source, display, FONT_SIZE);
    };
    for snippet in note {
        look_up(cache, &snippet.source, snippet.display);
    }
    for snippet in typed {
        for frame in 0..3 {
            let visible = note.iter().skip(frame * 10).take(30);
            visible.for_each(|visible| look_up(cache, &visible.source, visible.display));
        }
        let source = &snippet.source;
        for (at, character) in source.char_indices() {
            let prefix = fill_empty_arguments(&source[..at + character.len_utf8()]);
            look_up(cache, &prefix, snippet.display);
            look_up(cache, &prefix, snippet.display);
        }
    }
    (hits, lookups)
}

/// Typst memoises every layout. The live bytes it keeps after the corpus is
/// rendered to pixels, as the apps do, with no explicit eviction.
fn layout_memory(report: &mut Report, equations: &[MathSnippet]) {
    evict_layout_memory(0);
    let started = CountingAllocator::live_bytes();
    for snippet in equations {
        let _ = rasterize_latex(
            &snippet.source,
            snippet.display,
            FONT_SIZE,
            PIXELS_PER_POINT,
        );
    }
    let grown = CountingAllocator::live_bytes() as f64 - started as f64;
    report.bytes("memory kept after the corpus to pixels", grown, 30e6);
}
