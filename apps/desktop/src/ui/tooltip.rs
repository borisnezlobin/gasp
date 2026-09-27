//! A small dark label under a control: what it does, and its shortcut.

use gpui::{AnyView, App, AppContext, Context, SharedString, Window, div, prelude::*};

use super::ui_theme;

/// A tooltip's text. Build one for GPUI's `.tooltip(...)` with
/// [`Tooltip::builder`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tooltip {
    pub label: SharedString,
    pub shortcut: Option<SharedString>,
}

impl Tooltip {
    pub fn new(label: impl Into<SharedString>, shortcut: Option<SharedString>) -> Tooltip {
        Tooltip {
            label: label.into(),
            shortcut,
        }
    }

    /// For a command: its title and current shortcut.
    pub fn for_command(id: &str, cx: &App) -> Tooltip {
        Tooltip::new(
            super::hints::command_title(id),
            super::hints::shortcut(id, cx),
        )
    }

    /// The text as one line, as a screen reader or a test reads it.
    pub fn text(&self) -> String {
        match &self.shortcut {
            Some(shortcut) => format!("{} {shortcut}", self.label),
            None => self.label.to_string(),
        }
    }

    /// A closure for GPUI's `.tooltip(...)`.
    pub fn builder(self) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        move |_, cx| cx.new(|_| self.clone()).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui_theme(cx);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_md)
            .px(theme.tooltip_padding_x)
            .py(theme.tooltip_padding_y)
            .rounded(theme.tooltip_radius)
            .bg(theme.tooltip_background)
            .shadow(theme.menu_shadows())
            .font_family(theme.font_family.clone())
            .text_size(theme.small_font_size)
            .text_color(theme.tooltip_text)
            .whitespace_nowrap()
            .child(self.label.clone())
            .children(
                self.shortcut
                    .clone()
                    .map(|shortcut| div().text_color(theme.tooltip_hint).child(shortcut)),
            )
    }
}
