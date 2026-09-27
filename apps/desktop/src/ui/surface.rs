//! The surfaces everything floats on, so menus, dialogs, the find bar and
//! tooltips share one radius, shadow and ring: [`popover`] for menus and
//! small floating panels, [`dialog`] for what opens over the workspace.

use gpui::{Div, div, prelude::*};

use crate::theme::UiTheme;

/// A menu's surface: white, rounded, a soft shadow and a hairline ring,
/// in the UI font.
pub fn popover(ui: &UiTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .p(ui.menu_padding)
        .rounded(ui.menu_radius)
        .bg(ui.menu_background)
        .shadow(ui.menu_shadows())
        .font_family(ui.font_family.clone())
        .text_size(ui.font_size)
        .text_color(ui.text)
}

/// A dialog's surface: like [`popover`], with a larger radius and a
/// higher shadow. It sets no width; each dialog picks one of the
/// `*dialog_width` tokens.
pub fn dialog(ui: &UiTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .max_w_full()
        .rounded(ui.dialog_radius)
        .bg(ui.menu_background)
        .shadow(ui.dialog_shadows())
        .font_family(ui.font_family.clone())
        .text_size(ui.font_size)
        .text_color(ui.text)
}
