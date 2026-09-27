//! Drawing the settings screen: a modal with the section list on the
//! left and the current page on the right, its rows grouped on cards.

use editor_config::Platform;
use editor_config::keys::KeyChord;
use std::rc::Rc;

use gpui::{
    AnyElement, ClickEvent, Context, DismissEvent, MouseButton, MouseDownEvent, Pixels, Render,
    SharedString, Size, Window, div, list, prelude::*, relative,
};

use super::controls::{button, control_note, icon_button, two_column_row};
use super::model::{PAGES, Page, PageSpec, ShortcutQuery};
use super::view::{Card, ControlRow, ListShows, PaneLayout, SettingsFocus, SettingsView};
use crate::icons::{IconName, icon};
use crate::picker::shortcut::Shortcut;
use crate::theme::SettingsTheme;
use crate::ui::keycap;

/// The modal's size: a share of the window, up to the theme's maximums.
pub fn modal_size(window: Size<Pixels>, style: &SettingsTheme) -> Size<Pixels> {
    Size {
        width: (window.width * style.modal_fraction).min(style.modal_max_width),
        height: (window.height * style.modal_fraction).min(style.modal_max_height),
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let style = self.style.clone();
        let size = modal_size(window.viewport_size(), &style);
        div()
            .key_context("Settings")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, window, cx| view.close_menu(window, cx)),
            )
            .w(size.width)
            .h(size.height)
            .flex()
            .overflow_hidden()
            .rounded(style.modal_radius)
            .bg(style.background)
            .shadow(vec![style.outline(), style.popover_shadow()])
            .font_family(style.font_family.clone())
            .text_size(style.text_size)
            .line_height(relative(style.line_height_factor))
            .text_color(style.text)
            .child(self.render_nav(size.width, window, cx))
            .child(self.render_content(cx))
    }
}

impl SettingsView {
    fn has_focus(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.focus_handle.contains_focused(window, cx)
    }

    // ---- Section list ----

    fn render_nav(
        &mut self,
        modal_width: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let style = &self.style;
        let visible = self.visible_sections();
        let mut list = div()
            .id("settings-sections")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col();
        let mut group = "";
        for spec in PAGES.iter().filter(|spec| visible.contains(&spec.page)) {
            if spec.group != group {
                group = spec.group;
                list = list.child(self.group_label(group));
            }
            let index = visible.iter().position(|page| *page == spec.page);
            list = list.child(self.nav_item(spec, index.unwrap_or(0), window, cx));
        }
        div()
            .flex_none()
            .w(style.nav_width.min(modal_width * style.nav_fraction))
            .h_full()
            .flex()
            .flex_col()
            .gap(style.gap_sm)
            .p(style.nav_padding)
            .child(self.search_box(window, cx))
            .child(self.key_search_button(cx))
            .child(list)
    }

    /// "Search by keys": the next chord pressed becomes the search. While
    /// it waits, the search box says so and the button reads as pressed.
    fn key_search_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let style = &self.style;
        let waiting = self.searching_by_keys();
        let hover = style.hover;
        let button = div()
            .id("search-by-keys")
            .debug_selector(|| "search-by-keys".to_string())
            .flex_none()
            .h(style.nav_item_height - style.gap_sm)
            .px(style.control_gap)
            .flex()
            .items_center()
            .gap(style.gap_sm)
            .text_size(style.small_text_size)
            .rounded(style.radius)
            .cursor_pointer()
            .text_color(style.text_muted)
            .when(waiting, |button| {
                button.bg(style.selected).text_color(style.text)
            })
            .when(!waiting, |button| button.hover(move |b| b.bg(hover)))
            .child(
                icon(IconName::Keyboard)
                    .flex_none()
                    .size(style.small_icon_size)
                    .text_color(style.text_muted),
            )
            .child("Search by keys")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                if view.searching_by_keys() {
                    view.cancel_capture(cx);
                } else {
                    view.start_key_search(window, cx);
                }
            }));
        div().flex_none().flex().justify_end().child(button)
    }

    fn search_box(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let style = &self.style;
        let waiting = self.searching_by_keys();
        let focused = waiting || self.focus == SettingsFocus::Search && self.has_focus(window, cx);
        let rejection = self
            .capture
            .as_ref()
            .filter(|_| waiting)
            .and_then(|capture| capture.rejection.clone())
            .map(|message| control_note(message, style));
        let field = if waiting {
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(style.text_muted)
                .child("Press the keys to find")
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .child(self.search.clone())
                .into_any_element()
        };
        div()
            .relative()
            .flex_none()
            .h(style.control_height + style.gap_sm * 2.)
            .px(style.control_gap)
            .flex()
            .items_center()
            .gap(style.control_gap)
            .rounded(style.radius)
            .bg(style.hover)
            .when(focused, |field| field.shadow(vec![style.focus()]))
            .child(
                icon(IconName::MagnifyingGlass)
                    .flex_none()
                    .size(style.icon_size)
                    .text_color(style.text_muted),
            )
            .child(field)
            .children(rejection)
    }

    fn group_label(&self, label: &str) -> impl IntoElement {
        let style = &self.style;
        div()
            .pt(style.nav_group_gap)
            .pb(style.gap_sm)
            .px(style.control_gap)
            .text_size(style.small_text_size)
            .text_color(style.text_faint)
            .child(label.to_string())
    }

    fn nav_item(
        &self,
        spec: &PageSpec,
        index: usize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let style = &self.style;
        let selected = index == self.current;
        let list_focused = self.focus == SettingsFocus::Sections && self.has_focus(window, cx);
        let hover = style.hover;
        div()
            .id(("settings-section", index))
            .flex_none()
            .h(style.nav_item_height)
            .px(style.control_gap)
            .flex()
            .items_center()
            .gap(style.control_gap)
            .rounded(style.radius)
            .cursor_pointer()
            .when(selected, |item| item.bg(style.selected))
            .when(!selected, |item| item.hover(move |s| s.bg(hover)))
            .when(selected && list_focused, |item| {
                item.shadow(vec![style.focus()])
            })
            .child(
                icon(spec.icon)
                    .flex_none()
                    .size(style.icon_size)
                    .text_color(if selected {
                        style.text
                    } else {
                        style.text_muted
                    }),
            )
            .child(div().truncate().child(spec.title))
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                view.select_section(index, cx);
                view.set_focus(SettingsFocus::Sections, window, cx);
            }))
    }

    // ---- Page ----

    fn render_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let style = self.style.clone();
        let layout = self.layout();
        let items: Rc<[PaneItem]> = pane_items(&layout).into();
        self.sync_list(items.len());
        // Only the items in view are built each frame: the shortcuts page
        // has over a hundred rows.
        let pane = list(
            self.list.clone(),
            cx.processor(move |view, index: usize, window, cx| {
                let last = index + 1 == items.len();
                view.render_item(&layout, items[index], last, window, cx)
            }),
        )
        .size_full();
        let close = icon_button("settings-close", IconName::X, style.text_muted, &style)
            .debug_selector(|| "settings-close".to_string())
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(DismissEvent)));
        div()
            .id("settings-pane")
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .child(pane)
            .child(
                div()
                    .absolute()
                    .top(style.nav_padding)
                    .right(style.nav_padding)
                    .child(close),
            )
    }

    /// Tells the list how many items the page has now. A new page or
    /// search starts at the top; a changed row keeps the place.
    fn sync_list(&self, count: usize) {
        let now = ListShows {
            page: self.current_section(),
            query: self.query.clone(),
            count,
        };
        let mut shown = self.list_shows.borrow_mut();
        match shown.as_ref() {
            Some(old) if old.page == now.page && old.query == now.query => {
                if old.count != count {
                    let kept = old.count.min(count);
                    self.list.splice(kept..old.count, count - kept);
                }
            }
            _ => self.list.reset(count),
        }
        *shown = Some(now);
    }

    /// Draws one item of the page's list.
    fn render_item(
        &mut self,
        layout: &PaneLayout,
        item: PaneItem,
        last: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = self.style.clone();
        let element = match item {
            PaneItem::Title => self.render_title(),
            PaneItem::CardTitle(card) => card_title(&layout.cards[card], &style),
            PaneItem::Row { index, card } => self.render_card_row(index, card, layout, window, cx),
            PaneItem::Empty => self.render_empty(cx),
        };
        div()
            .px(style.content_padding_x)
            .when(item == PaneItem::Title, |item| {
                item.pt(style.content_padding_y)
            })
            .when(last, |item| item.pb(style.content_padding_y))
            .child(element)
            .into_any_element()
    }

    fn render_title(&self) -> AnyElement {
        let style = &self.style;
        let title = self
            .current_section()
            .map(|page| self.section_title(page))
            .unwrap_or_else(|| "Nothing matches".to_string());
        div()
            .pb(style.gap_sm)
            .text_size(style.page_title_size)
            .font_weight(style.strong_weight)
            .child(title)
            .into_any_element()
    }

    /// What shows when nothing matches: the search in words, or the keys
    /// searched for, which are then free to use, and a way back.
    fn render_empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let query = ShortcutQuery::new(&self.query);
        let keys = query.keys().and_then(|keys| {
            Some(Shortcut::new(
                KeyChord::new(keys.modifiers, keys.key?),
                Platform::current(),
            ))
        });
        let message = match keys {
            Some(shortcut) => div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(style.gap_sm)
                .child(keycap(shortcut, &self.keycaps))
                .child("runs nothing yet, so it's free to use.")
                .into_any_element(),
            None => div()
                .child(format!(
                    "No setting or command matches “{}”.",
                    self.query.trim()
                ))
                .into_any_element(),
        };
        let clear = button("clear-search", "Clear search", false, false, style)
            .debug_selector(|| "clear-search".to_string())
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                view.set_query("", cx);
                view.focus_search(window, cx);
            }));
        div()
            .pt(style.control_gap)
            .flex()
            .flex_col()
            .items_start()
            .gap(style.card_gap * 0.5)
            .text_color(style.text_muted)
            .child(message)
            .child(clear)
            .into_any_element()
    }

    fn render_card_row(
        &mut self,
        index: usize,
        card_index: usize,
        layout: &PaneLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = self.style.clone();
        let card = &layout.cards[card_index];
        let first = index == card.rows.start;
        let last = index + 1 == card.rows.end;
        let spaced = card_index > 0 && card.title.is_none();
        let dense = self.current_section() == Some(Page::Shortcuts);
        let content = self.render_row(index, &layout.rows[index], window, cx);
        card_row(index, first, last, dense, content, &style)
            .when(first && spaced, |row| row.mt(style.card_gap))
            .when(first && card_index == 0 && card.title.is_none(), |row| {
                row.mt(style.control_gap)
            })
            .into_any_element()
    }

    fn render_row(
        &mut self,
        index: usize,
        row: &ControlRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focus == SettingsFocus::Control(index) && self.has_focus(window, cx);
        let text = self.row_text(row);
        let control = self.row_control(index, row, focused, window, cx);
        let page = self
            .current_section()
            .map_or("", |page| PageSpec::get(page).id);
        let inactive = row.item().is_some_and(|item| self.is_inactive(item));
        two_column_row(&format!("{page}-{index}"), text, control, &self.style)
            .when(inactive, |row| row.opacity(self.style.inactive_opacity))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                    if view.focus != SettingsFocus::Control(index) {
                        view.set_focus(SettingsFocus::Control(index), window, cx);
                    }
                }),
            )
            .into_any_element()
    }
}

/// One item of the page's scrolling list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneItem {
    Title,
    CardTitle(usize),
    Row {
        index: usize,
        card: usize,
    },
    /// What shows when nothing matches.
    Empty,
}

/// The page as list items: its title, then each card's title and rows,
/// in the order [`SettingsView::child_index`] counts them.
fn pane_items(layout: &PaneLayout) -> Vec<PaneItem> {
    let mut items = vec![PaneItem::Title];
    for (card_index, card) in layout.cards.iter().enumerate() {
        if card.title.is_some() {
            items.push(PaneItem::CardTitle(card_index));
        }
        items.extend(card.rows.clone().map(|index| PaneItem::Row {
            index,
            card: card_index,
        }));
    }
    if layout.rows.is_empty() {
        items.push(PaneItem::Empty);
    }
    items
}

/// The title above a card of shortcuts.
fn card_title(card: &Card, style: &SettingsTheme) -> AnyElement {
    div()
        .w_full()
        .max_w(style.content_max_width)
        .pt(style.card_gap)
        .pb(style.control_gap)
        .px(style.gap_sm)
        .font_weight(style.strong_weight)
        .children(card.title.clone())
        .into_any_element()
}

/// One row on a card: the card's fill, rounded at its ends, with a
/// hairline between rows.
fn card_row(
    index: usize,
    first: bool,
    last: bool,
    dense: bool,
    content: AnyElement,
    style: &SettingsTheme,
) -> gpui::Stateful<gpui::Div> {
    let padding_y = if dense {
        style.list_row_padding_y
    } else {
        style.row_padding_y
    };
    div()
        .id(SharedString::from(format!("settings-row-{index}")))
        .flex_none()
        .w_full()
        .max_w(style.content_max_width)
        .px(style.card_padding_x)
        .bg(style.card_background)
        .when(first, |row| row.rounded_t(style.card_radius))
        .when(last, |row| row.rounded_b(style.card_radius))
        .child(
            div()
                .py(padding_y)
                .when(!first, |line| {
                    line.border_t(style.hairline).border_color(style.divider)
                })
                .child(content),
        )
}
