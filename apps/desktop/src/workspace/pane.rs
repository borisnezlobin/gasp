//! A pane: a row of tabs, an optional toolbar (the find bar goes there)
//! and the active tab's note or launcher.

use std::path::Path;

use gpui::{
    AnyView, App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, ScrollHandle,
    SharedString, Subscription, Window, div, prelude::*,
};

use super::files::note_title;
use super::history::NavHistory;
use super::launcher::Launcher;
use super::note_doc::{Conflict, NoteDoc};
use super::title_input::TitleInput;
use crate::editor::EditorView;
use crate::icons::{IconName, icon};
use crate::theme::{Theme, WorkspaceTheme};

/// What an empty tab is called.
pub const NEW_TAB_TITLE: &str = "New tab";

/// A tab showing a note.
#[derive(Clone)]
pub struct NoteTab {
    pub doc: Entity<NoteDoc>,
    pub editor: Entity<EditorView>,
    pub title: Entity<TitleInput>,
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

/// What the pane's own controls ask the workspace to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneEvent {
    ActivateTab(usize),
    CloseTab(usize),
    NewTab,
}

pub struct Pane {
    pub(crate) focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    active: usize,
    pub(crate) history: NavHistory,
    toolbar: Option<AnyView>,
    tab_scroll: ScrollHandle,
    theme: Theme,
    show_inline_title: bool,
    /// Whether this is the focused pane of several, which is marked.
    pub(crate) marked_focused: bool,
}

impl EventEmitter<PaneEvent> for Pane {}

impl Focusable for Pane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Pane {
    pub fn new(show_inline_title: bool, cx: &mut Context<Self>) -> Self {
        Pane {
            focus_handle: cx.focus_handle(),
            tabs: Vec::new(),
            active: 0,
            history: NavHistory::default(),
            toolbar: None,
            tab_scroll: ScrollHandle::new(),
            theme: Theme::default(),
            show_inline_title,
            marked_focused: false,
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

    fn render_tab(&self, index: usize, tab: &Tab, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme.workspace;
        let active = index == self.active;
        let group: SharedString = format!("tab-{index}").into();
        let state = TabState::of(tab, cx);
        div()
            .id(("tab", index))
            .group(group.clone())
            .relative()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(theme.space_sm)
            .h_full()
            .min_w(theme.tab_min_width)
            .max_w(theme.tab_max_width)
            .pl(theme.space_lg)
            .pr(theme.space_sm)
            .border_r(theme.divider_width)
            .border_color(theme.divider)
            .text_color(if active { theme.text } else { theme.text_muted })
            .when(active, |tab| tab.bg(theme.active_tab_background))
            .when(!active, |tab| {
                tab.hover(|style| style.bg(theme.hover_background))
            })
            .when(active && self.marked_focused, |tab| {
                tab.child(focus_line(theme))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| cx.emit(PaneEvent::ActivateTab(index))),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |_, _, _, cx| cx.emit(PaneEvent::CloseTab(index))),
            )
            .when(state.conflict.is_some(), |tab| {
                tab.child(
                    icon(IconName::WarningCircle)
                        .flex_none()
                        .size(theme.small_icon_size)
                        .text_color(theme.conflict),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(SharedString::from(state.title)),
            )
            .child(self.render_tab_end(index, group, state.dirty, cx))
    }

    /// The close button, which shows on hover, over the unsaved dot.
    fn render_tab_end(
        &self,
        index: usize,
        group: SharedString,
        dirty: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme.workspace;
        let slot = theme.icon_size + theme.space_sm;
        div()
            .relative()
            .flex_none()
            .size(slot)
            .when(dirty, |end| {
                end.child(
                    div()
                        .absolute()
                        .top((slot - theme.dirty_dot_size) / 2.)
                        .left((slot - theme.dirty_dot_size) / 2.)
                        .size(theme.dirty_dot_size)
                        .rounded_full()
                        .bg(theme.text_muted)
                        .group_hover(group.clone(), |style| style.invisible()),
                )
            })
            .child(
                div()
                    .id(("close-tab", index))
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radius_sm)
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .hover(|style| style.bg(theme.hover_background))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(PaneEvent::CloseTab(index))))
                    .child(
                        icon(IconName::X)
                            .size(theme.small_icon_size)
                            .text_color(theme.text_muted),
                    ),
            )
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.workspace.clone();
        let tabs: Vec<_> = (0..self.tabs.len())
            .map(|index| {
                self.render_tab(index, &self.tabs[index], cx)
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_row()
            .flex_none()
            .h(theme.tab_height)
            .bg(theme.chrome_background)
            .border_b(theme.divider_width)
            .border_color(theme.divider)
            .child(
                div()
                    .id("tab-strip")
                    .flex()
                    .flex_row()
                    .min_w_0()
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_scroll)
                    .children(tabs),
            )
            .child(
                div()
                    .id("new-tab")
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .w(theme.tab_height)
                    .h_full()
                    .hover(|style| style.bg(theme.hover_background))
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(PaneEvent::NewTab)))
                    .child(
                        icon(IconName::Plus)
                            .size(theme.small_icon_size)
                            .text_color(theme.text_muted),
                    ),
            )
    }

    fn render_content(&self) -> gpui::AnyElement {
        let Some(tab) = self.active_tab() else {
            return div().flex_1().into_any_element();
        };
        match &tab.content {
            TabContent::Launcher(launcher) => div()
                .flex_1()
                .min_h_0()
                .child(launcher.clone())
                .into_any_element(),
            TabContent::Note(note) => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .bg(self.theme.background)
                .when(self.show_inline_title, |content| {
                    content.child(note.title.clone())
                })
                .child(div().flex_1().min_h_0().child(note.editor.clone()))
                .into_any_element(),
        }
    }
}

/// A tab's visible state.
struct TabState {
    title: String,
    dirty: bool,
    conflict: Option<Conflict>,
}

impl TabState {
    fn of(tab: &Tab, cx: &App) -> TabState {
        let doc = tab.note().map(|note| note.doc.read(cx));
        TabState {
            title: tab.title(cx),
            dirty: doc.is_some_and(NoteDoc::is_dirty),
            conflict: doc.and_then(NoteDoc::conflict),
        }
    }
}

fn focus_line(theme: &WorkspaceTheme) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(theme.focus_line_width)
        .bg(theme.accent)
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("pane")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .font_family(self.theme.body_font_family.clone())
            .text_size(self.theme.workspace.ui_font_size)
            .child(self.render_tab_bar(cx))
            .when_some(self.toolbar.clone(), |pane, toolbar| {
                pane.child(div().flex_none().child(toolbar))
            })
            .child(self.render_content())
    }
}
