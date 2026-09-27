//! Splitting, closing and focusing panes, and dragging their dividers.

use gpui::{AppContext, Context, Entity, Pixels, Point, Window};

use super::pane::{Pane, PaneEvent};
use super::pane_tree::{Axis, Direction, SplitId};
use super::{Drag, Workspace};

impl Workspace {
    pub(crate) fn subscribe_to_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = pane.read(cx).focus_handle.clone();
        let focused_pane = pane.clone();
        let subscriptions = vec![
            cx.subscribe_in(pane, window, Self::on_pane_event),
            cx.on_focus_in(&focus, window, move |workspace, window, cx| {
                workspace.on_pane_focused(&focused_pane, window, cx)
            }),
        ];
        self.pane_subscriptions
            .insert(pane.entity_id(), subscriptions);
    }

    fn on_pane_event(
        &mut self,
        pane: &Entity<Pane>,
        event: &PaneEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match *event {
            PaneEvent::ActivateTab(index) => {
                self.activate_pane(pane, window, cx);
                self.activate_tab(index, window, cx);
            }
            PaneEvent::CloseTab(index) => self.close_tab(pane, index, window, cx),
            PaneEvent::NewTab => {
                self.activate_pane(pane, window, cx);
                self.new_tab(window, cx);
            }
        }
    }

    /// Focus landed in a pane, by click or by command: it becomes active.
    /// Focus on the pane itself moves on to its note.
    fn on_pane_focused(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *pane != self.active_pane {
            self.set_active_pane(pane.clone(), cx);
        }
        if pane.read(cx).focus_handle.is_focused(window) {
            self.focus_active(window, cx);
        }
    }

    fn set_active_pane(&mut self, pane: Entity<Pane>, cx: &mut Context<Self>) {
        self.active_pane = pane;
        self.mark_focused_pane(cx);
        self.refresh_status(cx);
        cx.notify();
    }

    /// Marks the active pane when there's more than one.
    fn mark_focused_pane(&mut self, cx: &mut Context<Self>) {
        let several = self.panes.len() > 1;
        for pane in self.panes.panes() {
            let marked = several && pane == self.active_pane;
            pane.update(cx, |pane, cx| {
                if pane.marked_focused != marked {
                    pane.marked_focused = marked;
                    cx.notify();
                }
            });
        }
    }

    /// Makes `pane` active and focuses its tab.
    pub(crate) fn activate_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_pane(pane.clone(), cx);
        self.focus_active(window, cx);
    }

    /// Adds an empty pane beside the active one. With `duplicate`, it
    /// shows the active note too.
    pub(crate) fn split_pane(
        &mut self,
        axis: Axis,
        duplicate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Pane> {
        let show_title = self.config.settings.editor.show_inline_title;
        let pane = cx.new(|cx| Pane::new(show_title, cx));
        let active = self.active_pane.clone();
        self.panes.split(&active, pane.clone(), axis);
        self.subscribe_to_pane(&pane, window, cx);
        let path = duplicate.then(|| self.active_path(cx)).flatten();
        match path {
            Some(path) => {
                if let Err(error) = self.show_path_in_pane(&pane, &path, false, window, cx) {
                    eprintln!("could not open {}: {error}", path.display());
                }
            }
            None if duplicate => self.add_launcher_tab(&pane, window, cx),
            None => self.set_active_pane(pane.clone(), cx),
        }
        cx.notify();
        pane
    }

    /// Removes a pane after its tabs have closed, and focuses a neighbour.
    pub(crate) fn remove_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let neighbor = self.panes.adjacent(pane);
        if !self.panes.remove(pane) {
            return;
        }
        self.pane_subscriptions.remove(&pane.entity_id());
        if let Some(neighbor) = neighbor.filter(|_| *pane == self.active_pane) {
            self.activate_pane(&neighbor, window, cx);
        }
        self.mark_focused_pane(cx);
        cx.notify();
    }

    /// Closes the active pane and its tabs. The last pane is emptied
    /// instead.
    pub(crate) fn close_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane.clone();
        let count = pane.read(cx).len();
        for index in (0..count).rev() {
            if !self.panes.contains(&pane) {
                break;
            }
            self.close_tab(&pane, index, window, cx);
        }
    }

    pub(crate) fn focus_pane_toward(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.panes.neighbor(&self.active_pane, direction) {
            self.activate_pane(&pane, window, cx);
        }
    }

    pub(crate) fn start_drag(&mut self, drag: Drag, cx: &mut Context<Self>) {
        self.drag = Some(drag);
        cx.notify();
    }

    pub(crate) fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.notify();
        }
    }

    /// Follows the pointer while a divider or the sidebar edge is dragged.
    pub(crate) fn drag_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        match self.drag {
            Some(Drag::Divider(id)) => self.drag_divider(id, position),
            Some(Drag::Sidebar) => self.drag_sidebar(position),
            None => return,
        }
        cx.notify();
    }

    fn drag_divider(&mut self, id: SplitId, position: Point<Pixels>) {
        let Some(split) = self.panes.split_by_id(id) else {
            return;
        };
        let bounds = split.bounds.get();
        let ratio = match split.axis {
            Axis::Row => (position.x - bounds.left()) / bounds.size.width.max(gpui::px(1.)),
            Axis::Column => (position.y - bounds.top()) / bounds.size.height.max(gpui::px(1.)),
        };
        self.panes.set_ratio(id, ratio);
    }

    fn drag_sidebar(&mut self, position: Point<Pixels>) {
        let theme = &self.theme.workspace;
        self.left_panel.width = position
            .x
            .clamp(theme.sidebar_min_width, theme.sidebar_max_width);
    }
}
