//! One open note: its file, the editors showing it, whether it has unsaved
//! edits or a conflict with the disk, and autosaving.

use std::io;
use std::path::{Path, PathBuf};

use crate::paste::{PasteContext, set_paste_context};
use std::time::Duration;

use gasp_config::Config;
use gasp_vault::encryption::NoteText;
use gpui::{AppContext, Context, Entity, Subscription, Task, WeakEntity};

use super::files::{LineEnding, atomic_write};
use crate::editor::{EditorEvent, EditorView};
use crate::preview::source::Source;

/// Notes this long open as plain lines and are parsed in the background,
/// so opening one never stalls a frame; shorter ones parse faster than
/// the plain frame would show.
pub const BACKGROUND_PARSE_BYTES: usize = 64 * 1024;

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

/// Whether a note's file is encrypted, and if so whether it's open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sealing {
    #[default]
    Plain,
    /// Encrypted with no key in hand: its editors hold nothing and its
    /// tabs show the locked view.
    Locked,
    /// Encrypted and open: its editors hold the plain text, and every save
    /// seals it.
    Unlocked,
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
    sealing: Sealing,
}

impl NoteDoc {
    /// Reads the note at `path`. An encrypted note opens only when a key
    /// for it is in hand; otherwise it starts locked, with no text.
    pub fn load(
        path: &Path,
        image_dirs: Vec<PathBuf>,
        cx: &mut gpui::App,
    ) -> io::Result<NoteDoc> {
        let raw = std::fs::read_to_string(path)?;
        let (sealing, text) = decode(path, &raw, cx);
        let mut doc = NoteDoc::from_text(path, text, image_dirs);
        doc.disk_text = raw;
        doc.sealing = sealing;
        Ok(doc)
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
            sealing: Sealing::Plain,
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

    /// The text as last saved or read from disk.
    pub fn saved_text(&self) -> &str {
        &self.saved_text
    }

    pub fn conflict(&self) -> Option<Conflict> {
        self.conflict
    }

    pub fn sealing(&self) -> Sealing {
        self.sealing
    }

    pub fn is_locked(&self) -> bool {
        self.sealing == Sealing::Locked
    }

    pub fn is_encrypted(&self) -> bool {
        self.sealing != Sealing::Plain
    }

    pub fn is_unlocked_encrypted(&self) -> bool {
        self.sealing == Sealing::Unlocked
    }

    /// The file's text as last read or written: an encrypted note's
    /// sealed file.
    pub fn disk_text(&self) -> &str {
        &self.disk_text
    }

    /// Whether an autosave is waiting for typing to pause.
    pub fn has_pending_save(&self) -> bool {
        self.autosave.is_some()
    }

    /// A new editor on this note. Edits in any editor reach the others.
    pub fn new_editor(&mut self, config: &Config, cx: &mut Context<Self>) -> Entity<EditorView> {
        let image_dirs = self.image_dirs.clone();
        // A split starts from the parse an open editor already has, which
        // copies far faster than a long note parses.
        let parsed = self
            .live_editors()
            .first()
            .map(|editor| editor.read(cx).source.clone());
        let editor = match parsed {
            Some(source) => cx.new(|cx| EditorView::with_source(source, image_dirs, config, cx)),
            None => {
                let text = self.current_text(cx);
                match text.len() >= BACKGROUND_PARSE_BYTES {
                    true => {
                        let source = Source::unparsed(&text);
                        self.parse_in_background(text, cx);
                        cx.new(|cx| EditorView::with_source(source, image_dirs, config, cx))
                    }
                    false => cx.new(|cx| EditorView::with_config(&text, image_dirs, config, cx)),
                }
            }
        };
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

    /// Parses `text` off the main thread and hands the tree to every
    /// editor still showing it as plain lines, splits included.
    fn parse_in_background(&mut self, text: String, cx: &mut Context<Self>) {
        let parsing = cx.background_spawn(async move { Source::new(&text) });
        cx.spawn(async move |doc, cx| {
            let parsed = parsing.await;
            doc.update(cx, |doc, cx| {
                for editor in doc.live_editors() {
                    let parsed = parsed.clone();
                    editor.update(cx, |editor, cx| editor.take_parsed(parsed, cx));
                }
            })
            .ok();
        })
        .detach();
    }

    /// The note's text as the editors have it.
    pub fn current_text(&self, cx: &gpui::App) -> String {
        self.live_editors()
            .first()
            .map_or_else(|| self.saved_text.clone(), |editor| editor.read(cx).text())
    }

    pub(crate) fn live_editors(&self) -> Vec<Entity<EditorView>> {
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
        if self.is_locked() {
            return;
        }
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

    /// Saves now if there are edits and no conflict, with a notice when
    /// it fails. A failed save leaves the note marked unsaved.
    pub fn save_or_log(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.save(cx) {
            let what = format!("Couldn’t save “{}”", super::files::note_title(&self.path));
            let notice = crate::notices::failure(what, error);
            let detail = notice.detail.clone().unwrap_or_default();
            let notice = notice.with_detail(format!("{detail} Your edits are still open here."));
            crate::notices::show(notice, cx);
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
        let contents = self.file_contents(&text, cx)?;
        if crate::sandbox::writes_allowed() {
            if !self.is_encrypted() {
                crate::recovery::before_save(&self.path, &self.saved_text, cx);
            }
            atomic_write(&self.path, &contents)?;
        }
        self.saved_text = text;
        self.disk_text = contents;
        self.dirty = false;
        cx.notify();
        Ok(())
    }

    /// What the file should hold for `text`: the text itself, or sealed
    /// when the note is encrypted. An encrypted note with no key in hand
    /// can't be written, so its plain text never reaches the disk.
    fn file_contents(&self, text: &str, cx: &gpui::App) -> io::Result<String> {
        let contents = self.line_ending.apply(text);
        if !self.is_encrypted() {
            return Ok(contents);
        }
        crate::encryption::seal(&self.path, &contents, cx)
            .ok_or_else(|| io::Error::other("The note is locked. Unlock it to save your edits."))
    }

    /// Checks the file after the watcher saw it change.
    pub fn disk_changed(&mut self, cx: &mut Context<Self>) -> DiskOutcome {
        let raw = match std::fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return DiskOutcome::Missing,
            Err(_) => return DiskOutcome::Unchanged,
        };
        if raw == self.disk_text {
            return DiskOutcome::Unchanged;
        }
        let (sealing, contents) = decode(&self.path, &raw, cx);
        if self.dirty && self.current_text(cx) != contents {
            self.autosave = None;
            self.conflict = Some(Conflict::ChangedOnDisk);
            self.disk_text = raw;
            cx.notify();
            return DiskOutcome::Conflicted;
        }
        // Whatever replaced the file, the version it replaced stays
        // recoverable, unless it's encrypted: snapshots are plain text.
        if !self.is_encrypted() && sealing == Sealing::Plain {
            crate::recovery::keep_version(&self.path, &self.saved_text, cx);
        }
        self.take_decoded(sealing, contents, raw, cx);
        DiskOutcome::Reloaded
    }

    /// Shows a file's decoded text as the saved state, locked or not.
    fn take_decoded(
        &mut self,
        sealing: Sealing,
        text: String,
        raw: String,
        cx: &mut Context<Self>,
    ) {
        if sealing == Sealing::Plain && self.is_encrypted() {
            crate::encryption::with_encryption(&self.path, cx, |encryption, relative| {
                encryption.decrypt(relative)
            });
        }
        let was_locked = self.is_locked();
        self.sealing = sealing;
        if was_locked != self.is_locked() {
            self.reset_editors(&text, cx);
        }
        self.adopt(text, cx);
        self.disk_text = raw;
    }

    /// Puts `text` in every editor with no undo history to go back
    /// through, as when a note locks or unlocks.
    fn reset_editors(&mut self, text: &str, cx: &mut Context<Self>) {
        for editor in self.live_editors() {
            editor.update(cx, |editor, cx| editor.reset_text(text, cx));
        }
    }

    /// Locks an open encrypted note: unsaved edits are saved (sealed)
    /// first, then the editors drop the plain text and the tabs show the
    /// locked view.
    pub fn relock(&mut self, cx: &mut Context<Self>) {
        if self.sealing != Sealing::Unlocked {
            return;
        }
        self.keep_edits_before_locking(cx);
        self.sealing = Sealing::Locked;
        self.saved_text.clear();
        self.dirty = false;
        self.conflict = None;
        self.autosave = None;
        self.reset_editors("", cx);
        cx.notify();
    }

    fn keep_edits_before_locking(&mut self, cx: &mut Context<Self>) {
        if !self.dirty {
            return;
        }
        if self.conflict.is_none() {
            self.save_or_log(cx);
            return;
        }
        if let Err(error) = self.save_conflicted_copy(cx) {
            crate::notices::failed("Couldn’t keep your edits before locking", error, cx);
        }
    }

    /// Opens a locked note with a key already in hand, as after the vault
    /// was unlocked from another note. Answers whether it opened.
    pub fn reopen(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.is_locked() {
            return false;
        }
        let Ok(raw) = std::fs::read_to_string(&self.path) else {
            return false;
        };
        let (sealing, text) = decode(&self.path, &raw, cx);
        if sealing == Sealing::Locked {
            return false;
        }
        self.take_decoded(sealing, text, raw, cx);
        true
    }

    /// Opens the locked note with `password`: its own, or the vault's.
    pub fn unlock_with_password(
        &mut self,
        password: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let raw = std::fs::read_to_string(&self.path).map_err(|error| error.to_string())?;
        crate::encryption::reload_lock(&self.path, cx);
        crate::encryption::with_encryption(&self.path, cx, |encryption, relative| {
            encryption.unlock_note(relative, &raw, password)
        })
        .ok_or_else(not_in_vault)?
        .map_err(|error| error.to_string())?;
        self.reopen_after_unlock(raw, cx);
        Ok(())
    }

    /// Opens the locked note with a key kept behind Touch ID.
    pub fn unlock_with_key(
        &mut self,
        key: gasp_crypt::Key,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let raw = std::fs::read_to_string(&self.path).map_err(|error| error.to_string())?;
        crate::encryption::with_encryption(&self.path, cx, |encryption, relative| {
            encryption.unlock_note_with_key(relative, &raw, key)
        })
        .ok_or_else(not_in_vault)?
        .map_err(|error| error.to_string())?;
        self.reopen_after_unlock(raw, cx);
        Ok(())
    }

    fn reopen_after_unlock(&mut self, raw: String, cx: &mut Context<Self>) {
        let (sealing, text) = decode(&self.path, &raw, cx);
        self.take_decoded(sealing, text, raw, cx);
    }

    /// Makes this note encrypted from now on and saves it sealed. Its
    /// recovery snapshots, which are plain text, are deleted.
    pub fn encrypt(
        &mut self,
        options: gasp_crypt::NoteOptions,
        own_password: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let text = self.current_text(cx);
        let body = self.line_ending.apply(&text);
        let sealed = crate::encryption::with_encryption(&self.path, cx, |encryption, relative| {
            encryption.encrypt(relative, &body, options, own_password)
        })
        .ok_or_else(not_in_vault)?
        .map_err(|error| error.to_string())?;
        self.sealing = Sealing::Unlocked;
        crate::recovery::forget(&self.path, cx);
        self.write_sealed(text, sealed, cx)
            .map_err(|error| error.to_string())?;
        crate::encryption::mark(&self.path, Some(options.name), cx);
        Ok(())
    }

    /// Stops encrypting this open note and saves its plain text.
    pub fn decrypt(&mut self, cx: &mut Context<Self>) -> io::Result<()> {
        if self.sealing != Sealing::Unlocked {
            return Ok(());
        }
        crate::encryption::with_encryption(&self.path, cx, |encryption, relative| {
            encryption.decrypt(relative)
        });
        self.sealing = Sealing::Plain;
        self.dirty = true;
        self.conflict = None;
        let written = self.write(cx);
        crate::encryption::mark(&self.path, None, cx);
        written
    }

    /// Saves the open encrypted note again now, as after its options
    /// changed.
    pub fn reseal(&mut self, cx: &mut Context<Self>) -> io::Result<()> {
        if self.sealing != Sealing::Unlocked {
            return Ok(());
        }
        self.dirty = true;
        self.conflict = None;
        self.write(cx)
    }

    fn write_sealed(
        &mut self,
        text: String,
        sealed: String,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        if crate::sandbox::writes_allowed() {
            atomic_write(&self.path, &sealed)?;
        }
        self.saved_text = text;
        self.disk_text = sealed;
        self.dirty = false;
        self.autosave = None;
        cx.notify();
        Ok(())
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
            Ok(raw) => {
                let (sealing, contents) = decode(&self.path, &raw, cx);
                self.take_decoded(sealing, contents, raw, cx);
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
        atomic_write(&copy, &self.file_contents(&text, cx)?)?;
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

/// How the file at `path`, holding `raw`, opens, and the text its
/// editors get: never an encrypted note's sealed text.
fn decode(path: &Path, raw: &str, cx: &mut gpui::App) -> (Sealing, String) {
    match crate::encryption::read(path, raw, cx) {
        NoteText::Plain(text) => (Sealing::Plain, text),
        NoteText::Locked(_) => (Sealing::Locked, String::new()),
        NoteText::Unlocked(plain) => (Sealing::Unlocked, plain.body.clone()),
    }
}

fn not_in_vault() -> String {
    "This note isn’t in an open vault.".to_owned()
}