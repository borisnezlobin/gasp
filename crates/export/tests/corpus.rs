//! Every note of the synthetic corpus converts and compiles to PDF.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use editor_export::pdf::{PdfError, PdfOptions, compile_note, typst_source, write_pdf};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus")
}

fn notes(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            notes(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            found.push(path);
        }
    }
}

/// Groups failures by their first error message.
fn failure_cause(error: &PdfError) -> String {
    match error {
        PdfError::Compile(errors) => errors.first().map_or_else(
            || "unknown compile error".to_owned(),
            |error| error.message.clone(),
        ),
        PdfError::Pdf(message) => format!("pdf: {message}"),
    }
}

#[test]
fn whole_corpus_compiles() {
    let root = corpus_root();
    let mut paths = Vec::new();
    notes(&root, &mut paths);
    paths.sort();
    assert!(paths.len() > 100, "corpus not found at {}", root.display());

    let options = PdfOptions::default();
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut replaced_math = 0;
    let mut unconverted_math = 0;
    for path in &paths {
        let markdown = std::fs::read_to_string(path).unwrap();
        let note = typst_source(&markdown, Some(path), Some(&root), &options);
        unconverted_math += note.body.unconverted_math.len();
        let result = compile_note(&note, &[]).and_then(|compiled| {
            replaced_math += compiled.replaced_math.len();
            write_pdf(&compiled.document)
        });
        if let Err(error) = result {
            let name = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            failures
                .entry(failure_cause(&error))
                .or_default()
                .push(name);
        }
    }
    println!(
        "{} notes, {} failed; equations shown as source: {unconverted_math} not converted, \
         {replaced_math} failed to typeset",
        paths.len(),
        failures.values().map(Vec::len).sum::<usize>()
    );
    for (cause, notes) in &failures {
        println!("{cause}: {} notes, e.g. {}", notes.len(), notes[0]);
    }
    assert!(failures.is_empty(), "failures by cause: {failures:#?}");
}
