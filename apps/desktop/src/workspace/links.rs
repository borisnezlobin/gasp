//! Following a link from a note: web links open in the browser, note
//! links open (or create) the note and jump to the heading.

use std::path::{Path, PathBuf};

use gpui::{Context, Entity, Window};

use super::{OpenIn, Workspace};
use crate::editor::EditorView;
use crate::note::markdown_files;
use crate::outline::headings;

const WEB_SCHEMES: [&str; 3] = ["http://", "https://", "mailto:"];

/// A link target split into the note part and the heading after `#`.
fn split_target(target: &str) -> (&str, Option<&str>) {
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

/// The offset of the heading titled `heading` in `text`, ignoring case.
fn heading_offset(text: &str, heading: &str) -> Option<usize> {
    headings(text)
        .into_iter()
        .find(|found| found.title.eq_ignore_ascii_case(heading.trim()))
        .map(|found| found.offset)
}

impl Workspace {
    pub(crate) fn follow_link(
        &mut self,
        target: &str,
        from: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if WEB_SCHEMES.iter().any(|scheme| target.starts_with(scheme)) {
            cx.open_url(target);
            return;
        }
        let (note, heading) = split_target(target);
        if note.is_empty() {
            if let Some(offset) = heading.and_then(|h| heading_offset(&from.read(cx).text(), h)) {
                from.update(cx, |editor, cx| editor.select(offset, offset, cx));
            }
            return;
        }
        let from_path = self.active_path(cx);
        let path = resolve_note(self.vault(), from_path.as_deref(), note)
            .unwrap_or_else(|| self.vault().join(format!("{note}.md")));
        if !path.exists() && std::fs::write(&path, "").is_err() {
            return;
        }
        if self
            .open_path(&path, OpenIn::ActiveTab, window, cx)
            .is_err()
        {
            return;
        }
        self.jump_to_heading(heading, cx);
    }

    fn jump_to_heading(&mut self, heading: Option<&str>, cx: &mut Context<Self>) {
        let (Some(heading), Some(editor)) = (heading, self.active_editor(cx)) else {
            return;
        };
        let offset = heading_offset(&editor.read(cx).text(), heading);
        if let Some(offset) = offset {
            editor.update(cx, |editor, cx| editor.select(offset, offset, cx));
        }
    }
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

    #[test]
    fn headings_match_ignoring_case() {
        let text = "# One\ntext\n## Two words\n";
        assert_eq!(heading_offset(text, "two WORDS"), Some(11));
        assert_eq!(heading_offset(text, "three"), None);
    }
}
