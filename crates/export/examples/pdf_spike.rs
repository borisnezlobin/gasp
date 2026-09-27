//! Phase 0 Typst PDF spike: exports one note and reports timings.
//!
//! `cargo run --release -p editor-export --example pdf_spike -- <note.md> <out.pdf>
//!  [--vault DIR] [--fonts DIR] [--typ OUT.typ] [--png OUT.png] [--all-pages] [--runs N] [--cold]`
//!
//! Memoized layout is cleared before each run, so the times are for a note
//! seen for the first time (fonts are parsed once, before the runs).
//! `--png` renders page 1 at 2x (every page with `--all-pages`); `--typ` writes the generated Typst source.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use editor_export::pdf::{
    PdfOptions, compile_note, evict_memory, fonts_for, load_fonts, typst_source, warm_up, write_pdf,
};

#[derive(Default)]
struct Args {
    note: PathBuf,
    out: PathBuf,
    vault: Option<PathBuf>,
    fonts: Vec<PathBuf>,
    typ: Option<PathBuf>,
    png: Option<PathBuf>,
    runs: usize,
    all_pages: bool,
    cold: bool,
}

/// Applies one `--flag value` option.
fn apply_option(args: &mut Args, flag: &str, value: String) -> Result<(), String> {
    match flag {
        "--vault" => args.vault = Some(value.into()),
        "--fonts" => args.fonts.push(value.into()),
        "--typ" => args.typ = Some(value.into()),
        "--png" => args.png = Some(value.into()),
        "--runs" => args.runs = value.parse().map_err(|_| "--runs takes a number")?,
        _ => return Err(format!("unknown option {flag}")),
    }
    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        runs: 5,
        ..Args::default()
    };
    let mut positional = Vec::new();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--all-pages" {
            args.all_pages = true;
        } else if arg == "--cold" {
            args.cold = true;
        } else if arg.starts_with("--") {
            let value = iter.next().ok_or(format!("{arg} needs a value"))?;
            apply_option(&mut args, &arg, value)?;
        } else {
            positional.push(PathBuf::from(arg));
        }
    }
    let [note, out] =
        <[PathBuf; 2]>::try_from(positional).map_err(|_| "usage: pdf_spike <note.md> <out.pdf>")?;
    args.note = note;
    args.out = out;
    Ok(args)
}

fn millis(duration: Duration) -> String {
    format!("{:.1} ms", duration.as_secs_f64() * 1000.0)
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times.get(times.len() / 2).copied().unwrap_or_default()
}

/// Writes page 1 to `path`, or every page to `path` with `.N` before the
/// extension.
fn render_pages(
    document: &typst_layout::PagedDocument,
    path: &std::path::Path,
    all_pages: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let render_options = typst_render::RenderOptions {
        pixel_per_pt: typst::utils::Scalar::new(2.0),
        render_bleed: false,
    };
    let count = if all_pages { document.pages().len() } else { 1 };
    for (index, page) in document.pages().iter().take(count).enumerate() {
        let target = if all_pages {
            path.with_extension(format!("{}.png", index + 1))
        } else {
            path.to_path_buf()
        };
        typst_render::render(page, &render_options).save_png(target)?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let markdown = std::fs::read_to_string(&args.note)?;
    let options = PdfOptions::default();

    let started = Instant::now();
    if args.cold {
        println!("cold start: no warm-up");
    } else {
        warm_up(&options);
    }
    let mut fonts = load_fonts(&args.fonts);
    fonts.extend(fonts_for(&options));
    println!("setup (fonts, library): {}", millis(started.elapsed()));

    let mut convert_times = Vec::new();
    let mut layout_times = Vec::new();
    let mut pdf_times = Vec::new();
    let mut output = None;
    for run in 0..args.runs.max(1) {
        evict_memory(0);
        let started = Instant::now();
        let note = typst_source(&markdown, Some(&args.note), args.vault.as_deref(), &options);
        let converted = Instant::now();
        let compiled = compile_note(&note, &fonts)?;
        let laid_out = Instant::now();
        let pdf = write_pdf(&compiled.document)?;
        let written = Instant::now();
        println!(
            "run {run}: convert {}, layout {}, pdf {}, total {}",
            millis(converted - started),
            millis(laid_out - converted),
            millis(written - laid_out),
            millis(written - started)
        );
        convert_times.push(converted - started);
        layout_times.push(laid_out - converted);
        pdf_times.push(written - laid_out);
        output = Some((note, compiled, pdf));
    }
    let Some((note, compiled, pdf)) = output else {
        return Ok(());
    };
    let started = Instant::now();
    let memoized = compile_note(&note, &fonts)?;
    write_pdf(&memoized.document)?;
    println!(
        "re-export of the unchanged note (memoized layout): {}",
        millis(started.elapsed())
    );
    println!(
        "median: convert {}, layout {}, pdf {}",
        millis(median(convert_times)),
        millis(median(layout_times)),
        millis(median(pdf_times))
    );
    println!(
        "{} pages, {} bytes, {} equations ({} not converted, {} failed to typeset), {} images",
        compiled.document.pages().len(),
        pdf.len(),
        note.body.math.len() + note.body.unconverted_math.len(),
        note.body.unconverted_math.len(),
        compiled.replaced_math.len(),
        note.body.images.len()
    );
    for site in &compiled.replaced_math {
        println!("  equation shown as source: {}", site.latex);
    }
    std::fs::write(&args.out, &pdf)?;
    if let Some(path) = &args.typ {
        std::fs::write(path, note.source())?;
    }
    if let Some(path) = &args.png {
        render_pages(&compiled.document, path, args.all_pages)?;
    }
    Ok(())
}
