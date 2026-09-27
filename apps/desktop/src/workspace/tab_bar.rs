//! A pane's tab bar: the sidebar button when the sidebar is hidden, the
//! tabs, a new-tab button and the list of every tab.

use gpui::{
    AnyElement, Context, MouseButton, SharedString, canvas, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};

use super::pane::{Pane, PaneEvent, PaneMenu, Tab, TabState};
use crate::icons::{IconName, icon};
use crate::theme::UiTheme;
use crate::ui::{IconButton, MenuAnchor, Tooltip, ui_theme};

/// The tab-list button, which its menu hangs under.
pub const TAB_LIST_KEY: &str = "pane-tab-list";

impl Pane {
    pub(super) fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let tabs: Vec<AnyElement> = self
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| self.render_tab(index, tab, &ui, cx))
            .collect();
        let toggle =
            self.show_sidebar_toggle.then(|| {
                IconButton::new("pane-sidebar-toggle", IconName::SidebarSimple)
                    .command("sidebar.files.toggle", cx)
                    .label("Show file sidebar")
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(PaneEvent::Run("sidebar.files.toggle".into()))
                    }))
            });
        let list_menu = self.menu.render_attached(TAB_LIST_KEY, ui.space_xs);
        div()
            .id("tab-bar")
            .debug_selector(|| "tab-bar".to_owned())
            .on_drag_move(cx.listener(Self::on_drag_over_tabs))
            .on_drop(cx.listener(Self::on_drop_on_tabs))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(ui.space_xs)
            .h(ui.tab_bar_height)
            .pl(self.corner_inset)
            .children(toggle)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        div()
                            .id("tab-strip")
                            .flex()
                            .flex_row()
                            .size_full()
                            .items_center()
                            .gap(ui.tab_gap)
                            .px(ui.space_xs)
                            .overflow_x_scroll()
                            .track_scroll(&self.tab_scroll)
                            .children(tabs),
                    )
                    .children(self.render_strip_fades(&ui))
                    .child(self.reveal_on_resize()),
            )
            .child(
                IconButton::new("pane-new-tab", IconName::Plus)
                    .command("tab.new", cx)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(PaneEvent::NewTab))),
            )
            .child(
                IconButton::new(TAB_LIST_KEY, IconName::CaretDown)
                    .tooltip("Show all tabs")
                    .active(list_menu.is_some())
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(PaneEvent::OpenMenu(
                            PaneMenu::TabList,
                            MenuAnchor::Below {
                                key: TAB_LIST_KEY.into(),
                                align_right: true,
                            },
                        ))
                    }))
                    .attach(list_menu),
            )
    }

    /// Scrolls the active tab back into view when the strip's width
    /// changes, as when the pane is split or the window narrows. The width
    /// is only known once the strip is laid out, so this asks for one more
    /// frame to show the scroll.
    fn reveal_on_resize(&self) -> impl IntoElement {
        let scroll = self.tab_scroll.clone();
        let revealed = self.revealed_width.clone();
        let active = self.active_index();
        canvas(
            move |bounds, window, _| {
                if bounds.size.width != revealed.get() {
                    revealed.set(bounds.size.width);
                    scroll.scroll_to_item(active);
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// Soft edges where tabs run past the strip, so a tab cut by the edge
    /// reads as "more this way" rather than as a broken tab.
    fn render_strip_fades(&self, ui: &UiTheme) -> Vec<AnyElement> {
        let offset = self.tab_scroll.offset().x;
        let overflow = self.tab_scroll.max_offset().width;
        let fade = |from_left: bool| {
            let (angle, solid, clear) = if from_left {
                (90., 0., 1.)
            } else {
                (90., 1., 0.)
            };
            let edge =
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .w(ui.tab_fade_width)
                    .bg(linear_gradient(
                        angle,
                        linear_color_stop(ui.app_background, solid),
                        linear_color_stop(ui.app_background.opacity(0.), clear),
                    ));
            if from_left {
                edge.left_0().into_any_element()
            } else {
                edge.right_0().into_any_element()
            }
        };
        let mut fades = Vec::new();
        if offset < px(-0.5) {
            fades.push(fade(true));
        }
        if overflow > px(0.5) && offset > -overflow + px(0.5) {
            fades.push(fade(false));
        }
        fades
    }

    fn render_tab(
        &self,
        index: usize,
        tab: &Tab,
        ui: &UiTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = index == self.active_index();
        let raised = active && (self.marked_focused || !self.in_split);
        let group: SharedString = format!("tab-{index}").into();
        let state = TabState::of(tab, cx);
        let title: SharedString = state.title.clone().into();
        let dragged = self.dragged_tab(index, title.clone(), state.dirty, cx);
        let slot = self.shown_slot(cx);
        let last = index + 1 == self.len();
        div()
            .id(("tab", index))
            .debug_selector(|| format!("tab-{}", state.title))
            .group(group.clone())
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap(ui.space_sm)
            .flex_1()
            .flex_basis(ui.tab_max_width)
            .min_w(ui.tab_min_width)
            .max_w(ui.tab_max_width)
            .h(ui.tab_height)
            .pl(ui.tab_padding_x)
            .pr(ui.space_sm)
            .rounded(ui.tab_radius)
            .text_color(if active { ui.text } else { ui.text_muted })
            .when(active, |tab| tab.bg(ui.note_background))
            .when(raised, |tab| tab.shadow(ui.tab_shadows()))
            .when(!active, |tab| tab.hover(|style| style.bg(ui.control_hover)))
            // The name in full, except under the tab's own menu.
            .when(!self.menu.is_open(), |tab| {
                tab.tooltip(Tooltip::new(title.clone(), None).builder())
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| cx.emit(PaneEvent::ActivateTab(index))),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |_, _, _, cx| cx.emit(PaneEvent::CloseTab(index))),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(Self::on_tab_right_click(index)),
            )
            .on_drag(dragged, |dragged, grab, _, cx| {
                Self::start_tab_drag(dragged, grab, cx)
            })
            .when(self.is_dragging(index, cx), |tab| {
                tab.opacity(ui.dragged_tab_opacity)
            })
            .when(slot == Some(index), |tab| {
                tab.child(drop_bar(ui).left(-(ui.tab_gap + ui.drop_indicator_width) / 2.))
            })
            .when(last && slot == Some(index + 1), |tab| {
                tab.child(drop_bar(ui).right(-(ui.tab_gap + ui.drop_indicator_width) / 2.))
            })
            .when(state.conflict.is_some(), |tab| {
                tab.child(
                    icon(IconName::WarningCircle)
                        .flex_none()
                        .size(ui.small_icon_size)
                        .text_color(ui.conflict),
                )
            })
            .child(crate::ui::truncated(title.clone()).grow())
            .child(self.render_tab_end(index, group, active, state.dirty, ui, cx))
            .into_any_element()
    }

    /// The close button over the unsaved dot. The active tab always shows
    /// its close button; other tabs show it on hover. An unsaved note
    /// shows its dot until hovered.
    fn render_tab_end(
        &self,
        index: usize,
        group: SharedString,
        active: bool,
        dirty: bool,
        ui: &UiTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let slot = ui.small_icon_size + ui.space_md;
        let hide_until_hover = !active || dirty;
        div()
            .relative()
            .flex_none()
            .size(slot)
            .when(dirty, |end| {
                end.child(
                    div()
                        .absolute()
                        .top((slot - ui.dirty_dot_size) / 2.)
                        .left((slot - ui.dirty_dot_size) / 2.)
                        .size(ui.dirty_dot_size)
                        .rounded_full()
                        .bg(ui.text_muted)
                        .group_hover(group.clone(), |style| style.invisible()),
                )
            })
            .child(
                div()
                    .id(("close-tab", index))
                    .debug_selector(move || format!("close-tab-{index}"))
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(ui.icon_button_radius)
                    .when(hide_until_hover, |close| {
                        close
                            .invisible()
                            .group_hover(group, |style| style.visible())
                    })
                    .hover(|style| style.bg(ui.control_hover))
                    .active(|style| style.bg(ui.control_pressed))
                    .tooltip(Tooltip::for_command("tab.close", cx).builder())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(PaneEvent::CloseTab(index))))
                    .child(
                        icon(IconName::X)
                            .size(ui.small_icon_size)
                            .text_color(ui.icon),
                    ),
            )
    }
}

/// The bar between two tabs where a dragged tab would go.
fn drop_bar(ui: &UiTheme) -> gpui::Div {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(ui.drop_indicator_width)
        .rounded_full()
        .bg(ui.drop_indicator)
}
