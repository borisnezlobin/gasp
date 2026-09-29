//! Files a note embeds, such as pasted images: where they go and what
//! they're called, as the Paste Image Rename plugin names them.

use std::collections::HashSet;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The attachments folder for `note`: relative settings resolve against
/// the note's folder.
pub fn attachments_dir(note: &Path, attachments: &str) -> PathBuf {
    let folder = Path::new(attachments);
    if folder.has_root() {
        return folder.to_path_buf();
    }
    let base = note.parent().unwrap_or(Path::new(""));
    let mut dir = base.to_path_buf();
    for component in folder.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                dir.pop();
            }
            other => dir.push(other),
        }
    }
    dir
}

/// `<note>-<n>.<extension>` with the smallest `n` from 1 whose name (with
/// any extension) isn't in `existing_stems`.
pub fn next_attachment_name(
    existing_stems: &HashSet<String>,
    note_stem: &str,
    extension: &str,
) -> String {
    let n = (1..)
        .find(|n| !existing_stems.contains(&format!("{note_stem}-{n}")))
        .expect("there is always a free number");
    format!("{note_stem}-{n}.{extension}")
}

/// The file stems in `dir`, or none when it doesn't exist yet.
fn stems_in(dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return HashSet::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .collect()
}

/// Writes `bytes` as the next free attachment for the note and returns the
/// new file's name.
pub fn save_attachment(
    dir: &Path,
    note_stem: &str,
    extension: &str,
    bytes: &[u8],
) -> io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let name = next_attachment_name(&stems_in(dir), note_stem, extension);
    let path = dir.join(&name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    io::Write::write_all(&mut file, bytes)?;
    Ok(name)
}

/// An Obsidian embed of an attachment.
pub fn embed(file_name: &str) -> String {
    format!("![[{file_name}]]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stems(names: &[&str]) -> HashSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn attachment_names_take_the_next_free_number() {
        assert_eq!(
            next_attachment_name(&stems(&[]), "Note", "png"),
            "Note-1.png"
        );
        let taken = stems(&["Note-1", "Note-2", "Other-3"]);
        assert_eq!(next_attachment_name(&taken, "Note", "jpg"), "Note-3.jpg");
        let gap = stems(&["Note-2"]);
        assert_eq!(next_attachment_name(&gap, "Note", "png"), "Note-1.png");
    }

    #[test]
    fn attachments_resolve_next_to_the_note() {
        let note = Path::new("/vault/Maths/Lemma.md");
        assert_eq!(
            attachments_dir(note, "./images"),
            Path::new("/vault/Maths/images")
        );
        assert_eq!(
            attachments_dir(note, "images"),
            Path::new("/vault/Maths/images")
        );
        assert_eq!(
            attachments_dir(note, "../assets"),
            Path::new("/vault/assets")
        );
        assert_eq!(attachments_dir(note, "/abs/pics"), Path::new("/abs/pics"));
    }
}
