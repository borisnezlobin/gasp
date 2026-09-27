//! Loading the note to edit: one file, or a folder of notes joined into
//! one long note for benchmarks.

use std::io;
use std::path::{Path, PathBuf};

/// Lines a folder of notes is repeated up to.
pub const LONG_NOTE_LINES: usize = 5_000;

/// Text to edit and where its images live.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedNote {
    pub text: String,
    pub image_dirs: Vec<PathBuf>,
}

/// Reads a note, or joins every Markdown file under a folder (repeating
/// them if needed) until the result has at least `min_lines` lines.
pub fn load(path: &Path, min_lines: usize) -> io::Result<LoadedNote> {
    if path.is_file() {
        return Ok(LoadedNote {
            text: std::fs::read_to_string(path)?,
            image_dirs: path.parent().map(Path::to_path_buf).into_iter().collect(),
        });
    }
    let notes = markdown_files(path)?;
    if notes.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no Markdown files in that folder",
        ));
    }
    let texts = notes
        .iter()
        .map(std::fs::read_to_string)
        .collect::<io::Result<Vec<_>>>()?;
    Ok(LoadedNote {
        text: join_until(&texts, min_lines),
        image_dirs: note_dirs(&notes),
    })
}

/// Joins texts in order, cycling, until there are `min_lines` lines.
pub fn join_until(texts: &[String], min_lines: usize) -> String {
    let mut joined = String::new();
    if texts.iter().all(String::is_empty) {
        return joined;
    }
    let mut lines = 0;
    for text in texts.iter().cycle() {
        if lines >= min_lines {
            break;
        }
        let text = text.trim_end_matches('\n');
        joined.push_str(text);
        joined.push_str("\n\n");
        lines += text.lines().count() + 1;
    }
    joined
}

/// Every Markdown file under `root`, sorted, skipping hidden folders such
/// as `.git`, `.obsidian`, `.editor` and `.trash`.
pub fn markdown_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if is_hidden(&path) {
                continue;
            }
            // The listing says what each entry is; only a symlink needs a
            // look at what it points to.
            let is_dir = match entry.file_type() {
                Ok(kind) if !kind.is_symlink() => kind.is_dir(),
                _ => path.is_dir(),
            };
            if is_dir {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn note_dirs(notes: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = notes
        .iter()
        .filter_map(|note| note.parent().map(Path::to_path_buf))
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_and_repeats_until_long_enough() {
        let texts = vec!["a\nb\n".to_owned(), "c".to_owned()];
        let joined = join_until(&texts, 7);
        assert_eq!(joined, "a\nb\n\nc\n\na\nb\n\n");
    }

    #[test]
    fn empty_texts_do_not_loop_forever() {
        assert_eq!(join_until(&[String::new()], 10), "");
    }

    #[test]
    fn loads_the_corpus_as_one_long_note() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus");
        if !corpus.is_dir() {
            return;
        }
        let note = load(&corpus, LONG_NOTE_LINES).unwrap();
        assert!(note.text.lines().count() >= LONG_NOTE_LINES);
        assert!(!note.image_dirs.is_empty());
    }
}
