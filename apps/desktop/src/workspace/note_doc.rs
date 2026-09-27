//! One open note: its file, the editors showing it, whether it has unsaved
//! edits or a conflict with the disk, and autosaving.

use std::io;
use std::path::{Path, PathBuf};

use crate::paste::{PasteContext, set_paste_context};
use std::time::Duration;

use editor_config::Config;
use gpui::{AppContext, Context, Entity, Subscription, Task, WeakEntity};

use super::files::{LineEnding, atomic_write};
use crate::editor::{EditorEvent, EditorView};

/// How long typing must pause before the note saves.
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(1000);

/// Why the editor's text and the file disagree in a way autosave mustn't
/// paper over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// The file changed on disk while there were unsaved edits.
    ChangedOnDisk,
    /// The file was deleted or moved away while there were unsaved edits.
    DeletedOnDisk,
}

/// What a change on disk did to the note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskOutcome {
    /// The file holds what we last saw, such as after our own save.
    Unchanged,
    /// The editors now show the new file.
    Reloaded,
    /// The file changed under unsaved edits; the edits stay.
    Conflicted,
    /// The file is gone.
    Missing,
}

/// Where pasted images go when the settings say nothing else.
const DEFAULT_ATTACHMENTS: &str = "./images";

/// An open note, shared by every tab that shows it.
pub struct NoteDoc {
    path: PathBuf,
    /// The editor text as of the last load or save.
    saved_text: String,
    /// The file's bytes as of the last load or save.
    disk_text: String,
    line_ending: LineEnding,
    image_dirs: Vec<PathBuf>,
    /// The attachments folder setting, for images pasted into this note.
    attachments: String,
    editors: Vec<WeakEntity<EditorView>>,
    subscriptions: Vec<Subscription>,
    dirty: bool,
    conflict: Option<Conflict>,
    autosave: Option<Task<()>>,
}

impl NoteDoc {
    /// Reads the note at `path`.
    pub fn load(path: &Path, image_dirs: Vec<PathBuf>) -> io::Result<NoteDoc> {
        let text = std::fs::read_to_string(path)?;
        Ok(NoteDoc::from_text(path, text, image_dirs))
    }

    /// A note whose file holds `text`.
    pub fn from_text(path: &Path, text: String, image_dirs: Vec<PathBuf>) -> NoteDoc {
        NoteDoc {
            path: path.to_path_buf(),
            line_ending: LineEnding::detect(&text),
            saved_text: text.clone(),
            disk_text: text,
            image_dirs,
            attachments: DEFAULT_ATTACHMENTS.to_owned(),
            editors: Vec::new(),
            subscriptions: Vec::new(),
            dirty: false,
            conflict: None,
            autosave: None,
        }
    }

    /// Uses `folder` (the `files.attachments-folder` setting) for pasted images.
    pub fn with_attachments(mut self, folder: &str) -> NoteDoc {
        self.attachments = folder.to_owned();
        self
    }

    fn paste_context(&self) -> PasteContext {
        PasteContext {
            note_path: Some(self.path.clone()),
            attachments: self.attachments.clone(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn conflict(&self) -> Option<Conflict> {
        self.conflict
    }

    /// Whether an autosave is waiting for typing to pause.
    pub fn has_pending_save(&self) -> bool {
        self.autosave.is_some()
    }

    /// A new editor on this note. Edits in any editor reach the others.
    pub fn new_editor(&mut self, config: &Config, cx: &mut Context<Self>) -> Entity<EditorView> {
        let text = self.current_text(cx);
        let image_dirs = self.image_dirs.clone();
        let editor = cx.new(|cx| EditorView::with_config(&text, image_dirs, config, cx));
        set_paste_context(&editor, self.paste_context(), cx);
        let subscription = cx.subscribe(&editor, |doc, editor, event, cx| {
            if *event == EditorEvent::Edited {
                doc.on_edited(&editor, cx);
            }
        });
        self.editors.push(editor.downgrade());
        self.subscriptions.push(subscription);
        editor
    }

    /// The note's text as the editors have it.
    pub fn current_text(&self, cx: &gpui::App) -> String {
        self.live_editors()
            .first()
            .map_or_else(|| self.saved_text.clone(), |editor| editor.read(cx).text())
    }

    fn live_editors(&self) -> Vec<Entity<EditorView>> {
        self.editors
            .iter()
            .filter_map(WeakEntity::upgrade)
            .collect()
    }

    /// Forgets editors whose tabs have closed.
    pub fn prune_editors(&mut self) {
        self.editors.retain(|editor| editor.upgrade().is_some());
    }

    /// Whether any tab still shows this note.
    pub fn has_editors(&self) -> bool {
        self.editors.iter().any(|editor| editor.upgrade().is_some())
    }

    fn on_edited(&mut self, source: &Entity<EditorView>, cx: &mut Context<Self>) {
        let text = source.read(cx).text();
        for editor in self.live_editors() {
            if editor != *source && editor.read(cx).text() != text {
                editor.update(cx, |editor, cx| editor.replace_all_text(&text, cx));
            }
        }
        let dirty = text != self.saved_text;
        if dirty != self.dirty {
            self.dirty = dirty;
            cx.notify();
        }
        if dirty && self.conflict.is_none() {
            self.schedule_autosave(cx);
        } else if !dirty {
            self.autosave = None;
        }
    }

    fn schedule_autosave(&mut self, cx: &mut Context<Self>) {
        self.autosave = Some(cx.spawn(async move |doc, cx| {
            cx.background_executor().timer(AUTOSAVE_DELAY).await;
            doc.update(cx, |doc, cx| {
                doc.autosave = None;
                doc.save_or_log(cx);
            })
            .ok();
        }));
    }

    /// Saves now if there are edits and no conflict, logging a failure.
    /// A failed save leaves the note marked unsaved.
    pub fn save_or_log(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.save(cx) {
            eprintln!("could not save {}: {error}", self.path.display());
        }
    }

    /// Writes unsaved edits to disk. Does nothing while in conflict, so an
    /// outside change is never overwritten without asking.
    pub fn save(&mut self, cx: &mut Context<Self>) -> io::Result<()> {
        self.autosave = None;
        if !self.dirty || self.conflict.is_some() {
            return Ok(());
        }
        self.write(cx)
    }

    fn write(&mut self, cx: &mut Context<Self>) -> io::Result<()> {
        let text = self.current_text(cx);
        let contents = self.line_ending.apply(&text);
        atomic_write(&self.path, &contents)?;
        self.saved_text = text;
        self.disk_text = contents;
        self.dirty = false;
        cx.notify();
        Ok(())
    }

    /// Checks the file after the watcher saw it change.
    pub fn disk_changed(&mut self, cx: &mut Context<Self>) -> DiskOutcome {
        let contents = match std::fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return DiskOutcome::Missing,
            Err(_) => return DiskOutcome::Unchanged,
        };
        if contents == self.disk_text {
            return DiskOutcome::Unchanged;
        }
        if self.dirty && self.current_text(cx) != contents {
            self.autosave = None;
            self.conflict = Some(Conflict::ChangedOnDisk);
            self.disk_text = contents;
            cx.notify();
            return DiskOutcome::Conflicted;
        }
        self.adopt(contents, cx);
        DiskOutcome::Reloaded
    }

    /// Marks the note as deleted under unsaved edits.
    pub fn mark_deleted(&mut self, cx: &mut Context<Self>) {
        self.autosave = None;
        self.conflict = Some(Conflict::DeletedOnDisk);
        cx.notify();
    }

    /// Shows `contents` in every editor as the saved state.
    fn adopt(&mut self, contents: String, cx: &mut Context<Self>) {
        self.line_ending = LineEnding::detect(&contents);
        self.saved_text = contents.clone();
        self.disk_text = contents.clone();
        self.dirty = false;
        self.conflict = None;
        self.autosave = None;
        for editor in self.live_editors() {
            editor.update(cx, |editor, cx| editor.replace_all_text(&contents, cx));
        }
        cx.notify();
    }

    /// Resolves a conflict by writing the editor's text over the file.
    pub fn keep_mine(&mut self, cx: &mut Context<Self>) -> io::Result<()> {
        self.conflict = None;
        self.dirty = true;
        self.write(cx)
    }

    /// Resolves a conflict by showing the file's text. Returns false when
    /// the file is gone.
    pub fn take_disk(&mut self, cx: &mut Context<Self>) -> bool {
        match std::fs::read_to_string(&self.path) {
            Ok(contents) => {
                self.adopt(contents, cx);
                true
            }
            Err(_) => false,
        }
    }

    /// Writes the editor's text to a sibling file so a conflicted note's
    /// edits survive the window closing. Returns where it went.
    pub fn save_conflicted_copy(&mut self, cx: &mut Context<Self>) -> io::Result<PathBuf> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        let base = format!("{} (conflicted copy)", super::files::note_title(&self.path));
        let copy = super::files::unique_note_path(dir, &base);
        let text = self.current_text(cx);
        atomic_write(&copy, &self.line_ending.apply(&text))?;
        Ok(copy)
    }

    /// Follows a rename, ours or one seen on disk.
    pub fn set_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.path = path;
        for editor in self.live_editors() {
            set_paste_context(&editor, self.paste_context(), cx);
        }
        cx.notify();
    }
}
