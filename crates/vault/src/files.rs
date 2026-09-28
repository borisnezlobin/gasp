//! Writing files so a crash never leaves half of one.

use std::io::{self, Write};
use std::path::Path;

/// Writes `contents` to a temporary file next to `path`, then renames it
/// over `path`, so a crash never leaves half a note.
pub fn atomic_write(path: &Path, contents: &str) -> io::Result<()> {
    atomic_write_bytes(path, contents.as_bytes())
}

/// [`atomic_write`] for any file, such as an attachment.
pub fn atomic_write_bytes(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a note needs a folder"))?;
    let name = path
        .file_name()
        .map_or_else(Default::default, |name| name.to_string_lossy().into_owned());
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = write_and_sync(&temp, contents).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn write_and_sync(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "old").unwrap();
        atomic_write(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(names.len(), 1);
    }
}
