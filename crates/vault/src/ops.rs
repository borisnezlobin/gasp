//! File operations behind the tree: creating, renaming, moving and
//! trashing, plus updating links after a rename.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use gasp_config::settings::TrashMode;

use super::entries::{EntryKind, NOTE_EXTENSION, is_hidden};
use crate::build::in_parallel;
use crate::link_update::{LinkUpdater, expand_folder_move};

/// Where the vault-trash mode puts deleted files, as Obsidian does.
pub const VAULT_TRASH_DIR: &str = ".trash";

/// Characters that can't appear in a name, because file systems or links
/// would break on them.
const FORBIDDEN_CHARACTERS: &[char] =
    &['\\', ':', '*', '?', '"', '<', '>', '|', '[', ']', '#', '^'];

/// Why a name was refused, as a short sentence to show under the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    StartsWithDot,
    Forbidden(char),
    Exists,
    IntoItself,
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameError::Empty => f.write_str("Give it a name."),
            NameError::StartsWithDot => f.write_str("Names can’t start with a dot."),
            NameError::Forbidden(ch) => write!(f, "Names can’t contain {ch}"),
            NameError::Exists => f.write_str("Something with that name is already here."),
            NameError::IntoItself => f.write_str("A folder can’t move into itself."),
        }
    }
}

/// What a rename or move did, for the workspace to follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Renamed {
    /// Relative to the vault root.
    pub from: PathBuf,
    pub to: PathBuf,
    /// Notes whose links were rewritten, at their new paths.
    pub updated_notes: Vec<PathBuf>,
}

/// Checks a typed name. A name may contain `/` to put the entry in a
/// subfolder, but no part of it may be empty or start with a dot.
pub fn validate_name(name: &str) -> Result<(), NameError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if let Some(ch) = name
        .chars()
        .find(|ch| FORBIDDEN_CHARACTERS.contains(ch) || ch.is_control())
    {
        return Err(NameError::Forbidden(ch));
    }
    for part in name.split('/') {
        if part.trim().is_empty() {
            return Err(NameError::Empty);
        }
        if is_hidden(part.trim()) {
            return Err(NameError::StartsWithDot);
        }
    }
    Ok(())
}

/// The file name for a typed name: notes get `.md` unless it's already there.
pub fn file_name_for(typed: &str, kind: EntryKind) -> String {
    let typed = typed.trim();
    let suffix = format!(".{NOTE_EXTENSION}");
    if kind == EntryKind::Note && !typed.to_lowercase().ends_with(&suffix) {
        format!("{typed}{suffix}")
    } else {
        typed.to_string()
    }
}

/// `Untitled`, or `Untitled 2`, `Untitled 3`… whichever is free in `folder`.
pub fn unique_name(root: &Path, folder: &Path, base: &str, kind: EntryKind) -> String {
    (1..)
        .map(|n| match n {
            1 => base.to_string(),
            n => format!("{base} {n}"),
        })
        .find(|name| !root.join(folder).join(file_name_for(name, kind)).exists())
        .unwrap_or_else(|| base.to_string())
}

/// Creates an empty note or folder called `typed` in `folder`.
pub fn create(root: &Path, folder: &Path, typed: &str, kind: EntryKind) -> io::Result<PathBuf> {
    validate_name(typed).map_err(invalid)?;
    let relative = folder.join(file_name_for(typed, kind));
    let path = root.join(&relative);
    if path.exists() {
        return Err(invalid(NameError::Exists));
    }
    if kind == EntryKind::Folder {
        fs::create_dir_all(&path)?;
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
    }
    Ok(relative)
}

/// Renames or moves `from` to `to` (both relative to the root), then
/// rewrites links to it across the vault when `update_links` is set.
pub fn rename(root: &Path, from: &Path, to: &Path, update_links: bool) -> io::Result<Renamed> {
    check_destination(root, from, to)?;
    let files = if update_links {
        vault_files(root)
    } else {
        Vec::new()
    };
    let destination = root.join(to);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(root.join(from), &destination)?;
    let updated_notes = if update_links {
        update_links_after_move(root, &files, from, to)?
    } else {
        Vec::new()
    };
    Ok(Renamed {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        updated_notes,
    })
}

/// Moves `from` into `folder`, keeping its name.
pub fn move_into(
    root: &Path,
    from: &Path,
    folder: &Path,
    update_links: bool,
) -> io::Result<Renamed> {
    let name = from.file_name().ok_or_else(|| invalid(NameError::Empty))?;
    rename(root, from, &folder.join(name), update_links)
}

fn check_destination(root: &Path, from: &Path, to: &Path) -> io::Result<()> {
    if to.starts_with(from) && to != from {
        return Err(invalid(NameError::IntoItself));
    }
    if to.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(invalid(NameError::Forbidden('/')));
    }
    // A change of case only is allowed even where the file system ignores case.
    let same_ignoring_case = path_key(from) == path_key(to);
    if root.join(to).exists() && !same_ignoring_case {
        return Err(invalid(NameError::Exists));
    }
    Ok(())
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// Deletes `path` the way the vault's settings ask.
pub fn trash(root: &Path, path: &Path, mode: TrashMode) -> io::Result<()> {
    let full = root.join(path);
    match mode {
        TrashMode::System => move_to_system_trash(root, path),
        TrashMode::Vault => move_to_vault_trash(root, path),
        TrashMode::Delete if full.is_dir() => fs::remove_dir_all(full),
        TrashMode::Delete => fs::remove_file(full),
    }
}

#[cfg(not(target_os = "ios"))]
fn move_to_system_trash(root: &Path, path: &Path) -> io::Result<()> {
    trash::delete(root.join(path)).map_err(io::Error::other)
}

/// An iPhone app's files have no system trash to go to, so they go to the
/// vault's own.
#[cfg(target_os = "ios")]
fn move_to_system_trash(root: &Path, path: &Path) -> io::Result<()> {
    move_to_vault_trash(root, path)
}

fn move_to_vault_trash(root: &Path, path: &Path) -> io::Result<()> {
    let trash_dir = root.join(VAULT_TRASH_DIR);
    fs::create_dir_all(&trash_dir)?;
    let name = path
        .file_name()
        .ok_or_else(|| invalid(NameError::Empty))?
        .to_string_lossy()
        .into_owned();
    let free = (1..)
        .map(|n| match n {
            1 => name.clone(),
            n => format!("{n} {name}"),
        })
        .find(|candidate| !trash_dir.join(candidate).exists())
        .unwrap_or(name);
    fs::rename(root.join(path), trash_dir.join(free))
}

/// Every visible file in the vault, as `/`-separated relative paths.
pub fn vault_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_files(root, Path::new(""), &mut files);
    files
}

fn collect_files(root: &Path, folder: &Path, out: &mut Vec<String>) {
    let Ok(read) = fs::read_dir(root.join(folder)) else {
        return;
    };
    for item in read.flatten() {
        let Ok(name) = item.file_name().into_string() else {
            continue;
        };
        if is_hidden(&name) {
            continue;
        }
        let relative = folder.join(&name);
        if is_folder(&item) {
            collect_files(root, &relative, out);
        } else {
            out.push(slash_path(&relative));
        }
    }
}

/// Whether a listed entry is a folder, following links. Only a link is
/// looked up on disk; the listing says what everything else is.
fn is_folder(item: &fs::DirEntry) -> bool {
    match item.file_type() {
        Ok(kind) if !kind.is_symlink() => kind.is_dir(),
        _ => item.path().is_dir(),
    }
}

/// A relative path with `/` separators on every platform.
pub fn slash_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Rewrites links in every note after `from` moved to `to`. `files` is the
/// vault's file list from before the move. Returns the notes it changed.
pub fn update_links_after_move(
    root: &Path,
    files: &[String],
    from: &Path,
    to: &Path,
) -> io::Result<Vec<PathBuf>> {
    let (from, to) = (slash_path(from), slash_path(to));
    let mut moves = expand_folder_move(files, &from, &to);
    if files.contains(&from) {
        moves.push((from, to));
    }
    let updater = LinkUpdater::new(files, &moves);
    let mut moved: HashMap<&str, &str> = HashMap::with_capacity(moves.len());
    for (old, new) in &moves {
        moved.entry(old.as_str()).or_insert(new.as_str());
    }
    let notes: Vec<String> = files.iter().filter(|f| is_note(f)).cloned().collect();
    // Every note is read and checked on every core; the rewritten ones are
    // then written one at a time, in order, stopping at the first failure.
    let rewritten = in_parallel(notes, |note| {
        let now = moved.get(note.as_str()).copied().unwrap_or(&note);
        let path = root.join(now);
        let text = fs::read_to_string(&path).ok()?;
        let updated = updater.rewrite(&note, &text)?;
        Some((path, PathBuf::from(now), updated))
    });
    let mut changed = Vec::new();
    for (path, now, updated) in rewritten {
        crate::files::atomic_write(&path, &updated)?;
        changed.push(now);
    }
    Ok(changed)
}

fn is_note(path: &str) -> bool {
    EntryKind::of_file(path) == Some(EntryKind::Note)
}

fn invalid(error: NameError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Projects/images")).unwrap();
        fs::create_dir_all(dir.path().join(".obsidian")).unwrap();
        let files = [
            ("Plan.md", "See [[Roadmap]] and ![[chart.png]]."),
            ("Projects/Roadmap.md", "Back to [[Plan]]. [p](../Plan.md)"),
            ("Projects/images/chart.png", ""),
            ("Daily.md", "[[Projects/Roadmap|road]]"),
            (".obsidian/workspace.md", "[[Roadmap]]"),
        ];
        for (path, text) in files {
            fs::write(dir.path().join(path), text).unwrap();
        }
        dir
    }

    fn read(dir: &tempfile::TempDir, path: &str) -> String {
        fs::read_to_string(dir.path().join(path)).unwrap()
    }

    #[test]
    fn names_are_checked() {
        assert_eq!(validate_name("  "), Err(NameError::Empty));
        assert_eq!(validate_name(".secret"), Err(NameError::StartsWithDot));
        assert_eq!(validate_name("a|b"), Err(NameError::Forbidden('|')));
        assert_eq!(validate_name("a//b"), Err(NameError::Empty));
        assert_eq!(validate_name("Sub/.x"), Err(NameError::StartsWithDot));
        assert_eq!(validate_name("Projects/New plan"), Ok(()));
        assert_eq!(validate_name("Café 2"), Ok(()));
    }

    #[test]
    fn notes_get_their_extension_once() {
        assert_eq!(file_name_for("Idea", EntryKind::Note), "Idea.md");
        assert_eq!(file_name_for("Idea.md", EntryKind::Note), "Idea.md");
        assert_eq!(file_name_for("Ideas", EntryKind::Folder), "Ideas");
        assert_eq!(file_name_for("a.png", EntryKind::Image), "a.png");
    }

    #[test]
    fn unique_names_count_up() {
        let dir = vault();
        let root = dir.path();
        assert_eq!(
            unique_name(root, Path::new(""), "Untitled", EntryKind::Note),
            "Untitled"
        );
        create(root, Path::new(""), "Untitled", EntryKind::Note).unwrap();
        assert_eq!(
            unique_name(root, Path::new(""), "Untitled", EntryKind::Note),
            "Untitled 2"
        );
    }

    #[test]
    fn create_makes_notes_and_folders() {
        let dir = vault();
        let note = create(dir.path(), Path::new("Projects"), "Idea", EntryKind::Note).unwrap();
        assert_eq!(note, Path::new("Projects/Idea.md"));
        assert!(dir.path().join("Projects/Idea.md").is_file());
        let folder = create(dir.path(), Path::new(""), "Archive", EntryKind::Folder).unwrap();
        assert!(dir.path().join(folder).is_dir());
        let again = create(dir.path(), Path::new("Projects"), "Idea", EntryKind::Note);
        assert_eq!(again.unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn renaming_a_note_updates_links_across_the_vault() {
        let dir = vault();
        let renamed = rename(
            dir.path(),
            Path::new("Projects/Roadmap.md"),
            Path::new("Projects/Roadmap 2025.md"),
            true,
        )
        .unwrap();
        assert_eq!(
            read(&dir, "Plan.md"),
            "See [[Roadmap 2025]] and ![[chart.png]]."
        );
        assert_eq!(read(&dir, "Daily.md"), "[[Projects/Roadmap 2025|road]]");
        // Hidden folders aren't part of the vault's notes.
        assert_eq!(read(&dir, ".obsidian/workspace.md"), "[[Roadmap]]");
        let mut updated = renamed.updated_notes.clone();
        updated.sort();
        assert_eq!(
            updated,
            [PathBuf::from("Daily.md"), PathBuf::from("Plan.md")]
        );
    }

    #[test]
    fn renaming_without_link_updates_leaves_notes_alone() {
        let dir = vault();
        rename(
            dir.path(),
            Path::new("Plan.md"),
            Path::new("Master.md"),
            false,
        )
        .unwrap();
        assert_eq!(
            read(&dir, "Projects/Roadmap.md"),
            "Back to [[Plan]]. [p](../Plan.md)"
        );
    }

    #[test]
    fn moving_a_note_fixes_its_relative_links() {
        let dir = vault();
        move_into(
            dir.path(),
            Path::new("Projects/Roadmap.md"),
            Path::new(""),
            true,
        )
        .unwrap();
        assert_eq!(read(&dir, "Roadmap.md"), "Back to [[Plan]]. [p](Plan.md)");
        assert_eq!(read(&dir, "Daily.md"), "[[Roadmap|road]]");
    }

    #[test]
    fn renaming_a_folder_updates_paths_inside_it() {
        let dir = vault();
        rename(dir.path(), Path::new("Projects"), Path::new("Work"), true).unwrap();
        assert_eq!(read(&dir, "Daily.md"), "[[Work/Roadmap|road]]");
        assert_eq!(
            read(&dir, "Work/Roadmap.md"),
            "Back to [[Plan]]. [p](../Plan.md)"
        );
    }

    #[test]
    fn renames_refuse_to_overwrite_or_nest() {
        let dir = vault();
        let root = dir.path();
        let taken = rename(root, Path::new("Plan.md"), Path::new("Daily.md"), true);
        assert!(taken.is_err());
        let nested = rename(
            root,
            Path::new("Projects"),
            Path::new("Projects/images/P"),
            true,
        );
        assert!(nested.is_err());
        let escape = rename(root, Path::new("Plan.md"), Path::new("../Plan.md"), true);
        assert!(escape.is_err());
        assert!(root.join("Plan.md").exists());
    }

    #[test]
    fn a_rename_can_create_its_folder() {
        let dir = vault();
        rename(
            dir.path(),
            Path::new("Plan.md"),
            Path::new("Plans/2025/Plan.md"),
            true,
        )
        .unwrap();
        assert!(dir.path().join("Plans/2025/Plan.md").is_file());
    }

    #[test]
    fn vault_trash_keeps_both_copies() {
        let dir = vault();
        let root = dir.path();
        trash(root, Path::new("Plan.md"), TrashMode::Vault).unwrap();
        fs::write(root.join("Plan.md"), "again").unwrap();
        trash(root, Path::new("Plan.md"), TrashMode::Vault).unwrap();
        assert!(root.join(".trash/Plan.md").is_file());
        assert!(root.join(".trash/2 Plan.md").is_file());
        assert!(!root.join("Plan.md").exists());
    }

    #[test]
    fn delete_mode_removes_folders_too() {
        let dir = vault();
        trash(dir.path(), Path::new("Projects"), TrashMode::Delete).unwrap();
        trash(dir.path(), Path::new("Daily.md"), TrashMode::Delete).unwrap();
        assert!(!dir.path().join("Projects").exists());
        assert!(!dir.path().join("Daily.md").exists());
    }

    #[test]
    fn vault_files_skip_hidden_folders() {
        let dir = vault();
        let mut files = vault_files(dir.path());
        files.sort();
        assert_eq!(
            files,
            [
                "Daily.md",
                "Plan.md",
                "Projects/Roadmap.md",
                "Projects/images/chart.png"
            ]
        );
    }
}
