//! Searching a vault's notes in memory: plain substring matching that
//! ignores case and diacritics, ranked with the owner's Omnisearch weights,
//! and replacing matches across notes on disk.
//!
//! This is the simple first version. The Tantivy index (incremental
//! updates, camelCase splitting) and OCR of images and PDFs are later
//! phases.

use std::collections::BTreeMap;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use gasp_config::CONFIG_DIR;
use rayon::prelude::*;

use crate::fold::OffsetMap;
pub use crate::fold::{Folded, fold};
use crate::hits::collect_hits;
pub use crate::hits::{LineHit, MAX_HITS_PER_NOTE, line_hits};

/// Omnisearch's weights for where a query matches.
pub const FILE_NAME_WEIGHT: u32 = 10;
pub const FOLDER_WEIGHT: u32 = 7;
pub const HEADING_WEIGHTS: [u32; 3] = [6, 5, 4];
pub const TAG_WEIGHT: u32 = 2;
pub const BODY_WEIGHT: u32 = 1;

/// Notes a search thread takes at a time: enough that handing them out
/// costs little next to searching them.
const NOTES_PER_TASK: usize = 16;

/// A newer search started, so this one stopped.
struct Stale;

/// Folders never searched: the app's config, git and other apps' data.
const SKIPPED_FOLDERS: [&str; 4] = [CONFIG_DIR, ".git", ".obsidian", ".trash"];

/// A note in memory, with what every search reads from it prepared once.
/// Cloning one is cheap: the text is shared.
#[derive(Clone, Debug)]
pub struct Note {
    /// Relative to the vault root.
    pub path: PathBuf,
    pub text: Arc<str>,
    folded: Arc<str>,
    /// How offsets in the folded text move back into the text, for a note
    /// where folding changed some character's byte length. Other notes'
    /// folded offsets are offsets in the text.
    offsets: Option<Arc<OffsetMap>>,
    fields: Arc<NoteFields>,
}

impl Note {
    pub fn new(path: PathBuf, text: String) -> Self {
        let folded = fold(&text);
        let fields = Arc::new(note_fields(&path, &text));
        let same_offsets = folded.same_offsets;
        let (folded, offsets) = folded.into_parts();
        Self {
            path,
            text: text.into(),
            folded: folded.into(),
            offsets: (!same_offsets).then(|| Arc::new(offsets)),
            fields,
        }
    }

    /// Every match of the folded `query` in the note, in order, as ranges
    /// in its text.
    pub(crate) fn matches<'a>(&'a self, query: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        self.folded.match_indices(query).map(move |(start, found)| {
            let folded = start..start + found.len();
            match &self.offsets {
                Some(offsets) => offsets.original(folded),
                None => folded,
            }
        })
    }
}

/// Loads every Markdown note under `root`, sorted by path.
pub fn load_vault(root: &Path) -> Vec<Note> {
    let mut cache = NoteCache::default();
    cache.refresh(root);
    cache.notes()
}

/// When a file was last written, as far as telling a change goes.
type Stamp = (Option<SystemTime>, u64);

/// A vault's notes kept in memory between searches. A refresh reads only
/// the notes whose size or modification time changed since last time.
#[derive(Default)]
pub struct NoteCache {
    /// By path, so they list in path order.
    notes: BTreeMap<PathBuf, (Stamp, Note)>,
}

impl NoteCache {
    /// Every note, sorted by path.
    pub fn notes(&self) -> Vec<Note> {
        self.notes.values().map(|(_, note)| note.clone()).collect()
    }

    /// Brings every note under `root` up to date with the disk, reading
    /// the notes that changed on every core.
    pub fn refresh(&mut self, root: &Path) {
        let found = note_stamps(root);
        let unchanged = |(relative, stamp): &(PathBuf, Stamp)| {
            self.notes
                .get(relative)
                .is_some_and(|(seen, _)| seen == stamp)
        };
        if found.len() == self.notes.len() && found.iter().all(unchanged) {
            return;
        }
        let (kept, changed): (Vec<_>, Vec<_>) = found.into_iter().partition(unchanged);
        let read: Vec<(PathBuf, (Stamp, Note))> = changed
            .into_par_iter()
            .filter_map(|(relative, stamp)| {
                let note = read_note(root, &relative)?;
                Some((relative, (stamp, note)))
            })
            .collect();
        let mut old = std::mem::take(&mut self.notes);
        for (relative, _) in kept {
            if let Some(entry) = old.remove(&relative) {
                self.notes.insert(relative, entry);
            }
        }
        self.notes.extend(read);
    }

    /// Reads `paths` (relative to `root`) again: notes that changed, and
    /// files or folders that are gone, whose notes it forgets.
    pub fn reload(&mut self, root: &Path, paths: &[PathBuf]) {
        for relative in paths {
            let absolute = root.join(relative);
            let note = is_note(&absolute)
                .then(|| read_note(root, relative))
                .flatten();
            match note {
                Some(note) => {
                    let stamp = stamp_of(&absolute);
                    self.notes.insert(relative.clone(), (stamp, note));
                }
                None if !absolute.is_dir() => {
                    self.notes.retain(|path, _| !path.starts_with(relative));
                }
                None => {}
            }
        }
    }
}

fn read_note(root: &Path, relative: &Path) -> Option<Note> {
    let text = std::fs::read_to_string(root.join(relative)).ok()?;
    Some(Note::new(relative.to_path_buf(), text))
}

fn stamp_of(path: &Path) -> Stamp {
    std::fs::metadata(path).map_or((None, 0), |meta| (meta.modified().ok(), meta.len()))
}

/// Every note under `root`, relative, with its stamp.
fn note_stamps(root: &Path) -> Vec<(PathBuf, Stamp)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            match listed_kind(&entry, &path) {
                Some(Listed::Folder) if !is_skipped(&path) => pending.push(path),
                Some(Listed::Note(stamp)) => {
                    let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                    found.push((relative, stamp));
                }
                _ => {}
            }
        }
    }
    found
}

/// What a folder listing found that a refresh cares about.
enum Listed {
    Folder,
    Note(Stamp),
}

/// Whether `entry` is a folder or a note, following links. Only notes and
/// links are looked up on disk; the listing already says what the rest are.
fn listed_kind(entry: &std::fs::DirEntry, path: &Path) -> Option<Listed> {
    let plain_file = entry
        .file_type()
        .ok()
        .filter(|kind| !kind.is_symlink())
        .map(|kind| !kind.is_dir());
    if plain_file == Some(false) {
        return Some(Listed::Folder);
    }
    if plain_file == Some(true) && !is_note(path) {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() {
        return Some(Listed::Folder);
    }
    is_note(path).then(|| Listed::Note((meta.modified().ok(), meta.len())))
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
    let searched: Result<Vec<Option<NoteResult>>, Stale> = notes
        .par_iter()
        .with_min_len(NOTES_PER_TASK)
        .map(|note| {
            if generation.load(Ordering::Relaxed) != current {
                return Err(Stale);
            }
            Ok(search_note(note, &query))
        })
        .collect();
    let Ok(searched) = searched else {
        return Vec::new();
    };
    let mut results: Vec<NoteResult> = searched.into_iter().flatten().collect();
    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.match_count.cmp(&a.match_count))
            .then(a.path.cmp(&b.path))
    });
    results
}

fn search_note(note: &Note, query: &str) -> Option<NoteResult> {
    let mut matches = note.matches(query).peekable();
    let in_body = matches.peek().is_some();
    let score = score(&note.fields, in_body, query);
    if score == 0 {
        return None;
    }
    let (hits, match_count) = collect_hits(&note.text, matches);
    Some(NoteResult {
        path: note.path.clone(),
        score,
        match_count,
        hits,
    })
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
    use crate::hits::markup_prefix_len;

    #[test]
    fn excerpts_leave_out_line_markup() {
        assert_eq!(markup_prefix_len("- [x] schedule it"), 6);
        assert_eq!(markup_prefix_len("> - quoted item"), 4);
        assert_eq!(markup_prefix_len("## Heading"), 3);
        assert_eq!(markup_prefix_len("12. Twelfth"), 4);
        assert_eq!(markup_prefix_len("#tag here"), 0);
        let hits = line_hits("- [ ] schedule the test", std::slice::from_ref(&(6..14)));
        assert_eq!(hits[0].excerpt, "schedule the test");
        assert_eq!(hits[0].ranges, vec![0..8]);
        // A match inside the markup keeps it.
        let hits = line_hits("- [x] done", std::slice::from_ref(&(2..5)));
        assert_eq!(hits[0].excerpt, "[x] done");
    }

    #[test]
    fn excerpts_leave_out_inline_markup() {
        let text = "| a | ~~struck~~ **wave** `code` |";
        let at = text.find("wave").unwrap();
        let hits = line_hits(text, std::slice::from_ref(&(at..at + 4)));
        assert_eq!(hits[0].excerpt, "a struck wave code ");
        let range = hits[0].ranges[0].clone();
        assert_eq!(&hits[0].excerpt[range], "wave");
    }

    #[test]
    fn folding_ignores_case_and_diacritics() {
        assert_eq!(fold("Émile Ångström").text, "emile angstrom");
        assert!(!fold("Émile").same_offsets);
        assert!(fold("Plain — ASCII and a dash").same_offsets);
        let folded = fold("Café CAFE");
        assert_eq!(folded.find_all("cafe"), vec![0..5, 6..10]);
    }

    #[test]
    fn notes_find_the_same_matches_either_way() {
        for text in ["Plain CAT — cat, Cat", "Café CAFE café", "İstanbul cat"] {
            let note = Note::new(PathBuf::from("n.md"), text.to_owned());
            for query in ["cat", "cafe", "istanbul"] {
                let found: Vec<_> = note.matches(query).collect();
                assert_eq!(found, fold(text).find_all(query), "{text}");
            }
        }
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
    fn ties_rank_by_match_count_then_path() {
        let notes = notes(&[
            ("c.md", "one cat"),
            ("b.md", "cat cat"),
            ("a.md", "a cat"),
            ("Cats/x.md", "nothing"),
        ]);
        let results = run(&notes, "cat");
        let order: Vec<_> = results.iter().map(|r| r.path.to_str().unwrap()).collect();
        assert_eq!(order, vec!["Cats/x.md", "b.md", "a.md", "c.md"]);
        assert_eq!(results[0].score, FOLDER_WEIGHT);
        assert_eq!(results[1].score, BODY_WEIGHT);
    }

    #[test]
    fn every_match_counts_but_only_the_first_lines_show() {
        let text = "a cat and a cat\n".repeat(30);
        let notes = notes(&[("n.md", text.as_str())]);
        let result = &run(&notes, "cat")[0];
        assert_eq!(result.match_count, 60);
        assert_eq!(result.hits.len(), MAX_HITS_PER_NOTE);
        let last = &result.hits[MAX_HITS_PER_NOTE - 1];
        assert_eq!(last.line, MAX_HITS_PER_NOTE - 1);
        assert_eq!(last.offset, 16 * (MAX_HITS_PER_NOTE - 1) + 2);
        assert_eq!(last.ranges, vec![2..5, 12..15]);
    }

    #[test]
    fn accented_notes_map_matches_to_their_own_text() {
        let text = "Le Café\nİstanbul café, CAFÉ\ne\u{301}cole";
        let notes = notes(&[("n.md", text)]);
        let result = &run(&notes, "cafe")[0];
        assert_eq!(result.match_count, 3);
        let found: Vec<&str> = result
            .hits
            .iter()
            .flat_map(|hit| hit.ranges.iter().map(|range| &hit.excerpt[range.clone()]))
            .collect();
        assert_eq!(found, ["Café", "café", "CAFÉ"]);
        assert_eq!(result.hits[1].offset, text.find("café").unwrap());
        let result = &run(&notes, "ecole")[0];
        assert_eq!(result.hits[0].line, 2);
        assert_eq!(result.hits[0].excerpt, "e\u{301}cole");
        assert_eq!(result.hits[0].ranges, vec![0..7]);
        let result = &run(&notes, "istanbul")[0];
        assert_eq!(result.hits[0].ranges, vec![0..9]);
    }

    #[test]
    fn marks_inside_a_match_stay_and_windows_line_ends_go() {
        let text = "x **bold** and **bold**\r\nlast";
        let notes = notes(&[("n.md", text)]);
        let hit = &run(&notes, "**bold")[0].hits[0];
        assert_eq!(hit.excerpt, "x **bold and **bold");
        assert_eq!(hit.ranges, vec![2..8, 13..19]);
        let hit = &run(&notes, "bold")[0].hits[0];
        assert_eq!(hit.excerpt, "x bold and bold");
        assert_eq!(hit.ranges, vec![2..6, 11..15]);
    }

    #[test]
    fn a_match_across_lines_is_cut_at_the_line_end() {
        let notes = notes(&[("n.md", "one two\nthree")]);
        let result = &run(&notes, "two\nth")[0];
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].excerpt, "one two");
        assert_eq!(result.hits[0].ranges, vec![4..7]);
    }

    #[test]
    fn long_lines_keep_every_match_that_fits() {
        let line = format!("{} cat cat {}", "word ".repeat(30), "tail ".repeat(60));
        let notes = notes(&[("n.md", line.as_str())]);
        let hit = &run(&notes, "cat")[0].hits[0];
        assert!(hit.excerpt.starts_with("…") && hit.excerpt.ends_with('…'));
        assert_eq!(hit.ranges.len(), 2);
        for range in &hit.ranges {
            assert_eq!(&hit.excerpt[range.clone()], "cat");
        }
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
    fn refreshes_follow_edits_additions_and_removals() {
        let vault = tempfile::tempdir().unwrap();
        let root = vault.path();
        let texts = |cache: &NoteCache| -> Vec<(String, String)> {
            cache
                .notes()
                .iter()
                .map(|note| (slash(&note.path), note.text.to_string()))
                .collect()
        };
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::write(root.join("b/two.md"), "two").unwrap();
        std::fs::write(root.join("one.md"), "one").unwrap();
        std::fs::write(root.join("b/pic.png"), "png").unwrap();
        let mut cache = NoteCache::default();
        cache.refresh(root);
        assert_eq!(
            texts(&cache),
            [
                ("b/two.md".into(), "two".into()),
                ("one.md".into(), "one".into())
            ]
        );
        std::fs::write(root.join("one.md"), "one, longer").unwrap();
        std::fs::remove_file(root.join("b/two.md")).unwrap();
        std::fs::write(root.join("a.md"), "a").unwrap();
        cache.refresh(root);
        assert_eq!(
            texts(&cache),
            [
                ("a.md".into(), "a".into()),
                ("one.md".into(), "one, longer".into())
            ]
        );
        cache.refresh(root);
        assert_eq!(texts(&cache).len(), 2);
    }

    fn slash(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn vaults_load_and_replace_on_disk() {
        let vault = tempfile::tempdir().unwrap();
        let root = vault.path();
        std::fs::create_dir_all(root.join("Maths")).unwrap();
        std::fs::create_dir_all(root.join(CONFIG_DIR)).unwrap();
        std::fs::write(root.join("Maths/Lemma.md"), "old and Old").unwrap();
        std::fs::write(root.join("Other.md"), "nothing").unwrap();
        std::fs::write(root.join(CONFIG_DIR).join("settings.md"), "old").unwrap();
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
