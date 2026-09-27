//! Notes as files: creating, renaming, deleting, saving, following changes
//! on disk, and moving back and forward through history.

use std::io;
use std::path::{Path, PathBuf};

use editor_config::settings::TrashMode;
use gpui::{Context, Entity, Focusable, PromptButton, PromptLevel, Window};

use super::files::{atomic_write, clean_title, note_title, renamed_path, unique_untitled};
use super::history::{BIG_JUMP_LINES, Location};
use super::note_doc::{DiskOutcome, NoteDoc};
use super::pane::{NoteTab, Pane};
use super::status::StatusInfo;
use super::watcher::DiskChange;
use super::{CursorSeen, OpenIn, Workspace};
use crate::editor::{EditorEvent, EditorView};
use crate::text_input::{TextInput, TextInputEvent};

/// Where `trash = "vault"` puts deleted notes, as Obsidian does.
const VAULT_TRASH: &str = ".trash";

impl Workspace {
    pub(crate) fn on_editor_event(
        &mut self,
        editor: &Entity<EditorView>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let EditorEvent::OpenLink(target) = event {
            return self.follow_link(target, editor, window, cx);
        }
        let (offset, line) = {
            let view = editor.read(cx);
            (view.cursor(), view.doc().line_of_offset(view.cursor()))
        };
        let seen = self
            .cursors
            .insert(editor.entity_id(), CursorSeen { offset, line });
        let jumped = seen.is_some_and(|seen| seen.line.abs_diff(line) >= BIG_JUMP_LINES);
        if *event == EditorEvent::SelectionChanged && jumped {
            self.record_jump(editor, seen.map_or(0, |seen| seen.offset), cx);
        }
        if self.active_editor(cx).as_ref() == Some(editor) {
            self.refresh_status(cx);
        }
    }

    fn record_jump(&mut self, editor: &Entity<EditorView>, from: usize, cx: &mut Context<Self>) {
        let Some(pane) = self.editor_pane(editor, cx) else {
            return;
        };
        let path = pane.read(cx).tabs().iter().find_map(|tab| {
            let note = tab.note()?;
            (note.editor == *editor).then(|| note.doc.read(cx).path().to_path_buf())
        });
        if let Some(path) = path {
            pane.update(cx, |pane, _| pane.history.push(Location::new(path, from)));
        }
    }

    /// Recomputes the status bar from the active editor.
    pub(crate) fn refresh_status(&mut self, cx: &mut Context<Self>) {
        self.status = self
            .active_editor(cx)
            .map(|editor| StatusInfo::of_editor(editor.read(cx)));
        self.sync_tree_active(cx);
        cx.notify();
    }

    /// Marks the active note's row in the file tree when it changes.
    fn sync_tree_active(&mut self, cx: &mut Context<Self>) {
        let active = self.active_path(cx);
        if active == self.tree_active {
            return;
        }
        if let Some(tree) = self.file_tree.clone() {
            tree.update(cx, |tree, cx| tree.set_active_path(active.as_deref(), cx));
        }
        self.tree_active = active;
    }

    pub(crate) fn on_title_event(
        &mut self,
        title: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(note) = self.note_tab_with_title(title, cx) else {
            return;
        };
        match event {
            TextInputEvent::Submitted => {
                self.commit_title(&note, window, cx);
                window.focus(&note.editor.read(cx).focus_handle);
            }
            TextInputEvent::Cancelled => {
                let current = note_title(note.doc.read(cx).path());
                title.update(cx, |title, cx| title.set_text(&current, cx));
                window.focus(&note.editor.read(cx).focus_handle);
            }
            TextInputEvent::Blurred => self.commit_title(&note, window, cx),
            TextInputEvent::Changed => {}
        }
    }

    /// Renames the note to what its title says, if that changed.
    fn commit_title(&mut self, note: &NoteTab, window: &mut Window, cx: &mut Context<Self>) {
        let typed = note.title.read(cx).text().to_owned();
        if typed != note_title(note.doc.read(cx).path()) {
            self.rename_note(&note.doc, &typed, window, cx);
        }
    }

    fn note_tab_with_title(&self, title: &Entity<TextInput>, cx: &gpui::App) -> Option<NoteTab> {
        self.panes.panes().iter().find_map(|pane| {
            pane.read(cx)
                .tabs()
                .iter()
                .filter_map(|tab| tab.note())
                .find(|note| note.title == *title)
                .cloned()
        })
    }

    /// `note.new`: creates `Untitled.md` (or the next free number) in the
    /// vault root, opens it and puts the cursor in its title.
    pub fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) -> io::Result<PathBuf> {
        let path = unique_untitled(&self.vault);
        atomic_write(&path, "")?;
        let empty_tab = self
            .active_pane
            .read(cx)
            .active_tab()
            .is_some_and(|tab| tab.note().is_none());
        let open_in = if empty_tab {
            OpenIn::ActiveTab
        } else {
            OpenIn::NewTab
        };
        self.open_path(&path, open_in, window, cx)?;
        self.focus_title(window, cx);
        Ok(path)
    }

    /// `note.rename`: puts the cursor in the active note's title, with the
    /// title selected.
    pub fn focus_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane.clone();
        let Some(note) = pane
            .read(cx)
            .active_tab()
            .and_then(|tab| tab.note())
            .cloned()
        else {
            return;
        };
        pane.update(cx, |pane, cx| pane.set_show_inline_title(true, cx));
        note.title.update(cx, |title, cx| title.select_all(cx));
        window.focus(&note.title.focus_handle(cx));
    }

    /// Renames the note's file to `title`. A title that can't be a file
    /// name, or that another note has, puts the old title back.
    pub fn rename_note(
        &mut self,
        doc: &Entity<NoteDoc>,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old = doc.read(cx).path().to_path_buf();
        let result = clean_title(title)
            .map_err(|_| "A note's title can't be empty or contain / \\ : * ? \" < > |".to_owned())
            .and_then(|title| self.move_note_file(doc, &old, title, cx));
        if let Err(message) = result {
            self.set_titles(doc, &note_title(&old), cx);
            // Only an acknowledgement; there is nothing to do with the answer.
            drop(window.prompt(
                PromptLevel::Info,
                &message,
                None,
                &[PromptButton::ok("OK")],
                cx,
            ));
        }
    }

    fn move_note_file(
        &mut self,
        doc: &Entity<NoteDoc>,
        old: &Path,
        title: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let new = renamed_path(old, title);
        if new == old {
            return Ok(());
        }
        if new.exists() && !is_same_file(old, &new) {
            return Err(format!("There's already a note called “{title}”."));
        }
        doc.update(cx, |doc, cx| doc.save_or_log(cx));
        std::fs::rename(old, &new).map_err(|error| format!("Couldn't rename the note: {error}"))?;
        self.note_moved(doc, old, &new, cx);
        Ok(())
    }

    /// Updates everything that remembers a note's path.
    fn note_moved(
        &mut self,
        doc: &Entity<NoteDoc>,
        from: &Path,
        to: &Path,
        cx: &mut Context<Self>,
    ) {
        doc.update(cx, |doc, cx| doc.set_path(to.to_path_buf(), cx));
        for pane in self.panes.panes() {
            pane.update(cx, |pane, _| pane.history.rename(from, to));
        }
        for path in self.recent.iter_mut().chain(self.closed_tabs.iter_mut()) {
            if path == from {
                *path = to.to_path_buf();
            }
        }
        let (old_title, title) = (note_title(from), note_title(to));
        for (pane, index) in self.tabs_showing(doc, cx) {
            let note = pane.read(cx).tabs()[index].note().cloned();
            if let Some(note) = note {
                // Leave a title someone is typing a different name into.
                note.title.update(cx, |input, cx| {
                    let shown = input.text().trim();
                    if shown == old_title || shown == title {
                        input.set_text(&title, cx);
                    }
                });
            }
        }
        cx.notify();
    }

    fn set_titles(&mut self, doc: &Entity<NoteDoc>, title: &str, cx: &mut Context<Self>) {
        for (pane, index) in self.tabs_showing(doc, cx) {
            let note = pane.read(cx).tabs()[index].note().cloned();
            if let Some(note) = note {
                note.title.update(cx, |input, cx| input.set_text(title, cx));
            }
        }
    }

    /// `note.delete`: asks, then moves the active note to the trash.
    pub fn delete_active_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.active_path(cx) else {
            return;
        };
        let message = format!("Move “{}” to the trash?", note_title(&path));
        let answer = window.prompt(
            PromptLevel::Warning,
            &message,
            None,
            &["Move to trash", "Cancel"],
            cx,
        );
        let task = cx.spawn_in(window, async move |workspace, cx| {
            if answer.await != Ok(0) {
                return;
            }
            workspace
                .update_in(cx, |workspace, window, cx| {
                    if let Err(error) = workspace.trash_note(&path, window, cx) {
                        eprintln!("could not delete {}: {error}", path.display());
                    }
                })
                .ok();
        });
        self.tasks.push(task);
    }

    /// Removes the note at `path` the way the `files.trash` setting says,
    /// and closes its tabs without saving.
    pub fn trash_note(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        match self.config.settings.files.trash {
            TrashMode::System => trash::delete(path).map_err(io::Error::other)?,
            TrashMode::Vault => move_to_vault_trash(&self.vault, path)?,
            TrashMode::Delete => std::fs::remove_file(path)?,
        }
        if let Some(doc) = self.doc_for_path(path, cx) {
            self.close_doc_tabs(&doc, window, cx);
        }
        self.forget_path(path, cx);
        Ok(())
    }

    fn forget_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.recent.retain(|recent| recent != path);
        self.closed_tabs.retain(|closed| closed != path);
        for pane in self.panes.panes() {
            pane.update(cx, |pane, _| pane.history.forget(path));
        }
    }

    /// Closes every tab on `doc` without saving it.
    fn close_doc_tabs(
        &mut self,
        doc: &Entity<NoteDoc>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (pane, index) in self.tabs_showing(doc, cx) {
            if self.panes.contains(&pane) {
                self.close_tab_now(&pane, index, false, window, cx);
            }
        }
        let path = doc.read(cx).path().to_path_buf();
        self.closed_tabs.retain(|closed| *closed != path);
    }

    /// Saves every note with unsaved edits.
    pub fn save_all(&mut self, cx: &mut Context<Self>) {
        for doc in self.docs.clone() {
            doc.update(cx, |doc, cx| doc.save_or_log(cx));
        }
    }

    /// Saves everything before the window closes. Notes in conflict with
    /// the disk keep their edits in a conflicted copy beside them.
    pub fn save_for_close(&mut self, cx: &mut Context<Self>) {
        self.save_all(cx);
        for doc in self.docs.clone() {
            let conflicted = doc.read(cx).conflict().is_some() && doc.read(cx).is_dirty();
            if conflicted && let Err(error) = doc.update(cx, |doc, cx| doc.save_conflicted_copy(cx))
            {
                eprintln!("could not keep conflicting edits: {error}");
            }
        }
    }

    /// Follows changes the watcher saw on disk.
    pub fn apply_disk_changes(
        &mut self,
        changes: Vec<DiskChange>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_texts.apply(&changes);
        if let Some(sync) = self.sync.clone() {
            let paths: Vec<PathBuf> = changes.iter().flat_map(DiskChange::paths).collect();
            sync.update(cx, |sync, cx| sync.files_changed(&paths, cx));
        }
        for change in changes {
            match change {
                DiskChange::Renamed { from, to } => self.disk_renamed(&from, &to, cx),
                DiskChange::Changed(path) => self.disk_changed(&path, window, cx),
                DiskChange::Removed(path) => self.disk_removed(&path, window, cx),
            }
        }
        self.refresh_status(cx);
        cx.notify();
    }

    fn disk_renamed(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        for doc in self.docs.clone() {
            let path = doc.read(cx).path().to_path_buf();
            if let Ok(rest) = path.strip_prefix(from) {
                let moved = if rest.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rest)
                };
                self.note_moved(&doc, &path, &moved, cx);
            }
        }
    }

    fn disk_changed(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc_for_path(path, cx) else {
            return;
        };
        if doc.update(cx, |doc, cx| doc.disk_changed(cx)) == DiskOutcome::Missing {
            self.note_went_missing(&doc, window, cx);
        }
    }

    fn disk_removed(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        for doc in self.docs.clone() {
            let doc_path = doc.read(cx).path().to_path_buf();
            if doc_path.starts_with(path) && !doc_path.exists() {
                self.note_went_missing(&doc, window, cx);
            }
        }
    }

    /// A note's file is gone: its tabs close, unless it has unsaved edits,
    /// which stay open and marked.
    fn note_went_missing(
        &mut self,
        doc: &Entity<NoteDoc>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if doc.read(cx).is_dirty() {
            doc.update(cx, |doc, cx| doc.mark_deleted(cx));
            return;
        }
        let path = doc.read(cx).path().to_path_buf();
        self.close_doc_tabs(doc, window, cx);
        self.forget_path(&path, cx);
    }

    /// `history.back` and `history.forward` in the active pane.
    pub fn navigate_history(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane.clone();
        let current = self.pane_location(&pane, cx);
        let target = pane.update(cx, |pane, _| {
            if forward {
                pane.history.forward(current)
            } else {
                pane.history.back(current)
            }
        });
        if let Some(target) = target {
            self.go_to_location(&pane, &target, window, cx);
        }
    }

    fn go_to_location(
        &mut self,
        pane: &Entity<Pane>,
        target: &Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.show_path_in_pane(pane, &target.path, true, window, cx) {
            eprintln!("could not open {}: {error}", target.path.display());
            return;
        }
        let Some(editor) = pane.read(cx).active_editor() else {
            return;
        };
        let offset = target.offset.min(editor.read(cx).doc().len());
        let line = editor.read(cx).doc().line_of_offset(offset);
        self.cursors
            .insert(editor.entity_id(), CursorSeen { offset, line });
        editor.update(cx, |editor, cx| editor.select(offset, offset, cx));
        self.refresh_status(cx);
    }
}

fn is_same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn move_to_vault_trash(vault: &Path, path: &Path) -> io::Result<()> {
    let trash = vault.join(VAULT_TRASH);
    std::fs::create_dir_all(&trash)?;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let stem = note_title(path);
    let mut target = trash.join(&name);
    let mut number = 1;
    while target.exists() {
        target = trash.join(format!("{stem} {number}.md"));
        number += 1;
    }
    std::fs::rename(path, target)
}
