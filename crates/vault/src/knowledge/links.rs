//! Following a link: web links leave the vault, note links find their
//! note like Obsidian does.

use std::path::{Path, PathBuf};

use super::markdown_files;

/// Link targets that open in the browser.
pub const WEB_SCHEMES: [&str; 3] = ["http://", "https://", "mailto:"];

/// A link target split into the note part and the heading after `#`.
pub fn split_target(target: &str) -> (&str, Option<&str>) {
    match target.split_once('#') {
        Some((note, heading)) => (note, Some(heading).filter(|heading| !heading.is_empty())),
        None => (target, None),
    }
}

/// Resolves a note link like Obsidian: a path next to the linking note or
/// from the vault root first, then any note with that name, shortest path
/// wins. Matching ignores case and the `.md` extension.
pub fn resolve_note(vault: &Path, from_note: Option<&Path>, link: &str) -> Option<PathBuf> {
    let file = if link.ends_with(".md") {
        link.to_owned()
    } else {
        format!("{link}.md")
    };
    let direct = from_note
        .and_then(Path::parent)
        .map(|folder| folder.join(&file))
        .into_iter()
        .chain([vault.join(&file)])
        .find(|path| path.is_file());
    if direct.is_some() {
        return direct;
    }
    let wanted = file.to_lowercase();
    markdown_files(vault)
        .unwrap_or_default()
        .into_iter()
        .filter(|path| {
            let relative = path.strip_prefix(vault).unwrap_or(path);
            let relative = relative.to_string_lossy().replace('\\', "/").to_lowercase();
            relative == wanted || relative.ends_with(&format!("/{wanted}"))
        })
        .min_by_key(|path| path.components().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_split_at_the_heading() {
        assert_eq!(split_target("Note#Part"), ("Note", Some("Part")));
        assert_eq!(split_target("Note"), ("Note", None));
        assert_eq!(split_target("#Part"), ("", Some("Part")));
    }

    #[test]
    fn notes_resolve_by_name_anywhere_in_the_vault() {
        let vault = tempfile::tempdir().unwrap();
        let deep = vault.path().join("a/b");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Target.md"), "").unwrap();
        std::fs::write(vault.path().join("Other.md"), "").unwrap();
        let found = resolve_note(vault.path(), None, "target").unwrap();
        assert_eq!(found, deep.join("Target.md"));
        assert_eq!(
            resolve_note(vault.path(), None, "Other").unwrap(),
            vault.path().join("Other.md")
        );
        assert!(resolve_note(vault.path(), None, "Missing").is_none());
    }
}
