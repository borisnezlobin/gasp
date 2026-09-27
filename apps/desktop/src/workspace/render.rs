//! Drawing the workspace: the pane tree with its dividers, the left panel,
//! the status bar and the modal slot.

use editor_config::EventKind;
use gpui::{
    AnyElement, Context, CursorStyle, Entity, MouseButton, MouseMoveEvent, Window, canvas, div,
    prelude::*, relative,
};

use super::pane::Pane;
use super::pane_tree::{Axis, Node, Split};
use super::sidebar::{LEFT_EDGE_TARGET, PANEL_TARGET};
use super::status::render_status_bar;
use super::{Drag, Workspace};
use crate::keymap::WORKSPACE_CONTEXT;
use crate::ui::ui_theme;

impl Workspace {
    fn render_node(&self, node: &Node<Entity<Pane>>, cx: &mut Context<Self>) -> AnyElement {
        match node {
            Node::Leaf(pane) => div()
                .flex()
                .size_full()
                .min_w_0()
                .min_h_0()
                .child(pane.clone())
                .into_any_element(),
            Node::Split(split) => self.render_split(split, cx),
        }
    }

    fn render_split(&self, split: &Split<Entity<Pane>>, cx: &mut Context<Self>) -> AnyElement {
        let bounds = split.bounds.clone();
        let first = self.render_node(&split.first, cx);
        let second = self.render_node(&split.second, cx);
        let container = match split.axis {
            Axis::Row => div().flex().flex_row(),
            Axis::Column => div().flex().flex_col(),
        };
        container
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                canvas(move |area, _, _| bounds.set(area), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .min_w_0()
                    .min_h_0()
                    .flex_basis(relative(split.ratio))
                    .child(first),
            )
            .child(self.render_divider(split, cx))
            .child(div().flex().flex_1().min_w_0().min_h_0().child(second))
            .into_any_element()
    }

    /// The gap between two panes' surfaces, with a handle to drag.
    fn render_divider(&self, split: &Split<Entity<Pane>>, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme.workspace;
        let gap = ui_theme(cx).surface_gap;
        let id = split.id;
        let offset = (theme.divider_grab_width - gap) / 2.;
        let handle = div().id(("divider", id.0)).absolute().on_mouse_down(
            MouseButton::Left,
            cx.listener(move |workspace, _, _, cx| workspace.start_drag(Drag::Divider(id), cx)),
        );
        let handle = match split.axis {
            Axis::Row => handle
                .top_0()
                .bottom_0()
                .left(-offset)
                .w(theme.divider_grab_width)
                .cursor(CursorStyle::ResizeLeftRight),
            Axis::Column => handle
                .left_0()
                .right_0()
                .top(-offset)
                .h(theme.divider_grab_width)
                .cursor(CursorStyle::ResizeUpDown),
        };
        let line = div().relative().flex_none();
        let line = match split.axis {
            Axis::Row => line.w(gap).h_full(),
            Axis::Column => line.h(gap).w_full(),
        };
        line.child(handle).into_any_element()
    }

    fn render_left_panel(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let view = self.left_panel.view()?.clone();
        if !self.left_panel.is_visible() {
            return None;
        }
        let theme = &self.theme.workspace;
        let ui = ui_theme(cx);
        let overlays = self.left_panel.overlays();
        let panel = div()
            .id("left-panel")
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .h_full()
            .w(self.left_panel.width)
            .bg(ui.app_background)
            .on_hover(cx.listener(|workspace, hovered: &bool, window, cx| {
                let kind = if *hovered {
                    EventKind::PointerEnter
                } else {
                    EventKind::PointerLeave
                };
                workspace.pointer_event(kind, PANEL_TARGET, window, cx);
            }))
            .child(self.render_sidebar_header(super::window::window_buttons_inset(window, cx), cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px(ui.sidebar_padding - self.theme.workspace.space_sm)
                    .child(view),
            )
            .child(self.render_sidebar_footer(cx))
            .child(self.render_panel_edge(cx));
        let panel = if overlays {
            panel
                .absolute()
                .top_0()
                .left_0()
                .bottom_0()
                .shadow(vec![gpui::BoxShadow {
                    color: theme.shadow,
                    offset: gpui::point(theme.shadow_offset / 2., gpui::px(0.)),
                    blur_radius: theme.shadow_blur,
                    spread_radius: gpui::px(0.),
                }])
        } else {
            panel
        };
        Some(panel.into_any_element())
    }

    /// The panel's right edge, which resizes it.
    fn render_panel_edge(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme.workspace;
        div()
            .id("left-panel-edge")
            .absolute()
            .top_0()
            .bottom_0()
            .right(-(theme.divider_grab_width / 2.))
            .w(theme.divider_grab_width)
            .cursor(CursorStyle::ResizeLeftRight)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|workspace, _, _, cx| workspace.start_drag(Drag::Sidebar, cx)),
            )
    }

    /// The strip along the window's left edge that reveals the panel.
    fn render_left_edge(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.left_panel.wants_edge() {
            return None;
        }
        let edge = div()
            .id("left-edge")
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(self.theme.workspace.hover_edge_width)
            .on_hover(cx.listener(|workspace, hovered: &bool, window, cx| {
                if *hovered {
                    workspace.pointer_event(EventKind::PointerEnter, LEFT_EDGE_TARGET, window, cx);
                }
            }));
        Some(edge.into_any_element())
    }

    fn update_window_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let vault = super::files::folder_name(&self.vault);
        let title = match self.active_path(cx) {
            Some(path) => format!("{} — {vault}", super::files::note_title(&path)),
            None => vault,
        };
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.drag.is_some() {
            if event.pressed_button == Some(MouseButton::Left) {
                self.drag_to(event.position, cx);
            } else {
                self.end_drag(cx);
            }
        }
    }
}

impl Workspace {
    /// Shows the sidebar button in the top-left pane's tab bar while the
    /// sidebar, which has its own, is hidden.
    /// Puts the sidebar's show button, and room for the window buttons,
    /// in the top-left pane while the sidebar is hidden.
    fn sync_sidebar_toggle(&mut self, window: &Window, cx: &mut Context<Self>) {
        let has_panel = self.left_panel.view().is_some();
        let panel_hidden = !(has_panel && self.left_panel.is_visible());
        let first = self.panes.panes().first().cloned();
        let inset = super::window::window_buttons_inset(window, cx);
        for pane in self.panes.panes() {
            let corner = panel_hidden && Some(&pane) == first.as_ref();
            let show = corner && has_panel;
            let corner_inset = if corner { inset } else { gpui::px(0.) };
            pane.update(cx, |pane, cx| {
                if pane.show_sidebar_toggle != show || pane.corner_inset != corner_inset {
                    pane.show_sidebar_toggle = show;
                    pane.corner_inset = corner_inset;
                    cx.notify();
                }
            });
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_window_title(window, cx);
        self.sync_sidebar_toggle(window, cx);
        let theme = self.theme().clone();
        let ui = ui_theme(cx);
        let panes = self.render_node(self.panes.root(), cx);
        let overlays = self.left_panel.overlays();
        let panel = self.render_left_panel(window, cx);
        let (pushed, overlaid) = if overlays {
            (None, panel)
        } else {
            (panel, None)
        };
        let left_gap = if pushed.is_some() {
            gpui::px(0.)
        } else {
            ui.surface_gap
        };
        div()
            .id("workspace")
            .key_context(WORKSPACE_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_run_command))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|workspace, _, _, cx| workspace.end_drag(cx)),
            )
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(ui.app_background)
            .text_color(ui.text)
            .font_family(ui.font_family.clone())
            .text_size(ui.font_size)
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .children(pushed)
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .pl(left_gap)
                            .pr(ui.surface_gap)
                            .child(panes),
                    )
                    .children(overlaid)
                    .children(self.render_left_edge(cx)),
            )
            .child(render_status_bar(self.status.as_ref(), &ui))
            .children(self.menu.render_overlay(window, cx))
            .children(self.modal.render(&theme.workspace, cx))
    }
}
