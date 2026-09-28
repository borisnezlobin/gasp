//! What Obsidian users expect beyond the notes themselves: daily notes,
//! templates and the Moment.js dates both use. The desktop app and the
//! iPhone app share these.

pub mod daily;
pub mod dates;
pub mod templates;

use std::io;
use std::path::{Path, PathBuf};

use crate::entries::is_hidden;

/// A note's title: its file name without `.md`.
pub fn note_title(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Every visible note under `root`, as full paths.
pub fn markdown_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if is_hidden(&entry.file_name().to_string_lossy()) {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                found.push(path);
            }
        }
    }
    Ok(found)
}
