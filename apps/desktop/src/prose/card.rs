//! The card a flag shows when the pointer rests on it: what's wrong in a
//! sentence, each fix as a button that applies it, and Ignore, which
//! stops flagging the phrase anywhere.

use std::ops::Range;

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
    let labels: Vec<String> = flag
        .replacements
        .iter()
        .map(|fix| fix_label(fix))
        .chain([IGNORE.to_owned()])
        .collect();
    let button_widths: Vec<Pixels> = labels
        .iter()
        .map(|label| text_width(label, theme, window) + theme.button_padding_x * 2.)
        .collect();
    let message = text_width(&flag.message, theme, window);
    let layout = card_layout(message, &button_widths, theme);
    let view = cx.entity().downgrade();
    let mut buttons: Vec<Button> = flag
        .replacements
        .iter()
        .enumerate()
        .map(|(index, fix)| {
            let (view, flag, fix) = (view.clone(), flag.clone(), fix.clone());
            Button::new(("flag-fix", index), fix_label(&fix)).on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.accept_flag(&flag, &fix, cx))
                    .ok();
            })
        })
        .collect();
    buttons.push({
        let (view, flag) = (view.clone(), flag.clone());
        Button::new("flag-ignore", IGNORE)
            .quiet()
            .on_click(move |_, _, cx| {
                view.update(cx, |view, cx| view.ignore_flag(&flag, cx)).ok();
            })
    });
    // Rows are split here rather than by wrapping: a popover measures a
    // wrapping row narrower than it draws it, and leaves the difference
    // blank. Ignore ends the last row, at its far end.
    let mut buttons = buttons.into_iter();
    let rows = layout.rows.iter().map(|row| {
        let mut row_buttons: Vec<Button> = buttons.by_ref().take(row.len()).collect();
        let last = row.end == labels.len();
        let ignore = last.then(|| row_buttons.pop()).flatten();
        div()
            .flex()
            .items_center()
            .gap(theme.space_sm)
            .children(row_buttons)
            .when_some(ignore, |row, ignore| {
                row.child(div().flex_1()).child(ignore)
            })
    });
    let body = popover(theme)
        .p(padding)
        .gap(theme.space_md)
        .w(layout.width + padding * 2.)
        .child(div().w(layout.width).child(flag.message.clone()))
        .child(div().flex().flex_col().gap(theme.space_sm).children(rows));
    let rows = layout.rows.len() as f32;
    let height = padding * 2.
        + theme.text_line_height * layout.message_lines as f32
        + theme.space_md
        + theme.button_height * rows
        + theme.space_sm * (rows - 1.);
    (body, height)
}

/// What Ignore's button says.
const IGNORE: &str = "Ignore";

/// How a card lays out: its content width, how many lines the message
/// takes there, and which buttons share each row.
#[derive(Debug, PartialEq)]
struct CardLayout {
    width: Pixels,
    message_lines: usize,
    rows: Vec<Range<usize>>,
}

/// Fits the card to its message, wrapped at the card's reading width,
/// and to its buttons on one row, up to the card's widest; buttons past
/// that go onto more rows.
fn card_layout(message: Pixels, buttons: &[Pixels], theme: &UiTheme) -> CardLayout {
    let gap = theme.space_sm;
    let row = buttons.iter().fold(Pixels::ZERO, |sum, width| sum + *width)
        + gap * buttons.len().saturating_sub(1) as f32;
    let width = message
        .min(theme.flag_card_width)
        .max(row.min(theme.flag_card_max_width))
        .max(theme.flag_card_min_width);
    CardLayout {
        width,
        message_lines: (message / width).ceil().max(1.) as usize,
        rows: split_rows(buttons, gap, width),
    }
}

/// Which of `buttons` share each row at `width`, placed in order. A
/// button wider than the row still gets one of its own.
fn split_rows(buttons: &[Pixels], gap: Pixels, width: Pixels) -> Vec<Range<usize>> {
    let mut rows = Vec::new();
    let (mut start, mut used) = (0, Pixels::ZERO);
    for (index, button) in buttons.iter().enumerate() {
        let next = if index > start {
            used + gap + *button
        } else {
            *button
        };
        if next > width && index > start {
            rows.push(start..index);
            (start, used) = (index, *button);
        } else {
            used = next;
        }
    }
    rows.push(start..buttons.len());
    rows
}

/// How wide `text` is in the card's font, unwrapped. Measured here
/// because a popover lays out at its content's natural width, where
/// text doesn't know to wrap.
fn text_width(text: &str, theme: &UiTheme, window: &Window) -> Pixels {
    let run = TextRun {
        len: text.len(),
        font: gpui::font(theme.font_family.clone()),
        color: theme.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(text.to_owned().into(), theme.font_size, &[run], None)
        .width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_widens_for_its_buttons_and_splits_them_past_its_widest() {
        let theme = UiTheme::default();
        let px = gpui::px;
        let message = px(300.);
        // Three fixes and Ignore fit one row by widening past the
        // message's reading width.
        let buttons = [px(95.), px(90.), px(95.), px(60.)];
        let one_row = card_layout(message, &buttons, &theme);
        assert!(one_row.width > theme.flag_card_width.min(message));
        assert_eq!(one_row.rows, vec![Range { start: 0, end: 4 }]);
        // Far more than fit: the card stops at its widest and splits.
        let many = card_layout(message, &[px(100.); 12], &theme);
        assert_eq!(many.width, theme.flag_card_max_width);
        assert!(many.rows.len() > 1);
        assert_eq!(many.rows.last().unwrap().end, 12);
    }

    #[test]
    fn rows_fill_in_order() {
        let px = gpui::px;
        assert_eq!(
            split_rows(&[px(40.), px(40.), px(40.)], px(10.), px(100.)),
            [0..2, 2..3]
        );
        assert_eq!(
            split_rows(&[px(40.), px(40.)], px(10.), px(90.)),
            vec![Range { start: 0, end: 2 }]
        );
        assert_eq!(
            split_rows(&[px(200.)], px(10.), px(100.)),
            vec![Range { start: 0, end: 1 }]
        );
    }

    #[test]
    fn fixes_without_visible_text_say_what_they_do() {
        assert_eq!(fix_label("the"), "the");
        assert_eq!(fix_label(" "), "One space");
        assert_eq!(fix_label(""), "Remove");
    }
}
