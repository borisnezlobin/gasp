//! A square button with one Phosphor icon, a hover fill, a pressed fill,
//! an "on" state and a disabled state, and a tooltip naming what it does
//! and its shortcut.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, MouseButton, SharedString, Window, div, prelude::*,
};

use super::tooltip::Tooltip;
use super::ui_theme;
use crate::icons::{IconName, icon};
use crate::keymap::RunCommand;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// An icon button. With [`IconButton::command`] it runs that command and
/// its tooltip shows the command's title and shortcut.
#[derive(IntoElement)]
pub struct IconButton {
    id: SharedString,
    icon: IconName,
    tooltip: Option<Tooltip>,
    command: Option<SharedString>,
    active: bool,
    disabled: bool,
    small: bool,
    on_click: Option<ClickHandler>,
    attached: Option<AnyElement>,
}

impl IconButton {
    /// `id` is unique in its window; tests find the button by it.
    pub fn new(id: impl Into<SharedString>, icon: IconName) -> IconButton {
        IconButton {
            id: id.into(),
            icon,
            tooltip: None,
            command: None,
            active: false,
            disabled: false,
            small: false,
            on_click: None,
            attached: None,
        }
    }

    /// Runs command `id` on click, unless [`IconButton::on_click`] says
    /// otherwise, and shows its title and shortcut in the tooltip.
    pub fn command(mut self, id: &str, cx: &App) -> IconButton {
        self.tooltip = Some(Tooltip::for_command(id, cx));
        self.command = Some(id.to_owned().into());
        self
    }

    pub fn tooltip(mut self, label: impl Into<SharedString>) -> IconButton {
        self.tooltip = Some(Tooltip::new(label, None));
        self
    }

    /// A tooltip label in place of the command's title, keeping its shortcut.
    pub fn label(mut self, label: impl Into<SharedString>) -> IconButton {
        let shortcut = self.tooltip.take().and_then(|tooltip| tooltip.shortcut);
        self.tooltip = Some(Tooltip::new(label, shortcut));
        self
    }

    /// Shown as on, such as the sidebar view that's showing.
    pub fn active(mut self, active: bool) -> IconButton {
        self.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> IconButton {
        self.disabled = disabled;
        self
    }

    /// The smaller icon size, for dense rows such as tabs.
    pub fn small(mut self) -> IconButton {
        self.small = true;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> IconButton {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Something drawn in the button's box, such as the menu it opened.
    pub fn attach(mut self, element: Option<AnyElement>) -> IconButton {
        self.attached = element;
        self
    }

    fn click_handler(&self) -> Option<ClickHandler> {
        if self.disabled {
            return None;
        }
        if let Some(handler) = &self.on_click {
            return Some(handler.clone());
        }
        let id = self.command.clone()?;
        Some(Rc::new(move |_, window, cx| {
            window.dispatch_action(Box::new(RunCommand { id: id.clone() }), cx)
        }))
    }
}

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = ui_theme(cx);
        let handler = self.click_handler();
        let color = match (self.disabled, self.active) {
            (true, _) => theme.icon_disabled,
            (false, true) => theme.icon_active,
            (false, false) => theme.icon,
        };
        let icon_size = if self.small {
            theme.small_icon_size
        } else {
            theme.icon_size
        };
        let size = if self.small {
            theme.small_icon_size + theme.space_md
        } else {
            theme.icon_button_size
        };
        let selector = self.id.to_string();
        // No tooltip over the menu the button opened.
        let has_attached = self.attached.is_some();
        div()
            .id(ElementId::Name(self.id.clone()))
            .debug_selector(|| selector)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(size)
            .rounded(theme.icon_button_radius)
            .when(self.active, |button| button.bg(theme.control_active))
            .when(!self.disabled, |button| {
                button
                    .hover(|style| style.bg(theme.control_hover))
                    .active(|style| style.bg(theme.control_pressed))
            })
            .when_some(self.tooltip.filter(|_| !has_attached), |button, tooltip| {
                button.tooltip(tooltip.builder())
            })
            .when_some(handler, |button, handler| {
                button
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        handler(event, window, cx)
                    })
            })
            .child(icon(self.icon).size(icon_size).text_color(color))
            .children(self.attached)
    }
}
