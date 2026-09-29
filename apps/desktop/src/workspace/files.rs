//! Reading and writing notes on disk: atomic saves, line endings, note
//! names and finding the vault a note belongs to.

use std::path::{Path, PathBuf};

use gasp_config::CONFIG_DIR;
use gasp_config::names::LEGACY_CONFIG_DIR;

pub use gasp_vault::files::atomic_write;

/// The extension every note has.
pub const NOTE_EXTENSION: &str = "md";

/// The name new notes start from.
pub const UNTITLED: &str = "Untitled";

/// Folders that mark a vault root, the legacy config folder included
/// until the vault is opened and it moves.
const VAULT_MARKERS: [&str; 3] = [".obsidian", CONFIG_DIR, LEGACY_CONFIG_DIR];

/// Characters a note title can't contain, because file systems or links
/// reserve them.
const FORBIDDEN_TITLE_CHARS: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// How a file breaks its lines. New lines typed into a CRLF note are saved
/// as CRLF too, so a note keeps the style it came with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    /// The style of the first line break in `text`, or LF when there's none.
    pub fn detect(text: &str) -> LineEnding {
        match text.find('\n') {
            Some(at) if at > 0 && text.as_bytes()[at - 1] == b'\r' => LineEnding::CrLf,
            _ => LineEnding::Lf,
        }
    }

    /// `text` with every bare `\n` written in this style. LF text is left
    /// byte for byte as it is.
    pub fn apply(self, text: &str) -> String {
        if self == LineEnding::Lf {
            return text.to_owned();
        }
        let mut out = String::with_capacity(text.len() + text.len() / 32);
        let mut previous = '\0';
        for ch in text.chars() {
            if ch == '\n' && previous != '\r' {
                out.push('\r');
            }
            out.push(ch);
            previous = ch;
        }
        out
    }
}

/// Whether `path` names a note.
pub fn is_note(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == NOTE_EXTENSION)
}

/// Whether `path` is inside a hidden folder or is itself hidden, such as
/// `.gasp/device.toml` or a save's temporary file.
pub fn is_hidden(path: &Path, vault: &Path) -> bool {
    let relative = path.strip_prefix(vault).unwrap_or(path);
    relative
        .components()
        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
}

/// The title a note shows: its file name without the extension.
pub fn note_title(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The folder's own name, for the window title.
pub fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// `path` as people read it, with the home folder written `~`.
pub fn display_path(path: &Path) -> String {
    display_path_from(path, dirs::home_dir().as_deref())
}

fn display_path_from(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `Untitled.md`, or `Untitled 1.md`, `Untitled 2.md` and so on, whichever
/// is free in `dir`.
pub fn unique_untitled(dir: &Path) -> PathBuf {
    unique_note_path(dir, UNTITLED)
}

/// `<base>.md` in `dir`, numbered when the name is taken.
pub fn unique_note_path(dir: &Path, base: &str) -> PathBuf {
    let first = dir.join(format!("{base}.{NOTE_EXTENSION}"));
    if !first.exists() {
        return first;
    }
    (1..)
        .map(|number| dir.join(format!("{base} {number}.{NOTE_EXTENSION}")))
        .find(|candidate| !candidate.exists())
        .expect("some number is free")
}

/// Why a title can't become a file name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleError {
    Empty,
    ForbiddenCharacter(char),
}

/// The title trimmed, or why it can't name a file.
pub fn clean_title(title: &str) -> Result<&str, TitleError> {
    let title = title.trim();
    if title.is_empty() || title.chars().all(|ch| ch == '.') {
        return Err(TitleError::Empty);
    }
    match title.chars().find(|ch| FORBIDDEN_TITLE_CHARS.contains(ch)) {
        Some(ch) => Err(TitleError::ForbiddenCharacter(ch)),
        None => Ok(title),
    }
}

/// Where a note named `title` lives, next to `path`.
pub fn renamed_path(path: &Path, title: &str) -> PathBuf {
    let dir = path.parent().unwrap_or(Path::new(""));
    dir.join(format!("{title}.{NOTE_EXTENSION}"))
}

/// The vault a note belongs to: the nearest folder above it that has
/// `.obsidian` or `.gasp`, or else the note's own folder.
pub fn vault_for_note(note: &Path) -> PathBuf {
    let parent = note.parent().unwrap_or(Path::new("."));
    parent
        .ancestors()
        .find(|dir| VAULT_MARKERS.iter().any(|marker| dir.join(marker).is_dir()))
        .unwrap_or(parent)
        .to_path_buf()
}

/// Every note under `vault` with its modification time, newest first,
/// skipping hidden folders. Stops after `limit` folders' worth of reading
/// so a huge vault can't stall the caller.
pub fn notes_by_recency(vault: &Path, limit: usize) -> Vec<PathBuf> {
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    let mut pending = vec![vault.to_path_buf()];
    let mut visited = 0;
    while let Some(dir) = pending.pop() {
        visited += 1;
        if visited > limit {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            collect_entry(&entry, &mut pending, &mut found);
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, path)| path).collect()
}

fn collect_entry(
    entry: &std::fs::DirEntry,
    pending: &mut Vec<PathBuf>,
    found: &mut Vec<(std::time::SystemTime, PathBuf)>,
) {
    let path = entry.path();
    if entry.file_name().to_string_lossy().starts_with('.') {
        return;
    }
    let Ok(metadata) = entry.metadata() else {
        return;
    };
    if metadata.is_dir() {
        pending.push(path);
    } else if is_note(&path) {
        let modified = metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
        found.push((modified, path));
    }
}

/// `path` with its links resolved, as the vault's own path is kept: a
/// note named through a link to the vault, such as macOS's `/var` for
/// `/private/var`, is still the vault's. A path that's gone, as a note's
/// old path after a move, resolves through its folder.
pub(crate) fn canonical_path(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok().or_else(|| {
        let folder = std::fs::canonicalize(path.parent()?).ok()?;
        Some(folder.join(path.file_name()?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_under_home_read_from_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(
            display_path_from(Path::new("/Users/me/Notes"), Some(home)),
            "~/Notes"
        );
        assert_eq!(display_path_from(home, Some(home)), "~");
        assert_eq!(
            display_path_from(Path::new("/Volumes/x"), Some(home)),
            "/Volumes/x"
        );
    }

    #[test]
    fn line_endings_are_detected_and_kept() {
        assert_eq!(LineEnding::detect("a\r\nb\n"), LineEnding::CrLf);
        assert_eq!(LineEnding::detect("a\nb\r\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("no breaks"), LineEnding::Lf);
        assert_eq!(LineEnding::CrLf.apply("a\r\nb\nc"), "a\r\nb\r\nc");
        assert_eq!(LineEnding::Lf.apply("a\r\nb\n"), "a\r\nb\n");
    }

    #[test]
    fn untitled_names_count_up() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_untitled(dir.path()), dir.path().join("Untitled.md"));
        std::fs::write(dir.path().join("Untitled.md"), "").unwrap();
        assert_eq!(
            unique_untitled(dir.path()),
            dir.path().join("Untitled 1.md")
        );
        std::fs::write(dir.path().join("Untitled 1.md"), "").unwrap();
        assert_eq!(
            unique_untitled(dir.path()),
            dir.path().join("Untitled 2.md")
        );
    }

    #[test]
    fn titles_reject_reserved_characters() {
        assert_eq!(clean_title("  Plans "), Ok("Plans"));
        assert_eq!(clean_title("   "), Err(TitleError::Empty));
        assert_eq!(clean_title(".."), Err(TitleError::Empty));
        assert_eq!(clean_title("a/b"), Err(TitleError::ForbiddenCharacter('/')));
    }

    #[test]
    fn a_note_finds_its_marked_vault() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".obsidian")).unwrap();
        std::fs::create_dir_all(dir.path().join("sub/deeper")).unwrap();
        let note = dir.path().join("sub/deeper/n.md");
        assert_eq!(vault_for_note(&note), dir.path());
        let loose = tempfile::tempdir().unwrap();
        let note = loose.path().join("n.md");
        assert_eq!(vault_for_note(&note), loose.path());
    }

    #[test]
    fn hidden_paths_are_recognised() {
        let vault = Path::new("/v");
        assert!(is_hidden(
            &vault.join(CONFIG_DIR).join("device.toml"),
            vault
        ));
        assert!(is_hidden(Path::new("/v/notes/.a.md.1.tmp"), vault));
        assert!(!is_hidden(Path::new("/v/notes/a.md"), vault));
    }

    #[test]
    fn recent_notes_skip_hidden_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".obsidian")).unwrap();
        std::fs::write(dir.path().join(".obsidian/x.md"), "").unwrap();
        std::fs::write(dir.path().join("a.md"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        assert_eq!(
            notes_by_recency(dir.path(), 100),
            vec![dir.path().join("a.md")]
        );
    }
}
