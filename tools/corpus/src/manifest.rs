//! `manifest.json`: per-feature counts measured by scanning the generated files.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::scan::{NoteScan, scan_note};
use crate::targets::scaled_targets;
use crate::{FootnoteProblem, Vault};

/// Scans of every note, in vault order.
pub fn scan_vault(vault: &Vault) -> Vec<(String, NoteScan)> {
    vault
        .notes
        .iter()
        .map(|note| (note.path.clone(), scan_note(&note.text)))
        .collect()
}

/// Sum of each count over all notes.
pub fn totals(scans: &[(String, NoteScan)]) -> BTreeMap<String, usize> {
    let mut totals = BTreeMap::new();
    for (_, scan) in scans {
        for (key, count) in &scan.counts {
            *totals.entry(key.clone()).or_insert(0) += count;
        }
    }
    totals
}

/// Number of notes with a non-zero count, per key.
pub fn notes_with(scans: &[(String, NoteScan)]) -> BTreeMap<String, usize> {
    let mut notes = BTreeMap::new();
    for (_, scan) in scans {
        for (key, count) in &scan.counts {
            if *count > 0 {
                *notes.entry(key.clone()).or_insert(0) += 1;
            }
        }
    }
    notes
}

fn tally<'a>(items: impl Iterator<Item = &'a String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item.clone()).or_insert(0) += 1;
    }
    counts
}

fn footnote_problems(scans: &[(String, NoteScan)]) -> Value {
    let mut by_problem: BTreeMap<&'static str, Vec<Value>> = BTreeMap::new();
    for problem in [
        FootnoteProblem::Missing,
        FootnoteProblem::Unused,
        FootnoteProblem::Duplicate,
        FootnoteProblem::Empty,
        FootnoteProblem::Typo,
    ] {
        by_problem.insert(problem.name(), Vec::new());
    }
    for (path, scan) in scans {
        for (label, problem) in scan.footnote_problems() {
            let entry = json!({ "note": path, "label": label });
            by_problem.entry(problem.name()).or_default().push(entry);
        }
    }
    json!(by_problem)
}

fn targets_json(note_count: usize) -> Value {
    let map: Map<String, Value> = scaled_targets(note_count)
        .into_iter()
        .map(|t| {
            (
                t.key.to_string(),
                json!({ "uses": t.uses, "notes": t.notes }),
            )
        })
        .collect();
    Value::Object(map)
}

fn notes_json(vault: &Vault, scans: &[(String, NoteScan)]) -> Value {
    let notes: Vec<Value> = vault
        .notes
        .iter()
        .zip(scans)
        .map(|(note, (_, scan))| json!({ "path": note.path, "bytes": note.text.len(), "counts": scan.counts }))
        .collect();
    Value::Array(notes)
}

/// The manifest as pretty-printed JSON with a trailing newline.
pub fn manifest_json(vault: &Vault) -> String {
    let scans = scan_vault(vault);
    let note_bytes: usize = vault.notes.iter().map(|n| n.text.len()).sum();
    let attachment_bytes: usize = vault.attachments.iter().map(|a| a.bytes.len()).sum();
    let value = json!({
        "generator": "editor-corpus",
        "seed": vault.seed,
        "note_count": vault.notes.len(),
        "attachment_count": vault.attachments.len(),
        "note_bytes": note_bytes,
        "attachment_bytes": attachment_bytes,
        "targets": targets_json(vault.notes.len()),
        "totals": totals(&scans),
        "notes_with": notes_with(&scans),
        "callout_types": tally(scans.iter().flat_map(|(_, s)| &s.callout_types)),
        "code_languages": tally(scans.iter().flat_map(|(_, s)| &s.code_languages)),
        "html_tags": tally(scans.iter().flat_map(|(_, s)| &s.html_tags)),
        "footnote_problems": footnote_problems(&scans),
        "notes": notes_json(vault, &scans),
    });
    let mut text = serde_json::to_string_pretty(&value).expect("manifest serialises");
    text.push('\n');
    text
}
