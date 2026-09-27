//! Keeping links working when a note or folder moves, from its title or
//! the file tree: the notes the index says link to what moved, and the
//! notes that moved, are rewritten, and nothing else is read. Open notes
//! change in their editors, so each change is one undoable edit. Before
//! the index has finished its first build, every note is checked.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gpui::Context;

use super::build::relative;
use super::edit::edit_note;
use crate::file_tree::ops::vault_files;
use crate::link_update::{LinkUpdater, expand_folder_move};
use crate::workspace::Workspace;

/// Rewrites links to the note or folder that moved from `from` to `to`
/// (absolute paths), which has already moved on disk. Returns the notes
/// it changed.
pub fn update_links_after_rename(
    workspace: &Workspace,
    from: &Path,
    to: &Path,
    cx: &mut Context<Workspace>,
) -> Vec<PathBuf> {
    let vault = workspace.vault().to_path_buf();
    let (Some(old), Some(new)) = (relative(&vault, from), relative(&vault, to)) else {
        return Vec::new();
    };
    let index = workspace.vault_index().clone();
    let ready = index.read(cx).is_ready();
    let files = match ready {
        true => files_before(index.read(cx).links().file_paths(), &old, &new),
        false => files_before(vault_files(&vault), &old, &new),
    };
    let mut moves = expand_folder_move(&files, &old, &new);
    if files.contains(&old) {
        moves.push((old.clone(), new.clone()));
    }
    let sources = match ready {
        true => {
            let links = index.read(cx).links();
            let mut sources: BTreeSet<String> = moves
                .iter()
                .flat_map(|(moved, _)| links.linking_to(moved))
                .collect();
            sources.extend(moves.iter().map(|(moved, _)| moved.clone()));
            sources
        }
        false => files.iter().cloned().collect(),
    };
    index.update(cx, |index, cx| {
        for (old, new) in &moves {
            index.renamed(old, new);
        }
        cx.notify();
    });
    let updater = LinkUpdater::new(&files, &moves);
    let mut changed = Vec::new();
    for source in sources.into_iter().filter(|path| is_note(path)) {
        let now = moves
            .iter()
            .find(|(old, _)| *old == source)
            .map_or(source.as_str(), |(_, new)| new.as_str());
        let path = vault.join(now);
        let result = edit_note(workspace, &path, |text| updater.rewrite(&source, text), cx);
        match result {
            Ok(Some(_)) => changed.push(path),
            Ok(None) => {}
            Err(error) => eprintln!("could not update links in {}: {error}", path.display()),
        }
    }
    changed
}

/// The vault's files as they were before `old` moved to `new`, from a
/// list that may already have some at their new paths.
fn files_before(files: Vec<String>, old: &str, new: &str) -> Vec<String> {
    let inside = format!("{new}/");
    let mut files: Vec<String> = files
        .into_iter()
        .map(|file| match file.strip_prefix(&inside) {
            Some(rest) => format!("{old}/{rest}"),
            None if file == new => old.to_string(),
            None => file,
        })
        .collect();
    files.sort();
    files.dedup();
    files
}

fn is_note(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_listed_where_they_were() {
        let files = vec![
            "B.md".to_string(),
            "new/a.md".to_string(),
            "new/sub/c.png".to_string(),
            "newer.md".to_string(),
        ];
        assert_eq!(
            files_before(files, "old", "new"),
            ["B.md", "newer.md", "old/a.md", "old/sub/c.png"]
        );
        let files = vec!["Plan 2.md".to_string(), "A.md".to_string()];
        assert_eq!(
            files_before(files, "Plan.md", "Plan 2.md"),
            ["A.md", "Plan.md"]
        );
    }
}
