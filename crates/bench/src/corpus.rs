//! Fixtures built from the synthetic corpus in `fixtures/corpus`, which
//! mirrors the owner's vault: 204 notes with math, callouts, footnotes,
//! tables and images.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Overrides the corpus folder.
pub const CORPUS_VARIABLE: &str = "GASP_CORPUS";

pub fn corpus_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(CORPUS_VARIABLE) {
        return PathBuf::from(dir);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus")
}

/// Every Markdown note in the corpus as its vault-relative path (with `/`)
/// and text, sorted by path.
pub fn corpus_notes() -> Vec<(String, String)> {
    let root = corpus_dir();
    let mut notes: Vec<(String, String)> = files_under(&root)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .map(|path| {
            let text = fs::read_to_string(&path).expect("a corpus note reads");
            (relative_path(&root, &path), text)
        })
        .collect();
    notes.sort();
    notes
}

/// Every file under `root`, depth first, skipping hidden folders.
pub fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if path.is_dir() && !hidden {
                folders.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn relative_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let parts: Vec<String> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}

/// One long note of at least `min_bytes`, made by joining corpus notes (the
/// first keeps its frontmatter, the rest lose theirs).
pub fn long_note(min_bytes: usize) -> String {
    let notes = corpus_notes();
    let mut text = String::with_capacity(min_bytes + 16 * 1024);
    for (index, (_, note)) in notes.iter().cycle().enumerate() {
        if text.len() >= min_bytes {
            break;
        }
        let body = if index == 0 {
            note.as_str()
        } else {
            without_frontmatter(note)
        };
        text.push_str(body.trim_end());
        text.push_str("\n\n");
    }
    text
}

/// The note's text after a leading `---` frontmatter block, if it has one.
pub fn without_frontmatter(note: &str) -> &str {
    let Some(rest) = note.strip_prefix("---\n") else {
        return note;
    };
    match rest.find("\n---\n") {
        Some(end) => &rest[end + 5..],
        None => note,
    }
}

/// A folder that is removed with everything in it when dropped. It sits in
/// `GASP_BENCH_TMP` when that is set, or else the system's temporary folder.
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn new(name: &str) -> Self {
        let base = std::env::var_os("GASP_BENCH_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = base.join(format!("gasp-bench-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the scratch folder is created");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Copies the corpus (notes and attachments) into `dest` `copies` times, the
/// first copy at the top and the others under `copy-N/`, so a bench can work
/// on a vault several times the owner's size. Returns the files written.
pub fn copy_corpus(dest: &Path, copies: usize) -> usize {
    let root = corpus_dir();
    let files = files_under(&root);
    for copy in 0..copies {
        let target = match copy {
            0 => dest.to_path_buf(),
            n => dest.join(format!("copy-{n}")),
        };
        for file in &files {
            let to = target.join(file.strip_prefix(&root).expect("under the corpus"));
            fs::create_dir_all(to.parent().expect("a parent")).expect("folders are created");
            fs::copy(file, &to).expect("a corpus file copies");
        }
    }
    files.len() * copies
}

/// Writes `bytes` of incompressible bytes to `path`, standing in for a photo.
pub fn write_binary_file(path: &Path, bytes: usize, seed: u64) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("folders are created");
    let mut file = fs::File::create(path).expect("the file is created");
    let mut state = seed | 1;
    let mut chunk = vec![0u8; 64 * 1024];
    let mut left = bytes;
    while left > 0 {
        for byte in chunk.iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        let take = left.min(chunk.len());
        file.write_all(&chunk[..take]).expect("the file writes");
        left -= take;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corpus_has_its_notes() {
        let notes = corpus_notes();
        assert!(notes.len() >= 200, "{} notes", notes.len());
        assert!(notes.iter().all(|(path, _)| path.ends_with(".md")));
    }

    #[test]
    fn a_long_note_reaches_its_size() {
        let text = long_note(100 * 1024);
        assert!(text.len() >= 100 * 1024);
    }

    #[test]
    fn frontmatter_is_dropped() {
        assert_eq!(without_frontmatter("---\na: 1\n---\nbody"), "body");
        assert_eq!(without_frontmatter("body"), "body");
    }

    #[test]
    fn binary_files_have_their_size_and_scratch_folders_go_away() {
        let scratch = ScratchDir::new("binary-test");
        let path = scratch.path().join("images/photo.png");
        write_binary_file(&path, 100_000, 7);
        assert_eq!(fs::metadata(&path).unwrap().len(), 100_000);
        let folder = scratch.path().to_path_buf();
        drop(scratch);
        assert!(!folder.exists());
    }
}
