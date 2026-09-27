//! The card a flag shows when the pointer rests on it: what's wrong in a
//! sentence, each fix as a button that applies it, and Ignore, which
//! stops flagging the phrase anywhere.

use editor_prose::Flag;
use gpui::{Context, Div, Pixels, TextRun, Window, div, prelude::*};

use crate::editor::EditorView;
use crate::theme::UiTheme;
use crate::ui::{Button, popover};

/// What a fix's button says. A fix that only changes spaces or removes
/// text has nothing visible to show, so it says what it does.
pub fn fix_label(replacement: &str) -> String {
    if replacement.is_empty() {
        "Remove".to_owned()
    } else if replacement.trim().is_empty() {
        "One space".to_owned()
    } else {
        replacement.to_owned()
    }
}

/// The card's body and its height, for placing it above or below.
pub(crate) fn flag_card(
    flag: &Flag,
    theme: &UiTheme,
    window: &Window,
    cx: &mut Context<EditorView>,
) -> (Div, Pixels) {
    let padding = theme.popover_padding;
    let (width, lines) = message_size(&flag.message, theme, window);
    let view = cx.entity().downgrade();
    let fixes = flag.replacements.iter().enumerate().map(|(index, fix)| {
        let (view, flag, fix) = (view.clone(), flag.clone(), fix.clone());
        Button::new(("flag-fix", index), fix_label(&fix)).on_click(move |_, _, cx| {
            view.update(cx, |view, cx| view.accept_flag(&flag, &fix, cx))
                .ok();
        })
    });
    let ignore = {
        let (view, flag) = (view.clone(), flag.clone());
        Button::new("flag-ignore", "Ignore")
            .quiet()
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.ignore_flag(&flag, cx)).ok();
            })
    };
    let buttons = div()
        .flex()
        .items_center()
        .gap(theme.space_sm)
        .children(fixes)
        .child(div().flex_1())
        .child(ignore);
    let body = popover(theme)
        .p(padding)
        .gap(theme.space_md)
        .w(width + padding * 2.)
        .child(div().w(width).child(flag.message.clone()))
        .child(buttons);
    let height =
        padding * 2. + theme.text_line_height * lines as f32 + theme.space_md + theme.button_height;
    (body, height)
}

/// How wide the message needs to be, up to the card's width, and how
/// many lines it takes there. Measured here because a popover lays out
/// at its content's natural width, where text doesn't know to wrap.
fn message_size(message: &str, theme: &UiTheme, window: &Window) -> (Pixels, usize) {
    let run = TextRun {
        len: message.len(),
        font: gpui::font(theme.font_family.clone()),
        color: theme.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let natural = window
        .text_system()
        .shape_line(message.to_owned().into(), theme.font_size, &[run], None)
        .width;
    let widest = theme.flag_card_width;
    let lines = (natural / widest).ceil().max(1.) as usize;
    (natural.min(widest).max(theme.flag_card_min_width), lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_without_visible_text_say_what_they_do() {
        assert_eq!(fix_label("the"), "the");
        assert_eq!(fix_label(" "), "One space");
        assert_eq!(fix_label(""), "Remove");
    }
}
