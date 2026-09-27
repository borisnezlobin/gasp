//! The settings screen's state: which section is showing, which control
//! has focus, and writing each change to the vault's settings file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use editor_config::schema::SettingKind;
use editor_config::{ConfigLoader, Platform, RuleSet};
use gpui::{
    App, AppContext, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    ScrollHandle, Subscription, Window,
};
use serde_json::Value;

use super::model::{Section, SettingItem, ShortcutRow, humanize, settings_sections, shortcut_rows};
use super::store::{self, SettingsFile, settings_path};
use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::PanelTheme;

/// What the settings screen tells its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsEvent {
    /// A setting was written to `.editor/settings.toml`. The key is dotted,
    /// such as `files.trash`.
    Changed(String),
}

/// Where keyboard focus is on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsFocus {
    Search,
    Sections,
    /// A row in the right pane, by index.
    Control(usize),
}

/// A section in the left list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionRef {
    Settings(usize),
    Shortcuts,
}

/// One row in the right pane.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlRow {
    Setting(SettingItem),
    /// The field that adds an entry to a map setting, under its title.
    MapAdd(SettingItem),
    /// One entry of a map setting, such as a per-syntax override.
    MapEntry {
        map: SettingItem,
        item: SettingItem,
    },
    Shortcut(ShortcutRow),
}

impl ControlRow {
    /// The setting this row edits.
    pub fn item(&self) -> Option<&SettingItem> {
        match self {
            ControlRow::Setting(item) | ControlRow::MapAdd(item) => Some(item),
            ControlRow::MapEntry { item, .. } => Some(item),
            ControlRow::Shortcut(_) => None,
        }
    }

    /// Whether the row is edited through a text field.
    pub fn uses_field(&self) -> bool {
        match self {
            ControlRow::Setting(item) => item.kind == SettingKind::Text,
            ControlRow::MapAdd(_) => true,
            _ => false,
        }
    }
}

/// The settings screen for one vault. It fills whatever it's placed in,
/// so it works as a tab or inside a modal.
pub struct SettingsView {
    pub(super) focus_handle: FocusHandle,
    pub(super) vault_root: PathBuf,
    pub(super) theme: PanelTheme,
    pub(super) sections: Vec<Section>,
    pub(super) shortcuts: Vec<ShortcutRow>,
    pub(super) file: SettingsFile,
    pub(super) search: Entity<TextInput>,
    pub(super) query: String,
    pub(super) current: usize,
    pub(super) focus: SettingsFocus,
    pub(super) fields: HashMap<String, Entity<TextInput>>,
    pub(super) number_edit: Option<(String, String)>,
    pub(super) error: Option<(String, String)>,
    pub(super) scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsEvent> for SettingsView {}
impl EventEmitter<DismissEvent> for SettingsView {}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The key a map setting's "add" field is stored under.
fn add_field_key(map_key: &str) -> String {
    format!("{map_key}+")
}

impl SettingsView {
    /// A settings screen for the vault at `vault_root`, with the vault's
    /// own key rules in the shortcuts list.
    pub fn new(
        vault_root: impl Into<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let vault_root = vault_root.into();
        let mut loader = ConfigLoader::for_vault(&vault_root);
        loader.load_all();
        let rules = loader.config().rules.clone();
        Self::with_rules(vault_root, &rules, window, cx)
    }

    /// A settings screen that lists the shortcuts in `rules`.
    pub fn with_rules(
        vault_root: impl Into<PathBuf>,
        rules: &RuleSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let vault_root = vault_root.into();
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder("Search settings"));
        let mut subscriptions = vec![
            cx.subscribe_in(&search, window, Self::on_search_event),
            cx.on_focus(&search.focus_handle(cx), window, |view, _, cx| {
                view.focus = SettingsFocus::Search;
                cx.notify();
            }),
        ];
        let sections = settings_sections();
        let mut view = SettingsView {
            focus_handle: cx.focus_handle(),
            file: SettingsFile::load(&settings_path(&vault_root)).unwrap_or_default(),
            vault_root,
            theme: PanelTheme::default(),
            shortcuts: shortcut_rows(rules, Platform::current()),
            search,
            query: String::new(),
            current: 0,
            focus: SettingsFocus::Sections,
            fields: HashMap::new(),
            number_edit: None,
            error: None,
            scroll: ScrollHandle::new(),
            sections,
            _subscriptions: Vec::new(),
        };
        subscriptions.extend(view.build_fields(window, cx));
        view._subscriptions = subscriptions;
        view
    }

    /// Text fields for text settings and for adding map entries.
    fn build_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let keys: Vec<(String, bool)> = self
            .sections
            .iter()
            .flat_map(|section| &section.items)
            .filter_map(|item| match &item.kind {
                SettingKind::Text => Some((item.key.clone(), false)),
                SettingKind::Map(_) => Some((add_field_key(&item.key), true)),
                _ => None,
            })
            .collect();
        let mut subscriptions = Vec::new();
        for (key, adds) in keys {
            let placeholder = if adds { "Add by name" } else { "" };
            let field = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
            let event_key = key.clone();
            subscriptions.push(cx.subscribe_in(
                &field,
                window,
                move |view, _, event, window, cx| {
                    view.on_field_event(&event_key, event, window, cx);
                },
            ));
            let focus_key = key.clone();
            subscriptions.push(
                cx.on_focus(&field.focus_handle(cx), window, move |view, _, cx| {
                    view.focus_field_row(&focus_key, cx);
                }),
            );
            self.fields.insert(key, field);
        }
        self.sync_fields(cx);
        subscriptions
    }

    // ---- Public API ----

    pub fn vault_root(&self) -> &Path {
        &self.vault_root
    }

    /// Re-reads the settings file, such as after it changed on disk.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if let Ok(file) = SettingsFile::load(&settings_path(&self.vault_root)) {
            self.file = file;
        }
        self.sync_fields(cx);
        cx.notify();
    }

    /// Replaces the shortcuts list, such as after `rules.toml` changed.
    pub fn set_rules(&mut self, rules: &RuleSet, cx: &mut Context<Self>) {
        self.shortcuts = shortcut_rows(rules, Platform::current());
        cx.notify();
    }

    /// Focuses the search box, ready to type.
    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_focus(SettingsFocus::Search, window, cx);
    }

    /// Shows a section by id: the first part of its settings' keys, such
    /// as `files`, or [`super::model::SHORTCUTS_SECTION`]. Clears any search.
    pub fn show_section(&mut self, id: &str, cx: &mut Context<Self>) {
        self.search.update(cx, |field, cx| field.set_text("", cx));
        self.query.clear();
        let index = self
            .visible_sections()
            .iter()
            .position(|section| match section {
                SectionRef::Settings(index) => self.sections[*index].id == id,
                SectionRef::Shortcuts => id == super::model::SHORTCUTS_SECTION,
            });
        if let Some(index) = index {
            self.select_section(index, cx);
        }
    }

    /// Focuses the section list.
    pub fn focus_sections(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_focus(SettingsFocus::Sections, window, cx);
    }

    pub fn focus_state(&self) -> SettingsFocus {
        self.focus
    }

    /// The section showing on the right.
    pub fn current_section(&self) -> Option<SectionRef> {
        self.visible_sections().get(self.current).copied()
    }

    /// The value a setting has now: the file's, or else the default.
    pub fn value(&self, key: &str) -> Option<Value> {
        self.file
            .get(key)
            .or_else(|| self.item_for(key).map(|item| item.default.clone()))
    }

    /// The error from the last write, if it failed: (key, message).
    pub fn last_error(&self) -> Option<(&str, &str)> {
        self.error
            .as_ref()
            .map(|(key, message)| (key.as_str(), message.as_str()))
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    // ---- Sections and rows ----

    fn item_for(&self, key: &str) -> Option<&SettingItem> {
        self.sections
            .iter()
            .flat_map(|section| &section.items)
            .find(|item| item.key == key)
    }

    /// Sections with at least one row matching the search, then shortcuts.
    pub fn visible_sections(&self) -> Vec<SectionRef> {
        let mut visible: Vec<SectionRef> = (0..self.sections.len())
            .map(SectionRef::Settings)
            .filter(|section| !self.rows_for(*section).is_empty())
            .collect();
        if !self.rows_for(SectionRef::Shortcuts).is_empty() {
            visible.push(SectionRef::Shortcuts);
        }
        visible
    }

    pub fn section_title(&self, section: SectionRef) -> String {
        match section {
            SectionRef::Settings(index) => self.sections[index].title.clone(),
            SectionRef::Shortcuts => super::model::shortcuts_title().to_string(),
        }
    }

    /// The rows the right pane shows for the current section and search.
    pub fn rows(&self) -> Vec<ControlRow> {
        self.current_section()
            .map(|section| self.rows_for(section))
            .unwrap_or_default()
    }

    fn rows_for(&self, section: SectionRef) -> Vec<ControlRow> {
        let query = self.query.trim();
        match section {
            SectionRef::Shortcuts => self
                .shortcuts
                .iter()
                .filter(|row| row.matches(query))
                .cloned()
                .map(ControlRow::Shortcut)
                .collect(),
            SectionRef::Settings(index) => self.sections[index]
                .items
                .iter()
                .filter(|item| item.matches(query))
                .flat_map(|item| self.rows_for_item(item))
                .collect(),
        }
    }

    fn rows_for_item(&self, item: &SettingItem) -> Vec<ControlRow> {
        let SettingKind::Map(inner) = &item.kind else {
            return vec![ControlRow::Setting(item.clone())];
        };
        let entries =
            self.file
                .entries(&item.key)
                .into_iter()
                .map(|(name, _)| ControlRow::MapEntry {
                    map: item.clone(),
                    item: SettingItem {
                        key: format!("{}.{name}", item.key),
                        title: humanize(&name),
                        description: String::new(),
                        kind: (**inner).clone(),
                        default: Value::Null,
                    },
                });
        std::iter::once(ControlRow::MapAdd(item.clone()))
            .chain(entries)
            .collect()
    }

    // ---- Focus ----

    /// Moves focus and gives keyboard input to the right element.
    pub(super) fn set_focus(
        &mut self,
        focus: SettingsFocus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_number(cx);
        self.focus = focus;
        match focus {
            SettingsFocus::Search => window.focus(&self.search.focus_handle(cx)),
            SettingsFocus::Control(index) => {
                self.scroll.scroll_to_item(index + 1);
                match self.rows().get(index).and_then(|row| self.field_for(row)) {
                    Some(field) => window.focus(&field.focus_handle(cx)),
                    None => window.focus(&self.focus_handle),
                }
            }
            SettingsFocus::Sections => window.focus(&self.focus_handle),
        }
        cx.notify();
    }

    pub(super) fn field_for(&self, row: &ControlRow) -> Option<Entity<TextInput>> {
        let key = match row {
            ControlRow::Setting(item) if item.kind == SettingKind::Text => item.key.clone(),
            ControlRow::MapAdd(item) => add_field_key(&item.key),
            _ => return None,
        };
        self.fields.get(&key).cloned()
    }

    /// A text field got focus from a click: point the cursor at its row.
    fn focus_field_row(&mut self, key: &str, cx: &mut Context<Self>) {
        let index = self.rows().iter().position(|row| {
            row.item().is_some_and(|item| {
                item.key == key
                    || (add_field_key(&item.key) == key && matches!(row, ControlRow::MapAdd(_)))
            })
        });
        if let Some(index) = index {
            self.focus = SettingsFocus::Control(index);
            cx.notify();
        }
    }

    pub(super) fn select_section(&mut self, index: usize, cx: &mut Context<Self>) {
        let count = self.visible_sections().len();
        if count > 0 {
            self.current = index.min(count - 1);
            self.scroll.scroll_to_item(0);
            cx.notify();
        }
    }

    // ---- Changing values ----

    /// Writes a value (or removes the key, for `None`) and reports it.
    pub(super) fn write(
        &mut self,
        item: &SettingItem,
        value: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        match store::write_setting(&self.vault_root, &item.key, value.as_ref(), &item.default) {
            Ok(file) => {
                self.file = file;
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
        self.file
            .get(&item.key)
            .unwrap_or_else(|| item.default.clone())
    }

    pub(super) fn is_changed(&self, item: &SettingItem) -> bool {
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

    pub(super) fn toggle(&mut self, item: &SettingItem, cx: &mut Context<Self>) {
        let on = self.current_value(item).as_bool().unwrap_or(false);
        self.write(item, Some(Value::Bool(!on)), cx);
    }

    /// Moves a choice by `delta` options, stopping at the ends, or cycling
    /// when `wrap` is set.
    pub(super) fn step_choice(
        &mut self,
        item: &SettingItem,
        delta: isize,
        wrap: bool,
        cx: &mut Context<Self>,
    ) {
        let SettingKind::Choice(options) = &item.kind else {
            return;
        };
        let current = self.current_value(item);
        let index = options
            .iter()
            .position(|o| Some(o.as_str()) == current.as_str());
        let count = options.len() as isize;
        let next = match index {
            Some(index) if wrap => (index as isize + delta).rem_euclid(count),
            Some(index) => (index as isize + delta).clamp(0, count - 1),
            None => 0,
        };
        if Some(next as usize) != index {
            self.write(item, Some(Value::from(options[next as usize].clone())), cx);
        }
    }

    pub(super) fn choose(&mut self, item: &SettingItem, option: &str, cx: &mut Context<Self>) {
        self.write(item, Some(Value::from(option)), cx);
    }

    /// Steps a number by one unit. Settings whose default isn't negative
    /// stop at zero.
    pub(super) fn step_number(
        &mut self,
        item: &SettingItem,
        direction: i64,
        cx: &mut Context<Self>,
    ) {
        let current = self.current_value(item);
        let floor_at_zero = item.default.as_f64().is_some_and(|d| d >= 0.0);
        let value = match item.kind {
            SettingKind::Integer => {
                let next = current.as_i64().unwrap_or(0) + direction;
                Value::from(if floor_at_zero { next.max(0) } else { next })
            }
            _ => {
                let next = current.as_f64().unwrap_or(0.0) + direction as f64 * 0.5;
                Value::from(if floor_at_zero { next.max(0.0) } else { next })
            }
        };
        self.write(item, Some(value), cx);
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
        let value = match item.kind {
            SettingKind::Integer => buffer.parse::<i64>().ok().map(Value::from),
            _ => buffer.parse::<f64>().ok().map(Value::from),
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

    /// Puts the file's values into the text fields.
    fn sync_fields(&mut self, cx: &mut Context<Self>) {
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
    }

    fn on_field_event(
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

    fn add_map_entry(&mut self, map_key: &str, name: &str, cx: &mut Context<Self>) {
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

    fn on_search_event(
        &mut self,
        _: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Changed => {
                self.query = self.search.read(cx).text().to_string();
                self.select_section(0, cx);
            }
            TextInputEvent::Submitted if !self.rows().is_empty() => {
                self.set_focus(SettingsFocus::Control(0), window, cx);
            }
            TextInputEvent::Submitted | TextInputEvent::Blurred => {}
            TextInputEvent::Cancelled if !self.query.is_empty() => {
                self.search.update(cx, |field, cx| field.set_text("", cx));
                self.query.clear();
                self.select_section(0, cx);
            }
            TextInputEvent::Cancelled => cx.emit(DismissEvent),
        }
    }
}
