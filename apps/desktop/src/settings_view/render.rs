//! Drawing the settings screen: a modal with the section list on the
//! left and the current page on the right, its rows grouped on cards.

use gpui::{
    AnyElement, ClickEvent, Context, DismissEvent, MouseButton, MouseDownEvent, Pixels, Render,
    SharedString, Size, Window, div, prelude::*, relative,
};

use super::controls::{icon_button, two_column_row};
use super::model::{PAGES, PageSpec};
use super::view::{Card, ControlRow, SettingsFocus, SettingsView};
use crate::icons::{IconName, icon};
use crate::theme::SettingsTheme;

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
            .child(self.render_content(window, cx))
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
            .child(list)
    }

    fn search_box(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let style = &self.style;
        let focused = self.focus == SettingsFocus::Search && self.has_focus(window, cx);
        div()
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
            .child(div().flex_1().min_w_0().child(self.search.clone()))
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

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let style = self.style.clone();
        let layout = self.layout();
        let title = self
            .current_section()
            .map(|page| self.section_title(page))
            .unwrap_or_else(|| "Nothing matches".to_string());
        let mut children: Vec<AnyElement> = vec![
            div()
                .flex_none()
                .pb(style.gap_sm)
                .text_size(style.page_title_size)
                .font_weight(style.strong_weight)
                .child(title)
                .into_any_element(),
        ];
        for (card_index, card) in layout.cards.iter().enumerate() {
            children.extend(self.render_card(card_index, card, &layout.rows, window, cx));
        }
        let empty = layout.rows.is_empty().then(|| {
            div()
                .pt(style.card_gap)
                .text_color(style.text_muted)
                .child(format!("No settings match “{}”.", self.query.trim()))
        });
        let close = icon_button("settings-close", IconName::X, style.text_muted, &style)
            .debug_selector(|| "settings-close".to_string())
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(DismissEvent)));
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .child(
                div()
                    .id("settings-pane")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(style.content_padding_x)
                    .py(style.content_padding_y)
                    .flex()
                    .flex_col()
                    // Rows are direct children so `scroll_to_item` finds them.
                    .children(children)
                    .children(empty),
            )
            .child(
                div()
                    .absolute()
                    .top(style.nav_padding)
                    .right(style.nav_padding)
                    .child(close),
            )
    }

    fn render_card(
        &mut self,
        card_index: usize,
        card: &Card,
        rows: &[ControlRow],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let style = self.style.clone();
        let mut elements = Vec::new();
        if let Some(title) = &card.title {
            elements.push(
                div()
                    .flex_none()
                    .w_full()
                    .max_w(style.content_max_width)
                    .pt(style.card_gap)
                    .pb(style.control_gap)
                    .px(style.gap_sm)
                    .font_weight(style.strong_weight)
                    .child(title.clone())
                    .into_any_element(),
            );
        }
        let spaced = card_index > 0 && card.title.is_none();
        for index in card.rows.clone() {
            let first = index == card.rows.start;
            let last = index + 1 == card.rows.end;
            let content = self.render_row(index, &rows[index], window, cx);
            elements.push(
                card_row(index, first, last, content, &style)
                    .when(first && spaced, |row| row.mt(style.card_gap))
                    .when(first && card_index == 0 && card.title.is_none(), |row| {
                        row.mt(style.control_gap)
                    })
                    .into_any_element(),
            );
        }
        elements
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
        two_column_row(&format!("{page}-{index}"), text, control, &self.style)
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

/// One row on a card: the card's fill, rounded at its ends, with a
/// hairline between rows.
fn card_row(
    index: usize,
    first: bool,
    last: bool,
    content: AnyElement,
    style: &SettingsTheme,
) -> gpui::Stateful<gpui::Div> {
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
                .py(style.row_padding_y)
                .when(!first, |line| {
                    line.border_t(style.hairline).border_color(style.divider)
                })
                .child(content),
        )
}
