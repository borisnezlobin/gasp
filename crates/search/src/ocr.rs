//! Text recognised in a vault's images and PDFs, so search finds them.
//!
//! Recognition belongs to the platform (Apple's Vision framework on the
//! Mac and iPhone); this module keeps what it found, one entry per file
//! with the size and modification time it was read at, so each file is
//! read once, and searches it. The cache lives on the device, outside the
//! vault, in a plain text file: one entry a line, fields split by tabs,
//! with tabs, line breaks and backslashes escaped.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::engine::{LineHit, fold, line_hits};

/// Files whose text is recognised, by extension.
pub const RECOGNISED_EXTENSIONS: [&str; 9] = [
    "png", "jpg", "jpeg", "gif", "webp", "heic", "tiff", "bmp", "pdf",
];

/// Whether `path` names a file whose text is recognised.
pub fn is_recognised(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            RECOGNISED_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// A file's size and modification time, which say whether it changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileStamp {
    pub modified: u64,
    pub size: u64,
}

impl FileStamp {
    /// The stamp of the file at `path` now, or `None` when it's gone.
    pub fn of(path: &Path) -> Option<FileStamp> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs());
        Some(FileStamp {
            modified,
            size: metadata.len(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    stamp: FileStamp,
    text: String,
}

/// The recognised text of each file, by vault-relative path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OcrCache {
    entries: BTreeMap<String, Entry>,
}

/// A file whose recognised text matches a search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHit {
    /// Vault-relative.
    pub path: String,
    /// The matching lines of its text, as a note's are shown.
    pub hits: Vec<LineHit>,
}

impl OcrCache {
    /// The cache written in `text`; lines it can't read are skipped.
    pub fn parse(text: &str) -> OcrCache {
        let entries = text.lines().filter_map(parse_entry).collect();
        OcrCache { entries }
    }

    /// Reads the cache at `file`, or an empty one when there's none.
    pub fn load(file: &Path) -> OcrCache {
        std::fs::read_to_string(file)
            .map_or_else(|_| OcrCache::default(), |text| Self::parse(&text))
    }

    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for (path, entry) in &self.entries {
            let fields = [
                escape(path),
                entry.stamp.modified.to_string(),
                entry.stamp.size.to_string(),
                escape(&entry.text),
            ];
            text.push_str(&fields.join("\t"));
            text.push('\n');
        }
        text
    }

    pub fn save(&self, file: &Path) -> io::Result<()> {
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(file, self.to_text())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Keeps `text` as what `path` says, read when it had `stamp`.
    pub fn record(&mut self, path: &str, stamp: FileStamp, text: String) {
        self.entries.insert(path.to_owned(), Entry { stamp, text });
    }

    /// Of `files` (vault-relative, with their stamps now), the ones never
    /// read or changed since, and forgets files no longer among them.
    pub fn to_read(&mut self, files: &[(String, FileStamp)]) -> Vec<String> {
        self.entries
            .retain(|path, _| files.iter().any(|(file, _)| file == path));
        files
            .iter()
            .filter(|(path, stamp)| {
                self.entries
                    .get(path)
                    .is_none_or(|entry| entry.stamp != *stamp)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// The files whose text holds `query`, ignoring case and diacritics,
    /// in path order.
    pub fn search(&self, query: &str) -> Vec<FileHit> {
        let query = fold(query.trim()).text;
        if query.is_empty() {
            return Vec::new();
        }
        self.entries
            .iter()
            .filter_map(|(path, entry)| {
                let matches = fold(&entry.text).find_all(&query);
                (!matches.is_empty()).then(|| FileHit {
                    path: path.clone(),
                    hits: line_hits(&entry.text, &matches),
                })
            })
            .collect()
    }
}

fn parse_entry(line: &str) -> Option<(String, Entry)> {
    let mut fields = line.split('\t');
    let path = unescape(fields.next()?);
    let modified = fields.next()?.parse().ok()?;
    let size = fields.next()?.parse().ok()?;
    let text = unescape(fields.next().unwrap_or_default());
    Some((
        path,
        Entry {
            stamp: FileStamp { modified, size },
            text,
        },
    ))
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => {}
            other => escaped.push(other),
        }
    }
    escaped
}

fn unescape(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            plain.push(character);
            continue;
        }
        match characters.next() {
            Some('t') => plain.push('\t'),
            Some('n') => plain.push('\n'),
            Some(other) => plain.push(other),
            None => {}
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(modified: u64) -> FileStamp {
        FileStamp { modified, size: 10 }
    }

    #[test]
    fn the_cache_round_trips_through_its_file_text() {
        let mut cache = OcrCache::default();
        cache.record(
            "images/a b.png",
            stamp(5),
            "Line one\nwith\ttab \\ slash".into(),
        );
        cache.record("scan.pdf", stamp(6), String::new());
        assert_eq!(OcrCache::parse(&cache.to_text()), cache);
    }

    #[test]
    fn only_new_and_changed_files_are_read_again() {
        let mut cache = OcrCache::default();
        cache.record("same.png", stamp(1), "kept".into());
        cache.record("changed.png", stamp(1), "old".into());
        cache.record("gone.png", stamp(1), "old".into());
        let files = [
            ("same.png".to_owned(), stamp(1)),
            ("changed.png".to_owned(), stamp(2)),
            ("new.png".to_owned(), stamp(1)),
        ];
        assert_eq!(cache.to_read(&files), ["changed.png", "new.png"]);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn search_finds_words_in_recognised_text_ignoring_case() {
        let mut cache = OcrCache::default();
        cache.record(
            "board.png",
            stamp(1),
            "Lecture 4\nThe Schrödinger equation".into(),
        );
        cache.record("other.png", stamp(1), "nothing here".into());
        let found = cache.search("schrodinger");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "board.png");
        assert_eq!(found[0].hits[0].line, 1);
        assert!(is_recognised("Scan.PDF") && !is_recognised("Note.md"));
    }
}
