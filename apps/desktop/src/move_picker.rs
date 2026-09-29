//! Move note (`note.move`): the vault's folders, filtered by what you
//! type; Enter moves the open note into the one picked, updating links to
//! it as a drag in the file tree does.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    ParentElement, Render, SharedString, Subscription, Window, div, prelude::*,
};

use crate::picker::fuzzy::{Candidate, Matcher, Query};
use crate::picker::{Confirmed, Picker, PickerDelegate, highlighted_text};
use crate::theme::PickerTheme;

/// The folder picked, relative to the vault; empty for the vault itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveTo(pub PathBuf);

/// Folders listed at most, so a huge vault still opens the picker at once.
const MAX_FOLDERS: usize = 5_000;

/// Every folder in `vault` a note can live in, relative to it and sorted,
/// leaving out hidden ones such as `.git` and `.gasp`. The vault itself
/// comes first, as the empty path.
pub fn vault_folders(vault: &Path) -> Vec<PathBuf> {
    let mut folders = vec![PathBuf::new()];
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(vault.join(&relative)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let visible = !name.to_string_lossy().starts_with('.');
            if visible && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let child = relative.join(&name);
                folders.push(child.clone());
                pending.push(child);
            }
        }
        if folders.len() >= MAX_FOLDERS {
            break;
        }
    }
    folders.sort();
    folders
}

#[derive(Clone, Debug)]
struct FolderMatch {
    folder: usize,
    score: i32,
    positions: Vec<usize>,
}

pub struct MoveDelegate {
    vault_name: String,
    folders: Vec<PathBuf>,
    labels: Vec<String>,
    candidates: Vec<Candidate>,
    matches: Vec<FolderMatch>,
    /// The folder the note is in now, which it can't move to.
    current: PathBuf,
    matcher: Matcher,
}

impl MoveDelegate {
    /// `folders` of the vault called `vault_name`, for a note now in
    /// `current` (relative to the vault).
    pub fn new(vault_name: &str, folders: Vec<PathBuf>, current: PathBuf) -> Self {
        let folders: Vec<PathBuf> = folders.into_iter().filter(|f| *f != current).collect();
        let labels: Vec<String> = folders
            .iter()
            .map(|folder| folder_label(vault_name, folder))
            .collect();
        Self {
            vault_name: vault_name.to_owned(),
            candidates: labels.iter().map(|label| Candidate::new(label)).collect(),
            labels,
            folders,
            matches: Vec::new(),
            current,
            matcher: Matcher::new(),
        }
    }

    /// The folder shown in row `index`.
    pub fn folder_at(&self, index: usize) -> Option<&PathBuf> {
        self.folders.get(self.matches.get(index)?.folder)
    }
}

/// How a folder reads in the list: its path, or the vault's name for the
/// vault itself.
fn folder_label(vault_name: &str, folder: &Path) -> String {
    if folder.as_os_str().is_empty() {
        return vault_name.to_owned();
    }
    folder.to_string_lossy().replace('\\', "/")
}

impl PickerDelegate for MoveDelegate {
    type Event = MoveTo;

    fn placeholder(&self) -> SharedString {
        "Move the note to a folder".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn update_matches(&mut self, query: &str) {
        let query = Query::new(query);
        let mut matches: Vec<FolderMatch> = Vec::new();
        for (folder, candidate) in self.candidates.iter().enumerate() {
            if let Some(found) = self.matcher.score(&query, candidate) {
                matches.push(FolderMatch {
                    folder,
                    score: found.score,
                    positions: found.positions,
                });
            }
        }
        matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.folder.cmp(&b.folder)));
        self.matches = matches;
    }

    fn render_match(&self, index: usize, _selected: bool, theme: &PickerTheme) -> AnyElement {
        let Some(found) = self.matches.get(index) else {
            return div().into_any_element();
        };
        div()
            .flex()
            .w_full()
            .text_size(theme.row_font_size)
            .child(
                highlighted_text(self.labels[found.folder].clone(), &found.positions, theme).grow(),
            )
            .into_any_element()
    }

    fn confirm(&mut self, index: usize) -> Option<MoveTo> {
        Some(MoveTo(self.folder_at(index)?.clone()))
    }

    fn empty_message(&self, query: &str) -> SharedString {
        if self.folders.is_empty() {
            let here = folder_label(&self.vault_name, &self.current);
            return format!("There’s no other folder to move it to from {here}.").into();
        }
        format!("No folders match “{}”.", query.trim()).into()
    }
}

/// Move note. Emits [`MoveTo`], then [`DismissEvent`].
pub struct MovePicker {
    picker: Entity<Picker<MoveDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<MoveTo> for MovePicker {}
impl EventEmitter<DismissEvent> for MovePicker {}

impl Focusable for MovePicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl MovePicker {
    pub fn new(delegate: MoveDelegate, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let picker = cx.new(|cx| Picker::new(delegate, window, cx));
        let subscriptions = vec![
            cx.subscribe(&picker, |_, _, event: &Confirmed<MoveTo>, cx| {
                cx.emit(event.0.clone());
                cx.emit(DismissEvent);
            }),
            cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| cx.emit(DismissEvent)),
        ];
        Self {
            picker,
            _subscriptions: subscriptions,
        }
    }

    pub fn picker(&self) -> &Entity<Picker<MoveDelegate>> {
        &self.picker
    }
}

impl Render for MovePicker {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.picker.clone()
    }
}

/// `note.move`: the folder picker for the open note; picking one moves
/// the note there through the file tree, which updates links and tabs.
pub fn open(
    workspace: &mut crate::workspace::Workspace,
    window: &mut Window,
    cx: &mut Context<crate::workspace::Workspace>,
) {
    let Some(note) = workspace.active_path(cx) else {
        return;
    };
    let vault = workspace.vault().to_path_buf();
    let current = note
        .parent()
        .and_then(|folder| folder.strip_prefix(&vault).ok())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let vault_name = crate::workspace::files::folder_name(&vault);
    let delegate = MoveDelegate::new(&vault_name, vault_folders(&vault), current);
    workspace.toggle_modal(window, cx, |window, cx| {
        MovePicker::new(delegate, window, cx)
    });
    let Some(picker) = workspace.active_modal::<MovePicker>() else {
        return;
    };
    cx.subscribe_in(
        &picker,
        window,
        move |workspace, _, event: &MoveTo, _, cx| {
            move_note(workspace, &note, &vault.join(&event.0), cx);
        },
    )
    .detach();
}

fn move_note(
    workspace: &mut crate::workspace::Workspace,
    note: &Path,
    folder: &Path,
    cx: &mut Context<crate::workspace::Workspace>,
) {
    let Some(tree) = workspace.file_tree().cloned() else {
        return;
    };
    tree.update(cx, |tree, cx| tree.move_into(note, folder, cx));
    if !note.exists() {
        let title = crate::workspace::files::note_title(note);
        let place = folder.file_name().map_or_else(
            || "the vault".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let message = format!("Moved “{title}” to {place}.");
        crate::notices::show(crate::notices::Notice::done(message), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_visible_folders_with_the_vault_first() {
        let vault = tempfile::tempdir().unwrap();
        for folder in ["Physics/Waves", "Essays", ".git/objects", ".gasp"] {
            std::fs::create_dir_all(vault.path().join(folder)).unwrap();
        }
        let folders = vault_folders(vault.path());
        let expected: Vec<PathBuf> = ["", "Essays", "Physics", "Physics/Waves"]
            .iter()
            .map(PathBuf::from)
            .collect();
        assert_eq!(folders, expected);
    }

    #[test]
    fn the_notes_own_folder_is_left_out_and_typing_filters() {
        let folders = ["", "Essays", "Physics", "Physics/Waves"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let mut delegate = MoveDelegate::new("Vault", folders, PathBuf::from("Essays"));
        delegate.update_matches("");
        assert_eq!(delegate.match_count(), 3);
        delegate.update_matches("wav");
        assert_eq!(
            delegate.confirm(0),
            Some(MoveTo(PathBuf::from("Physics/Waves")))
        );
    }
}
