//! The inline suggestion popover: a short list under the text being
//! typed, such as note names after `[[` or tags after `#`. It looks like a
//! menu, but it never takes focus: the owner keeps the keyboard and moves
//! the highlight itself, so typing carries on filtering the list.

use gpui::{
    AnyElement, Context, Div, ElementId, HighlightStyle, MouseButton, SharedString, Stateful,
    Window, div, prelude::*,
};

use super::menu::menu_row;
use super::{Truncated, popover, truncated};
use crate::picker::match_ranges;
use crate::theme::UiTheme;

/// One row: a label with its matched characters, and an optional detail
/// on the right, such as the folder a note is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionRow {
    pub label: SharedString,
    /// Byte offsets of matched characters in `label`.
    pub positions: Vec<usize>,
    pub detail: Option<SharedString>,
    /// Nesting depth, such as a heading's level below the top one.
    pub indent: usize,
}

/// What a row does when the pointer acts on it.
pub type RowHandler<V> = fn(&mut V, usize, &mut Window, &mut Context<V>);

/// The rows `first..first + theme.suggestion_rows` of a suggestion list,
/// with row `highlighted` marked. Clicking a row calls `on_choose` and
/// hovering one calls `on_hover`, both with the row's index in `rows`.
pub fn suggestion_list<V: 'static>(
    rows: &[SuggestionRow],
    first: usize,
    highlighted: usize,
    theme: &UiTheme,
    cx: &mut Context<V>,
    on_choose: RowHandler<V>,
    on_hover: RowHandler<V>,
) -> Stateful<Div> {
    let end = rows.len().min(first + theme.suggestion_rows);
    let rendered: Vec<AnyElement> = (first..end)
        .map(|index| {
            let row = &rows[index];
            let selector = format!("suggestion-{}", row.label);
            menu_row(
                ElementId::NamedInteger("suggestion".into(), index as u64),
                index == highlighted,
                false,
                theme,
            )
            .debug_selector(|| selector)
            .gap(theme.space_lg)
            .pl(theme.menu_row_padding_x + theme.space_lg * row.indent as f32)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _, window, cx| {
                    cx.stop_propagation();
                    on_choose(view, index, window, cx);
                }),
            )
            .on_hover(cx.listener(move |view, hovered: &bool, window, cx| {
                if *hovered {
                    on_hover(view, index, window, cx);
                }
            }))
            .child(label_text(row, theme).grow())
            .children(row.detail.clone().map(|detail| {
                div()
                    .flex_shrink()
                    .min_w_0()
                    .max_w(theme.menu_min_width / 2.)
                    .text_size(theme.small_font_size)
                    .text_color(theme.text_faint)
                    .child(truncated(detail))
            }))
            .into_any_element()
        })
        .collect();
    popover(theme)
        .id("suggestions")
        .occlude()
        .min_w(theme.menu_min_width)
        .max_w(theme.menu_max_width)
        .children(rendered)
}

/// The label, its matched characters in the match weight.
fn label_text(row: &SuggestionRow, theme: &UiTheme) -> Truncated {
    let style = HighlightStyle {
        font_weight: Some(theme.match_weight),
        ..HighlightStyle::default()
    };
    let highlights = match_ranges(&row.label, &row.positions)
        .into_iter()
        .map(|range| (range, style));
    truncated(row.label.clone()).with_highlights(highlights)
}

/// The first visible row that keeps `highlighted` on screen, moving the
/// window as little as possible from `first`.
pub fn scroll_to_show(first: usize, highlighted: usize, visible: usize) -> usize {
    if highlighted < first {
        highlighted
    } else if highlighted >= first + visible {
        highlighted + 1 - visible
    } else {
        first
    }
}

/// The popover's height for `rows` rows, for deciding whether it fits
/// below the line.
pub fn list_height(rows: usize, theme: &UiTheme) -> gpui::Pixels {
    theme.menu_row_height * rows.min(theme.suggestion_rows) as f32 + theme.menu_padding * 2.
}

#[cfg(test)]
mod tests {
    use super::scroll_to_show;

    #[test]
    fn the_window_follows_the_highlight() {
        assert_eq!(scroll_to_show(0, 3, 8), 0);
        assert_eq!(scroll_to_show(0, 8, 8), 1);
        assert_eq!(scroll_to_show(5, 2, 8), 2);
        assert_eq!(scroll_to_show(5, 12, 8), 5);
    }
}
