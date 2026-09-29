//! Reading the vault for the link index: the first build, spread over
//! the machine's cores, and re-reading the paths the watcher says
//! changed. Both run off the main thread; the workspace's
//! app's vault index applies what they find.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::index::{FileNames, LinkIndex, is_note_path};
use super::parse::{ParsedNote, parse_note};

/// What reading one changed path found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkChange {
    /// A note and its text, parsed.
    Note(String, Arc<str>, ParsedNote),
    /// A file that isn't a note, which links can still point at.
    File(String),
}

/// Reads the files at `paths`, and everything in any that is a folder.
/// Paths that no longer exist are left out; removals are the caller's.
pub fn read_changes(root: &Path, paths: &[PathBuf]) -> Vec<LinkChange> {
    let mut changes = Vec::new();
    for path in paths {
        let Some(relative) = relative(root, path) else {
            continue;
        };
        let Ok(metadata) = std::fs::metadata(path) else {
            continue;
        };
        if metadata.is_dir() {
            let (files, notes) = scan(root, path);
            changes.extend(files.into_iter().map(LinkChange::File));
            changes.extend(read_notes(root, notes).into_iter().map(note_change));
        } else if is_note_path(&relative) {
            changes.extend(read_note(root, relative).map(note_change));
        } else {
            changes.push(LinkChange::File(relative));
        }
    }
    changes
}

fn note_change((path, text, parsed): (String, Arc<str>, ParsedNote)) -> LinkChange {
    LinkChange::Note(path, text, parsed)
}

/// `path` relative to `root`, `/`-separated, if it's inside and visible.
pub fn relative(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.is_empty() || parts.iter().any(|part| part.starts_with('.')) {
        return None;
    }
    Some(parts.join("/"))
}

/// Reads and indexes every note in the vault, spreading the reading and
/// parsing over the machine's cores.
pub fn build_index(root: &Path) -> LinkIndex {
    let (files, notes) = scan(root, root);
    let names = FileNames::new(files.iter().chain(&notes));
    let notes = in_parallel(notes, |path| {
        let (path, text, parsed) = read_note(root, path)?;
        let resolved = names.resolve_all(&path, &parsed.links);
        Some((path, text, parsed, resolved))
    });
    LinkIndex::from_resolved(names, notes)
}

/// Every visible file under `folder`: files that aren't notes, then notes.
pub fn scan(root: &Path, folder: &Path) -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    let mut notes = Vec::new();
    let is_root = folder
        .strip_prefix(root)
        .is_ok_and(|rest| rest.as_os_str().is_empty());
    let start = if is_root {
        Some(String::new())
    } else {
        relative(root, folder)
    };
    // Each folder with its path relative to the root, so an entry's path
    // is its folder's plus its name.
    let mut pending: Vec<(PathBuf, String)> = start
        .map(|prefix| (folder.to_path_buf(), prefix))
        .into_iter()
        .collect();
    while let Some((dir, prefix)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let relative = join_relative(&prefix, &name);
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push((entry.path(), relative)),
                Ok(_) if is_note_path(&relative) => notes.push(relative),
                Ok(_) => files.push(relative),
                Err(_) => {}
            }
        }
    }
    (files, notes)
}

fn join_relative(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// Reads and parses `notes` on as many threads as there are cores.
pub fn read_notes(root: &Path, notes: Vec<String>) -> Vec<(String, Arc<str>, ParsedNote)> {
    in_parallel(notes, |path| read_note(root, path))
}

/// `work` on every path, spread over the machine's cores, keeping what
/// it returns.
pub fn in_parallel<T: Send>(
    paths: Vec<String>,
    work: impl Fn(String) -> Option<T> + Sync,
) -> Vec<T> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |count| count.get())
        .min(8);
    if paths.len() < 64 || threads == 1 {
        return paths.into_iter().filter_map(work).collect();
    }
    let chunk = paths.len().div_ceil(threads);
    let work = &work;
    std::thread::scope(|scope| {
        let workers: Vec<_> = paths
            .chunks(chunk)
            .map(|chunk| {
                scope.spawn(move || chunk.iter().cloned().filter_map(work).collect::<Vec<_>>())
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect()
    })
}

pub fn read_note(root: &Path, path: String) -> Option<(String, Arc<str>, ParsedNote)> {
    let text = std::fs::read_to_string(root.join(&path)).ok()?;
    let parsed = parse_note(&text);
    Some((path, Arc::from(text), parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_from_disk_leaving_hidden_folders_out() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("a/.hidden")).unwrap();
        std::fs::create_dir_all(root.join(".obsidian")).unwrap();
        std::fs::write(root.join("a/One.md"), "[[Two]] #x").unwrap();
        std::fs::write(root.join("Two.md"), "![[pic.png]]").unwrap();
        std::fs::write(root.join("a/pic.png"), "").unwrap();
        std::fs::write(root.join("a/.hidden/Three.md"), "[[Two]]").unwrap();
        std::fs::write(root.join(".obsidian/x.md"), "[[Two]]").unwrap();
        let index = build_index(root);
        assert_eq!(index.note_count(), 2);
        assert_eq!(index.linking_to("Two.md"), ["a/One.md"]);
        assert_eq!(index.linking_to("a/pic.png"), ["Two.md"]);
    }

    #[test]
    fn changes_are_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("new")).unwrap();
        std::fs::write(root.join("new/A.md"), "[[B]]").unwrap();
        std::fs::write(root.join("B.md"), "").unwrap();
        std::fs::write(root.join("new/pic.png"), "").unwrap();
        let changes = read_changes(root, &[root.join("new"), root.join("gone.md")]);
        assert_eq!(changes.len(), 2);
        let mut index = LinkIndex::new();
        index.set_note("B.md", "");
        for change in changes {
            match change {
                LinkChange::Note(path, text, parsed) => index.set_parsed_note(&path, text, parsed),
                LinkChange::File(path) => index.add_file(&path),
            }
        }
        assert!(index.contains_file("new/pic.png"));
        assert_eq!(index.linking_to("B.md"), ["new/A.md"]);
    }
}
