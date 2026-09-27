//! Searching a vault's notes in memory: plain substring matching that
//! ignores case and diacritics, ranked with the owner's Omnisearch weights,
//! and replacing matches across notes on disk.
//!
//! This is the simple first version. The Tantivy index (incremental
//! updates, camelCase splitting) and OCR of images and PDFs are later
//! phases.

use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Omnisearch's weights for where a query matches.
pub const FILE_NAME_WEIGHT: u32 = 10;
pub const FOLDER_WEIGHT: u32 = 7;
pub const HEADING_WEIGHTS: [u32; 3] = [6, 5, 4];
pub const TAG_WEIGHT: u32 = 2;
pub const BODY_WEIGHT: u32 = 1;

/// Lines shown per note; the match count still covers every match.
pub const MAX_HITS_PER_NOTE: usize = 20;
/// Characters of context kept before a match in a long line.
const EXCERPT_LEAD: usize = 40;
/// Longest excerpt, in characters.
const EXCERPT_CHARS: usize = 160;

/// Folders never searched: the app's config, git and other apps' data.
const SKIPPED_FOLDERS: [&str; 4] = [".editor", ".git", ".obsidian", ".trash"];

/// A note in memory.
#[derive(Clone, Debug)]
pub struct Note {
    /// Relative to the vault root.
    pub path: PathBuf,
    pub text: String,
    folded: String,
}

impl Note {
    pub fn new(path: PathBuf, text: String) -> Self {
        let folded = fold(&text).text;
        Self { path, text, folded }
    }
}

/// Text lowercased and stripped of diacritics, with a map back to the
/// original byte offsets.
#[derive(Clone, Debug, Default)]
pub struct Folded {
    pub text: String,
    /// For each folded byte, the original character's byte range.
    origin: Vec<(usize, usize)>,
}

impl Folded {
    /// Every non-overlapping match of the folded `needle`, as ranges in the
    /// original text.
    pub fn find_all(&self, needle: &str) -> Vec<Range<usize>> {
        if needle.is_empty() {
            return Vec::new();
        }
        self.text
            .match_indices(needle)
            .map(|(start, found)| {
                let end = start + found.len() - 1;
                self.origin[start].0..self.origin[end].1
            })
            .collect()
    }
}

/// Lowercases and strips combining marks, so "Émile" matches "emile".
pub fn fold(text: &str) -> Folded {
    let mut folded = Folded {
        text: String::with_capacity(text.len()),
        origin: Vec::with_capacity(text.len()),
    };
    for (start, ch) in text.char_indices() {
        let span = (start, start + ch.len_utf8());
        let lowered = ch
            .to_lowercase()
            .nfd()
            .filter(|mark| !is_combining_mark(*mark));
        for out in lowered {
            folded.text.push(out);
            folded
                .origin
                .extend(std::iter::repeat_n(span, out.len_utf8()));
        }
    }
    folded
}

/// Loads every Markdown note under `root`, sorted by path.
pub fn load_vault(root: &Path) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            if path.is_dir() {
                if !is_skipped(&path) {
                    pending.push(path);
                }
            } else if is_note(&path)
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                notes.push(Note::new(relative, text));
            }
        }
    }
    notes.sort_by(|a, b| a.path.cmp(&b.path));
    notes
}

fn is_skipped(dir: &Path) -> bool {
    dir.file_name()
        .is_some_and(|name| SKIPPED_FOLDERS.iter().any(|skipped| name == *skipped))
}

fn is_note(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "md")
}

/// The parts of a note that carry extra weight, already folded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteFields {
    pub file_name: String,
    pub folder: String,
    /// Heading texts for levels 1, 2 and 3.
    pub headings: [Vec<String>; 3],
    pub tags: Vec<String>,
}

/// Reads the weighted fields of a note.
pub fn note_fields(path: &Path, text: &str) -> NoteFields {
    let file_name = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let folder = path
        .parent()
        .map_or_else(String::new, |parent| parent.to_string_lossy().into_owned());
    let mut fields = NoteFields {
        file_name: fold(&file_name).text,
        folder: fold(&folder).text,
        ..NoteFields::default()
    };
    for line in text.lines() {
        if let Some((level, heading)) = heading_of(line) {
            fields.headings[level - 1].push(fold(heading).text);
        }
        fields.tags.extend(tags_in(line).map(|tag| fold(tag).text));
    }
    fields
}

/// `(level, text)` for an ATX heading of level 1 to 3.
fn heading_of(line: &str) -> Option<(usize, &str)> {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    let rest = &line[hashes..];
    let is_heading = (1..=3).contains(&hashes) && (rest.is_empty() || rest.starts_with(' '));
    is_heading.then(|| (hashes, rest.trim()))
}

/// `#tags` in a line: a `#` at a word start followed by a tag character.
fn tags_in(line: &str) -> impl Iterator<Item = &str> {
    line.match_indices('#').filter_map(move |(start, _)| {
        let at_word_start = line[..start]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        let rest = &line[start + 1..];
        let len = rest
            .find(|ch: char| !(ch.is_alphanumeric() || "_-/".contains(ch)))
            .unwrap_or(rest.len());
        let has_letter = rest[..len].chars().any(char::is_alphabetic);
        (at_word_start && has_letter).then(|| &rest[..len])
    })
}

/// A note's score for a folded query: the weight of every field that
/// contains it, plus [`BODY_WEIGHT`] when the body does.
pub fn score(fields: &NoteFields, in_body: bool, query: &str) -> u32 {
    let mut total = 0;
    if fields.file_name.contains(query) {
        total += FILE_NAME_WEIGHT;
    }
    if fields.folder.contains(query) {
        total += FOLDER_WEIGHT;
    }
    for (headings, weight) in fields.headings.iter().zip(HEADING_WEIGHTS) {
        if headings.iter().any(|heading| heading.contains(query)) {
            total += weight;
        }
    }
    if fields.tags.iter().any(|tag| tag.contains(query)) {
        total += TAG_WEIGHT;
    }
    if in_body {
        total += BODY_WEIGHT;
    }
    total
}

/// A matching line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineHit {
    /// Zero-based line number.
    pub line: usize,
    /// Byte offset of the first match on the line, in the note.
    pub offset: usize,
    pub excerpt: String,
    /// Matches within `excerpt`.
    pub ranges: Vec<Range<usize>>,
}

/// One note's matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteResult {
    /// Relative to the vault root.
    pub path: PathBuf,
    pub score: u32,
    pub match_count: usize,
    pub hits: Vec<LineHit>,
}

/// Searches `notes` for `query`, best first. Stops early and returns
/// nothing when `generation` moves past `current`, so a newer search can
/// take over.
pub fn search(
    notes: &[Note],
    query: &str,
    generation: &AtomicUsize,
    current: usize,
) -> Vec<NoteResult> {
    let query = fold(query.trim()).text;
    if query.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for note in notes {
        if generation.load(Ordering::Relaxed) != current {
            return Vec::new();
        }
        if let Some(result) = search_note(note, &query) {
            results.push(result);
        }
    }
    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.match_count.cmp(&a.match_count))
            .then(a.path.cmp(&b.path))
    });
    results
}

fn search_note(note: &Note, query: &str) -> Option<NoteResult> {
    let in_body = note.folded.contains(query);
    let fields = note_fields(&note.path, &note.text);
    let score = score(&fields, in_body, query);
    if score == 0 {
        return None;
    }
    let matches = if in_body {
        fold(&note.text).find_all(query)
    } else {
        Vec::new()
    };
    Some(NoteResult {
        path: note.path.clone(),
        score,
        match_count: matches.len(),
        hits: line_hits(&note.text, &matches),
    })
}

/// Groups matches by line, one hit per line, up to [`MAX_HITS_PER_NOTE`].
fn line_hits(text: &str, matches: &[Range<usize>]) -> Vec<LineHit> {
    let mut hits: Vec<LineHit> = Vec::new();
    let mut line_number = 0;
    let mut line_start = 0;
    for range in matches {
        while let Some(newline) = text[line_start..range.start].find('\n') {
            line_start += newline + 1;
            line_number += 1;
        }
        let line_end = text[line_start..]
            .find('\n')
            .map_or(text.len(), |end| line_start + end);
        let local = range.start - line_start..range.end.min(line_end) - line_start;
        if let Some(hit) = hits.last_mut().filter(|hit| hit.line == line_number) {
            hit.ranges.push(local);
            continue;
        }
        if hits.len() == MAX_HITS_PER_NOTE {
            break;
        }
        hits.push(LineHit {
            line: line_number,
            offset: range.start,
            excerpt: text[line_start..line_end].to_owned(),
            ranges: vec![local],
        });
    }
    hits.iter_mut().for_each(trim_excerpt);
    hits
}

/// The markup that starts a line, such as `- [x] `, `## ` or `> `, which
/// an excerpt leaves out so it reads as text.
fn markup_prefix_len(line: &str) -> usize {
    let mut rest = line;
    loop {
        let next = strip_one_marker(rest);
        if next.len() == rest.len() {
            return line.len() - rest.len();
        }
        rest = next;
    }
}

fn strip_one_marker(text: &str) -> &str {
    const MARKERS: [&str; 7] = ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ ", "> "];
    if let Some(marker) = MARKERS.iter().find(|marker| text.starts_with(*marker)) {
        return &text[marker.len()..];
    }
    let hashes = text.len() - text.trim_start_matches('#').len();
    if (1..=6).contains(&hashes) && text[hashes..].starts_with(' ') {
        return &text[hashes + 1..];
    }
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 && text[digits..].starts_with(". ") {
        return &text[digits + 2..];
    }
    text
}

/// Shortens a long line around its first match, keeping ranges aligned.
fn trim_excerpt(hit: &mut LineHit) {
    let text = hit.excerpt.trim_end_matches('\r');
    let first = hit.ranges[0].start.min(text.len());
    let indent = text.len() - text.trim_start().len();
    let lead = text[..first]
        .char_indices()
        .rev()
        .nth(EXCERPT_LEAD - 1)
        .map_or(0, |(index, _)| index);
    let body = indent + markup_prefix_len(&text[indent..]);
    let start = lead.max(body).min(first);
    let end = text[start..]
        .char_indices()
        .nth(EXCERPT_CHARS)
        .map_or(text.len(), |(index, _)| start + index);
    let prefix = if start > body { "…" } else { "" };
    let suffix = if end < text.len() { "…" } else { "" };
    let shift = |offset: usize| offset.clamp(start, end) - start + prefix.len();
    hit.ranges = hit
        .ranges
        .iter()
        .map(|range| shift(range.start)..shift(range.end))
        .filter(|range| !range.is_empty())
        .collect();
    hit.excerpt = format!("{prefix}{}{suffix}", &text[start..end]);
}

/// `text` with every match of the (unfolded) `query` replaced, and how
/// many matches there were.
pub fn replace_in_text(text: &str, query: &str, replacement: &str) -> (String, usize) {
    let folded_query = fold(query).text;
    let matches = fold(text).find_all(&folded_query);
    let mut replaced = String::with_capacity(text.len());
    let mut copied = 0;
    for range in &matches {
        replaced.push_str(&text[copied..range.start]);
        replaced.push_str(replacement);
        copied = range.end;
    }
    replaced.push_str(&text[copied..]);
    (replaced, matches.len())
}

/// What a replace across notes did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplaceReport {
    /// Notes rewritten, relative to the vault root.
    pub changed: Vec<PathBuf>,
    pub matches: usize,
    /// Notes that couldn't be read or written, with the reason.
    pub failed: Vec<(PathBuf, String)>,
}

/// Replaces `query` in each note of `paths` (relative to `root`), reading
/// each from disk again so edits since the search aren't lost, and writing
/// each atomically.
pub fn replace_in_vault(
    root: &Path,
    paths: &[PathBuf],
    query: &str,
    replacement: &str,
) -> ReplaceReport {
    let mut report = ReplaceReport::default();
    for path in paths {
        match replace_in_file(&root.join(path), query, replacement) {
            Ok(0) => {}
            Ok(count) => {
                report.matches += count;
                report.changed.push(path.clone());
            }
            Err(error) => report.failed.push((path.clone(), error.to_string())),
        }
    }
    report
}

fn replace_in_file(path: &Path, query: &str, replacement: &str) -> io::Result<usize> {
    let text = std::fs::read_to_string(path)?;
    let (replaced, count) = replace_in_text(&text, query, replacement);
    if count > 0 {
        write_atomically(path, &replaced)?;
    }
    Ok(count)
}

/// Writes through a temporary file in the same folder and renames it over
/// `path`, so a crash never leaves a half-written note.
pub fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let temporary = dir.join(format!(".{name}.replace-tmp"));
    std::fs::write(&temporary, contents)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        std::fs::remove_file(&temporary).ok();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_leave_out_line_markup() {
        assert_eq!(markup_prefix_len("- [x] schedule it"), 6);
        assert_eq!(markup_prefix_len("> - quoted item"), 4);
        assert_eq!(markup_prefix_len("## Heading"), 3);
        assert_eq!(markup_prefix_len("12. Twelfth"), 4);
        assert_eq!(markup_prefix_len("#tag here"), 0);
        let hits = line_hits("- [ ] schedule the test", &[6..14]);
        assert_eq!(hits[0].excerpt, "schedule the test");
        assert_eq!(hits[0].ranges, vec![0..8]);
        // A match inside the markup keeps it.
        let hits = line_hits("- [x] done", &[2..5]);
        assert_eq!(hits[0].excerpt, "[x] done");
    }

    #[test]
    fn folding_ignores_case_and_diacritics() {
        assert_eq!(fold("Émile Ångström").text, "emile angstrom");
        let folded = fold("Café CAFE");
        assert_eq!(folded.find_all("cafe"), vec![0..5, 6..10]);
    }

    #[test]
    fn folded_matches_map_back_to_whole_characters() {
        let text = "naïve İstanbul";
        let folded = fold(text);
        let ranges = folded.find_all(&fold("NAIVE").text);
        assert_eq!(&text[ranges[0].clone()], "naïve");
        let ranges = folded.find_all("istanbul");
        assert_eq!(&text[ranges[0].clone()], "İstanbul");
    }

    #[test]
    fn headings_and_tags_are_read() {
        let fields = note_fields(
            Path::new("Maths/Lemma.md"),
            "# Top\n## Middle\n### Low\n#### Deep\ntext #topic and #2 and a#b",
        );
        assert_eq!(fields.file_name, "lemma");
        assert_eq!(fields.folder, "maths");
        assert_eq!(fields.headings[0], vec!["top"]);
        assert_eq!(fields.headings[1], vec!["middle"]);
        assert_eq!(fields.headings[2], vec!["low"]);
        assert_eq!(fields.tags, vec!["topic"]);
    }

    #[test]
    fn scores_use_the_omnisearch_weights() {
        let fields = NoteFields {
            file_name: "limit ideas".into(),
            folder: "limit".into(),
            headings: [vec!["limit".into()], vec!["limit".into()], vec!["x".into()]],
            tags: vec!["limits".into()],
        };
        assert_eq!(score(&fields, true, "limit"), 10 + 7 + 6 + 5 + 2 + 1);
        assert_eq!(score(&fields, false, "x"), 4);
        assert_eq!(score(&fields, false, "nothing"), 0);
    }

    fn notes(files: &[(&str, &str)]) -> Vec<Note> {
        files
            .iter()
            .map(|(path, text)| Note::new(PathBuf::from(path), (*text).to_owned()))
            .collect()
    }

    fn run(notes: &[Note], query: &str) -> Vec<NoteResult> {
        search(notes, query, &AtomicUsize::new(0), 0)
    }

    #[test]
    fn file_names_rank_above_headings_above_body() {
        let notes = notes(&[
            ("a.md", "the prism is here\nprism again"),
            ("b.md", "# Prism\nbody"),
            ("Prism notes.md", "nothing"),
        ]);
        let results = run(&notes, "PRISM");
        let order: Vec<_> = results.iter().map(|r| r.path.to_str().unwrap()).collect();
        assert_eq!(order, vec!["Prism notes.md", "b.md", "a.md"]);
        assert_eq!(results[2].match_count, 2);
        assert_eq!(results[2].hits.len(), 2);
        assert_eq!(results[2].hits[1].line, 1);
        assert_eq!(results[2].hits[1].offset, 18);
        assert!(results[0].hits.is_empty());
    }

    #[test]
    fn hits_group_matches_by_line_with_excerpt_ranges() {
        let notes = notes(&[("n.md", "first\n  one cat, two cat\nlast")]);
        let hit = &run(&notes, "cat")[0].hits[0];
        assert_eq!(hit.line, 1);
        assert_eq!(hit.excerpt, "one cat, two cat");
        assert_eq!(hit.ranges, vec![4..7, 13..16]);
        assert_eq!(&hit.excerpt[hit.ranges[1].clone()], "cat");
    }

    #[test]
    fn long_lines_are_trimmed_around_the_match() {
        let line = format!("{}needle{}", "a ".repeat(100), " b".repeat(200));
        let notes = notes(&[("n.md", line.as_str())]);
        let hit = &run(&notes, "needle")[0].hits[0];
        assert!(hit.excerpt.starts_with('…') && hit.excerpt.ends_with('…'));
        assert_eq!(&hit.excerpt[hit.ranges[0].clone()], "needle");
    }

    #[test]
    fn stale_searches_stop() {
        let notes = notes(&[("n.md", "match")]);
        assert!(search(&notes, "match", &AtomicUsize::new(2), 1).is_empty());
    }

    #[test]
    fn empty_queries_find_nothing() {
        let notes = notes(&[("n.md", "text")]);
        assert!(run(&notes, "  ").is_empty());
    }

    #[test]
    fn replacing_ignores_case_and_diacritics() {
        assert_eq!(
            replace_in_text("Café and cafe and CAFÉ", "cafe", "tea"),
            ("tea and tea and tea".to_owned(), 3)
        );
        assert_eq!(replace_in_text("none", "x", "y"), ("none".to_owned(), 0));
    }

    #[test]
    fn vaults_load_and_replace_on_disk() {
        let vault = tempfile::tempdir().unwrap();
        let root = vault.path();
        std::fs::create_dir_all(root.join("Maths")).unwrap();
        std::fs::create_dir_all(root.join(".editor")).unwrap();
        std::fs::write(root.join("Maths/Lemma.md"), "old and Old").unwrap();
        std::fs::write(root.join("Other.md"), "nothing").unwrap();
        std::fs::write(root.join(".editor/settings.md"), "old").unwrap();
        std::fs::write(root.join("image.png"), "old").unwrap();
        let loaded = load_vault(root);
        let paths: Vec<_> = loaded.iter().map(|note| note.path.clone()).collect();
        assert_eq!(
            paths,
            vec![PathBuf::from("Maths/Lemma.md"), PathBuf::from("Other.md")]
        );

        let report = replace_in_vault(root, &paths, "old", "new");
        assert_eq!(report.matches, 2);
        assert_eq!(report.changed, vec![PathBuf::from("Maths/Lemma.md")]);
        assert!(report.failed.is_empty());
        let text = std::fs::read_to_string(root.join("Maths/Lemma.md")).unwrap();
        assert_eq!(text, "new and new");
        let leftovers = std::fs::read_dir(root.join("Maths")).unwrap().count();
        assert_eq!(leftovers, 1);
    }
}
