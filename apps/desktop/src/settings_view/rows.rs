//! What each kind of row shows: its text column and its control.

use editor_config::schema::SettingKind;
use gpui::{
    AnyElement, ClickEvent, Context, Focusable, MouseButton, SharedString, Window, div, prelude::*,
};
use serde_json::Value;

use super::controls::{
    button, capture_field, control_note, dropdown_button, field_box, icon_button, menu_option,
    menu_panel, popover, removable_keycap, row_text, small_icon_button, stepper, swatch,
    toggle_switch,
};
use super::menu::MenuTarget;
use super::model::{FontSlot, SettingItem, ShortcutQuery, ShortcutRow, choice_label, map_names};
use super::view::{ControlRow, SettingsFocus, SettingsView, theme_key};
use crate::icons::IconName;
use crate::picker::shortcut::Shortcut;
use crate::theme::parse_color;
use crate::ui::{Tooltip, keycap};

impl SettingsView {
    /// Title, description and notes for a row.
    pub(super) fn row_text(&self, row: &ControlRow) -> AnyElement {
        let description = match row {
            ControlRow::Shortcut(shortcut) => self.shortcut_default(shortcut),
            _ => {
                let description = self.row_description(row);
                (!description.is_empty()).then(|| div().child(description).into_any_element())
            }
        };
        row_text(row.title(), description, self.row_notes(row), &self.style).into_any_element()
    }

    /// The key an error from the last write is kept under, for each row.
    fn error_key(&self, row: &ControlRow) -> Option<String> {
        match row {
            ControlRow::Setting(item) | ControlRow::MapAdd(item) | ControlRow::ListAdd(item) => {
                Some(item.key.clone())
            }
            ControlRow::SyncRemote => Some(super::sync_page::REMOTE_FIELD.to_string()),
            ControlRow::SyncAccount => Some(super::sync_page::TOKEN_KEY.to_string()),
            ControlRow::SnippetsFile => Some(super::snippets_page::SNIPPETS_KEY.to_string()),
            ControlRow::Replacement(_) => Some(super::snippets_page::REPLACEMENTS_KEY.to_string()),
            ControlRow::MapEntry { item, .. } => Some(item.key.clone()),
            ControlRow::Font(slot) => Some(theme_key(slot.token())),
            ControlRow::Accent => Some(theme_key(self.accent_token())),
            ControlRow::Shortcut(shortcut) => Some(shortcut.id.clone()),
            _ => None,
        }
    }

    /// Why the last write for this row failed, or why the chord just
    /// pressed for it was refused.
    pub(super) fn row_error(&self, row: &ControlRow) -> Option<String> {
        if let (ControlRow::Shortcut(shortcut), Some(capture)) = (row, &self.capture)
            && self.capturing() == Some(shortcut.id.as_str())
        {
            return capture.rejection.clone();
        }
        let key = self.error_key(row)?;
        self.error
            .as_ref()
            .filter(|(failed, _)| *failed == key)
            .map(|(_, message)| message.clone())
    }

    /// Lasting warnings under a row's description: a shortcut another
    /// command also uses, or a font that isn't installed.
    fn row_notes(&self, row: &ControlRow) -> Vec<AnyElement> {
        match row {
            ControlRow::Shortcut(shortcut) => self.shortcut_notes(shortcut),
            ControlRow::Font(slot) => self
                .font_note(*slot)
                .map(|note| {
                    div()
                        .text_color(self.style.text_muted)
                        .child(note)
                        .into_any_element()
                })
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    fn shortcut_notes(&self, shortcut: &ShortcutRow) -> Vec<AnyElement> {
        let keycaps = self.keycaps.clone().compact().on_text(self.style.warning);
        shortcut
            .conflicts
            .iter()
            .filter_map(|(label, other)| {
                let key = shortcut.keys.iter().find(|key| key.label == *label)?;
                Some(self.inline_keys(&[key.shortcut], &format!("also runs “{other}”."), &keycaps))
            })
            .collect()
    }

    /// What a changed shortcut row comes with, so reset says what it does.
    fn shortcut_default(&self, shortcut: &ShortcutRow) -> Option<AnyElement> {
        let defaults = shortcut.changed_from.as_ref()?;
        if defaults.is_empty() {
            return Some(
                div()
                    .child("It has no shortcut by default.")
                    .into_any_element(),
            );
        }
        let keycaps = self
            .keycaps
            .clone()
            .compact()
            .on_text(self.style.text_muted);
        let line = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(self.style.gap_sm)
            .child("The default is")
            .children(defaults.iter().map(|shortcut| keycap(*shortcut, &keycaps)));
        Some(line.into_any_element())
    }

    /// A line of small key chips followed by `text`.
    fn inline_keys(
        &self,
        shortcuts: &[Shortcut],
        text: &str,
        keycaps: &crate::theme::KeycapTheme,
    ) -> AnyElement {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(self.style.gap_sm)
            .children(shortcuts.iter().map(|shortcut| keycap(*shortcut, keycaps)))
            .child(text.to_string())
            .into_any_element()
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
            ControlRow::MapAdd(map) => self.map_add_control(index, map, row, focused, cx),
            ControlRow::MapEntry { item, .. } => self.map_entry_control(index, item, focused, cx),
            ControlRow::Font(slot) => self.font_control(index, *slot, focused, cx),
            ControlRow::Accent => {
                let typing = self.hex_field.focus_handle(cx).is_focused(window);
                self.accent_control(focused && !typing, typing, cx)
            }
            ControlRow::Vault => self.vault_control(focused, cx),
            // The Snippets page's rows are drawn above.
            ControlRow::Version
            | ControlRow::SnippetsFile
            | ControlRow::Snippet(_)
            | ControlRow::SnippetEditor
            | ControlRow::Replacement(_) => return None,
            ControlRow::Shortcut(shortcut) => self.shortcut_control(shortcut, focused, cx),
            ControlRow::SyncRemote => return self.remote_control(row, focused),
            ControlRow::SyncAccount => self.account_control(focused, cx),
            ControlRow::ListAdd(item) => self.list_add_control(item, row, focused, cx),
            ControlRow::ListEntry { list, value } => {
                self.list_entry_control(list, value, focused, cx)
            }
        };
        // An error hangs under the control rather than pushing rows down.
        let note = self
            .row_error(row)
            .map(|message| control_note(message, &self.style));
        Some(
            div()
                .relative()
                .child(control)
                .children(note)
                .into_any_element(),
        )
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

    pub(super) fn reset_button(
        &self,
        key: &str,
        cx: &mut Context<Self>,
        reset: impl Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
    ) -> AnyElement {
        let style = &self.style;
        let selector = format!("reset-{key}");
        let id = SharedString::from(selector.clone());
        icon_button(id, IconName::ArrowCounterClockwise, style.text_muted, style)
            .debug_selector(|| selector)
            .tooltip(Tooltip::new("Reset to default", None).builder())
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
                MenuTarget::Choice(_) | MenuTarget::MapAdd(_) => label,
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

    pub(super) fn field_control(&self, row: &ControlRow, focused: bool) -> AnyElement {
        let Some(field) = self.field_for(row) else {
            return div().into_any_element();
        };
        field_box(field, None, focused, &self.style)
            .w(self.style.field_width)
            .into_any_element()
    }

    /// Adds an entry to a map: a menu of the names it can take when
    /// they're a fixed list, else a field to type a name into.
    fn map_add_control(
        &self,
        index: usize,
        map: &SettingItem,
        row: &ControlRow,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if map_names(&map.key).is_none() {
            return self.field_control(row, focused);
        }
        let label = div()
            .text_color(self.style.text_muted)
            .child("Add a kind of syntax");
        self.dropdown(
            index,
            format!("add-{}", map.key),
            label.into_any_element(),
            focused,
            cx,
        )
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
        let token = self.accent_token();
        let current = self.token(token).unwrap_or_default();
        let swatches = self.accent_choices().iter().map(|hex| {
            let color = parse_color(hex).unwrap_or(style.accent);
            let chosen = hex.eq_ignore_ascii_case(&current);
            let selector = format!("swatch-{hex}");
            swatch(SharedString::from(selector.clone()), color, chosen, style)
                .debug_selector(|| selector)
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.write_token(token, Some(hex), cx)
                }))
        });
        let swatches: Vec<_> = swatches.collect();
        let reset = self.is_token_changed(token).then(|| {
            self.reset_button(token, cx, move |view, cx| view.write_token(token, None, cx))
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
                    // Opaque under the ring, which would otherwise fill
                    // the group in behind the swatches.
                    .when(focused, |group| {
                        group.bg(style.card_background).shadow(vec![style.focus()])
                    })
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
        let query = ShortcutQuery::new(&self.query);
        let conflicting: Vec<&str> = shortcut.conflicts.iter().map(|(k, _)| k.as_str()).collect();
        let caps = shortcut.keys.iter().enumerate().map(|(position, key)| {
            let marked = query
                .keys()
                .is_some_and(|keys| keys.matches(key.shortcut.chord));
            let warning = conflicting.contains(&key.label.as_str());
            let id = key
                .rule
                .clone()
                .unwrap_or_else(|| format!("{}-{position}", shortcut.id));
            let selector = format!("remove-key-{id}");
            let (chip, remove) = removable_keycap(
                SharedString::from(selector.clone()),
                key.shortcut,
                marked,
                warning,
                &self.keycaps,
                style,
            );
            let command = shortcut.id.clone();
            let key = key.clone();
            chip.debug_selector(|| format!("key-{id}")).child(
                remove.debug_selector(|| selector).on_click(cx.listener(
                    move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.remove_key(&command, &key, cx)
                    },
                )),
            )
        });
        let caps: Vec<_> = caps.collect();
        let unbound = (shortcut.keys.is_empty() && !capturing).then(|| {
            div()
                .text_size(style.small_text_size)
                .text_color(style.text_faint)
                .child("No shortcut")
        });
        let waiting = capturing.then(|| self.capture_box("Press a shortcut", cx));
        let reset = shortcut.changed_from.is_some().then(|| {
            let command = shortcut.id.clone();
            self.reset_button(&shortcut.id, cx, move |view, cx| {
                view.reset_shortcuts(&command, cx)
            })
        });
        let command = shortcut.id.clone();
        let selector = format!("add-key-{command}");
        let add = (!capturing).then(|| {
            icon_button(
                SharedString::from(selector.clone()),
                IconName::Plus,
                style.text_muted,
                style,
            )
            .debug_selector(|| selector)
            .tooltip(Tooltip::new("Add a shortcut", None).builder())
            .when(focused, |add| {
                add.bg(style.control_background).shadow(vec![style.focus()])
            })
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                view.start_capture(&command, window, cx)
            }))
        });
        div()
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap(style.control_gap)
            .children(reset)
            .children(caps)
            .children(unbound)
            .children(waiting)
            .children(add)
            .into_any_element()
    }

    /// The ringed box a chord is pressed into, with a cancel button.
    pub(super) fn capture_box(&self, prompt: &str, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let cancel = small_icon_button("cancel-capture", IconName::X, style)
            .debug_selector(|| "cancel-capture".to_string())
            .tooltip(Tooltip::new("Stop waiting for keys", None).builder())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.cancel_capture(cx)
            }));
        capture_field(prompt.to_string(), cancel, style).into_any_element()
    }
}

fn number_text(value: &Value) -> String {
    match value {
        // 720.0 reads as 720.
        Value::Number(number) if number.as_f64().is_some_and(|n| n.fract() == 0.) => {
            format!("{}", number.as_f64().unwrap_or_default() as i64)
        }
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}
