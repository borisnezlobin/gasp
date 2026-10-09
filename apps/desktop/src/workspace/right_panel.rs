//! The right panel slot. It hosts one view (the knowledge sidebar goes
//! here: backlinks, outgoing links, outline and tags), sits beside the
//! notes, can be resized from its left edge, and remembers whether it was
//! open, which view it showed and its width in `device.toml`.

use gasp_config::device::RightSidebarState;
use gpui::{
    AnyElement, AnyView, Context, CursorStyle, FocusHandle, MouseButton, MouseDownEvent, Pixels,
    Window, div, prelude::*, px,
};

use super::{Drag, Workspace};
use crate::ui::{Selectable, ui_theme};

/// The right panel's state.
pub struct RightPanel {
    view: Option<AnyView>,
    /// Where the hosted view takes the keyboard, so the workspace knows
    /// when it has it.
    focus: Option<FocusHandle>,
    pub width: Pixels,
    visible: bool,
    /// Which of the hosted view's views shows, remembered for next time.
    pub view_key: String,
    /// The pointer's x and the width when a resize started.
    drag_origin: Option<(Pixels, Pixels)>,
}

impl RightPanel {
    pub fn new(state: &RightSidebarState, default_width: Pixels) -> RightPanel {
        RightPanel {
            view: None,
            focus: None,
            width: state.width.map_or(default_width, |width| px(width as f32)),
            visible: state.open,
            view_key: state.view.clone(),
            drag_origin: None,
        }
    }

    pub fn view(&self) -> Option<&AnyView> {
        self.view.as_ref()
    }

    /// Whether it shows: open, with something in it.
    pub fn is_visible(&self) -> bool {
        self.visible && self.view.is_some()
    }

    /// What `device.toml` keeps of it.
    pub fn state(&self, default_width: Pixels) -> RightSidebarState {
        let width = (self.width != default_width).then(|| f32::from(self.width).round() as u32);
        RightSidebarState {
            open: self.visible,
            view: self.view_key.clone(),
            width,
        }
    }
}

impl Workspace {
    /// Hosts `view` in the right panel.
    pub fn set_right_panel(&mut self, view: AnyView, cx: &mut Context<Self>) {
        self.right_panel.view = Some(view);
        cx.notify();
    }

    /// Tells the workspace where the right panel's view takes the keyboard.
    pub fn set_right_panel_focus(&mut self, focus: FocusHandle) {
        self.right_panel.focus = Some(focus);
    }

    /// Whether the right panel's view has the keyboard.
    pub fn right_panel_has_focus(&self, window: &gpui::Window, cx: &gpui::App) -> bool {
        self.right_panel
            .focus
            .as_ref()
            .is_some_and(|focus| focus.contains_focused(window, cx))
    }

    pub fn right_panel(&self) -> &RightPanel {
        &self.right_panel
    }

    /// Opens or closes the right panel.
    pub fn set_right_panel_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.right_panel.visible != visible {
            self.right_panel.visible = visible;
            cx.notify();
        }
    }

    /// Keeps `subscription` for as long as the workspace lives.
    pub fn keep_subscription(&mut self, subscription: gpui::Subscription) {
        self._subscriptions.push(subscription);
    }

    /// Remembers which view the right panel shows.
    pub fn set_right_panel_view_key(&mut self, key: &str) {
        key.clone_into(&mut self.right_panel.view_key);
    }

    /// Puts the right sidebar's show button at the end of the top-right
    /// pane's tab bar while the sidebar is hidden, and room for the
    /// window's buttons when the app draws them and no sidebar is there.
    pub(super) fn sync_right_sidebar_toggle(&mut self, window: &Window, cx: &mut Context<Self>) {
        let hidden = self.right_panel.view.is_some() && !self.right_panel.visible;
        let controls = if self.right_panel.is_visible() {
            px(0.)
        } else {
            crate::window_controls::width(window, cx)
        };
        let corner = self
            .panes
            .rects()
            .into_iter()
            .filter(|(_, rect)| rect.y < 0.001)
            .max_by(|(_, a), (_, b)| a.x.total_cmp(&b.x))
            .map(|(pane, _)| pane);
        for pane in self.panes.panes() {
            let at_corner = Some(&pane) == corner.as_ref();
            let show = hidden && at_corner;
            let inset = if at_corner { controls } else { px(0.) };
            pane.update(cx, |pane, cx| {
                if pane.show_right_sidebar_toggle != show || pane.controls_inset != inset {
                    pane.show_right_sidebar_toggle = show;
                    pane.controls_inset = inset;
                    cx.notify();
                }
            });
        }
    }

    pub(super) fn render_right_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.right_panel.is_visible() {
            return None;
        }
        let view = self.right_panel.view.clone()?;
        let ui = ui_theme(cx);
        let grab = self.theme.workspace.divider_grab_width;
        let group: gpui::SharedString = "right-panel-edge".into();
        let dragging = self.drag == Some(Drag::RightSidebar);
        let edge = div()
            .id("right-panel-edge")
            .selector(|| "right-panel-edge".to_owned())
            .group(group.clone())
            .child(super::render::resize_line(group, dragging, grab, &ui))
            .absolute()
            .top_0()
            .bottom_0()
            .left(-(grab / 2.))
            .w(grab)
            .cursor(CursorStyle::ResizeLeftRight)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|workspace, event: &MouseDownEvent, _, cx| {
                    let origin = (event.position.x, workspace.right_panel.width);
                    workspace.right_panel.drag_origin = Some(origin);
                    workspace.start_drag(Drag::RightSidebar, cx);
                }),
            );
        Some(
            div()
                .id("right-panel")
                .relative()
                .flex()
                .flex_col()
                .flex_none()
                .h_full()
                .w(self.right_panel.width)
                .bg(ui.app_background)
                .child(view)
                .child(edge)
                .into_any_element(),
        )
    }

    /// Follows the pointer while the right panel's edge is dragged.
    pub(super) fn drag_right_panel(&mut self, x: Pixels) {
        let Some((start_x, start_width)) = self.right_panel.drag_origin else {
            return;
        };
        let theme = &self.theme.workspace;
        self.right_panel.width =
            (start_width + (start_x - x)).clamp(theme.sidebar_min_width, theme.sidebar_max_width);
    }
}
