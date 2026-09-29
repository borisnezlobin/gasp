//! Drawing the workspace: the pane tree with its dividers, the left panel,
//! the status bar and the modal slot.

use gasp_config::EventKind;
use gasp_config::toolbars::Place;
use gpui::{
    AnyElement, Context, CursorStyle, DispatchPhase, Entity, MouseButton, MouseDownEvent,
    MouseExitEvent, MouseMoveEvent, MouseUpEvent, SharedString, Window, canvas, div, prelude::*,
    relative,
};

use super::pane::Pane;
use super::pane_tree::{Axis, Node, Split};
use super::sidebar::LEFT_EDGE_TARGET;
use super::{Drag, Workspace};
use crate::keymap::WORKSPACE_CONTEXT;
use crate::ui::Selectable;
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

    /// The gap between two panes' surfaces, with a handle to drag. A line
    /// shows in the gap under the pointer and while dragging, so the
    /// handle is findable; a double-click shares the space evenly.
    fn render_divider(&self, split: &Split<Entity<Pane>>, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme.workspace;
        let ui = ui_theme(cx);
        let gap = ui.surface_gap;
        let grab = theme.divider_grab_width.max(gap);
        let id = split.id;
        let dragging = self.drag == Some(Drag::Divider(id));
        let group: SharedString = format!("divider-{}", id.0).into();
        let inset = (grab - ui.divider_line_width) / 2.;
        let line = div()
            .absolute()
            .rounded_full()
            .when(dragging, |line| line.bg(ui.divider_active))
            .group_hover(group.clone(), |style| style.bg(ui.divider_active));
        let line = match split.axis {
            Axis::Row => line.top_0().bottom_0().left(inset).w(ui.divider_line_width),
            Axis::Column => line.left_0().right_0().top(inset).h(ui.divider_line_width),
        };
        let handle = div()
            .id(("divider", id.0))
            .selector(move || format!("divider-{}", id.0))
            .group(group)
            .absolute()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |workspace, event: &MouseDownEvent, _, cx| {
                    if event.click_count >= 2 {
                        workspace.equalize_split(id, cx);
                    } else {
                        workspace.start_drag(Drag::Divider(id), cx);
                    }
                }),
            )
            .child(line);
        let offset = (grab - gap) / 2.;
        let handle = match split.axis {
            Axis::Row => handle
                .top_0()
                .bottom_0()
                .left(-offset)
                .w(grab)
                .cursor(CursorStyle::ResizeLeftRight),
            Axis::Column => handle
                .left_0()
                .right_0()
                .top(-offset)
                .h(grab)
                .cursor(CursorStyle::ResizeUpDown),
        };
        let gap_box = div().relative().flex_none();
        let gap_box = match split.axis {
            Axis::Row => gap_box.w(gap).h_full(),
            Axis::Column => gap_box.h(gap).w_full(),
        };
        gap_box.child(handle).into_any_element()
    }

    fn render_left_panel(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let view = self.left_panel.view()?.clone();
        if !self.left_panel.is_visible() {
            self.left_panel.area().set(None);
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
            .child(self.render_panel_area_probe())
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
            // Covers the note, so the wheel and pointer stop here.
            panel
                .absolute()
                .top_0()
                .left_0()
                .bottom_0()
                .occlude()
                .shadow(vec![gpui::BoxShadow {
                    color: ui.overlay_shadow,
                    offset: gpui::point(theme.shadow_offset / 2., gpui::px(0.)),
                    blur_radius: theme.shadow_blur,
                    spread_radius: gpui::px(0.),
                }])
        } else {
            panel
        };
        Some(panel.into_any_element())
    }

    /// Records where the panel is drawn, with the outer half of its edge,
    /// for hover reveal to tell whether the pointer is in it.
    fn render_panel_area_probe(&self) -> impl IntoElement {
        let overhang = self.theme.workspace.divider_grab_width / 2.;
        self.left_panel
            .area()
            .probe()
            .top_0()
            .bottom_0()
            .left_0()
            .right(-overhang)
    }

    /// Follows the pointer anywhere in the window, and out of it, for the
    /// panel and bars shown on hover. It listens before anything under
    /// the pointer can stop the event, so a menu, popover or drag over
    /// them can't hide where the pointer is.
    fn render_pointer_watch(&self, cx: &mut Context<Self>) -> AnyElement {
        let workspace = cx.entity().downgrade();
        let watch = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let follow = move |workspace: &gpui::WeakEntity<Workspace>,
                                   position,
                                   window: &mut Window,
                                   cx: &mut gpui::App| {
                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.follow_pointer(position, window, cx)
                        })
                        .ok();
                };
                let moves = workspace.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        follow(&moves, event.position, window, cx);
                    }
                });
                let ups = workspace.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        follow(&ups, event.position, window, cx);
                    }
                });
                let exits = workspace.clone();
                window.on_mouse_event(move |event: &MouseExitEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        follow(&exits, event.position, window, cx);
                    }
                });
            },
        )
        .absolute()
        .size_0();
        watch.into_any_element()
    }

    /// The panel's right edge, which resizes it. The half hanging over
    /// the note counts as the panel for hover reveal.
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

    fn follow_drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button == Some(MouseButton::Left) {
            self.drag_to(event.position, cx);
        } else {
            self.end_drag(window, cx);
        }
    }

    /// While a divider or panel edge is dragged, follows the pointer
    /// wherever it goes. Listeners on the workspace only hear a pointer
    /// over it, and a sidebar over the note hides the workspace beneath,
    /// so a resize moving back into that sidebar would stall there and
    /// miss its release.
    fn render_drag_tracker(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.drag?;
        let workspace = cx.entity().downgrade();
        let tracker = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let moves = workspace.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        moves
                            .update(cx, |workspace, cx| workspace.follow_drag(event, window, cx))
                            .ok();
                    }
                });
                let ups = workspace.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        ups.update(cx, |workspace, cx| workspace.end_drag(window, cx))
                            .ok();
                    }
                });
            },
        )
        .absolute()
        .size_full();
        Some(tracker.into_any_element())
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

impl Workspace {
    /// The notes with the bars docked above and below them.
    fn render_center(
        &mut self,
        panes: AnyElement,
        (left_gap, right_gap): (gpui::Pixels, gpui::Pixels),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let top = self.docked_bars(Place::EditorTop, window, cx);
        let bottom = self.docked_bars(Place::EditorBottom, window, cx);
        let notes = div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(panes)
            .children(top.edge)
            .children(bottom.edge)
            .children(top.overlay)
            .children(bottom.overlay);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .pl(left_gap)
            .pr(right_gap)
            .children(top.strip)
            .child(notes)
            .children(bottom.strip)
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = crate::trace::span("workspace-render");
        self.update_window_title(window, cx);
        self.sync_sidebar_toggle(window, cx);
        self.sync_right_sidebar_toggle(cx);
        self.drop_stale_toolbar_focus(window, cx);
        let ui = ui_theme(cx);
        let panes = self.render_node(self.panes.root(), cx);
        let overlays = self.left_panel.overlays();
        let panel = self.render_left_panel(window, cx);
        let (pushed, overlaid) = if overlays {
            (None, panel)
        } else {
            (panel, None)
        };
        let right = self.render_right_panel(cx);
        let right_gap = if right.is_some() {
            gpui::px(0.)
        } else {
            ui.surface_gap
        };
        let left_gap = if pushed.is_some() {
            gpui::px(0.)
        } else {
            ui.surface_gap
        };
        let center = self.render_center(panes, (left_gap, right_gap), window, cx);
        let left_bars = self.docked_bars(Place::WindowLeft, window, cx);
        let right_bars = self.docked_bars(Place::WindowRight, window, cx);
        let status = self.docked_bars(Place::StatusBar, window, cx);
        div()
            .id("workspace")
            .key_context(WORKSPACE_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_run_command))
            .on_action(cx.listener(Self::on_press_toolbar_item))
            .on_action(cx.listener(Self::on_add_to_toolbar))
            .on_key_down(cx.listener(Self::on_toolbar_key))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .capture_any_mouse_down(cx.listener(|workspace, event: &MouseDownEvent, _, cx| {
                workspace.spend_shortcut_sheet(event.modifiers.modified(), cx)
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|workspace, _, window, cx| {
                    workspace.end_drag(window, cx);
                    workspace.clear_tab_drops(cx);
                }),
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
                    .children(left_bars.strip)
                    .children(pushed)
                    .child(center)
                    .children(right)
                    .children(right_bars.strip)
                    .children(overlaid)
                    .children(self.render_left_edge(cx))
                    .children(left_bars.edge)
                    .children(right_bars.edge)
                    .children(left_bars.overlay)
                    .children(right_bars.overlay),
            )
            .child(crate::ui::focus_visible::pointer_watch())
            .children(self.render_drag_tracker(cx))
            .child(self.render_pointer_watch(cx))
            .children(status.strip)
            .children(status.edge)
            .children(status.overlay)
            .children(self.menu.render_overlay(window, cx))
            .children(self.modal.render(&ui, cx))
            .children(self.render_shortcut_sheet(window, cx))
    }
}
