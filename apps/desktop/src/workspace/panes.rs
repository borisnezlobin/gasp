//! Splitting, closing and focusing panes, and dragging their dividers.

use gpui::{AppContext, Context, Entity, Pixels, Point, Window};

use super::pane::{Pane, PaneEvent};
use super::pane_tree::{Axis, Direction, SplitId};
use super::{Drag, DragStart, Workspace};

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
        match event {
            PaneEvent::ActivateTab(index) => {
                self.activate_pane(pane, window, cx);
                self.activate_tab(*index, window, cx);
            }
            PaneEvent::CloseTab(index) => self.close_tab(pane, *index, window, cx),
            PaneEvent::NewTab => {
                self.activate_pane(pane, window, cx);
                self.new_tab(window, cx);
            }
            PaneEvent::Run(id) => self.run_in_pane(pane, id, window, cx),
            PaneEvent::Reveal(folder) => self.reveal_in_tree(folder, false, window, cx),
            PaneEvent::OpenMenu(kind, anchor) => {
                self.open_pane_menu(pane, *kind, anchor.clone(), window, cx)
            }
            PaneEvent::Drop { item, target } => self.drop_on_pane(item, pane, *target, window, cx),
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
                if pane.marked_focused != marked || pane.in_split != several {
                    pane.marked_focused = marked;
                    pane.in_split = several;
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
        let pane = self.new_pane(window, cx);
        let active = self.active_pane.clone();
        self.panes.split(&active, pane.clone(), axis);
        let path = duplicate.then(|| self.active_path(cx)).flatten();
        match path {
            Some(path) => {
                if let Err(error) = self.show_path_in_pane(&pane, &path, false, window, cx) {
                    crate::notices::open_failed(&path, error, cx);
                }
            }
            None if duplicate => self.add_launcher_tab(&pane, window, cx),
            None => self.set_active_pane(pane.clone(), cx),
        }
        cx.notify();
        pane
    }

    /// A pane with no tabs yet, which the caller puts in the tree.
    pub(crate) fn new_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Pane> {
        let show_title = self.config.settings.editor.show_inline_title;
        let vault = self.vault.clone();
        let probe = self.reading_probe.clone();
        let pane = cx.new(|cx| {
            let mut pane = Pane::new(&vault, show_title, cx);
            pane.reading_probe = probe;
            pane
        });
        self.subscribe_to_pane(&pane, window, cx);
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
        self.drag_start = self.size_before(drag);
        cx.notify();
    }

    /// The size a resize changes, as it is before the resize starts.
    fn size_before(&self, drag: Drag) -> Option<DragStart> {
        Some(match drag {
            Drag::Divider(id) => DragStart::Ratio(id, self.panes.split_by_id(id)?.ratio),
            Drag::Sidebar => DragStart::LeftWidth(self.left_panel.width),
            Drag::RightSidebar => DragStart::RightWidth(self.right_panel.width),
        })
    }

    /// Escape during a drag: a resize goes back to where it started, and a
    /// tab or note being dragged is put down where it came from.
    pub(crate) fn cancel_drags(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let dragged = cx.stop_active_drag(window);
        if dragged {
            self.clear_tab_drops(cx);
        }
        let resizing = self.drag.is_some();
        match self.drag_start.take() {
            Some(DragStart::Ratio(id, ratio)) => self.panes.set_ratio(id, ratio),
            Some(DragStart::LeftWidth(width)) => self.left_panel.width = width,
            Some(DragStart::RightWidth(width)) => self.right_panel.width = width,
            None => {}
        }
        self.end_drag(window, cx);
        dragged || resizing
    }

    pub(crate) fn end_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drag_start = None;
        if self.drag.take().is_none() {
            return;
        }
        // A resize kept the panel in use; the usual hide delay starts now
        // if the pointer ended outside it.
        self.refresh_panel_use(window, cx);
        cx.notify();
    }

    /// Follows the pointer while a divider or the sidebar edge is dragged.
    pub(crate) fn drag_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        match self.drag {
            Some(Drag::Divider(id)) => self.drag_divider(id, position, cx),
            Some(Drag::Sidebar) => self.drag_sidebar(position),
            Some(Drag::RightSidebar) => self.drag_right_panel(position.x),
            None => return,
        }
        cx.notify();
    }

    /// Moves a divider under the pointer, stopping where either side
    /// would get smaller than a pane can usefully be.
    fn drag_divider(&mut self, id: SplitId, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(split) = self.panes.split_by_id(id) else {
            return;
        };
        let ui = crate::ui::ui_theme(cx);
        let bounds = split.bounds.get();
        let (offset, length, min) = match split.axis {
            Axis::Row => (
                position.x - bounds.left(),
                bounds.size.width,
                ui.pane_min_width,
            ),
            Axis::Column => (
                position.y - bounds.top(),
                bounds.size.height,
                ui.pane_min_height,
            ),
        };
        let length = length.max(gpui::px(1.));
        // When the split is too small for two minimum panes, it stays even.
        let floor = (min / length).min(0.5);
        let ratio = (offset / length).clamp(floor, 1. - floor);
        self.panes.set_ratio(id, ratio);
    }

    /// A double-click on a divider shares its space evenly.
    pub(crate) fn equalize_split(&mut self, id: SplitId, cx: &mut Context<Self>) {
        self.drag = None;
        self.panes.equalize(id);
        cx.notify();
    }

    fn drag_sidebar(&mut self, position: Point<Pixels>) {
        let theme = &self.theme.workspace;
        self.left_panel.width = position
            .x
            .clamp(theme.sidebar_min_width, theme.sidebar_max_width);
    }
}
