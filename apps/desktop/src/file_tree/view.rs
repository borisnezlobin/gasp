//! The file tree view's state and everything it can do. Keys are in
//! `keys.rs` and drawing in `render.rs`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use editor_config::ConfigLoader;
use editor_config::settings::{FileSettings, TrashMode};
use futures::StreamExt;
use gpui::{
    App, AppContext, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels,
    Point, ScrollStrategy, Subscription, Task, UniformListScrollHandle, Window,
};

use super::FileTreeEvent;
use super::entries::{Entry, EntryKind};
use super::keys::TypeAhead;
use super::menu::{ContextMenu, MenuItem};
use super::model::{Row, TreeModel};
use super::ops::{self, validate_name};
use super::watch::{self, VaultWatcher};
use crate::text_input::{TextInput, TextInputEvent, TextInputStyle};
use crate::theme::PanelTheme;

/// Changes closer together than this refresh the tree once.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(150);
const UNTITLED: &str = "Untitled";

/// How the tree treats files, from the vault's `files` settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileTreeOptions {
    pub update_links_on_rename: bool,
    pub trash: TrashMode,
    /// Refresh when files change on disk.
    pub watch: bool,
}

impl FileTreeOptions {
    pub fn from_settings(files: &FileSettings) -> FileTreeOptions {
        FileTreeOptions {
            update_links_on_rename: files.update_links_on_rename,
            trash: files.trash,
            watch: true,
        }
    }

    /// The options in the vault's `.editor/settings.toml`, over the defaults.
    pub fn for_vault(root: &Path) -> FileTreeOptions {
        let mut loader = ConfigLoader::for_vault(root);
        loader.load_all();
        FileTreeOptions::from_settings(&loader.config().settings.files)
    }
}

/// What an inline name field is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EditTarget {
    Rename(Entry),
    Create { folder: PathBuf, kind: EntryKind },
}

/// An inline name field in the tree.
pub(super) struct InlineEdit {
    pub target: EditTarget,
    pub field: Entity<TextInput>,
    pub error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

/// A row as drawn: an entry, or the name field of an entry being created.
pub(super) enum DisplayRow {
    Entry(usize),
    NewEntry { depth: usize, kind: EntryKind },
}

/// The file explorer over one vault.
pub struct FileTree {
    pub(super) focus_handle: FocusHandle,
    pub(super) model: TreeModel,
    pub(super) theme: PanelTheme,
    pub(super) selected: Option<PathBuf>,
    pub(super) active: Option<PathBuf>,
    pub(super) scroll: UniformListScrollHandle,
    pub(super) edit: Option<InlineEdit>,
    pub(super) pending_trash: Option<PathBuf>,
    pub(super) menu: Option<ContextMenu>,
    pub(super) cut: Option<PathBuf>,
    pub(super) type_ahead: TypeAhead,
    pub(super) options: FileTreeOptions,
    _watcher: Option<VaultWatcher>,
    _watch_task: Option<Task<()>>,
}

impl EventEmitter<FileTreeEvent> for FileTree {}

impl Focusable for FileTree {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FileTree {
    /// A tree over `vault_root` that follows the vault's settings and
    /// watches the folder for changes.
    pub fn new(
        vault_root: impl Into<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let root = vault_root.into();
        let options = FileTreeOptions::for_vault(&root);
        Self::with_options(root, options, window, cx)
    }

    pub fn with_options(
        vault_root: impl Into<PathBuf>,
        options: FileTreeOptions,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut tree = FileTree {
            focus_handle: cx.focus_handle(),
            model: TreeModel::new(vault_root),
            theme: PanelTheme::default(),
            selected: None,
            active: None,
            scroll: UniformListScrollHandle::new(),
            edit: None,
            pending_trash: None,
            menu: None,
            cut: None,
            type_ahead: TypeAhead::default(),
            options,
            _watcher: None,
            _watch_task: None,
        };
        if options.watch {
            tree.start_watching(cx);
        }
        tree
    }

    fn start_watching(&mut self, cx: &mut Context<Self>) {
        let Ok((watcher, mut changes)) = watch::watch(self.model.root()) else {
            return;
        };
        self._watcher = Some(watcher);
        self._watch_task = Some(cx.spawn(async move |this, cx| {
            while changes.next().await.is_some() {
                cx.background_executor().timer(REFRESH_DEBOUNCE).await;
                while changes.try_recv().is_ok() {}
                if this.update(cx, |tree, cx| tree.refresh(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    // ---- Public API for the workspace ----

    pub fn root(&self) -> &Path {
        self.model.root()
    }

    /// The visible rows, top to bottom.
    pub fn rows(&self) -> &[Row] {
        self.model.rows()
    }

    /// The selected entry's absolute path.
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected.as_ref().map(|path| self.absolute(path))
    }

    /// Marks the open note's row. Paths outside the vault clear it.
    pub fn set_active_path(&mut self, path: Option<&Path>, cx: &mut Context<Self>) {
        self.active = path.and_then(|path| self.relative(path));
        cx.notify();
    }

    /// Selects `path` and scrolls to it, expanding the folders above it.
    /// Returns false when the path isn't a visible entry of this vault.
    pub fn reveal(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        let Some(relative) = self.relative(path) else {
            return false;
        };
        self.model.expand_ancestors(&relative);
        let Some(index) = self.model.index_of(&relative) else {
            return false;
        };
        self.select_index(index, cx);
        self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        cx.emit(FileTreeEvent::ExpansionChanged);
        true
    }

    /// Expanded folders, relative to the vault root, for saving.
    pub fn expanded_folders(&self) -> Vec<PathBuf> {
        self.model.expanded().map(Path::to_path_buf).collect()
    }

    /// Restores saved expanded folders (relative to the vault root).
    pub fn set_expanded_folders(&mut self, folders: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.model.set_expanded(folders);
        cx.notify();
    }

    /// Re-reads the folders from disk.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.model.refresh();
        cx.notify();
    }

    pub fn set_options(&mut self, options: FileTreeOptions) {
        self.options = FileTreeOptions {
            watch: self.options.watch,
            ..options
        };
    }

    /// The inline name field, while renaming or creating.
    pub fn editing_field(&self) -> Option<Entity<TextInput>> {
        self.edit.as_ref().map(|edit| edit.field.clone())
    }

    /// The inline field's error, such as a name that's taken.
    pub fn edit_error(&self) -> Option<&str> {
        self.edit.as_ref()?.error.as_deref()
    }

    pub fn context_menu_items(&self) -> Option<Vec<MenuItem>> {
        self.menu.as_ref().map(|menu| menu.items.clone())
    }

    /// The entry waiting for the user to confirm it goes to the trash.
    pub fn pending_trash(&self) -> Option<PathBuf> {
        self.pending_trash.as_ref().map(|path| self.absolute(path))
    }

    // ---- Paths ----

    pub(super) fn absolute(&self, relative: &Path) -> PathBuf {
        self.model.root().join(relative)
    }

    fn relative(&self, path: &Path) -> Option<PathBuf> {
        // On Windows `/elsewhere` has no drive letter, so it counts as
        // relative, but it still starts from a root and isn't in the vault.
        if path.is_relative() && !path.has_root() {
            return Some(path.to_path_buf());
        }
        path.strip_prefix(self.model.root())
            .ok()
            .map(Path::to_path_buf)
    }

    pub(super) fn selected_index(&self) -> Option<usize> {
        self.model.index_of(self.selected.as_deref()?)
    }

    fn selected_entry(&self) -> Option<Entry> {
        let index = self.selected_index()?;
        Some(self.model.row(index)?.entry.clone())
    }

    /// The folder new entries go in: the selected folder, or the selected
    /// file's folder, or the root.
    fn target_folder(&self) -> PathBuf {
        match self.selected_entry() {
            Some(entry) if entry.is_folder() => entry.path,
            Some(entry) => entry.parent().to_path_buf(),
            None => PathBuf::new(),
        }
    }

    // ---- Rows as drawn ----

    /// Where the new-entry field sits: (row index, depth, kind).
    fn creation_slot(&self) -> Option<(usize, usize, EntryKind)> {
        let EditTarget::Create { folder, kind } = &self.edit.as_ref()?.target else {
            return None;
        };
        if folder.as_os_str().is_empty() {
            return Some((0, 0, *kind));
        }
        let index = self.model.index_of(folder)?;
        let depth = self.model.row(index)?.depth + 1;
        Some((index + 1, depth, *kind))
    }

    pub(super) fn display_row_count(&self) -> usize {
        self.model.rows().len() + usize::from(self.creation_slot().is_some())
    }

    pub(super) fn display_row(&self, index: usize) -> DisplayRow {
        match self.creation_slot() {
            Some((slot, depth, kind)) if slot == index => DisplayRow::NewEntry { depth, kind },
            Some((slot, ..)) if index > slot => DisplayRow::Entry(index - 1),
            _ => DisplayRow::Entry(index),
        }
    }

    // ---- Selection and expansion ----

    pub(super) fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.model.row(index) {
            self.selected = Some(row.entry.path.clone());
            self.scroll.scroll_to_item(index, ScrollStrategy::Top);
            cx.notify();
        }
    }

    pub(super) fn select_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Some(index) = self.model.index_of(path) {
            self.select_index(index, cx);
        }
    }

    /// Moves the selection by `delta` rows, stopping at the ends.
    pub(super) fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.model.rows().len();
        if count == 0 {
            return;
        }
        let target = match self.selected_index() {
            Some(index) => (index as isize + delta).clamp(0, count as isize - 1) as usize,
            None if delta < 0 => count - 1,
            None => 0,
        };
        self.select_index(target, cx);
    }

    /// Left: collapses an open folder, or goes to the parent folder.
    pub(super) fn collapse_or_parent(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.selected_index() else {
            return;
        };
        let row = &self.model.rows()[index];
        if row.expanded {
            let path = row.entry.path.clone();
            self.set_expanded(&path, false, cx);
        } else if let Some(parent) = self.model.parent_index(index) {
            self.select_index(parent, cx);
        }
    }

    /// Right: expands a closed folder, or goes into an open one.
    pub(super) fn expand_or_child(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.selected_index() else {
            return;
        };
        let row = &self.model.rows()[index];
        if !row.entry.is_folder() {
            return;
        }
        if row.expanded {
            let has_children = self
                .model
                .row(index + 1)
                .is_some_and(|next| next.depth > row.depth);
            if has_children {
                self.select_index(index + 1, cx);
            }
        } else {
            let path = row.entry.path.clone();
            self.set_expanded(&path, true, cx);
        }
    }

    pub(super) fn set_expanded(&mut self, folder: &Path, expanded: bool, cx: &mut Context<Self>) {
        let changed = if expanded {
            self.model.expand(folder)
        } else {
            self.model.collapse(folder)
        };
        if changed {
            cx.emit(FileTreeEvent::ExpansionChanged);
            cx.notify();
        }
    }

    pub(super) fn toggle(&mut self, folder: &Path, cx: &mut Context<Self>) {
        let expanded = self.model.is_expanded(folder);
        self.set_expanded(folder, !expanded, cx);
    }

    /// Enter: opens a file, or opens and closes a folder.
    pub(super) fn activate(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if entry.is_folder() {
            self.toggle(&entry.path, cx);
        } else {
            cx.emit(FileTreeEvent::Open {
                path: self.absolute(&entry.path),
                new_tab,
            });
        }
    }

    /// A click on a row: selects it, then opens a file or toggles a folder.
    pub(super) fn click_row(
        &mut self,
        index: usize,
        new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.menu = None;
        self.select_index(index, cx);
        self.activate(new_tab, cx);
    }

    // ---- Inline rename and create ----

    /// F2: edits the selected entry's name in place.
    pub fn start_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let label = entry.label().to_string();
        let stem_len = if entry.kind == EntryKind::Image || entry.kind == EntryKind::Pdf {
            label.rfind('.').unwrap_or(label.len())
        } else {
            label.len()
        };
        self.begin_edit(EditTarget::Rename(entry), &label, 0..stem_len, window, cx);
    }

    /// Starts naming a new note or folder in the selected folder.
    pub fn start_create(&mut self, kind: EntryKind, window: &mut Window, cx: &mut Context<Self>) {
        let folder = self.target_folder();
        if !folder.as_os_str().is_empty() {
            self.set_expanded(&folder, true, cx);
        }
        let name = ops::unique_name(self.model.root(), &folder, UNTITLED, kind);
        let target = EditTarget::Create { folder, kind };
        self.begin_edit(target, &name, 0..name.len(), window, cx);
    }

    fn begin_edit(
        &mut self,
        target: EditTarget,
        text: &str,
        selection: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        let field = cx.new(|cx| {
            let mut field = TextInput::new(window, cx).with_style(TextInputStyle::Inline);
            field.set_text(text, cx);
            field.select(selection, cx);
            field
        });
        let subscriptions = vec![cx.subscribe_in(&field, window, Self::on_field_event)];
        window.focus(&field.focus_handle(cx));
        self.edit = Some(InlineEdit {
            target,
            field,
            error: None,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    fn on_field_event(
        &mut self,
        _: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submitted => self.commit_edit(window, cx),
            TextInputEvent::Cancelled => self.cancel_edit(window, cx),
            // Clicking away keeps a good name and drops a bad one.
            TextInputEvent::Blurred => {
                self.commit_edit(window, cx);
                if self.edit.is_some() {
                    self.cancel_edit(window, cx);
                }
            }
            TextInputEvent::Changed => {
                if let Some(edit) = self.edit.as_mut() {
                    edit.error = None;
                }
                cx.notify();
            }
        }
    }

    pub fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.take().is_some() {
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    /// Applies the typed name. On a bad name the field stays open with
    /// the reason under it.
    pub fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.as_ref() else {
            return;
        };
        let typed = edit.field.read(cx).text().trim().to_string();
        let result = match edit.target.clone() {
            EditTarget::Rename(entry) => self.rename_to(&entry, &typed, cx),
            EditTarget::Create { folder, kind } => self.create(&folder, &typed, kind, cx),
        };
        match result {
            Ok(()) => {
                self.edit = None;
                window.focus(&self.focus_handle);
            }
            Err(message) => {
                if let Some(edit) = self.edit.as_mut() {
                    edit.error = Some(message);
                }
            }
        }
        cx.notify();
    }

    fn rename_to(
        &mut self,
        entry: &Entry,
        typed: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if typed == entry.label() {
            return Ok(());
        }
        validate_name(typed).map_err(|error| error.to_string())?;
        let to = entry.parent().join(ops::file_name_for(typed, entry.kind));
        self.rename_entry(&entry.path, &to, cx)
    }

    fn create(
        &mut self,
        folder: &Path,
        typed: &str,
        kind: EntryKind,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let created =
            ops::create(self.model.root(), folder, typed, kind).map_err(|e| e.to_string())?;
        self.model.refresh();
        self.model.expand_ancestors(&created);
        self.select_path(&created, cx);
        let path = self.absolute(&created);
        cx.emit(FileTreeEvent::Created { path: path.clone() });
        if kind == EntryKind::Note {
            cx.emit(FileTreeEvent::Open {
                path,
                new_tab: false,
            });
        }
        Ok(())
    }

    // ---- Moving ----

    /// Renames or moves `from` to `to` (relative), updating links and
    /// telling the workspace.
    pub(super) fn rename_entry(
        &mut self,
        from: &Path,
        to: &Path,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let root = self.model.root().to_path_buf();
        let renamed = ops::rename(&root, from, to, self.options.update_links_on_rename)
            .map_err(|error| error.to_string())?;
        self.model.follow_move(from, to);
        self.model.expand_ancestors(to);
        self.active = self
            .active
            .take()
            .map(|active| moved_path(&active, from, to));
        self.select_path(to, cx);
        cx.emit(FileTreeEvent::Renamed {
            from: root.join(from),
            to: root.join(to),
        });
        if !renamed.updated_notes.is_empty() {
            let paths = renamed.updated_notes.iter().map(|p| root.join(p)).collect();
            cx.emit(FileTreeEvent::LinksUpdated { paths });
        }
        Ok(())
    }

    /// Moves an entry into a folder, as a drop or a paste does.
    pub fn move_into(&mut self, from: &Path, folder: &Path, cx: &mut Context<Self>) {
        let (Some(from), Some(folder)) = (self.relative(from), self.relative(folder)) else {
            return;
        };
        let Some(name) = from.file_name() else {
            return;
        };
        let to = folder.join(name);
        if to == from {
            return;
        }
        if let Err(message) = self.rename_entry(&from, &to, cx) {
            cx.emit(FileTreeEvent::Failed { message });
        }
        cx.notify();
    }

    /// Mod+X: marks the selection to move with the next paste.
    pub(super) fn cut_selected(&mut self, cx: &mut Context<Self>) {
        self.cut = self.selected.clone();
        cx.notify();
    }

    /// Mod+V: moves the cut entry into the selected folder.
    pub(super) fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(cut) = self.cut.take() else {
            return;
        };
        let folder = self.target_folder();
        self.move_into(&cut, &folder, cx);
    }

    // ---- Trash ----

    /// Asks to confirm moving the selection to the trash.
    pub fn request_trash(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        self.pending_trash = self.selected.clone();
        cx.notify();
    }

    pub fn cancel_trash(&mut self, cx: &mut Context<Self>) {
        self.pending_trash = None;
        cx.notify();
    }

    pub fn confirm_trash(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.pending_trash.take() else {
            return;
        };
        let next = self.selected_index();
        match ops::trash(self.model.root(), &path, self.options.trash) {
            Ok(()) => {
                self.model.refresh();
                self.reselect_near(next, cx);
                cx.emit(FileTreeEvent::Trashed {
                    path: self.absolute(&path),
                });
            }
            Err(error) => cx.emit(FileTreeEvent::Failed {
                message: format!("Couldn't move it to the trash: {error}"),
            }),
        }
        cx.notify();
    }

    fn reselect_near(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.selected = None;
        let count = self.model.rows().len();
        if let (Some(index), true) = (index, count > 0) {
            self.select_index(index.min(count - 1), cx);
        }
    }

    // ---- Context menu ----

    /// Opens the context menu for an entry (or the root, for `None`).
    pub fn open_menu(
        &mut self,
        target: Option<PathBuf>,
        position: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = &target {
            self.select_path(path, cx);
        }
        self.menu = Some(ContextMenu::new(target, position));
        cx.notify();
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        cx.notify();
    }

    /// Runs a menu item on the menu's target.
    pub fn run_menu_item(&mut self, item: MenuItem, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.menu.take().and_then(|menu| menu.target);
        if target.is_none() {
            self.selected = None;
        }
        let path = self.absolute(target.as_deref().unwrap_or(Path::new("")));
        match item {
            MenuItem::NewNote => self.start_create(EntryKind::Note, window, cx),
            MenuItem::NewFolder => self.start_create(EntryKind::Folder, window, cx),
            MenuItem::Rename => self.start_rename(window, cx),
            MenuItem::Reveal => cx.reveal_path(&path),
            MenuItem::CopyPath => cx.write_to_clipboard(ClipboardItem::new_string(
                path.to_string_lossy().into_owned(),
            )),
            MenuItem::Trash => self.request_trash(cx),
        }
        cx.notify();
    }

    /// Copies the selected entry's absolute path.
    pub(super) fn copy_selected_path(&self, cx: &mut Context<Self>) {
        if let Some(path) = self.selected_path() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                path.to_string_lossy().into_owned(),
            ));
        }
    }

    // ---- Type to jump ----

    /// Selects the next row whose name starts with what's been typed.
    pub(super) fn type_to_jump(&mut self, text: &str, cx: &mut Context<Self>) {
        let prefix = self.type_ahead.push(text, std::time::Instant::now());
        let labels: Vec<&str> = self
            .model
            .rows()
            .iter()
            .map(|row| row.entry.label())
            .collect();
        let from = self.selected_index();
        if let Some(index) = super::keys::next_match(&labels, from, &prefix) {
            self.select_index(index, cx);
        }
    }
}

/// Where `path` is after `from` moved to `to`.
fn moved_path(path: &Path, from: &Path, to: &Path) -> PathBuf {
    match path.strip_prefix(from) {
        Ok(rest) if rest.as_os_str().is_empty() => to.to_path_buf(),
        Ok(rest) => to.join(rest),
        Err(_) => path.to_path_buf(),
    }
}
