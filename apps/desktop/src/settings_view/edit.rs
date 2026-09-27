//! Changing values: settings go to `.editor/settings.toml` and theme
//! tokens to `.editor/theme.toml`, each reported as it's written.

use editor_config::schema::SettingKind;
use editor_config::theme::{Theme as Tokens, TokenValue};
use gpui::{Context, Entity, Window};
use serde_json::Value;

use super::config_files::{self, default_token};
use super::model::{ACCENT_TOKEN, FontSlot, SettingItem, minimum_for, theme_number};
use super::store;
use super::view::{ControlRow, SettingsEvent, SettingsView, add_field_key, theme_key};
use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::{ACCENT_CHOICES, parse_color};

impl SettingsView {
    /// Writes a value (or removes the key, for `None`) and reports it.
    pub(super) fn write(
        &mut self,
        item: &SettingItem,
        value: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        if let Some(number) = theme_number(&item.key) {
            let value = value
                .and_then(|value| value.as_f64())
                .map(|n| number.clamp(n));
            self.write_token_number(number.token, value, cx);
            return;
        }
        match store::write_setting(&self.vault_root, &item.key, value.as_ref(), &item.default) {
            Ok(file) => {
                self.file = file;
                self.invalidate_layouts();
                self.error = None;
                let changed = self.setting_key_of(&item.key);
                cx.emit(SettingsEvent::Changed(changed));
            }
            Err(message) => self.error = Some((item.key.clone(), message)),
        }
        cx.notify();
    }

    /// The setting a key belongs to: a map entry reports its map.
    fn setting_key_of(&self, key: &str) -> String {
        match self.item_for(key) {
            Some(_) => key.to_string(),
            None => key.rsplit_once('.').map_or(key, |(map, _)| map).to_string(),
        }
    }

    pub(super) fn current_value(&self, item: &SettingItem) -> Value {
        if let Some(number) = theme_number(&item.key) {
            return self
                .tokens
                .get(number.token)
                .and_then(TokenValue::as_f64)
                .map_or_else(|| item.default.clone(), Value::from);
        }
        self.file
            .get(&item.key)
            .unwrap_or_else(|| item.default.clone())
    }

    pub(super) fn is_changed(&self, item: &SettingItem) -> bool {
        if theme_number(&item.key).is_some() {
            return self.current_value(item) != item.default;
        }
        self.file.get(&item.key).is_some()
    }

    /// Resets a setting to its default by removing it from the file.
    pub fn reset(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(item) = self
            .rows()
            .iter()
            .filter_map(ControlRow::item)
            .find(|i| i.key == key)
            .cloned()
        else {
            return;
        };
        self.write(&item, None, cx);
        self.sync_fields(cx);
    }

    /// Whether `item` can't apply because a switch it needs is off.
    pub(super) fn is_inactive(&self, item: &SettingItem) -> bool {
        super::model::required_switch(&item.key)
            .and_then(|switch| self.item_for(switch))
            .is_some_and(|switch| self.current_value(switch) == Value::Bool(false))
    }

    pub(super) fn toggle(&mut self, item: &SettingItem, cx: &mut Context<Self>) {
        if self.is_inactive(item) {
            return;
        }
        let on = self.current_value(item).as_bool().unwrap_or(false);
        self.write(item, Some(Value::Bool(!on)), cx);
    }

    /// Moves a choice by `delta` options, stopping at the ends.
    pub(super) fn step_choice(&mut self, item: &SettingItem, delta: isize, cx: &mut Context<Self>) {
        let SettingKind::Choice(options) = &item.kind else {
            return;
        };
        let current = self.current_value(item);
        let index = options
            .iter()
            .position(|o| Some(o.as_str()) == current.as_str());
        let last = options.len() as isize - 1;
        let next = index.map_or(0, |index| (index as isize + delta).clamp(0, last)) as usize;
        if Some(next) != index {
            self.write(item, Some(Value::from(options[next].clone())), cx);
        }
    }

    pub(super) fn choose(&mut self, item: &SettingItem, option: &str, cx: &mut Context<Self>) {
        if self.current_value(item).as_str() != Some(option) {
            self.write(item, Some(Value::from(option)), cx);
        }
    }

    /// Steps a number by one unit, never below the setting's minimum.
    pub(super) fn step_number(
        &mut self,
        item: &SettingItem,
        direction: i64,
        cx: &mut Context<Self>,
    ) {
        let current = self.current_value(item);
        if let Some(number) = theme_number(&item.key) {
            let next = number.stepped(current.as_f64().unwrap_or(number.min), direction);
            self.write(item, Some(Value::from(next)), cx);
            return;
        }
        let minimum = minimum_for(&item.key);
        let value = match item.kind {
            SettingKind::Integer => {
                Value::from((current.as_i64().unwrap_or(0) + direction).max(minimum))
            }
            _ => {
                let next = current.as_f64().unwrap_or(0.0) + direction as f64 * 0.5;
                Value::from(next.max(minimum as f64))
            }
        };
        if value != current {
            self.write(item, Some(value), cx);
        }
    }

    /// Adds typed digits to the number being edited.
    pub(super) fn type_number(&mut self, item: &SettingItem, text: &str, cx: &mut Context<Self>) {
        let buffer = match self.number_edit.take() {
            Some((key, buffer)) if key == item.key => buffer + text,
            _ => text.to_string(),
        };
        self.number_edit = Some((item.key.clone(), buffer));
        cx.notify();
    }

    pub(super) fn backspace_number(&mut self, cx: &mut Context<Self>) {
        if let Some((_, buffer)) = self.number_edit.as_mut() {
            buffer.pop();
        }
        cx.notify();
    }

    /// Writes the number being typed, if any.
    pub(super) fn commit_number(&mut self, cx: &mut Context<Self>) {
        let Some((key, buffer)) = self.number_edit.take() else {
            return;
        };
        let Some(item) = self.item_for(&key).cloned() else {
            return;
        };
        let minimum = minimum_for(&key);
        let value = match item.kind {
            SettingKind::Integer => buffer
                .parse::<i64>()
                .ok()
                .map(|n| Value::from(n.max(minimum))),
            _ => buffer
                .parse::<f64>()
                .ok()
                .map(|n| Value::from(n.max(minimum as f64))),
        };
        match value {
            Some(value) => self.write(&item, Some(value), cx),
            None if buffer.is_empty() => {}
            None => {
                self.error = Some((key, format!("“{buffer}” isn't a number.")));
                cx.notify();
            }
        }
    }

    // ---- Text fields ----

    /// Puts the files' values into the text fields.
    pub(super) fn sync_fields(&mut self, cx: &mut Context<Self>) {
        let values: Vec<(Entity<TextInput>, String)> = self
            .fields
            .iter()
            .map(|(key, field)| {
                let text = self
                    .item_for(key)
                    .map(|item| self.current_value(item))
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_default();
                (field.clone(), text)
            })
            .collect();
        for (field, text) in values {
            field.update(cx, |field, cx| field.set_text(&text, cx));
        }
        self.sync_remote_field(cx);
        let accent = self.token(ACCENT_TOKEN).unwrap_or_default();
        self.hex_field.update(cx, |field, cx| {
            field.set_text(&accent, cx);
            field.set_invalid(false, cx);
        });
    }

    pub(super) fn on_field_event(
        &mut self,
        key: &str,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submitted => {
                self.commit_field(key, cx);
                window.focus(&self.focus_handle);
            }
            TextInputEvent::Cancelled => {
                self.sync_fields(cx);
                window.focus(&self.focus_handle);
            }
            TextInputEvent::Blurred => self.commit_field(key, cx),
            TextInputEvent::Changed => {}
        }
    }

    /// Saves a text setting, or adds the typed map entry.
    fn commit_field(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(field) = self.fields.get(key) else {
            return;
        };
        let text = field.read(cx).text().trim().to_string();
        if key == super::sync_page::REMOTE_FIELD {
            self.commit_remote(&text, cx);
            return;
        }
        if let Some(list_key) = key.strip_suffix('+').filter(|k| self.is_list(k)) {
            self.add_list_entry(list_key, &text, cx);
            return;
        }
        if let Some(map_key) = key.strip_suffix('+') {
            self.add_map_entry(map_key, &text, cx);
            return;
        }
        let Some(item) = self.item_for(key).cloned() else {
            return;
        };
        if self.current_value(&item).as_str() != Some(text.as_str()) {
            self.write(&item, Some(Value::from(text)), cx);
        }
    }

    pub(super) fn add_map_entry(&mut self, map_key: &str, name: &str, cx: &mut Context<Self>) {
        let Some(map) = self.item_for(map_key).cloned() else {
            return;
        };
        let SettingKind::Map(inner) = &map.kind else {
            return;
        };
        let name = name.to_lowercase().replace(' ', "-");
        if name.is_empty() {
            return;
        }
        let first = match inner.as_ref() {
            SettingKind::Choice(options) => options.first().cloned().map(Value::from),
            SettingKind::Bool => Some(Value::Bool(true)),
            SettingKind::Text => Some(Value::from("")),
            _ => Some(Value::from(0)),
        };
        let entry = SettingItem {
            key: format!("{map_key}.{name}"),
            default: Value::Null,
            ..map.clone()
        };
        self.write(&entry, first, cx);
        if self
            .error
            .as_ref()
            .is_some_and(|(key, _)| *key == entry.key)
        {
            self.error = Some((map.key.clone(), format!("There's no “{name}” here.")));
            return;
        }
        if let Some(field) = self.fields.get(&add_field_key(map_key)) {
            field.update(cx, |field, cx| field.set_text("", cx));
        }
    }

    // ---- Theme tokens ----

    /// Writes a theme token (`None` resets it) and reports `theme.<token>`.
    pub(super) fn write_token(&mut self, token: &str, value: Option<&str>, cx: &mut Context<Self>) {
        let written = config_files::write_theme_token(&self.vault_root, token, value);
        self.take_tokens(token, written, cx);
        self.sync_fields(cx);
    }

    /// Writes a number token (`None` resets it) and reports
    /// `theme.<token>`.
    fn write_token_number(&mut self, token: &str, value: Option<f64>, cx: &mut Context<Self>) {
        let written = config_files::write_theme_number(&self.vault_root, token, value);
        self.take_tokens(token, written, cx);
    }

    /// Uses the theme a token write produced, or keeps its error, and
    /// reports `theme.<token>`.
    fn take_tokens(
        &mut self,
        token: &str,
        written: Result<Tokens, String>,
        cx: &mut Context<Self>,
    ) {
        let key = theme_key(token);
        match written {
            Ok(tokens) => {
                self.tokens = tokens;
                self.error = None;
                self.restyle();
                cx.emit(SettingsEvent::Changed(key));
            }
            Err(message) => self.error = Some((key, message)),
        }
        cx.notify();
    }

    /// Whether a token differs from the built-in theme.
    pub(super) fn is_token_changed(&self, token: &str) -> bool {
        let current = self.token(token);
        current.is_some()
            && !current
                .zip(default_token(token))
                .is_some_and(|(now, default)| now.eq_ignore_ascii_case(&default))
    }

    pub(super) fn set_font(&mut self, slot: FontSlot, family: &str, cx: &mut Context<Self>) {
        if self.token(slot.token()).as_deref() != Some(family) {
            self.write_token(slot.token(), Some(family), cx);
        }
    }

    /// Picks the next or previous accent swatch.
    pub(super) fn step_accent(&mut self, delta: isize, cx: &mut Context<Self>) {
        let current = self.token(ACCENT_TOKEN).unwrap_or_default();
        let index = ACCENT_CHOICES
            .iter()
            .position(|choice| choice.eq_ignore_ascii_case(&current));
        let last = ACCENT_CHOICES.len() as isize - 1;
        let next = index.map_or(0, |index| (index as isize + delta).clamp(0, last)) as usize;
        if Some(next) != index {
            self.write_token(ACCENT_TOKEN, Some(ACCENT_CHOICES[next]), cx);
        }
    }

    pub(super) fn on_hex_event(
        &mut self,
        _: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submitted => {
                self.commit_hex(cx);
                window.focus(&self.focus_handle);
            }
            TextInputEvent::Cancelled => {
                self.sync_fields(cx);
                window.focus(&self.focus_handle);
            }
            TextInputEvent::Blurred => self.commit_hex(cx),
            TextInputEvent::Changed => {
                let valid = parse_color(self.hex_field.read(cx).text()).is_some();
                self.hex_field
                    .update(cx, |field, cx| field.set_invalid(!valid, cx));
            }
        }
    }

    fn commit_hex(&mut self, cx: &mut Context<Self>) {
        let text = self.hex_field.read(cx).text().trim().to_string();
        let current = self.token(ACCENT_TOKEN).unwrap_or_default();
        if text.eq_ignore_ascii_case(&current) {
            return;
        }
        if parse_color(&text).is_none() {
            self.error = Some((
                theme_key(ACCENT_TOKEN),
                format!("“{text}” isn't a colour. Try one like #2f5fd0."),
            ));
            cx.notify();
            return;
        }
        self.write_token(ACCENT_TOKEN, Some(&text), cx);
    }
}
