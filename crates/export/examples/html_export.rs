//! Exports one note as an article and reports what it contains.
//!
//! `cargo run -p gasp-export --example html_export -- <note.md> <out.html>
//!  [--vault DIR] [--page]`
//!
//! `--page` writes the standalone preview page (title, description and the
//! article stylesheet) instead of the bare article body.

use std::path::PathBuf;
use std::time::Instant;

use gasp_export::html::{HtmlOptions, export_html, standalone_page};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut positional = Vec::new();
    let mut vault = None;
    let mut page = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vault" => vault = args.next().map(PathBuf::from),
            "--page" => page = true,
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let [note, out] = <[PathBuf; 2]>::try_from(positional)
        .map_err(|_| "usage: html_export <note.md> <out.html> [--vault DIR] [--page]")?;
    let markdown = std::fs::read_to_string(&note)?;
    let started = Instant::now();
    let export = export_html(
        &markdown,
        Some(&note),
        vault.as_deref(),
        &HtmlOptions::default(),
    );
    println!(
        "{:.1} ms: \"{}\" ({}), {} bytes, {} equations failed, {} images missing",
        started.elapsed().as_secs_f64() * 1000.0,
        export.title,
        export.slug,
        export.html.len(),
        export.failed_math.len(),
        export.missing_images.len()
    );
    let html = if page {
        standalone_page(&export)
    } else {
        export.html
    };
    std::fs::write(out, html)?;
    Ok(())
}
