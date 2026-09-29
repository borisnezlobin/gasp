//! Turning the paths an agent sends into files inside the vault.
//!
//! Every tool takes vault-relative paths with `/` separators. A path that
//! could reach outside the vault is refused before anything touches the
//! disk: absolute paths, `..`, and symbolic links that lead out. Hidden
//! files and folders (`.git`, `.gasp`, `.obsidian`, `.trash`) are off
//! limits too; the settings tools reach `.gasp` on their own terms, and
//! nothing ever reaches `.git`.

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::tool::ToolError;

/// A file or folder inside the vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultPath {
    /// Vault-relative, `/`-separated, as tools report it.
    pub relative: String,
    pub absolute: PathBuf,
}

impl VaultPath {
    /// The vault root itself, for listing from the top.
    pub fn root(root: &Path) -> VaultPath {
        VaultPath {
            relative: String::new(),
            absolute: root.to_path_buf(),
        }
    }

    pub fn exists(&self) -> bool {
        self.absolute.exists()
    }

    pub fn is_note(&self) -> bool {
        is_note_name(&self.relative)
    }
}

/// Whether a file name or path is a note's.
pub fn is_note_name(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".md")
}

/// Resolves `input` inside `root`, which must already be canonical.
pub fn resolve(root: &Path, input: &str) -> Result<VaultPath, ToolError> {
    let parts = relative_parts(input)?;
    let absolute = parts
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part));
    check_links(root, &parts)?;
    Ok(VaultPath {
        relative: parts.join("/"),
        absolute,
    })
}

/// Like [`resolve`], for a note: a path without `.md` gets it, so `Plans`
/// names `Plans.md` as a wikilink would.
pub fn resolve_note(root: &Path, input: &str) -> Result<VaultPath, ToolError> {
    let trimmed = input.trim();
    if is_note_name(trimmed) {
        return resolve(root, trimmed);
    }
    resolve(root, &format!("{trimmed}.md"))
}

/// Like [`resolve`], for anything that isn't a note.
pub fn resolve_attachment(root: &Path, input: &str) -> Result<VaultPath, ToolError> {
    let path = resolve(root, input)?;
    if path.is_note() {
        return Err(ToolError::new(format!(
            "{} is a note; use the note tools for it",
            path.relative
        )));
    }
    Ok(path)
}

/// A folder to list from, where an empty one is the whole vault.
pub fn resolve_folder(root: &Path, input: Option<&str>) -> Result<VaultPath, ToolError> {
    match input
        .map(str::trim)
        .filter(|folder| !folder.is_empty() && *folder != ".")
    {
        Some(folder) => resolve(root, folder.trim_end_matches('/')),
        None => Ok(VaultPath::root(root)),
    }
}

/// The path's parts, refusing any that could leave the vault or reach
/// a hidden folder.
fn relative_parts(input: &str) -> Result<Vec<String>, ToolError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ToolError::new("the path is empty"));
    }
    if input.contains('\\') || input.contains('\0') {
        return Err(ToolError::new(format!(
            "{input:?} isn't a vault path: use / between folders"
        )));
    }
    let mut parts = Vec::new();
    for component in Path::new(input).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(visible_part(input, part)?),
            Component::ParentDir => return Err(outside(input, "`..` leaves the vault")),
            Component::RootDir | Component::Prefix(_) => {
                return Err(outside(input, "paths are relative to the vault"));
            }
        }
    }
    if parts.is_empty() {
        return Err(ToolError::new(format!("{input:?} names no file")));
    }
    Ok(parts)
}

fn visible_part(input: &str, part: &std::ffi::OsStr) -> Result<String, ToolError> {
    let part = part.to_string_lossy();
    if part.starts_with('.') {
        return Err(ToolError::new(format!(
            "{input:?} is inside a hidden file or folder, which these tools don't touch"
        )));
    }
    Ok(part.into_owned())
}

fn outside(input: &str, why: &str) -> ToolError {
    ToolError::new(format!("{input:?} is outside the vault: {why}"))
}

/// Refuses a path that passes through a symbolic link leading out of the
/// vault. Parts that don't exist yet can't be links, so the check stops
/// at the first one.
fn check_links(root: &Path, parts: &[String]) -> Result<(), ToolError> {
    let mut path = root.to_path_buf();
    for part in parts {
        path.push(part);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(ToolError::io(&path_label(root, &path), &error)),
        };
        if !metadata.file_type().is_symlink() {
            continue;
        }
        let inside = std::fs::canonicalize(&path).is_ok_and(|target| target.starts_with(root));
        if !inside {
            let label = path_label(root, &path);
            return Err(ToolError::new(format!(
                "{label} is a link that leads outside the vault"
            )));
        }
    }
    Ok(())
}

fn path_label(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("Notes")).unwrap();
        std::fs::write(root.join("Notes/Plan.md"), "plan").unwrap();
        (dir, root)
    }

    #[test]
    fn plain_paths_resolve_inside_the_vault() {
        let (_dir, root) = vault();
        let path = resolve(&root, "Notes/Plan.md").unwrap();
        assert_eq!(path.relative, "Notes/Plan.md");
        assert_eq!(path.absolute, root.join("Notes/Plan.md"));
        assert_eq!(resolve(&root, "./Notes/./Plan.md").unwrap(), path);
        // New files resolve too, for creating them.
        let new = resolve(&root, "New folder/New.md").unwrap();
        assert_eq!(new.relative, "New folder/New.md");
        assert!(!new.exists());
    }

    #[test]
    fn notes_get_their_extension() {
        let (_dir, root) = vault();
        assert_eq!(
            resolve_note(&root, "Notes/Plan").unwrap().relative,
            "Notes/Plan.md"
        );
        assert_eq!(
            resolve_note(&root, "Notes/Plan.MD").unwrap().relative,
            "Notes/Plan.MD"
        );
        assert!(resolve_attachment(&root, "Notes/Plan.md").is_err());
        assert!(resolve_attachment(&root, "images/a.png").is_ok());
    }

    #[test]
    fn paths_that_leave_the_vault_are_refused() {
        let (_dir, root) = vault();
        for bad in [
            "../outside.md",
            "Notes/../../outside.md",
            "/etc/passwd",
            "",
            "  ",
            ".",
            "Notes\\..\\..\\x.md",
        ] {
            let error = resolve(&root, bad).unwrap_err();
            assert!(!error.message().is_empty(), "{bad:?} should be refused");
        }
        let error = resolve(&root, "../x.md").unwrap_err();
        assert!(error.message().contains("outside the vault"));
    }

    #[test]
    fn hidden_folders_are_off_limits() {
        let (_dir, root) = vault();
        let settings = format!("{}/settings.toml", gasp_config::CONFIG_DIR);
        for hidden in [".git/config", &settings, "Notes/.hidden.md"] {
            let error = resolve(&root, hidden).unwrap_err();
            assert!(error.message().contains("hidden"), "{hidden}: {error:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn links_out_of_the_vault_are_refused() {
        let (_dir, root) = vault();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("secret.md"), "no").unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), root.join("Out")).unwrap();
        std::os::unix::fs::symlink(root.join("Notes"), root.join("In")).unwrap();
        let error = resolve(&root, "Out/secret.md").unwrap_err();
        assert!(error.message().contains("outside the vault"));
        // A dangling link can't be followed to check where it goes.
        std::os::unix::fs::symlink(root.join("gone"), root.join("Dangling.md")).unwrap();
        assert!(resolve(&root, "Dangling.md").is_err());
        // A link that stays inside is fine.
        assert!(resolve(&root, "In/Plan.md").is_ok());
    }

    #[test]
    fn folders_default_to_the_whole_vault() {
        let (_dir, root) = vault();
        assert_eq!(resolve_folder(&root, None).unwrap().absolute, root);
        assert_eq!(resolve_folder(&root, Some("")).unwrap().absolute, root);
        assert_eq!(
            resolve_folder(&root, Some("Notes/")).unwrap().relative,
            "Notes"
        );
    }
}
