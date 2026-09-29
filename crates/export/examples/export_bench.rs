//! Times exporting notes: HTML for the website and PDF through Typst, for
//! a typical corpus note, the one with the most math and a long note, plus
//! the first PDF export in a fresh process, which pays for fonts, Typst's
//! library, the template and the mitex scope.
//!
//! Typst's memoised layouts are cleared before each PDF run, so the times
//! are for a note exported for the first time.
//!
//! `cargo run --release -p gasp-export --example export_bench`
//!
//! Set `GASP_BENCH_ENFORCE=1` to fail when a line is over its budget.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use gasp_bench::corpus::{corpus_dir, corpus_notes, long_note};
use gasp_bench::{CountingAllocator, Report, Samples};
use gasp_export::html::{HtmlOptions, export_html};
use gasp_export::pdf::{PdfOptions, evict_memory, export_pdf, fonts_for};
use gasp_math::find_math;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const LONG_NOTE_BYTES: usize = 100 * 1024;
const STARTUP_RUNS: usize = 3;
const STARTUP_CHILD: &str = "--startup-child";
const HTML_RUNS: usize = 10;
const PDF_RUNS: usize = 5;

fn ms(millis: f64) -> Duration {
    Duration::from_secs_f64(millis / 1000.)
}

struct Note {
    name: &'static str,
    path: PathBuf,
    text: String,
    html_budget: Duration,
    pdf_budget: Duration,
}

fn main() {
    if std::env::args().any(|arg| arg == STARTUP_CHILD) {
        startup_child();
        return;
    }
    let mut report = Report::new("gasp-export");
    startup(&mut report);
    let options = PdfOptions::default();
    let fonts = fonts_for(&options);
    for note in notes() {
        html(&mut report, &note);
        pdf(&mut report, &note, &options, &fonts);
    }
    report.finish();
}

fn notes() -> Vec<Note> {
    let root = corpus_dir();
    let mut corpus = corpus_notes();
    corpus.sort_by_key(|(_, text)| text.len());
    let (typical_path, typical) = corpus[corpus.len() / 2].clone();
    let (math_path, math) = corpus
        .iter()
        .max_by_key(|(_, text)| find_math(text).len())
        .cloned()
        .expect("the corpus has notes");
    vec![
        Note {
            name: "typical note",
            path: root.join(typical_path),
            text: typical,
            html_budget: ms(0.5),
            pdf_budget: ms(60.),
        },
        Note {
            name: "most math",
            path: root.join(math_path),
            text: math,
            html_budget: ms(90.),
            pdf_budget: ms(250.),
        },
        Note {
            name: "long note",
            path: root.join("Long note.md"),
            text: long_note(LONG_NOTE_BYTES),
            html_budget: ms(450.),
            pdf_budget: ms(1500.),
        },
    ]
}

fn html(report: &mut Report, note: &Note) {
    let options = HtmlOptions::default();
    let root = corpus_dir();
    let export = || export_html(&note.text, Some(&note.path), Some(&root), &options);
    let mut samples = Samples::new();
    for _ in 0..HTML_RUNS {
        evict_memory(0);
        samples.time(export);
    }
    evict_memory(0);
    let (_, stats) = CountingAllocator::measure(export);
    report.time(
        format!("{}: HTML (median)", note.name),
        samples.median(),
        note.html_budget,
    );
    report.note_time(format!("{}: HTML (best)", note.name), samples.min());
    report.note_bytes(
        format!("{}: HTML bytes allocated", note.name),
        stats.allocated_bytes as f64,
    );
}

fn pdf(report: &mut Report, note: &Note, options: &PdfOptions, fonts: &[typst::text::Font]) {
    let root = corpus_dir();
    let mut samples = Samples::new();
    let mut pages = 0;
    for _ in 0..PDF_RUNS {
        evict_memory(0);
        let export =
            samples.time(|| export_pdf(&note.text, Some(&note.path), Some(&root), options, fonts));
        pages = export.expect("the note exports").pages;
    }
    report.note_count(format!("{}: pages", note.name), pages as f64);
    report.time(
        format!("{}: PDF (median)", note.name),
        samples.median(),
        note.pdf_budget,
    );
    report.note_time(format!("{}: PDF (best)", note.name), samples.min());
}

/// Run in a fresh process: the first export pays for the fonts, the
/// standard library, the template and the mitex scope.
fn startup_child() {
    let root = corpus_dir();
    let (path, text) = corpus_notes()
        .into_iter()
        .find(|(_, text)| !find_math(text).is_empty())
        .expect("a note with math");
    let path = root.join(path);
    let options = PdfOptions::default();
    let started = Instant::now();
    let fonts = fonts_for(&options);
    let fonts_found = started.elapsed();
    export_pdf(&text, Some(&path), Some(&root), &options, &fonts).expect("the note exports");
    let first = started.elapsed();
    let html = Instant::now();
    export_html(&text, Some(&path), Some(&root), &HtmlOptions::default());
    println!(
        "{} {} {}",
        first.as_nanos(),
        fonts_found.as_nanos(),
        html.elapsed().as_nanos()
    );
}

fn startup(report: &mut Report) {
    let exe = std::env::current_exe().expect("the bench knows where it is");
    let mut first = Samples::new();
    let mut fonts = Samples::new();
    let mut html = Samples::new();
    for _ in 0..STARTUP_RUNS {
        let output = Command::new(&exe)
            .arg(STARTUP_CHILD)
            .output()
            .expect("the startup run starts");
        let text = String::from_utf8_lossy(&output.stdout);
        let numbers: Vec<u64> = text
            .split_whitespace()
            .filter_map(|word| word.parse().ok())
            .collect();
        let [first_ns, fonts_ns, html_ns] = numbers[..] else {
            panic!("the startup run printed {text:?}");
        };
        first.push(Duration::from_nanos(first_ns));
        fonts.push(Duration::from_nanos(fonts_ns));
        html.push(Duration::from_nanos(html_ns));
    }
    report.time(
        "first PDF export in a fresh process (median)",
        first.median(),
        ms(200.),
    );
    report.note_time("… of which finding the system fonts", fonts.median());
    report.time(
        "first HTML export after it (median)",
        html.median(),
        ms(80.),
    );
}
