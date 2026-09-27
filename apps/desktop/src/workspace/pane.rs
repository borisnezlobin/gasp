//! A pane: a row of tabs, and under it the note surface with the note's
//! header bar, an optional toolbar (the find bar goes there) and the
//! active tab's note or launcher. The tab bar is drawn in `tab_bar.rs` and
//! the header in `note_header.rs`.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{
    AnyElement, AnyView, App, Context, Div, Entity, EventEmitter, FocusHandle, Focusable,
    MouseButton, MouseDownEvent, Pixels, ScrollHandle, ScrollWheelEvent, SharedString,
    Subscription, Window, canvas, div, prelude::*, px,
};

use super::files::note_title;
use super::history::NavHistory;
use super::launcher::Launcher;
use super::note_doc::{Conflict, NoteDoc};
use crate::editor::EditorView;
use crate::keymap::{KEY_CONTEXT, RunCommand};
use crate::text_input::TextInput;
use crate::theme::Theme;
use crate::ui::{HasMenuSlot, MenuAnchor, MenuItem, MenuSlot, ui_theme};

/// What an empty tab is called.
pub const NEW_TAB_TITLE: &str = "New tab";

/// Tells whether an editor shows its note as a finished page, with the
/// Markdown symbols hidden. The reading-view button shows this state.
pub type ReadingProbe = Rc<dyn Fn(&EditorView) -> bool>;

/// A tab showing a note.
#[derive(Clone)]
pub struct NoteTab {
    pub doc: Entity<NoteDoc>,
    pub editor: Entity<EditorView>,
    pub title: Entity<TextInput>,
}

/// What a tab holds.
#[derive(Clone)]
pub enum TabContent {
    Note(NoteTab),
    Launcher(Entity<Launcher>),
}

/// One tab. Dropping it drops its editor and subscriptions.
pub struct Tab {
    pub content: TabContent,
    _subscriptions: Vec<Subscription>,
}

impl Tab {
    pub fn new(content: TabContent, subscriptions: Vec<Subscription>) -> Tab {
        Tab {
            content,
            _subscriptions: subscriptions,
        }
    }

    pub fn note(&self) -> Option<&NoteTab> {
        match &self.content {
            TabContent::Note(note) => Some(note),
            TabContent::Launcher(_) => None,
        }
    }

    pub fn path<'a>(&self, cx: &'a App) -> Option<&'a Path> {
        self.note().map(|note| note.doc.read(cx).path())
    }

    pub fn title(&self, cx: &App) -> String {
        self.path(cx)
            .map_or_else(|| NEW_TAB_TITLE.to_owned(), note_title)
    }

    /// Where keyboard focus goes when the tab is shown.
    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.content {
            TabContent::Note(note) => note.editor.focus_handle(cx),
            TabContent::Launcher(launcher) => launcher.focus_handle(cx),
        }
    }
}

/// The menus a pane's controls open. The workspace fills them in, since
/// it knows which commands can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneMenu {
    /// The `⌄` list of every tab.
    TabList,
    /// The `⋯` menu in the note header.
    More,
    /// A right-click on the note.
    Editor,
}

/// What the pane's own controls ask the workspace to do.
#[derive(Clone, Debug, PartialEq)]
pub enum PaneEvent {
    ActivateTab(usize),
    CloseTab(usize),
    NewTab,
    /// Run a command in this pane.
    Run(SharedString),
    /// Show this folder in the file tree.
    Reveal(PathBuf),
    OpenMenu(PaneMenu, MenuAnchor),
}

pub struct Pane {
    pub(crate) focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    active: usize,
    pub(crate) history: NavHistory,
    toolbar: Option<AnyView>,
    pub(super) tab_scroll: ScrollHandle,
    /// The tab strip's width when the active tab was last scrolled into
    /// view: a pane that narrows, as when it's split, shows it again.
    pub(super) revealed_width: std::cell::Cell<Pixels>,
    pub(super) theme: Theme,
    pub(super) vault: PathBuf,
    show_inline_title: bool,
    /// Whether this is the focused pane of several, which is marked.
    pub(crate) marked_focused: bool,
    /// Whether the workspace has other panes.
    pub(crate) in_split: bool,
    /// Whether the tab bar starts with the sidebar button, because the
    /// sidebar that has its own is hidden.
    pub show_sidebar_toggle: bool,
    /// Room at the tab bar's left for the window's own buttons, when this
    /// pane is at the window's top-left.
    pub corner_inset: Pixels,
    pub(crate) reading_probe: Option<ReadingProbe>,
    pub(crate) menu: MenuSlot,
}

impl EventEmitter<PaneEvent> for Pane {}

impl Focusable for Pane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl HasMenuSlot for Pane {
    fn menu_slot(&mut self) -> &mut MenuSlot {
        &mut self.menu
    }
}

impl Pane {
    /// An empty pane on `vault`'s notes.
    pub fn new(vault: &Path, show_inline_title: bool, cx: &mut Context<Self>) -> Self {
        Pane {
            focus_handle: cx.focus_handle(),
            tabs: Vec::new(),
            active: 0,
            history: NavHistory::default(),
            toolbar: None,
            tab_scroll: ScrollHandle::new(),
            revealed_width: std::cell::Cell::new(px(0.)),
            theme: Theme::default(),
            vault: vault.to_path_buf(),
            show_inline_title,
            marked_focused: false,
            in_split: false,
            show_sidebar_toggle: false,
            corner_inset: px(0.),
            reading_probe: None,
            menu: MenuSlot::default(),
        }
    }

    /// Shows a view above the note, such as the find bar, or removes it.
    pub fn set_toolbar(&mut self, toolbar: Option<AnyView>, cx: &mut Context<Self>) {
        self.toolbar = toolbar;
        cx.notify();
    }

    pub fn toolbar(&self) -> Option<&AnyView> {
        self.toolbar.as_ref()
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_editor(&self) -> Option<Entity<EditorView>> {
        self.active_tab()?.note().map(|note| note.editor.clone())
    }

    /// The tab showing `path`, if any.
    pub fn index_of_path(&self, path: &Path, cx: &App) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.path(cx) == Some(path))
    }

    pub fn can_go_back(&self) -> bool {
        self.history.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.history.can_go_forward()
    }

    /// The open menu, if any.
    pub fn open_menu(&self) -> Option<Entity<crate::ui::DropdownMenu>> {
        self.menu.menu()
    }

    /// Opens a menu of `items` from one of the pane's controls.
    pub fn show_menu(
        &mut self,
        items: Vec<MenuItem>,
        anchor: MenuAnchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu.open(items, anchor, window, cx);
    }

    /// Adds a tab after the active one and shows it.
    pub fn add_tab(&mut self, tab: Tab, cx: &mut Context<Self>) -> usize {
        let index = if self.tabs.is_empty() {
            0
        } else {
            self.active + 1
        };
        self.tabs.insert(index, tab);
        self.activate(index, cx);
        index
    }

    /// Puts `tab` in place of the tab at `index`, returning the old one.
    pub fn replace_tab(&mut self, index: usize, tab: Tab, cx: &mut Context<Self>) -> Option<Tab> {
        let slot = self.tabs.get_mut(index)?;
        let old = std::mem::replace(slot, tab);
        cx.notify();
        Some(old)
    }

    pub fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active = index;
            self.tab_scroll.scroll_to_item(index);
            cx.notify();
        }
    }

    /// Removes a tab. The one to its right, or else its left, becomes
    /// active when it was the active tab.
    pub fn remove_tab(&mut self, index: usize, cx: &mut Context<Self>) -> Option<Tab> {
        if index >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(index);
        if index < self.active || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        cx.notify();
        Some(tab)
    }

    pub fn set_show_inline_title(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_inline_title = show;
        cx.notify();
    }

    /// Whether the active note shows as a finished page.
    pub fn is_reading(&self, cx: &App) -> bool {
        match (&self.reading_probe, self.active_editor()) {
            (Some(probe), Some(editor)) => probe(editor.read(cx)),
            _ => false,
        }
    }

    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(tab) = self.active_tab() else {
            return div().flex_1().into_any_element();
        };
        match &tab.content {
            TabContent::Launcher(launcher) => div()
                .flex_1()
                .min_h_0()
                .child(launcher.clone())
                .into_any_element(),
            TabContent::Note(note) => self.render_note(note, cx),
        }
    }

    /// The inline title. Tab or Down moves into the note.
    fn render_title(&self, note: &NoteTab, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = note.editor.clone();
        div()
            .key_context(KEY_CONTEXT)
            .w_full()
            .px(self.theme.text_padding)
            .pt(self.theme.workspace.space_xxl)
            .on_action(cx.listener(move |_, action: &RunCommand, window, cx| {
                match action.id.as_ref() {
                    "edit.indent" | "cursor.down" => window.focus(&editor.focus_handle(cx)),
                    _ => cx.propagate(),
                }
            }))
            .child(note.title.clone())
    }

    /// The title and the note in a column of readable width, centred.
    /// Wheel turns beside the column scroll the note too.
    fn render_note(&self, note: &NoteTab, cx: &mut Context<Self>) -> AnyElement {
        // The column follows the editor's own readable width, which zoom
        // scales and `view.toggle-readable-width` turns off.
        let column_width = {
            let view = note.editor.read(cx);
            let theme = view.theme();
            view.is_readable_width()
                .then(|| theme.editor_max_width + theme.text_padding * 2.)
        };
        let editor = note.editor.clone();
        let gutter = |id: &'static str| {
            let editor = editor.clone();
            div()
                .id(id)
                .debug_selector(|| id.to_owned())
                .flex_1()
                .min_w_0()
                .h_full()
                .on_scroll_wheel(move |event: &ScrollWheelEvent, _, cx| {
                    let theme = &editor.read(cx).theme;
                    let line = theme.line_height(theme.body_font_size);
                    let delta = event.delta.pixel_delta(line);
                    editor.update(cx, |editor, cx| editor.scroll_by(-delta.y, cx));
                })
        };
        let column = div()
            .flex()
            .flex_col()
            .flex_shrink()
            .map(|column| match column_width {
                Some(width) => column.w(width),
                None => column.flex_1(),
            })
            .min_w_0()
            .h_full()
            .child(self.render_note_body(note, cx));
        div()
            .id("pane-note")
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_note_right_click))
            .child(gutter("pane-gutter-left"))
            .child(column)
            .child(gutter("pane-gutter-right"))
            .into_any_element()
    }

    /// The editor, with the inline title drawn in the room the editor
    /// leaves above its first line, so the two scroll together.
    fn render_note_body(&self, note: &NoteTab, cx: &mut Context<Self>) -> Div {
        let body = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(note.editor.clone());
        if !self.show_inline_title {
            note.editor
                .update(cx, |editor, cx| editor.set_header_height(px(0.), cx));
            return body;
        }
        let (scroll, title_size) = {
            let editor = note.editor.read(cx);
            (editor.scroll_offset(), editor.theme().title_font_size)
        };
        note.title
            .update(cx, |title, cx| title.set_title_font_size(title_size, cx));
        let editor = note.editor.clone();
        let measure = canvas(
            move |bounds, _, cx| {
                editor.update(cx, |editor, cx| {
                    editor.set_header_height(bounds.size.height, cx)
                })
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        body.child(
            div()
                .id("note-title-header")
                .absolute()
                .left_0()
                .right_0()
                .top(-scroll)
                .child(self.render_title(note, cx))
                .child(measure),
        )
    }

    /// A right-click on the note: the editor takes focus, keeping its
    /// selection, and its context menu opens.
    fn on_note_right_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.active_editor() {
            window.focus(&editor.focus_handle(cx));
        }
        cx.stop_propagation();
        cx.emit(PaneEvent::OpenMenu(
            PaneMenu::Editor,
            MenuAnchor::Pointer(event.position),
        ));
    }
}

/// A tab's visible state.
pub(super) struct TabState {
    pub title: String,
    pub dirty: bool,
    pub conflict: Option<Conflict>,
}

impl TabState {
    pub fn of(tab: &Tab, cx: &App) -> TabState {
        let doc = tab.note().map(|note| note.doc.read(cx));
        TabState {
            title: tab.title(cx),
            dirty: doc.is_some_and(NoteDoc::is_dirty),
            conflict: doc.and_then(NoteDoc::conflict),
        }
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        // The toolbar (the find bar) floats over the note's top right
        // corner, under the header, so opening it never moves the text.
        let toolbar = self.toolbar.clone().map(|toolbar| {
            div()
                .absolute()
                .top(ui.note_header_height)
                .left(ui.space_md)
                .right(ui.space_md)
                .flex()
                .justify_end()
                .child(toolbar)
        });
        let surface = div()
            .id("pane-surface")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(self.theme.background)
            .rounded(ui.surface_radius)
            .shadow(ui.surface_shadows())
            .child(self.render_note_header(cx))
            .child(self.render_content(cx))
            .children(toolbar);
        div()
            .id("pane")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(self.render_tab_bar(cx))
            .child(surface)
            .children(self.menu.render_overlay(window, cx))
    }
}
