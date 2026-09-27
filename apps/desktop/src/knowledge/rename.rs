//! Keeping links working when a note is renamed from its title: the notes
//! the index says link to it are rewritten, and nothing else is read.
//! Before the index has finished its first build, every note is checked
//! as the file tree does.

use std::path::{Path, PathBuf};

use gpui::Context;

use super::build::relative;
use super::edit::edit_note;
use crate::file_tree::ops::{update_links_after_move, vault_files};
use crate::link_update::LinkUpdater;
use crate::workspace::Workspace;

/// Rewrites links to the note that moved from `from` to `to` (absolute
/// paths). Returns the notes it changed.
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
    if !index.read(cx).is_ready() {
        return rewrite_everywhere(&vault, &old, &new);
    }
    let (files, sources) = {
        let index = index.read(cx).links();
        let mut files = index.file_paths();
        files.retain(|file| *file != new);
        if !files.contains(&old) {
            files.push(old.clone());
        }
        (files, index.linking_to(&old))
    };
    index.update(cx, |index, cx| {
        index.renamed(&old, &new);
        cx.notify();
    });
    let updater = LinkUpdater::new(&files, &[(old.clone(), new.clone())]);
    let mut changed = Vec::new();
    for source in sources {
        let now = if source == old { &new } else { &source };
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

/// Checks every note, for when the index isn't ready. `old` has already
/// moved to `new` on disk.
fn rewrite_everywhere(vault: &Path, old: &str, new: &str) -> Vec<PathBuf> {
    let files: Vec<String> = vault_files(vault)
        .into_iter()
        .map(|file| if file == new { old.to_string() } else { file })
        .collect();
    update_links_after_move(vault, &files, Path::new(old), Path::new(new))
        .map(|changed| changed.into_iter().map(|path| vault.join(path)).collect())
        .unwrap_or_else(|error| {
            eprintln!("could not update links: {error}");
            Vec::new()
        })
}
