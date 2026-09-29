//! Small drawings the tour explains things with: sheets of paper with
//! lines of text on them, a folder, a laptop, a phone, a repository and a
//! Keychain prompt. They're drawn from the theme's own surfaces and fills,
//! so they look like the app in both themes.

use gpui::{Div, Hsla, Pixels, div, prelude::*};

use crate::icons::{IconName, icon};
use crate::theme::UiTheme;

/// A line of text as a bar, `width` long.
pub fn text_bar(width: Pixels, ui: &UiTheme) -> Div {
    bar(width, ui.tour.sea_line * 0.8, ui.fill_strong)
}

/// A heading as a thicker, darker bar.
pub fn heading_bar(width: Pixels, ui: &UiTheme) -> Div {
    bar(width, ui.tour.sea_line * 1.4, ui.text_faint)
}

pub fn bar(width: Pixels, height: Pixels, color: Hsla) -> Div {
    div()
        .flex_none()
        .w(width)
        .h(height)
        .rounded_full()
        .bg(color)
}

/// A sheet of paper: the note surface, raised off what's under it.
pub fn paper(ui: &UiTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(ui.menu_radius)
        .bg(ui.note_background)
        .shadow(ui.menu_shadows())
}

/// A note on a sheet: a heading and lines of `width` in the proportions
/// of `lines`.
pub fn note(width: Pixels, lines: &[f32], ui: &UiTheme) -> Div {
    let inner = width - ui.space_lg * 2.;
    paper(ui)
        .w(width)
        .p(ui.space_lg)
        .gap(ui.space_md)
        .child(heading_bar(inner * 0.55, ui))
        .children(lines.iter().map(|share| text_bar(inner * *share, ui)))
}

/// A row of a list in a picker, with an icon and a line of text.
pub fn list_row(width: Pixels, share: f32, lit: bool, ui: &UiTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(ui.space_md)
        .w(width)
        .h(ui.tour.sea_line * 4.)
        .px(ui.space_md)
        .rounded(ui.menu_row_radius)
        .when(lit, |row| row.bg(ui.fill_strong))
        .child(
            icon(IconName::FileText)
                .size(ui.small_icon_size)
                .text_color(ui.icon),
        )
        .child(text_bar((width - ui.space_md * 4.) * share, ui))
}

/// A folder holding notes, drawn as its tab and body with sheets
/// standing up out of it.
pub fn folder(width: Pixels, ui: &UiTheme) -> Div {
    let height = width * 0.62;
    let sheet = width * 0.34;
    let sheets = [(-0.26, 0.9), (0., 0.7), (0.26, 0.85)].map(|(shift, rise)| {
        div()
            .absolute()
            .left(width / 2. - sheet / 2. + width * shift)
            .bottom(height * 0.35)
            .child(note(sheet, &[0.9, 0.7, 0.8][..(rise * 3.) as usize], ui).h(sheet * 1.2 * rise))
    });
    div()
        .relative()
        .w(width)
        .h(height + width * 0.3)
        .child(
            div()
                .absolute()
                .left(width * 0.06)
                .bottom(height - ui.menu_radius)
                .w(width * 0.34)
                .h(width * 0.08)
                .rounded_t(ui.menu_radius)
                .bg(ui.fill_strong),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .bottom_0()
                .w(width)
                .h(height)
                .rounded(ui.dialog_radius)
                .bg(ui.fill_strong),
        )
        .children(sheets)
        .child(
            div()
                .absolute()
                .left_0()
                .bottom_0()
                .w(width)
                .h(height * 0.62)
                .rounded(ui.dialog_radius)
                .bg(ui.app_background)
                .shadow(ui.menu_shadows())
                .flex()
                .items_center()
                .justify_center()
                .child(icon(IconName::Vault).size(width * 0.16).text_color(ui.icon)),
        )
}

/// A laptop: a screen with a note on it, over a keyboard deck.
pub fn laptop(width: Pixels, ui: &UiTheme) -> Div {
    let screen = width * 0.82;
    div()
        .flex()
        .flex_col()
        .items_center()
        .w(width)
        .child(
            div()
                .w(screen)
                .h(screen * 0.64)
                .p(ui.space_sm)
                .rounded_t(ui.dialog_radius)
                .bg(ui.text_faint)
                .child(screen_note(screen, ui).size_full()),
        )
        .child(
            div()
                .w(width)
                .h(width * 0.05)
                .rounded_b(ui.dialog_radius)
                .bg(ui.text_faint),
        )
}

/// A phone: a tall screen with a note on it.
pub fn phone(width: Pixels, ui: &UiTheme) -> Div {
    div()
        .w(width)
        .h(width * 2.)
        .p(ui.space_sm)
        .rounded(width * 0.2)
        .bg(ui.text_faint)
        .child(screen_note(width, ui).size_full().rounded(width * 0.16))
}

/// What a screen `width` wide shows: a note's first lines.
fn screen_note(width: Pixels, ui: &UiTheme) -> Div {
    let inner = width * 0.6;
    div()
        .flex()
        .flex_col()
        .gap(ui.space_sm)
        .p(ui.space_md)
        .rounded(ui.menu_radius)
        .bg(ui.note_background)
        .overflow_hidden()
        .child(heading_bar(inner * 0.6, ui))
        .child(text_bar(inner, ui))
        .child(text_bar(inner * 0.7, ui))
}

/// A repository: a stack of versions, the newest on top.
pub fn repository(width: Pixels, ui: &UiTheme) -> Div {
    let layer = |shift: f32| {
        div()
            .absolute()
            .left(width * shift)
            .top(width * shift)
            .child(note(width * 0.8, &[0.8, 0.6], ui))
    };
    div()
        .relative()
        .w(width)
        .h(width * 0.95)
        .child(layer(0.))
        .child(layer(0.08))
        .child(layer(0.16))
}

/// The Keychain's question, with `Always Allow` ringed in the caret's red
/// as the button to press.
pub fn keychain_prompt(width: Pixels, ui: &UiTheme) -> Div {
    let button = |label: &'static str, ringed: bool| {
        div()
            .flex_none()
            .px(ui.space_md)
            .h(ui.button_height * 0.8)
            .flex()
            .items_center()
            .rounded(ui.menu_radius)
            .text_size(ui.small_font_size)
            .bg(crate::theme::over(ui.fill_strong, ui.note_background))
            .when(ringed, |button| {
                button.shadow(vec![
                    ui.ring(ui.caret_mark),
                    ui.ring(ui.caret_mark.opacity(0.3)),
                ])
            })
            .child(label)
    };
    paper(ui)
        .w(width)
        .p(ui.space_lg)
        .gap(ui.space_md)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(ui.space_md)
                .child(
                    icon(IconName::Key)
                        .size(ui.icon_size * 1.5)
                        .text_color(ui.icon),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(ui.space_sm)
                        .child(heading_bar(width * 0.5, ui))
                        .child(text_bar(width * 0.62, ui)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap(ui.space_sm)
                .child(button("Deny", false))
                .child(button("Allow", false))
                .child(button("Always Allow", true)),
        )
}
