//! `editor --bench-index VAULT`: how long the link index takes to build,
//! to follow one saved note, to find a note's unlinked mentions and to
//! rewrite links after a rename, printed as plain lines.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::build::{build_index, in_parallel, read_note, scan};
use super::index::FileNames;
use super::index::LinkIndex;
use super::mentions::MentionSearch;
use super::parse::parse_note;
use crate::link_update::LinkUpdater;

/// Builds the index for `vault` a few times and times the rest against it.
pub fn run(vault: &Path) -> String {
    let mut builds = Vec::new();
    let mut index = LinkIndex::new();
    for _ in 0..3 {
        // Drop the last build first, so it isn't timed with the next one.
        drop(std::mem::take(&mut index));
        let started = Instant::now();
        index = build_index(vault);
        builds.push(started.elapsed());
    }
    let mut report = vec![format!(
        "index: {} notes, built in {}",
        index.note_count(),
        builds
            .iter()
            .map(|time| ms(*time))
            .collect::<Vec<_>>()
            .join(", "),
    )];
    report.push(build_phases(vault));
    let mut notes: Vec<String> = index.note_paths().map(str::to_string).collect();
    notes.sort();
    let hub = notes
        .iter()
        .max_by_key(|note| index.linking_to(note).len())
        .cloned()
        .unwrap_or_default();
    report.push(save_timings(&mut index, &notes));
    report.push(backlink_timing(&index, &hub));
    report.push(mention_timing(&index, &hub));
    report.push(rename_timing(&index, &hub));
    report.join("\n")
}

/// Where a build's time goes: listing files, then on every core reading,
/// parsing and resolving links, then filling the maps on one.
fn build_phases(vault: &Path) -> String {
    let started = Instant::now();
    let (files, notes) = scan(vault, vault);
    let names = FileNames::new(files.iter().chain(&notes));
    let listed = started.elapsed();
    let read = in_parallel(notes.clone(), |path| read_note(vault, path));
    let parsed = started.elapsed();
    let resolved = in_parallel(notes, |path| {
        let (path, text, parsed) = read_note(vault, path)?;
        let resolved = names.resolve_all(&path, &parsed.links);
        Some((path, text, parsed, resolved))
    });
    let with_links = started.elapsed();
    let index = LinkIndex::from_resolved(names, resolved);
    let filled = started.elapsed();
    format!(
        "build phases: list {}, read and parse {} ({} notes), read, parse and resolve {}, fill maps {} ({} notes)",
        ms(listed),
        ms(parsed - listed),
        read.len(),
        ms(with_links - parsed),
        ms(filled - with_links),
        index.note_count()
    )
}

/// A save: parse the new text and update the index, as the watcher does.
fn save_timings(index: &mut LinkIndex, notes: &[String]) -> String {
    let step = (notes.len() / 200).max(1);
    let mut times: Vec<Duration> = notes
        .iter()
        .step_by(step)
        .filter_map(|note| {
            let text = format!("{} [[{}]] #edited\n", index.note(note)?.text, notes[0]);
            let started = Instant::now();
            let parsed = parse_note(&text);
            index.set_parsed_note(note, Arc::from(text), parsed);
            Some(started.elapsed())
        })
        .collect();
    times.sort();
    format!(
        "save: {} notes, median {}, p95 {}, worst {}",
        times.len(),
        ms(percentile(&times, 50)),
        ms(percentile(&times, 95)),
        ms(times.last().copied().unwrap_or_default()),
    )
}

fn backlink_timing(index: &LinkIndex, hub: &str) -> String {
    let started = Instant::now();
    let rows = crate::knowledge::sidebar::backlink_rows(Path::new("/"), index, hub);
    format!(
        "backlinks: {} rows for {hub} in {}",
        rows.len(),
        ms(started.elapsed())
    )
}

fn mention_timing(index: &LinkIndex, hub: &str) -> String {
    let started = Instant::now();
    let search = MentionSearch::new(index, hub);
    let snapshot = started.elapsed();
    let found = search.run(200);
    format!(
        "unlinked mentions: {} found in {} ({} to take the snapshot on the main thread)",
        found.len(),
        ms(started.elapsed()),
        ms(snapshot)
    )
}

fn rename_timing(index: &LinkIndex, hub: &str) -> String {
    let started = Instant::now();
    let files = index.file_paths();
    let renamed = format!("{hub} renamed.md");
    let updater = LinkUpdater::new(&files, &[(hub.to_string(), renamed)]);
    let sources = index.linking_to(hub);
    let changed = sources
        .iter()
        .filter_map(|source| updater.rewrite(source, &index.note(source)?.text))
        .count();
    format!(
        "rename: {changed} of {} linking notes rewritten in {} (in memory)",
        sources.len(),
        ms(started.elapsed())
    )
}

fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    sorted[(sorted.len() * percent / 100).min(sorted.len() - 1)]
}

fn ms(duration: Duration) -> String {
    format!("{:.2} ms", duration.as_secs_f64() * 1000.)
}
