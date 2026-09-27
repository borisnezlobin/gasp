//! Drawing the settings screen: the section list on the left and the
//! controls on the right.

use editor_config::schema::SettingKind;
use gpui::{
    AnyElement, ClickEvent, Context, Div, Hsla, Render, SharedString, Stateful, Window, div,
    prelude::*,
};
use serde_json::Value;

use super::model::{SettingItem, ShortcutRow, humanize};
use super::view::{ControlRow, SectionRef, SettingsFocus, SettingsView};
use crate::icons::{IconName, icon};
use crate::theme::PanelTheme;

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.clone();
        div()
            .key_context("Settings")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .bg(theme.pane_background)
            .font_family(theme.font_family)
            .text_size(theme.font_size)
            .text_color(theme.text)
            .child(self.render_sidebar(window, cx))
            .child(self.render_pane(window, cx))
    }
}

impl SettingsView {
    fn has_focus(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.focus_handle.contains_focused(window, cx)
    }

    fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let list_focused = self.focus == SettingsFocus::Sections && self.has_focus(window, cx);
        let sections = self
            .visible_sections()
            .into_iter()
            .enumerate()
            .map(|(index, section)| {
                let selected = index == self.current;
                let background = match (selected, list_focused) {
                    (true, true) => Some(theme.selected_focused),
                    (true, false) => Some(theme.selected),
                    _ => None,
                };
                let hover = theme.hover;
                div()
                    .id(("settings-section", index))
                    .h(theme.row_height)
                    .px(theme.padding_x)
                    .flex()
                    .items_center()
                    .gap(theme.gap)
                    .rounded(theme.radius)
                    .when_some(background, |row, color| row.bg(color))
                    .when(background.is_none(), |row| row.hover(move |s| s.bg(hover)))
                    .when(selected && list_focused, |row| {
                        row.shadow(vec![theme.focus_ring()])
                    })
                    .when(selected, |row| row.font_weight(theme.strong_weight))
                    .when(section == SectionRef::Shortcuts, |row| {
                        row.child(
                            icon(IconName::Keyboard)
                                .flex_none()
                                .size(theme.icon_size)
                                .text_color(theme.icon),
                        )
                    })
                    .child(self.section_title(section))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.select_section(index, cx);
                        view.set_focus(SettingsFocus::Sections, window, cx);
                    }))
            });
        div()
            .flex_none()
            .w(theme.sidebar_width)
            .h_full()
            .p(theme.section_padding)
            .flex()
            .flex_col()
            .gap(theme.gap)
            .bg(theme.background)
            .child(self.search.clone())
            .child(
                div()
                    .id("settings-sections")
                    .flex()
                    .flex_col()
                    .children(sections),
            )
    }

    fn render_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.clone();
        let title = self
            .current_section()
            .map(|section| self.section_title(section))
            .unwrap_or_else(|| "Nothing matches".to_string());
        let focused = self.has_focus(window, cx);
        let rows = self.rows();
        let shortcuts = self.current_section() == Some(SectionRef::Shortcuts);
        let rows: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                // A text field shows its own focus ring, so its row doesn't.
                let is_focused = focused
                    && self.focus == SettingsFocus::Control(index)
                    && self.field_for(row).is_none();
                let element = self.render_row(index, row, is_focused, cx);
                let previous = index.checked_sub(1).and_then(|i| rows.get(i));
                match new_category(row, previous) {
                    Some(category) => self.with_category(category, element),
                    None => element,
                }
            })
            .collect();
        let gap = if shortcuts {
            theme.gap
        } else {
            theme.setting_gap
        };
        let empty = rows.is_empty().then(|| {
            div()
                .text_color(theme.muted_text)
                .child(format!("No settings match “{}”.", self.query.trim()))
        });
        div()
            .id("settings-pane")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(theme.section_padding)
            .flex()
            .flex_col()
            .gap(gap)
            // Rows are direct children so `scroll_to_item` can find them.
            .child(
                div()
                    .text_size(theme.title_font_size)
                    .font_weight(theme.strong_weight)
                    .child(title),
            )
            .children(
                rows.into_iter()
                    .map(|row| div().flex_none().max_w(theme.content_max_width).child(row)),
            )
            .children(empty)
    }

    fn row_shell(&self, index: usize, focused: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(("settings-row", index))
            .flex()
            .items_center()
            .justify_between()
            .gap(theme.section_padding)
            .px(theme.padding_x)
            .py(theme.padding_y)
            .rounded(theme.radius)
            .when(focused, |row| {
                row.bg(theme.selected).shadow(vec![theme.focus_ring()])
            })
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                if view.focus != SettingsFocus::Control(index) {
                    view.set_focus(SettingsFocus::Control(index), window, cx);
                }
            }))
    }

    fn render_row(
        &self,
        index: usize,
        row: &ControlRow,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shell = self.row_shell(index, focused, cx);
        match row {
            ControlRow::Shortcut(shortcut) => shell
                .child(self.shortcut_title(shortcut))
                .child(self.keycaps(shortcut)),
            ControlRow::Setting(item) | ControlRow::MapEntry { item, .. } => shell
                .child(self.describe(item))
                .child(self.control_with_reset(item, cx)),
            ControlRow::MapAdd(item) => shell
                .child(self.describe(item))
                .child(self.field_control(row)),
        }
        .into_any_element()
    }

    /// Title, description and any error for a setting.
    fn describe(&self, item: &SettingItem) -> impl IntoElement {
        let theme = &self.theme;
        let error = self
            .error
            .as_ref()
            .filter(|(key, _)| *key == item.key)
            .map(|(_, message)| message.clone());
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_weight(theme.strong_weight)
                    .child(item.title.clone()),
            )
            .when(!item.description.is_empty(), |column| {
                column.child(
                    div()
                        .text_size(theme.small_font_size)
                        .text_color(theme.muted_text)
                        .child(item.description.clone()),
                )
            })
            .children(error.map(|message| {
                div()
                    .text_size(theme.small_font_size)
                    .text_color(theme.error_text)
                    .child(message)
            }))
    }

    fn control_with_reset(&self, item: &SettingItem, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let control = match &item.kind {
            SettingKind::Bool => self.toggle_control(item, cx),
            SettingKind::Choice(options) => self.choice_control(item, options, cx),
            SettingKind::Integer | SettingKind::Number => self.number_control(item, cx),
            _ => self.field_control(&ControlRow::Setting(item.clone())),
        };
        let reset = self.is_changed(item).then(|| {
            let key = item.key.clone();
            div()
                .id(SharedString::from(format!("reset-{}", item.key)))
                .flex_none()
                .p(theme.padding_y)
                .rounded(theme.radius)
                .hover(|s| s.bg(theme.hover))
                .child(
                    icon(IconName::ArrowCounterClockwise)
                        .size(theme.icon_size)
                        .text_color(theme.icon),
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.reset(&key, cx);
                }))
        });
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(theme.gap)
            .children(reset)
            .child(control)
    }

    fn toggle_control(&self, item: &SettingItem, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let on = self.current_value(item).as_bool().unwrap_or(false);
        let track = if on {
            theme.control_selected
        } else {
            theme.control_background
        };
        let item = item.clone();
        let knob_size = theme.toggle_height - theme.toggle_knob_inset * 2.;
        div()
            .id(SharedString::from(format!("toggle-{}", item.key)))
            .debug_selector(|| format!("toggle-{}", item.key))
            .flex_none()
            .w(theme.toggle_width)
            .h(theme.toggle_height)
            .p(theme.toggle_knob_inset)
            .flex()
            .when(on, |track| track.justify_end())
            .rounded(theme.toggle_height)
            .bg(track)
            .child(
                div()
                    .size(knob_size)
                    .rounded(knob_size)
                    .bg(theme.toggle_knob),
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.toggle(&item, cx)))
            .into_any_element()
    }

    fn choice_control(
        &self,
        item: &SettingItem,
        options: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let current = self.current_value(item);
        let segments = options.iter().map(|option| {
            let chosen = current.as_str() == Some(option.as_str());
            let (background, text) = segment_colors(theme, chosen);
            let (item, value) = (item.clone(), option.clone());
            div()
                .id(SharedString::from(format!("choice-{}-{option}", item.key)))
                .debug_selector(|| format!("choice-{}-{option}", item.key))
                .px(theme.padding_x)
                .py(theme.padding_y)
                .rounded(theme.radius)
                .when_some(background, |segment, color| segment.bg(color))
                .text_color(text)
                .child(humanize(option))
                .on_click(
                    cx.listener(move |view, _: &ClickEvent, _, cx| view.choose(&item, &value, cx)),
                )
        });
        div()
            .flex()
            .flex_wrap()
            .p(theme.control_inset)
            .rounded(theme.radius)
            .bg(theme.control_background)
            .children(segments)
            .into_any_element()
    }

    fn number_control(&self, item: &SettingItem, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let editing = self
            .number_edit
            .as_ref()
            .filter(|(key, _)| *key == item.key)
            .map(|(_, buffer)| buffer.clone());
        let shown = editing.unwrap_or_else(|| number_text(&self.current_value(item)));
        let step = |name: IconName, direction: i64, id: &str| {
            let item = item.clone();
            div()
                .id(SharedString::from(format!("{id}-{}", item.key)))
                .p(theme.padding_y)
                .rounded(theme.radius)
                .hover(|s| s.bg(theme.hover))
                .child(icon(name).size(theme.icon_size).text_color(theme.icon))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.step_number(&item, direction, cx)
                }))
        };
        div()
            .flex()
            .items_center()
            .gap(theme.gap)
            .p(theme.control_inset)
            .rounded(theme.radius)
            .bg(theme.control_background)
            .child(step(IconName::Minus, -1, "decrease"))
            .child(
                div()
                    .min_w(theme.icon_size * 2.)
                    .flex()
                    .justify_center()
                    .child(shown),
            )
            .child(step(IconName::Plus, 1, "increase"))
            .into_any_element()
    }

    fn field_control(&self, row: &ControlRow) -> AnyElement {
        let Some(field) = self.field_for(row) else {
            return div().into_any_element();
        };
        div()
            .flex_none()
            .w(self.theme.sidebar_width)
            .child(field)
            .into_any_element()
    }

    fn shortcut_title(&self, shortcut: &ShortcutRow) -> impl IntoElement {
        div().flex_1().min_w_0().child(shortcut.title.clone())
    }

    /// A shortcut row under the name of the group it starts.
    fn with_category(&self, category: String, row: AnyElement) -> AnyElement {
        let theme = &self.theme;
        div()
            .flex()
            .flex_col()
            .gap(theme.gap)
            .pt(theme.setting_gap)
            .child(
                div()
                    .px(theme.padding_x)
                    .text_size(theme.small_font_size)
                    .text_color(theme.muted_text)
                    .child(category),
            )
            .child(row)
            .into_any_element()
    }

    fn keycaps(&self, shortcut: &ShortcutRow) -> impl IntoElement {
        let theme = &self.theme;
        let caps = shortcut.keys.iter().map(|keys| {
            div()
                .px(theme.padding_x)
                .py(theme.keycap_padding_y)
                .rounded(theme.radius)
                .bg(theme.keycap_background)
                .child(keys.clone())
        });
        let unbound = shortcut
            .keys
            .is_empty()
            .then(|| div().text_color(theme.muted_text).child("No shortcut"));
        div()
            .flex_none()
            .flex()
            .gap(theme.gap)
            .children(caps)
            .children(unbound)
    }
}

/// The category a shortcut row starts, when it differs from the row above.
fn new_category(row: &ControlRow, previous: Option<&ControlRow>) -> Option<String> {
    let ControlRow::Shortcut(shortcut) = row else {
        return None;
    };
    let previous = match previous {
        Some(ControlRow::Shortcut(previous)) => Some(previous.category.as_str()),
        _ => None,
    };
    (previous != Some(shortcut.category.as_str())).then(|| shortcut.category.clone())
}

fn segment_colors(theme: &PanelTheme, chosen: bool) -> (Option<Hsla>, Hsla) {
    if chosen {
        (Some(theme.control_selected), theme.control_selected_text)
    } else {
        (None, theme.text)
    }
}

fn number_text(value: &Value) -> String {
    match value {
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}
