//! Reading an AI app's config file and replacing it in one step, so the
//! app never reads half a file.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::ClientError;

/// The file's text, or `None` when there's no file yet.
pub fn read(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Reads `path`, passes its text to `change`, and writes back what it
/// returns. When `change` refuses, the file stays as it was.
pub fn rewrite(
    path: &Path,
    change: impl FnOnce(Option<&str>) -> Result<String, ClientError>,
) -> Result<(), ClientError> {
    let text = read(path)?;
    let changed = change(text.as_deref())?;
    write_atomically(path, &changed)?;
    Ok(())
}

/// Writes `text` to a temporary file beside `path`, then renames it over
/// `path`. A link is followed, so a config kept in a dotfiles folder stays
/// linked. The new file keeps the old one's permissions.
pub fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    let target = link_target(path);
    let folder = target
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "no folder"))?;
    fs::create_dir_all(folder)?;
    let temporary = temporary_beside(&target);
    let written = fs::write(&temporary, text)
        .and_then(|()| keep_permissions(&target, &temporary))
        .and_then(|()| fs::rename(&temporary, &target));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// The file a link at `path` points to, or `path` itself.
fn link_target(path: &Path) -> PathBuf {
    let is_link = fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
    if is_link {
        return fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    }
    path.to_path_buf()
}

fn temporary_beside(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(".{name}.gasp-{}.tmp", std::process::id()))
}

fn keep_permissions(original: &Path, replacement: &Path) -> io::Result<()> {
    match fs::metadata(original) {
        Ok(meta) => fs::set_permissions(replacement, meta.permissions()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
