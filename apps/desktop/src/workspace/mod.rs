//! The window's root view: a vault's panes of tabbed notes, the left panel
//! slot, the toolbars (the status bar is one) and the modal slot.
//!
//! The workspace sets the `"Workspace"` key context and runs every command
//! that isn't the editor's (tabs, panes, notes, history, the sidebar).
//! Other pieces plug in through [`Workspace::set_left_panel`],
//! [`Workspace::set_file_tree`], [`Workspace::set_reading_probe`],
//! [`Workspace::toggle_modal`], [`Workspace::on_command`],
//! [`Workspace::open_path`], [`Workspace::active_editor`] and
//! [`Pane::set_toolbar`].
//!
//! Everything it runs can be reached with the mouse too: the sidebar's
//! buttons and footer, each pane's tab bar and note header, their menus
//! and the note's right-click menu, all built from `crate::ui`.

mod commands;
pub mod deleted;
mod edit_tracking;
pub mod file_opening;
pub mod files;
pub mod help;
pub mod history;
pub mod image_tab;
pub mod launcher;
mod layout;
pub mod links;
mod mcp;
pub mod menus;
pub mod modal;
pub mod note_doc;
pub mod note_header;
pub mod note_shape;
mod notes;
pub mod pane;
mod pane_menus;
pub mod pane_tree;
mod panel;
mod panes;
mod positions;
pub mod prompt;
mod render;
pub mod right_panel;
mod shortcut_sheet;
pub mod sidebar;
mod sidebar_chrome;
pub mod startup;
pub mod state;
pub mod status;
pub mod tab_bar;
pub mod tab_drag;
mod tab_moves;
mod tabs;
mod toolbars;
pub mod vault_choices;
pub mod watcher;
pub mod window;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gasp_config::Config;
use gpui::{
    AnyView, App, AppContext, Context, Entity, EntityId, FocusHandle, Focusable, ManagedView,
    Subscription, Task, Window, WindowBounds,
};

pub use self::commands::handles;
use self::modal::{ModalHost, ModalLayer};
use self::note_doc::NoteDoc;
pub use self::pane::{Pane, ReadingProbe};
use self::pane_tree::{PaneTree, SplitId};
use self::right_panel::RightPanel;
pub use self::shortcut_sheet::{FocusArea, HOLD_DELAY, SheetGroup, sheet_groups};
use self::sidebar::LeftPanel;
use self::startup::VaultStart;
use self::status::StatusInfo;
use crate::editor::EditorView;
use crate::file_tree::FileTree;
use crate::note_texts::NoteTexts;
use crate::sync::SyncService;
use crate::theme::Theme;
use crate::ui::{HasMenuSlot, MenuSlot};
use crate::vault_index::{VaultIndex, index_changes};

/// Where [`Workspace::open_path`] puts a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenIn {
    /// In place of the active tab's note.
    ActiveTab,
    /// In a new tab in the active pane.
    NewTab,
    /// In a new pane to the right of the active one.
    SplitRight,
}

/// A command handler added with [`Workspace::on_command`].
pub type CommandHandler = Rc<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

/// What the pointer is dragging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drag {
    Divider(SplitId),
    Sidebar,
    RightSidebar,
}

/// What a resize changes, as it was when the resize started.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DragStart {
    Ratio(SplitId, f32),
    LeftWidth(gpui::Pixels),
    RightWidth(gpui::Pixels),
}

/// The cursor as last seen, for spotting big jumps.
#[derive(Clone, Copy, Debug, Default)]
struct CursorSeen {
    offset: usize,
    line: usize,
}

/// Closed tabs kept for reopening.
const MAX_CLOSED_TABS: usize = 50;

pub struct Workspace {
    vault: PathBuf,
    config: Config,
    focus_handle: FocusHandle,
    panes: PaneTree<Entity<Pane>>,
    active_pane: Entity<Pane>,
    docs: Vec<Entity<NoteDoc>>,
    closed_tabs: Vec<PathBuf>,
    /// Where the reader was in recently shown notes, oldest first.
    positions: Vec<gasp_config::device::NotePosition>,
    recent: Vec<PathBuf>,
    /// The vault's notes by modification time, newest first, as read
    /// while the app started, for the first launcher.
    recency: Option<Vec<PathBuf>>,
    left_panel: LeftPanel,
    right_panel: RightPanel,
    file_tree: Option<Entity<FileTree>>,
    /// The note the file tree marks as open.
    tree_active: Option<PathBuf>,
    menu: MenuSlot,
    reading_probe: Option<ReadingProbe>,
    modal: ModalLayer,
    status: Option<StatusInfo>,
    drag: Option<Drag>,
    /// What a resize under way changes, as it was, for Escape to put back.
    drag_start: Option<DragStart>,
    theme: Theme,
    extra_commands: HashMap<String, CommandHandler>,
    cursors: HashMap<EntityId, CursorSeen>,
    pane_subscriptions: HashMap<EntityId, Vec<Subscription>>,
    window_bounds: Option<WindowBounds>,
    window_title: String,
    watcher: Option<crate::vault_watch::WatchHandle>,
    /// The vault's note texts, for vault search.
    note_texts: NoteTexts,
    rule_clock: Option<sidebar::ExecutorClock>,
    sync: Option<Entity<SyncService>>,
    sync_indicator: Option<AnyView>,
    tasks: Vec<Task<()>>,
    /// The notes and tags editors suggest from.
    vault_index: Entity<VaultIndex>,
    /// The shortcuts that holding Mod shows.
    sheet: shortcut_sheet::ShortcutSheet,
    /// Time spent editing each note, on every device.
    edit_time: crate::edit_time::EditTime,
    /// Whether a write of this device's edit times is on its way.
    edit_time_save_pending: bool,
    /// Answers `gasp mcp` about this window.
    mcp: mcp::McpBridge,
    /// Which toolbar the keyboard is in, and where.
    toolbar_focus: Option<crate::toolbar::ToolbarFocus>,
    toolbar_hover: toolbars::ToolbarHover,
    /// The toolbar an add button asked the settings page to add to.
    toolbar_to_add_to: Option<String>,
    /// Notes moved to the trash this session, for `note.restore-deleted`.
    deleted: Vec<deleted::DeletedNote>,
    /// The notice saying the last typed title couldn't be the note's name.
    rename_notice: Option<u64>,
    /// This window's theme, from its own vault: other windows may show
    /// vaults with different ones.
    window_theme: Option<crate::ui::WindowTheme>,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalHost for Workspace {
    fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal.close(window);
        cx.notify();
    }
}

impl HasMenuSlot for Workspace {
    fn menu_slot(&mut self) -> &mut MenuSlot {
        &mut self.menu
    }
}

impl Workspace {
    /// A workspace on `vault` with one empty tab. Reads the vault's config
    /// from `.gasp/`. Call [`Workspace::watch_vault`] to follow changes
    /// on disk and [`Workspace::restore_session`] to reopen saved tabs.
    pub fn new(vault: &Path, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::from_start(VaultStart::load(vault), window, cx)
    }

    /// A workspace on a vault whose config and recent notes are already
    /// read, as when they were read while the app started.
    pub fn from_start(start: VaultStart, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let VaultStart {
            vault,
            config,
            recent: recency,
        } = start;
        let theme = Theme::default();
        let show_title = config.settings.editor.show_inline_title;
        crate::ui::hints::set_rules(config.rules.clone(), cx);
        let pane = cx.new(|cx| Pane::new(&vault, show_title, cx));
        let vault_index = cx.new(|_| VaultIndex::new(&vault));
        let left_panel = LeftPanel::new(
            &config.settings,
            &config.rules,
            theme.workspace.sidebar_width,
        );
        let note_texts = NoteTexts::new(&vault);
        let positions = config.device.positions.clone();
        let right_panel =
            RightPanel::new(&config.device.right_sidebar, theme.workspace.sidebar_width);
        let deleted = deleted::remembered(&vault, &config.device.deleted_notes);
        let mut workspace = Workspace {
            vault,
            config,
            focus_handle: cx.focus_handle(),
            panes: PaneTree::new(pane.clone()),
            active_pane: pane.clone(),
            docs: Vec::new(),
            closed_tabs: Vec::new(),
            positions,
            recent: Vec::new(),
            recency,
            left_panel,
            right_panel,
            file_tree: None,
            tree_active: None,
            menu: MenuSlot::default(),
            reading_probe: None,
            modal: ModalLayer::default(),
            status: None,
            drag: None,
            drag_start: None,
            theme,
            extra_commands: HashMap::new(),
            cursors: HashMap::new(),
            pane_subscriptions: HashMap::new(),
            window_bounds: None,
            window_title: String::new(),
            watcher: None,
            note_texts,
            rule_clock: None,
            sync: None,
            sync_indicator: None,
            tasks: Vec::new(),
            vault_index,
            sheet: Default::default(),
            edit_time: crate::edit_time::EditTime::new("", ""),
            edit_time_save_pending: false,
            mcp: Default::default(),
            toolbar_focus: None,
            toolbar_hover: Default::default(),
            toolbar_to_add_to: None,
            deleted,
            rename_notice: None,
            window_theme: None,
            _subscriptions: Vec::new(),
        };
        workspace.scan_vault_index(cx);
        workspace.subscribe_to_pane(&pane, window, cx);
        workspace.observe_window(window, cx);
        workspace.observe_appearance(window, cx);
        workspace.watch_keystrokes_for_sheet(window, cx);
        workspace.start_edit_time(cx);
        workspace.add_launcher_tab(&pane, window, cx);
        workspace._subscriptions.push(crate::notices::observe(cx));
        workspace
    }

    pub fn vault(&self) -> &Path {
        &self.vault
    }

    /// The vault's note texts as last read for vault search, kept up to
    /// date while the vault is watched.
    pub fn note_texts(&self) -> &NoteTexts {
        &self.note_texts
    }

    /// The loaded config: settings, rules, theme tokens and device state.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The pane that has focus, or had it last.
    pub fn active_pane(&self) -> &Entity<Pane> {
        &self.active_pane
    }

    /// Every pane, left to right and top to bottom.
    pub fn panes(&self) -> Vec<Entity<Pane>> {
        self.panes.panes()
    }

    /// The editor in the active pane's active tab.
    pub fn active_editor(&self, cx: &App) -> Option<Entity<EditorView>> {
        self.active_pane.read(cx).active_editor()
    }

    /// The path of the active pane's active note.
    pub fn active_path(&self, cx: &App) -> Option<PathBuf> {
        let pane = self.active_pane.read(cx);
        pane.active_tab()?.path(cx).map(Path::to_path_buf)
    }

    /// The open note at `path`, if any tab shows it.
    pub fn doc_for_path(&self, path: &Path, cx: &App) -> Option<Entity<NoteDoc>> {
        self.docs
            .iter()
            .find(|doc| doc.read(cx).path() == path)
            .cloned()
    }

    /// Notes opened this session, most recent first.
    pub fn recent_notes(&self) -> &[PathBuf] {
        &self.recent
    }

    /// What the status bar shows.
    pub fn status(&self) -> Option<&StatusInfo> {
        self.status.as_ref()
    }

    /// Hosts `view` in the left panel. `focus` is where `file-tree.focus`
    /// sends the keyboard.
    pub fn set_left_panel(
        &mut self,
        view: AnyView,
        focus: Option<FocusHandle>,
        cx: &mut Context<Self>,
    ) {
        self.left_panel.set_view(view, focus);
        cx.notify();
    }

    pub fn left_panel(&self) -> &LeftPanel {
        &self.left_panel
    }

    /// Hosts the file tree in the left panel, with the sidebar's buttons
    /// for it: new folder, sort order and collapse all.
    pub fn set_file_tree(&mut self, tree: Entity<FileTree>, cx: &mut Context<Self>) {
        let focus = tree.read(cx).focus_handle(cx);
        self.set_left_panel(tree.clone().into(), Some(focus), cx);
        self.file_tree = Some(tree);
    }

    pub fn file_tree(&self) -> Option<&Entity<FileTree>> {
        self.file_tree.as_ref()
    }

    /// Syncs the vault with `service`, shown in the status bar by
    /// `indicator`. Notes it has conflicts in get a banner.
    pub fn set_sync(
        &mut self,
        service: Entity<SyncService>,
        indicator: AnyView,
        cx: &mut Context<Self>,
    ) {
        let observe = cx.observe(&service, |workspace, _, cx| {
            workspace.show_sync_conflicts(cx);
            cx.notify();
        });
        self._subscriptions.push(observe);
        self.sync = Some(service);
        self.sync_indicator = Some(indicator);
        cx.notify();
    }

    /// Shows `widget` where the status bar's sync indicator goes, for a
    /// vault something other than git keeps in step, such as iCloud.
    pub fn set_sync_widget(&mut self, widget: AnyView, cx: &mut Context<Self>) {
        self.sync_indicator = Some(widget);
        cx.notify();
    }

    pub fn sync(&self) -> Option<&Entity<SyncService>> {
        self.sync.as_ref()
    }

    /// Tells each pane which of its notes sync left in conflict.
    fn show_sync_conflicts(&mut self, cx: &mut Context<Self>) {
        let Some(service) = &self.sync else {
            return;
        };
        let service = service.read(cx);
        let root = service.root().to_path_buf();
        let conflicted: Vec<PathBuf> = service
            .conflicts()
            .iter()
            .map(|file| root.join(&file.path))
            .collect();
        for pane in self.panes.panes() {
            pane.update(cx, |pane, cx| {
                pane.set_sync_conflicts(conflicted.clone(), cx)
            });
        }
    }

    /// How the reading-view button learns whether a note shows as a
    /// finished page. Without one the button always offers reading view.
    pub fn set_reading_probe(&mut self, probe: ReadingProbe, cx: &mut Context<Self>) {
        for pane in self.panes.panes() {
            pane.update(cx, |pane, cx| {
                pane.reading_probe = Some(probe.clone());
                cx.notify();
            });
        }
        self.reading_probe = Some(probe);
    }

    /// The open sidebar or pane menu, if any.
    pub fn open_menu(&self, cx: &App) -> Option<Entity<crate::ui::DropdownMenu>> {
        self.menu.menu().or_else(|| {
            self.panes
                .panes()
                .iter()
                .find_map(|pane| pane.read(cx).open_menu())
        })
    }

    /// Opens a `V` in the modal slot, or closes it if one is open. The
    /// modal closes on its `DismissEvent`, Escape or a click outside, and
    /// focus goes back to where it was.
    pub fn toggle_modal<V: ManagedView>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl FnOnce(&mut Window, &mut Context<V>) -> V,
    ) {
        let mut modal = std::mem::take(&mut self.modal);
        modal.toggle(window, cx, build);
        self.modal = modal;
    }

    /// Lets the open modal draw at its own size.
    pub fn set_modal_self_sized(&mut self, cx: &mut Context<Self>) {
        self.modal.set_self_sized();
        cx.notify();
    }

    /// Reads the vault's config files again and restyles every open note.
    pub fn reload_config(&mut self, cx: &mut Context<Self>) {
        let mut loader = gasp_config::ConfigLoader::for_vault(&self.vault);
        loader.load_all();
        self.config = loader.config().clone();
        let snippets = self.config.typing.snippets.engine.clone();
        cx.background_spawn(async move { snippets.warm() }).detach();
        self.left_panel
            .apply_settings(&self.config.settings, &self.config.rules);
        self.apply_theme(cx);
        self.show_dock_icon(cx);
        crate::recovery::configure(&self.vault, &self.config.settings.recovery, cx);
        let grammar = &self.config.settings.prose.grammar;
        crate::prose::checker::configure(&self.vault, grammar, cx);
        self.sync_mcp_bridge(cx);
        self.restyle_editors(cx);
    }

    /// Follows the system's light or dark appearance as it changes, for
    /// the theme setting that matches it and the Dock icon's dark version.
    fn observe_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::ui::set_system_dark(crate::ui::is_dark_appearance(window.appearance()), cx);
        self.apply_theme(cx);
        self.show_dock_icon(cx);
        let observe = cx.observe_window_appearance(window, |workspace, window, cx| {
            let dark = crate::ui::is_dark_appearance(window.appearance());
            crate::ui::set_system_dark(dark, cx);
            workspace.show_dock_icon(cx);
            if workspace.apply_theme(cx) {
                workspace.restyle_editors(cx);
            }
        });
        self._subscriptions.push(observe);
    }

    /// Gives the Dock the app icon the settings pick, in the system's
    /// light or dark version.
    fn show_dock_icon(&self, cx: &mut Context<Self>) {
        crate::dock_icon::show_in_dock(self.config.settings.appearance.app_icon, cx);
    }

    /// Builds this window's light or dark theme as the settings ask and
    /// puts it in effect, redrawing only this window. Answers whether it
    /// changed.
    fn apply_theme(&mut self, cx: &mut Context<Self>) -> bool {
        let choice = self.config.settings.appearance.theme;
        let dark = choice.is_dark(crate::ui::system_dark(cx));
        if self.window_theme.as_ref().is_some_and(|theme| theme.matches(&self.config.theme, dark)) {
            return false;
        }
        self.window_theme = Some(crate::ui::WindowTheme::new(&self.config.theme, dark, cx));
        self.use_own_theme(cx);
        cx.notify();
        true
    }

    /// Puts this window's theme in effect for what's drawn or handled
    /// next, as it draws and when it comes to the front.
    pub(crate) fn use_own_theme(&mut self, cx: &mut App) {
        if let Some(theme) = self.window_theme.as_mut() {
            crate::ui::use_window_theme(theme, cx);
        }
    }

    /// Gives every open note the current config and theme.
    /// The editor of every note open in a tab.
    pub(crate) fn open_editors(&self, cx: &App) -> Vec<Entity<EditorView>> {
        self.panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx)
                    .tabs()
                    .iter()
                    .filter_map(|tab| tab.note().map(|note| note.editor.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn restyle_editors(&mut self, cx: &mut Context<Self>) {
        let editors = self.open_editors(cx);
        let config = self.config.clone();
        for editor in editors {
            editor.update(cx, |editor, cx| editor.apply_config(&config, cx));
        }
        cx.notify();
    }

    /// The open modal, if it's a `V`.
    pub fn active_modal<V: 'static>(&self) -> Option<Entity<V>> {
        self.modal.active::<V>()
    }

    /// Runs `handler` for the command `id`, in place of any built-in
    /// handling. This is how the find bar, switcher, palette and the rest
    /// are wired to their commands.
    pub fn on_command(
        &mut self,
        id: impl Into<String>,
        handler: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        self.extra_commands.insert(id.into(), Rc::new(handler));
    }

    /// Moves keyboard focus to the active tab's note or launcher.
    pub fn focus_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane.read(cx);
        if let Some(tab) = pane.active_tab() {
            let handle = tab.focus_handle(cx);
            window.focus(&handle);
        }
    }

    /// An absolute path for `path`, taken as vault-relative when relative.
    pub fn resolve(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.vault.join(path)
        }
    }

    /// The notes and tags editors suggest from.
    pub fn vault_index(&self) -> &Entity<VaultIndex> {
        &self.vault_index
    }

    /// Reads every note's name and tags on a background thread, once the
    /// window's first frame is on screen: it reads every note.
    fn scan_vault_index(&mut self, cx: &mut Context<Self>) {
        if crate::first_frame::is_waiting() {
            let this = cx.weak_entity();
            crate::first_frame::defer(move |cx| {
                this.update(cx, |workspace, cx| workspace.scan_vault_index(cx))
                    .ok();
            });
            return;
        }
        let root = self.vault.clone();
        let index = self.vault_index.clone();
        let scan = cx.background_spawn(async move { VaultIndex::scan(&root) });
        // Code block grammars take a moment to load; have them ready
        // before the first code block is drawn.
        cx.background_spawn(async {
            crate::preview::code_highlight::load_syntaxes();
        })
        .detach();
        let task = cx.spawn(async move |_, cx| {
            let scanned = scan.await;
            index
                .update(cx, |index, cx| {
                    *index = scanned;
                    cx.notify();
                })
                .ok();
        });
        self.tasks.push(task);
    }

    /// Brings the index up to date with files changed on disk, reading
    /// them on a background thread.
    pub(crate) fn update_vault_index(
        &mut self,
        changed: Vec<PathBuf>,
        removed: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let root = self.vault.clone();
        let index = self.vault_index.clone();
        let read = cx.background_spawn(async move { index_changes(&root, &changed, &removed) });
        cx.spawn(async move |_, cx| {
            let changes = read.await;
            index
                .update(cx, |index, cx| {
                    index.apply(changes);
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }
}
