//! The dropdown menu a choice or font row opens: arrows move, Enter picks,
//! Escape closes, and a font menu filters as you type.

use std::cell::Cell;

use gasp_config::schema::SettingKind;
use gpui::{
    AppContext, Context, Entity, Focusable, ScrollStrategy, Subscription, UniformListScrollHandle,
    Window,
};

use super::config_files::default_token;
use super::model::{
    FontSlot, SettingItem, choice_label, filter_fonts, font_choices, map_name_label, map_names,
};
use super::view::{ControlRow, SettingsView};
use crate::text_input::{TextInput, TextInputEvent, TextInputStyle};

/// What a menu changes when an option is picked.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuTarget {
    Choice(SettingItem),
    Font(FontSlot),
    /// Adds an entry to a map setting whose names are a fixed list.
    MapAdd(SettingItem),
}

/// An open dropdown menu.
pub struct OpenMenu {
    /// The row that opened it.
    pub row: usize,
    pub target: MenuTarget,
    /// Every option's value.
    pub options: Vec<String>,
    /// The options the filter lets through, in order.
    pub shown: Vec<String>,
    pub highlighted: usize,
    /// The filter field, for font menus.
    pub filter: Option<Entity<TextInput>>,
    /// The options' list, which builds only the options in view.
    pub scroll: UniformListScrollHandle,
    /// How many options the frame last drawn built: only those in view,
    /// so a long font list opens and filters quickly.
    pub built: Cell<usize>,
    _subscriptions: Vec<Subscription>,
}

impl OpenMenu {
    /// How an option reads in the menu and on its button.
    pub fn label(&self, option: &str) -> String {
        match self.target {
            MenuTarget::Choice(_) => choice_label(option),
            MenuTarget::Font(_) => option.to_string(),
            MenuTarget::MapAdd(ref map) => map_name_label(&map.key, option),
        }
    }
}

impl SettingsView {
    /// The menu target for a row, if the row has a dropdown.
    pub(super) fn menu_target(row: &ControlRow) -> Option<MenuTarget> {
        match row {
            ControlRow::Font(slot) => Some(MenuTarget::Font(*slot)),
            ControlRow::Setting(item) | ControlRow::MapEntry { item, .. } => {
                matches!(item.kind, SettingKind::Choice(_))
                    .then(|| MenuTarget::Choice(item.clone()))
            }
            ControlRow::MapAdd(map) => map_names(&map.key).map(|_| MenuTarget::MapAdd(map.clone())),
            _ => None,
        }
    }

    /// The value a menu target has now.
    pub(super) fn menu_value(&self, target: &MenuTarget) -> String {
        match target {
            MenuTarget::Choice(item) => self
                .current_value(item)
                .as_str()
                .unwrap_or_default()
                .to_string(),
            MenuTarget::Font(slot) => self.token(slot.token()).unwrap_or_default(),
            MenuTarget::MapAdd(_) => String::new(),
        }
    }

    /// Whether a dropdown menu is open.
    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// The options the open menu shows, in order.
    pub fn menu_options(&self) -> Vec<String> {
        self.menu
            .as_ref()
            .map(|menu| menu.shown.clone())
            .unwrap_or_default()
    }

    /// Opens the dropdown of row `index`, with its current value
    /// highlighted.
    pub fn open_menu(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.rows().get(index).and_then(Self::menu_target) else {
            return;
        };
        if matches!(target, MenuTarget::Font(_)) {
            crate::trace::presented(window, "font-menu-open");
        }
        let current = self.menu_value(&target);
        let options = self.menu_choices(&target, &current);
        let highlighted = options.iter().position(|o| *o == current).unwrap_or(0);
        let (filter, subscriptions) = match target {
            MenuTarget::Font(_) => self.menu_filter(window, cx),
            MenuTarget::Choice(_) | MenuTarget::MapAdd(_) => (None, Vec::new()),
        };
        self.menu = Some(OpenMenu {
            row: index,
            target,
            shown: options.clone(),
            options,
            highlighted,
            filter,
            scroll: UniformListScrollHandle::new(),
            built: Cell::new(0),
            _subscriptions: subscriptions,
        });
        self.scroll_menu(ScrollStrategy::Center);
        cx.notify();
    }

    /// Every option a menu offers, given the value it has now.
    fn menu_choices(&self, target: &MenuTarget, current: &str) -> Vec<String> {
        match target {
            MenuTarget::Choice(item) => match &item.kind {
                SettingKind::Choice(options) => options.clone(),
                _ => Vec::new(),
            },
            MenuTarget::Font(slot) => {
                let built_in = default_token(slot.token()).unwrap_or_default();
                let names = self.font_names.as_deref().unwrap_or_default();
                font_choices(names, current, &built_in)
            }
            MenuTarget::MapAdd(map) => self.names_to_add(&map.key),
        }
    }

    /// How many options the open menu built in the frame last drawn.
    pub fn menu_rows_built(&self) -> usize {
        self.menu.as_ref().map_or(0, |menu| menu.built.get())
    }

    /// Whether the open menu is a font menu still waiting for the fonts.
    pub fn menu_loading(&self) -> bool {
        self.font_names.is_none()
            && self
                .menu
                .as_ref()
                .is_some_and(|menu| matches!(menu.target, MenuTarget::Font(_)))
    }

    /// Fills an open font menu with the font names, as when they arrive
    /// after it opened, keeping its filter and highlighted option.
    pub(super) fn refill_font_menu(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.menu.as_ref().map(|menu| menu.target.clone()) else {
            return;
        };
        if !matches!(target, MenuTarget::Font(_)) {
            return;
        }
        let options = self.menu_choices(&target, &self.menu_value(&target));
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        let query = menu
            .filter
            .as_ref()
            .map(|field| field.read(cx).text().to_string())
            .unwrap_or_default();
        let highlighted = menu.shown.get(menu.highlighted).cloned();
        menu.shown = filter_fonts(&options, &query);
        menu.options = options;
        menu.highlighted = highlighted
            .and_then(|option| menu.shown.iter().position(|shown| *shown == option))
            .unwrap_or(0);
        self.scroll_menu(ScrollStrategy::Center);
    }

    /// The fixed names of map setting `map_key` it doesn't have yet.
    fn names_to_add(&self, map_key: &str) -> Vec<String> {
        let taken: Vec<String> = self
            .file
            .entries(map_key)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        map_names(map_key)
            .unwrap_or_default()
            .iter()
            .map(|(name, _)| (*name).to_string())
            .filter(|name| !taken.contains(name))
            .collect()
    }

    fn menu_filter(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<Entity<TextInput>>, Vec<Subscription>) {
        let field = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Find a font")
                .with_style(TextInputStyle::Query)
        });
        window.focus(&field.focus_handle(cx));
        let subscription = cx.subscribe_in(&field, window, Self::on_menu_filter_event);
        (Some(field), vec![subscription])
    }

    fn on_menu_filter_event(
        &mut self,
        field: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Changed => {
                let query = field.read(cx).text().to_string();
                if let Some(menu) = self.menu.as_mut() {
                    menu.shown = filter_fonts(&menu.options, &query);
                    menu.highlighted = 0;
                }
                self.scroll_menu(ScrollStrategy::Top);
                cx.notify();
            }
            TextInputEvent::Submitted => self.pick_highlighted(window, cx),
            TextInputEvent::Cancelled => self.close_menu(window, cx),
            TextInputEvent::Blurred => {}
        }
    }

    /// Closes the menu and gives the keyboard back to its row.
    pub fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    pub(super) fn move_menu(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        let last = menu.shown.len().saturating_sub(1) as isize;
        menu.highlighted = (menu.highlighted as isize + delta).clamp(0, last) as usize;
        // Moving down brings the option in at the bottom edge, and up at
        // the top, so the list scrolls no further than it must.
        let edge = if delta > 0 {
            ScrollStrategy::Bottom
        } else {
            ScrollStrategy::Top
        };
        self.scroll_menu(edge);
        cx.notify();
    }

    /// Scrolls the highlighted option into view, placed by `strategy`
    /// when it's out of view.
    fn scroll_menu(&self, strategy: ScrollStrategy) {
        if let Some(menu) = &self.menu {
            menu.scroll.scroll_to_item(menu.highlighted, strategy);
        }
    }

    pub(super) fn pick_highlighted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(option) = self
            .menu
            .as_ref()
            .and_then(|menu| menu.shown.get(menu.highlighted).cloned())
        else {
            return;
        };
        self.pick(&option, window, cx);
    }

    /// Applies an option and closes the menu.
    pub(super) fn pick(&mut self, option: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        window.focus(&self.focus_handle);
        match &menu.target {
            MenuTarget::Choice(item) => self.choose(item, option, cx),
            MenuTarget::Font(slot) => self.set_font(*slot, option, cx),
            MenuTarget::MapAdd(map) => self.add_map_entry(&map.key, option, cx),
        }
        cx.notify();
    }

    /// Keys while a menu is open. Returns whether the key was used.
    pub(super) fn menu_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "up" => self.move_menu(-1, cx),
            "down" => self.move_menu(1, cx),
            "pageup" => self.move_menu(-8, cx),
            "pagedown" => self.move_menu(8, cx),
            "enter" | "space" if self.menu_has_no_filter() => self.pick_highlighted(window, cx),
            "escape" | "tab" => self.close_menu(window, cx),
            _ => return false,
        }
        true
    }

    fn menu_has_no_filter(&self) -> bool {
        self.menu.as_ref().is_some_and(|menu| menu.filter.is_none())
    }
}
