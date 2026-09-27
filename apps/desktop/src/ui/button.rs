//! A text button: a primary one in the accent, a secondary one on a
//! faint fill, or a quiet one with no fill until hovered. Every dialog,
//! bar and prompt uses these.

use std::rc::Rc;

use gpui::{App, ClickEvent, ElementId, MouseButton, SharedString, Window, div, prelude::*};

use super::ui_theme;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// How much a button stands out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonKind {
    /// The one answer a dialog expects.
    Primary,
    /// Any other answer.
    #[default]
    Secondary,
    /// A control inside a bar, drawn as text until hovered.
    Quiet,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    kind: ButtonKind,
    focused: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl Button {
    /// `id` is unique in its window; tests find the button by it.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
        Button {
            id: id.into(),
            label: label.into(),
            kind: ButtonKind::default(),
            focused: false,
            disabled: false,
            on_click: None,
        }
    }

    pub fn kind(mut self, kind: ButtonKind) -> Button {
        self.kind = kind;
        self
    }

    pub fn primary(self) -> Button {
        self.kind(ButtonKind::Primary)
    }

    pub fn quiet(self) -> Button {
        self.kind(ButtonKind::Quiet)
    }

    /// Draws the focus ring, for buttons that take the keyboard as a group,
    /// such as a prompt's answers.
    pub fn focused(mut self, focused: bool) -> Button {
        self.focused = focused;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Button {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Button {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ui = ui_theme(cx);
        let (background, text, hover) = match self.kind {
            ButtonKind::Primary => (Some(ui.accent), ui.on_accent, ui.accent.opacity(0.85)),
            ButtonKind::Secondary => (
                Some(crate::theme::over(ui.button_background, ui.menu_background)),
                ui.text,
                crate::theme::over(ui.control_pressed, ui.menu_background),
            ),
            ButtonKind::Quiet => (None, ui.text, ui.control_hover),
        };
        let handler = self.on_click.filter(|_| !self.disabled);
        let label = self.label.to_string();
        div()
            .id(self.id)
            .debug_selector(move || format!("button-{label}"))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(ui.button_height)
            .px(ui.button_padding_x)
            .rounded(ui.icon_button_radius)
            .whitespace_nowrap()
            .text_color(text)
            .when_some(background, |button, color| button.bg(color))
            .when(self.focused, |button| button.shadow(vec![ui.focus()]))
            .when(self.disabled, |button| button.opacity(0.4))
            .when(!self.disabled, |button| {
                button.hover(move |style| style.bg(hover))
            })
            .when_some(handler, |button, handler| {
                button
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        handler(event, window, cx)
                    })
            })
            .child(self.label)
    }
}

#[cfg(test)]
mod tests {
    use gpui::hsla;

    use crate::theme::over;

    #[test]
    fn a_fill_over_white_is_opaque_and_as_light_as_it_looked() {
        let fill = over(hsla(0., 0., 0., 0.05), hsla(0., 0., 1., 1.));
        assert_eq!(fill.a, 1.);
        assert!((fill.l - 0.95).abs() < 0.001);
    }
}
