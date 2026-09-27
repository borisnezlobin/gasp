//! Opening notes into tabs, and closing, reopening and switching tabs.

use std::io;
use std::path::{Path, PathBuf};

use gpui::{AppContext, Context, Entity, PromptLevel, Window};

use super::files::note_title;
use super::history::Location;
use super::launcher::{Launcher, MAX_RECENT, OpenRecent};
use super::note_doc::NoteDoc;
use super::pane::{NoteTab, Pane, Tab, TabContent};
use super::{MAX_CLOSED_TABS, OpenIn, Workspace};
use crate::editor::EditorView;
use crate::text_input::{TextInput, TextInputStyle};

/// The choices when closing a note that changed on disk under edits.
const CONFLICT_ANSWERS: [&str; 3] = ["Keep my version", "Use the version on disk", "Cancel"];

impl Workspace {
    /// Opens the note at `path` (vault-relative or absolute). A note
    /// already open in the pane gets its tab shown instead.
    pub fn open_path(
        &mut self,
        path: &Path,
        open_in: OpenIn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        let path = self.resolve(path);
        let pane = match open_in {
            OpenIn::SplitRight => self.split_pane(super::pane_tree::Axis::Row, false, window, cx),
            _ => self.active_pane.clone(),
        };
        let replace = open_in == OpenIn::ActiveTab;
        let opened = self.open_in_pane(&pane, &path, replace, window, cx);
        if opened.is_err() && pane.read(cx).is_empty() {
            self.handle_empty_pane(&pane, window, cx);
        }
        opened
    }

    /// Opens `path` in `pane`, recording where the pane was for Back.
    pub(crate) fn open_in_pane(
        &mut self,
        pane: &Entity<Pane>,
        path: &Path,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        let from = self.pane_location(pane, cx);
        self.show_path_in_pane(pane, path, replace, window, cx)?;
        if let Some(from) = from.filter(|from| from.path != path) {
            pane.update(cx, |pane, _| pane.history.push(from));
        }
        Ok(())
    }

    /// Shows `path` in `pane` without touching its history.
    pub(crate) fn show_path_in_pane(
        &mut self,
        pane: &Entity<Pane>,
        path: &Path,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        let existing = pane.read(cx).index_of_path(path, cx);
        match existing {
            Some(index) => pane.update(cx, |pane, cx| pane.activate(index, cx)),
            None => {
                let tab = self.note_tab(path, window, cx)?;
                self.put_tab(pane, tab, replace, cx);
            }
        }
        self.touch_recent(path);
        self.activate_pane(pane, window, cx);
        Ok(())
    }

    /// Adds `tab` to `pane`, or puts it in place of the active tab.
    fn put_tab(&mut self, pane: &Entity<Pane>, tab: Tab, replace: bool, cx: &mut Context<Self>) {
        let replaceable = replace && !pane.read(cx).is_empty();
        if !replaceable {
            pane.update(cx, |pane, cx| pane.add_tab(tab, cx));
            return;
        }
        let index = pane.read(cx).active_index();
        let old = pane.update(cx, |pane, cx| pane.replace_tab(index, tab, cx));
        if let Some(old) = old {
            self.release_tab(old, true, cx);
        }
    }

    /// Where `pane`'s active note is, with its cursor.
    pub(crate) fn pane_location(&self, pane: &Entity<Pane>, cx: &gpui::App) -> Option<Location> {
        let tab = pane.read(cx).active_tab()?;
        let note = tab.note()?;
        let offset = note.editor.read(cx).cursor();
        Some(Location::new(note.doc.read(cx).path(), offset))
    }

    /// The open note for `path`, loading it if needed.
    fn doc(&mut self, path: &Path, cx: &mut Context<Self>) -> io::Result<Entity<NoteDoc>> {
        if let Some(doc) = self.doc_for_path(path, cx) {
            return Ok(doc);
        }
        let image_dirs = self.image_dirs(path);
        let doc = NoteDoc::load(path, image_dirs)?
            .with_attachments(&self.config.settings.files.attachments_folder);
        let doc = cx.new(|_| doc);
        self.docs.push(doc.clone());
        Ok(doc)
    }

    fn image_dirs(&self, path: &Path) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = path.parent().map(Path::to_path_buf).into_iter().collect();
        if let Some(parent) = path.parent() {
            dirs.push(parent.join(&self.config.settings.files.attachments_folder));
        }
        dirs.push(self.vault.clone());
        dirs
    }

    /// A new tab on the note at `path`.
    pub(crate) fn note_tab(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> io::Result<Tab> {
        let span = crate::trace::span("note-load");
        let doc = self.doc(path, cx)?;
        drop(span);
        let span = crate::trace::span("note-editor");
        let config = &self.config;
        let editor = doc.update(cx, |doc, cx| doc.new_editor(config, cx));
        drop(span);
        let title_text = note_title(path);
        let title = cx.new(|cx| {
            let mut title = TextInput::new(window, cx)
                .with_style(TextInputStyle::Title)
                .with_placeholder(super::files::UNTITLED);
            title.set_text(&title_text, cx);
            title
        });
        self.cursors
            .insert(editor.entity_id(), super::CursorSeen::default());
        let subscriptions = vec![
            cx.subscribe_in(&editor, window, Self::on_editor_event),
            cx.subscribe_in(&title, window, Self::on_title_event),
            cx.observe(&doc, |_, _, cx| cx.notify()),
        ];
        let content = TabContent::Note(NoteTab { doc, editor, title });
        Ok(Tab::new(content, subscriptions))
    }

    /// Adds an empty tab showing recent notes.
    pub(crate) fn add_launcher_tab(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let recent = self.launcher_notes();
        let vault = self.vault.clone();
        let launcher = cx.new(|cx| Launcher::new(&vault, recent, cx));
        let subscription = cx.subscribe_in(&launcher, window, Self::on_open_recent);
        let tab = Tab::new(TabContent::Launcher(launcher), vec![subscription]);
        pane.update(cx, |pane, cx| pane.add_tab(tab, cx));
        self.activate_pane(pane, window, cx);
    }

    /// Opens a note from a launcher in place of it. When the note already
    /// has a tab, that tab shows and the launcher closes.
    fn on_open_recent(
        &mut self,
        launcher: &Entity<Launcher>,
        event: &OpenRecent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = self.active_pane.clone();
        if let Err(error) = self.open_in_pane(&pane, &event.0, true, window, cx) {
            eprintln!("could not open {}: {error}", event.0.display());
            return;
        }
        let leftover = pane.read(cx).tabs().iter().position(
            |tab| matches!(&tab.content, TabContent::Launcher(other) if other == launcher),
        );
        if let Some(index) = leftover {
            let tab = pane.update(cx, |pane, cx| pane.remove_tab(index, cx));
            drop(tab);
            self.focus_active(window, cx);
        }
    }

    /// This session's recent notes for a new launcher, which adds the
    /// vault's most recently changed notes itself, off the main thread.
    /// The first launcher also gets the notes read while the app started,
    /// so the window's first frame lists them.
    fn launcher_notes(&mut self) -> Vec<PathBuf> {
        let mut notes: Vec<PathBuf> = self.recent.clone();
        for path in &std::mem::take(&mut self.recency) {
            if notes.len() >= MAX_RECENT {
                break;
            }
            if !notes.contains(path) {
                notes.push(path.clone());
            }
        }
        notes.retain(|path| path.is_file());
        notes
    }

    fn touch_recent(&mut self, path: &Path) {
        self.recent.retain(|recent| recent != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }

    /// Saves a closed tab's note, and forgets the note when no other tab
    /// shows it. `save` is false when the file is going away.
    pub(crate) fn release_tab(&mut self, tab: Tab, save: bool, cx: &mut Context<Self>) {
        let Some(note) = tab.note().cloned() else {
            return;
        };
        if let Some(editor) = tab.note().map(|note| note.editor.entity_id()) {
            self.cursors.remove(&editor);
        }
        if save {
            note.doc.update(cx, |doc, cx| doc.save_or_log(cx));
        }
        drop(tab);
        drop(note.editor);
        note.doc.update(cx, |doc, _| doc.prune_editors());
        if !self.doc_is_shown(&note.doc, cx) {
            self.docs.retain(|doc| *doc != note.doc);
        }
    }

    fn doc_is_shown(&self, doc: &Entity<NoteDoc>, cx: &gpui::App) -> bool {
        self.panes.panes().iter().any(|pane| {
            pane.read(cx)
                .tabs()
                .iter()
                .any(|tab| tab.note().is_some_and(|note| note.doc == *doc))
        })
    }

    /// Closes the active tab of the active pane.
    pub fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane.clone();
        let index = pane.read(cx).active_index();
        self.close_tab(&pane, index, window, cx);
    }

    /// Closes a tab, asking first when its note is in conflict with the
    /// disk.
    pub fn close_tab(
        &mut self,
        pane: &Entity<Pane>,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let doc = pane
            .read(cx)
            .tabs()
            .get(index)
            .and_then(|tab| tab.note())
            .map(|note| note.doc.clone());
        let conflicted = doc
            .as_ref()
            .is_some_and(|doc| doc.read(cx).conflict().is_some());
        match doc {
            Some(doc) if conflicted && !self.doc_shown_elsewhere(&doc, pane, index, cx) => {
                self.ask_then_close(pane.clone(), index, doc, window, cx);
            }
            _ => self.close_tab_now(pane, index, true, window, cx),
        }
    }

    fn doc_shown_elsewhere(
        &self,
        doc: &Entity<NoteDoc>,
        pane: &Entity<Pane>,
        index: usize,
        cx: &gpui::App,
    ) -> bool {
        self.panes.panes().iter().any(|other| {
            other.read(cx).tabs().iter().enumerate().any(|(at, tab)| {
                let same_tab = other == pane && at == index;
                !same_tab && tab.note().is_some_and(|note| note.doc == *doc)
            })
        })
    }

    fn ask_then_close(
        &mut self,
        pane: Entity<Pane>,
        index: usize,
        doc: Entity<NoteDoc>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = note_title(doc.read(cx).path());
        let message = format!("“{title}” changed on disk while you were editing it.");
        let answer = window.prompt(
            PromptLevel::Warning,
            &message,
            Some("Which version should the note keep?"),
            &CONFLICT_ANSWERS,
            cx,
        );
        let task = cx.spawn_in(window, async move |workspace, cx| {
            let Ok(choice) = answer.await else {
                return;
            };
            workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.resolve_and_close(&pane, index, &doc, choice, window, cx)
                })
                .ok();
        });
        self.tasks.push(task);
    }

    fn resolve_and_close(
        &mut self,
        pane: &Entity<Pane>,
        index: usize,
        doc: &Entity<NoteDoc>,
        choice: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match choice {
            0 => {
                if let Err(error) = doc.update(cx, |doc, cx| doc.keep_mine(cx)) {
                    eprintln!("could not save: {error}");
                    return;
                }
            }
            1 => {
                doc.update(cx, |doc, cx| doc.take_disk(cx));
            }
            _ => return,
        }
        self.close_tab_now(pane, index, false, window, cx);
    }

    /// Closes a tab without asking. The pane closes with its last tab when
    /// there are other panes; otherwise it shows an empty tab.
    pub(crate) fn close_tab_now(
        &mut self,
        pane: &Entity<Pane>,
        index: usize,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = pane.update(cx, |pane, cx| pane.remove_tab(index, cx)) else {
            return;
        };
        if let Some(path) = tab.path(cx).map(Path::to_path_buf) {
            self.remember_closed(path);
        }
        self.release_tab(tab, save, cx);
        if pane.read(cx).is_empty() {
            self.handle_empty_pane(pane, window, cx);
        } else if *pane == self.active_pane {
            self.focus_active(window, cx);
        }
        self.refresh_status(cx);
        cx.notify();
    }

    fn remember_closed(&mut self, path: PathBuf) {
        self.closed_tabs.push(path);
        if self.closed_tabs.len() > MAX_CLOSED_TABS {
            self.closed_tabs.remove(0);
        }
    }

    fn handle_empty_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.panes.len() > 1 {
            self.remove_pane(pane, window, cx);
        } else {
            self.add_launcher_tab(pane, window, cx);
        }
    }

    /// Reopens the most recently closed tab whose note still exists.
    pub fn reopen_closed_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(path) = self.closed_tabs.pop() {
            if path.is_file() {
                let pane = self.active_pane.clone();
                let replace = pane
                    .read(cx)
                    .active_tab()
                    .is_some_and(|tab| tab.note().is_none());
                if let Err(error) = self.open_in_pane(&pane, &path, replace, window, cx) {
                    eprintln!("could not reopen {}: {error}", path.display());
                }
                return;
            }
        }
    }

    /// Paths of closed tabs, oldest first.
    pub fn closed_tabs(&self) -> &[PathBuf] {
        &self.closed_tabs
    }

    /// Shows tab `index` of the active pane.
    pub fn activate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane.clone();
        pane.update(cx, |pane, cx| pane.activate(index, cx));
        self.focus_active(window, cx);
        self.refresh_status(cx);
        cx.notify();
    }

    /// `tab.go-N`: tabs 1 to 8 by position, and 9 for the last tab.
    pub(crate) fn go_to_tab(&mut self, number: usize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.active_pane.read(cx).len();
        let index = if number >= 9 {
            count.saturating_sub(1)
        } else {
            number - 1
        };
        if index < count {
            self.activate_tab(index, window, cx);
        }
    }

    /// Moves to the next (`1`) or previous (`-1`) tab, wrapping around.
    pub(crate) fn cycle_tab(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane.read(cx);
        let count = pane.len() as isize;
        if count == 0 {
            return;
        }
        let index = (pane.active_index() as isize + step).rem_euclid(count) as usize;
        self.activate_tab(index, window, cx);
    }

    pub(crate) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane.clone();
        self.add_launcher_tab(&pane, window, cx);
        cx.notify();
    }

    pub(crate) fn editor_pane(
        &self,
        editor: &Entity<EditorView>,
        cx: &gpui::App,
    ) -> Option<Entity<Pane>> {
        self.panes.panes().into_iter().find(|pane| {
            pane.read(cx)
                .tabs()
                .iter()
                .any(|tab| tab.note().is_some_and(|note| note.editor == *editor))
        })
    }

    /// Every tab showing `doc`, as (pane, index), last index first so they
    /// can be closed in order.
    pub(crate) fn tabs_showing(
        &self,
        doc: &Entity<NoteDoc>,
        cx: &gpui::App,
    ) -> Vec<(Entity<Pane>, usize)> {
        let mut found = Vec::new();
        for pane in self.panes.panes() {
            for (index, tab) in pane.read(cx).tabs().iter().enumerate().rev() {
                if tab.note().is_some_and(|note| note.doc == *doc) {
                    found.push((pane.clone(), index));
                }
            }
        }
        found
    }
}
