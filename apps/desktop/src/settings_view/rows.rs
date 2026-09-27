//! What each kind of row shows: its text column and its control.

use editor_config::schema::SettingKind;
use gpui::{
    AnyElement, ClickEvent, Context, Focusable, MouseButton, SharedString, Window, div, prelude::*,
};
use serde_json::Value;

use super::controls::{
    button, dropdown_button, field_box, icon_button, keycap, menu_option, menu_panel, popover,
    row_text, stepper, swatch, toggle_switch,
};
use super::menu::MenuTarget;
use super::model::{ACCENT_TOKEN, FontSlot, SettingItem, ShortcutRow, choice_label};
use super::view::{ControlRow, SettingsFocus, SettingsView, theme_key};
use crate::icons::IconName;
use crate::theme::{ACCENT_CHOICES, parse_color};

impl SettingsView {
    /// Title, description and notes for a row.
    pub(super) fn row_text(&self, row: &ControlRow) -> AnyElement {
        let description = self.row_description(row);
        let description = (!description.is_empty()).then(|| {
            let text = div().child(description);
            match row {
                ControlRow::Vault => text.font_family(self.style.code_font_family.clone()),
                _ => text,
            }
            .into_any_element()
        });
        row_text(row.title(), description, self.row_notes(row), &self.style).into_any_element()
    }

    /// Errors and warnings to show under a row's description.
    fn row_notes(&self, row: &ControlRow) -> Vec<AnyElement> {
        let error_key = match row {
            ControlRow::Setting(item) | ControlRow::MapAdd(item) => Some(item.key.clone()),
            ControlRow::MapEntry { item, .. } => Some(item.key.clone()),
            ControlRow::Font(slot) => Some(theme_key(slot.token())),
            ControlRow::Accent => Some(theme_key(ACCENT_TOKEN)),
            ControlRow::Shortcut(shortcut) => Some(shortcut.id.clone()),
            _ => None,
        };
        let mut notes: Vec<AnyElement> = self
            .error
            .iter()
            .filter(|(key, _)| Some(key) == error_key.as_ref())
            .map(|(_, message)| div().child(message.clone()).into_any_element())
            .collect();
        match row {
            ControlRow::Shortcut(shortcut) => notes.extend(self.shortcut_notes(shortcut)),
            ControlRow::Font(slot) => notes.extend(self.font_note(*slot).map(|note| {
                div()
                    .text_color(self.style.text_muted)
                    .child(note)
                    .into_any_element()
            })),
            _ => {}
        }
        notes
    }

    fn shortcut_notes(&self, shortcut: &ShortcutRow) -> Vec<AnyElement> {
        let rejection = self
            .capture
            .as_ref()
            .filter(|capture| capture.command == shortcut.id)
            .and_then(|capture| capture.rejection.clone());
        let conflicts = shortcut
            .conflicts
            .iter()
            .map(|(key, other)| format!("{key} also runs “{other}”."));
        rejection
            .into_iter()
            .chain(conflicts)
            .map(|note| div().child(note).into_any_element())
            .collect()
    }

    /// The control on the right of a row, if it has one.
    pub(super) fn row_control(
        &self,
        index: usize,
        row: &ControlRow,
        focused: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let control = match row {
            ControlRow::Setting(item) => self.setting_control(index, item, focused, cx),
            ControlRow::MapAdd(_) => self.field_control(row, focused),
            ControlRow::MapEntry { item, .. } => self.map_entry_control(index, item, focused, cx),
            ControlRow::Font(slot) => self.font_control(index, *slot, focused, cx),
            ControlRow::Accent => {
                let typing = self.hex_field.focus_handle(cx).is_focused(window);
                self.accent_control(focused && !typing, typing, cx)
            }
            ControlRow::Vault => self.vault_control(focused, cx),
            ControlRow::Version => return None,
            ControlRow::Shortcut(shortcut) => self.shortcut_control(shortcut, focused, cx),
        };
        Some(control)
    }

    fn setting_control(
        &self,
        index: usize,
        item: &SettingItem,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = match &item.kind {
            SettingKind::Bool => self.toggle_control(item, focused, cx),
            SettingKind::Choice(_) => self.dropdown_control(index, item, focused, cx),
            SettingKind::Integer | SettingKind::Number => self.number_control(item, focused, cx),
            _ => self.field_control(&ControlRow::Setting(item.clone()), focused),
        };
        let reset = self.is_changed(item).then(|| {
            let key = item.key.clone();
            self.reset_button(&item.key, cx, move |view, cx| view.reset(&key, cx))
        });
        div()
            .flex()
            .items_center()
            .gap(self.style.gap_sm)
            .children(reset)
            .child(control)
            .into_any_element()
    }

    fn reset_button(
        &self,
        key: &str,
        cx: &mut Context<Self>,
        reset: impl Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
    ) -> AnyElement {
        let style = &self.style;
        let id = SharedString::from(format!("reset-{key}"));
        icon_button(id, IconName::ArrowCounterClockwise, style.text_muted, style)
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                reset(view, cx);
            }))
            .into_any_element()
    }

    fn toggle_control(
        &self,
        item: &SettingItem,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on = self.current_value(item).as_bool().unwrap_or(false);
        let key = item.key.clone();
        let item = item.clone();
        toggle_switch(
            SharedString::from(format!("toggle-{key}")),
            on,
            focused,
            &self.style,
        )
        .debug_selector(|| format!("toggle-{key}"))
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.toggle(&item, cx)))
        .into_any_element()
    }

    /// A dropdown button for row `index`, with its menu hung under it
    /// while open.
    fn dropdown(
        &self,
        index: usize,
        id: String,
        label: AnyElement,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selector = id.clone();
        let button = dropdown_button(SharedString::from(id), label, focused, &self.style)
            .debug_selector(|| selector)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                let open = view.menu.as_ref().is_some_and(|menu| menu.row == index);
                if open {
                    view.close_menu(window, cx);
                } else {
                    view.set_focus(SettingsFocus::Control(index), window, cx);
                    view.open_menu(index, window, cx);
                }
            }));
        let menu = self
            .menu
            .as_ref()
            .filter(|menu| menu.row == index)
            .map(|_| popover(self.render_menu(cx), &self.style));
        div()
            .relative()
            .child(button)
            .children(menu)
            .into_any_element()
    }

    fn dropdown_control(
        &self,
        index: usize,
        item: &SettingItem,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = self.current_value(item);
        let label = choice_label(value.as_str().unwrap_or_default());
        let id = format!("dropdown-{}", item.key);
        self.dropdown(
            index,
            id,
            div().child(label).into_any_element(),
            focused,
            cx,
        )
    }

    fn render_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = self.menu.as_ref() else {
            return div().into_any_element();
        };
        let style = &self.style;
        let current = self.menu_value(&menu.target);
        let options = menu.shown.iter().enumerate().map(|(position, option)| {
            let label = div().child(menu.label(option));
            let label = match menu.target {
                MenuTarget::Font(_) => label.font_family(SharedString::from(option.clone())),
                MenuTarget::Choice(_) => label,
            };
            let value = option.clone();
            let selector = format!("menu-option-{option}");
            menu_option(
                ("settings-menu-option", position),
                label,
                *option == current,
                position == menu.highlighted,
                style,
            )
            .debug_selector(|| selector)
            .on_click(
                cx.listener(move |view, _: &ClickEvent, window, cx| view.pick(&value, window, cx)),
            )
        });
        let empty = menu.shown.is_empty().then(|| {
            div()
                .p(style.control_gap)
                .text_color(style.text_muted)
                .child("No fonts match.")
        });
        menu_panel(style)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .text_size(style.small_text_size)
            .children(menu.filter.clone().map(|filter| {
                field_box(filter, Some(IconName::MagnifyingGlass), false, style).w_full()
            }))
            .child(
                div()
                    .id("settings-menu-options")
                    .max_h(style.menu_max_height)
                    .overflow_y_scroll()
                    .track_scroll(&menu.scroll)
                    .flex()
                    .flex_col()
                    .children(options)
                    .children(empty),
            )
            .into_any_element()
    }

    fn number_control(
        &self,
        item: &SettingItem,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = &self.style;
        let editing = self
            .number_edit
            .as_ref()
            .filter(|(key, _)| *key == item.key)
            .map(|(_, buffer)| buffer.clone());
        let shown = editing.unwrap_or_else(|| number_text(&self.current_value(item)));
        let step = |name: IconName, direction: i64, id: &str| {
            let item = item.clone();
            let id = format!("{id}-{}", item.key);
            let selector = id.clone();
            icon_button(SharedString::from(id), name, style.text_muted, style)
                .debug_selector(|| selector)
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.step_number(&item, direction, cx)
                }))
        };
        let minus = step(IconName::Minus, -1, "decrease");
        let plus = step(IconName::Plus, 1, "increase");
        let id = SharedString::from(format!("stepper-{}", item.key));
        stepper(id, minus, shown, plus, focused, style).into_any_element()
    }

    fn field_control(&self, row: &ControlRow, focused: bool) -> AnyElement {
        let Some(field) = self.field_for(row) else {
            return div().into_any_element();
        };
        field_box(field, None, focused, &self.style)
            .w(self.style.field_width)
            .into_any_element()
    }

    fn map_entry_control(
        &self,
        index: usize,
        item: &SettingItem,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = match &item.kind {
            SettingKind::Choice(_) => self.dropdown_control(index, item, focused, cx),
            SettingKind::Bool => self.toggle_control(item, focused, cx),
            _ => self.number_control(item, focused, cx),
        };
        let key = item.key.clone();
        let style = &self.style;
        let remove = icon_button(
            SharedString::from(format!("remove-{key}")),
            IconName::X,
            style.text_muted,
            style,
        )
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.reset(&key, cx);
        }));
        div()
            .flex()
            .items_center()
            .gap(style.gap_sm)
            .child(remove)
            .child(control)
            .into_any_element()
    }

    fn font_control(
        &self,
        index: usize,
        slot: FontSlot,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let family = self.token(slot.token()).unwrap_or_default();
        let label = div()
            .font_family(SharedString::from(self.shown_font(slot).to_string()))
            .child(family)
            .into_any_element();
        let id = format!("dropdown-{}", slot.token());
        let dropdown = self.dropdown(index, id, label, focused, cx);
        let token = slot.token();
        let reset = self.is_token_changed(token).then(|| {
            self.reset_button(token, cx, move |view, cx| view.write_token(token, None, cx))
        });
        div()
            .flex()
            .items_center()
            .gap(self.style.gap_sm)
            .children(reset)
            .child(dropdown)
            .into_any_element()
    }

    fn accent_control(&self, focused: bool, typing: bool, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let current = self.token(ACCENT_TOKEN).unwrap_or_default();
        let swatches = ACCENT_CHOICES.iter().map(|hex| {
            let color = parse_color(hex).unwrap_or(style.accent);
            let chosen = hex.eq_ignore_ascii_case(&current);
            let selector = format!("swatch-{hex}");
            swatch(SharedString::from(selector.clone()), color, chosen, style)
                .debug_selector(|| selector)
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.write_token(ACCENT_TOKEN, Some(hex), cx)
                }))
        });
        let swatches: Vec<_> = swatches.collect();
        let reset = self.is_token_changed(ACCENT_TOKEN).then(|| {
            self.reset_button(ACCENT_TOKEN, cx, |view, cx| {
                view.write_token(ACCENT_TOKEN, None, cx)
            })
        });
        div()
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap(style.control_gap)
            .children(reset)
            .child(
                div()
                    .id("accent-swatches")
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(style.control_gap)
                    .p(style.gap_sm)
                    .rounded(style.radius)
                    .when(focused, |group| group.shadow(vec![style.focus()]))
                    .children(swatches),
            )
            .child(field_box(self.hex_field.clone(), None, typing, style).w(style.hex_field_width))
            .into_any_element()
    }

    fn vault_control(&self, focused: bool, cx: &mut Context<Self>) -> AnyElement {
        button(
            "open-vault",
            "Open another vault",
            false,
            focused,
            &self.style,
        )
        .debug_selector(|| "open-vault".to_string())
        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.request_command("vault.open", cx)))
        .into_any_element()
    }

    fn shortcut_control(
        &self,
        shortcut: &ShortcutRow,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = &self.style;
        let capturing = self.capturing() == Some(shortcut.id.as_str());
        let caps = shortcut.keys.iter().map(|key| {
            let remove = key.user_rule.clone().map(|rule_id| {
                let selector = format!("remove-key-{rule_id}");
                icon_button(
                    SharedString::from(selector.clone()),
                    IconName::X,
                    style.text_muted,
                    style,
                )
                .size(style.small_icon_size + style.gap_sm)
                .debug_selector(|| selector)
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.remove_shortcut(&rule_id, cx)
                }))
            });
            keycap(key.label.clone(), style).children(remove)
        });
        let unbound = (shortcut.keys.is_empty() && !capturing).then(|| {
            div()
                .text_size(style.small_text_size)
                .text_color(style.text_faint)
                .child("No shortcut")
        });
        let waiting = capturing.then(|| {
            keycap("Press a shortcut", style)
                .text_color(style.text_muted)
                .shadow(vec![style.focus()])
        });
        let command = shortcut.id.clone();
        let selector = format!("add-key-{command}");
        let add = icon_button(
            SharedString::from(selector.clone()),
            IconName::Plus,
            style.text_muted,
            style,
        )
        .debug_selector(|| selector)
        .when(focused && !capturing, |add| add.shadow(vec![style.focus()]))
        .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
            view.start_capture(&command, window, cx)
        }));
        div()
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap(style.control_gap)
            .children(caps)
            .children(unbound)
            .children(waiting)
            .child(add)
            .into_any_element()
    }
}

fn number_text(value: &Value) -> String {
    match value {
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}
