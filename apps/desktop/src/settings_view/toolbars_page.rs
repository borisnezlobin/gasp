//! The Toolbars page: every toolbar in the vault's `toolbars.toml` (the
//! built-in status bar, selection bar and iPhone keyboard bar, and any the
//! vault adds), each on its own card with where it sits, when it shows,
//! how its buttons read and what's on it. Items are added from a
//! searchable picker, reordered by dragging or with Alt and an arrow, and
//! removed with Delete. Changes go straight to `toolbars.toml` and are
//! reported as `toolbars`.

use gasp_config::commands::{BUILTIN_COMMANDS, command_spec};
use gasp_config::toolbar_files;
use gasp_config::toolbars::{
    Behaviour, ButtonStyle, Density, MENU_PREFIX, Place, SEPARATOR, SPACER, Toolbar,
    ToolbarContext, ToolbarItem, Toolbars, Widget, choice_name,
};
use gpui::{Context, Window};
use serde_json::Value;

use super::model::words_match;
use super::view::{ControlRow, PaneLayout, SettingsEvent, SettingsFocus, SettingsView};
use crate::icons::IconName;

/// The key toolbar changes are reported and their errors kept under.
pub const TOOLBARS_KEY: &str = "toolbars";

/// One of a toolbar's choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolbarField {
    Place,
    Behaviour,
    Style,
    Density,
}

impl ToolbarField {
    pub const ALL: [ToolbarField; 4] = [
        ToolbarField::Place,
        ToolbarField::Behaviour,
        ToolbarField::Style,
        ToolbarField::Density,
    ];

    /// The field's name in `toolbars.toml`.
    pub fn key(self) -> &'static str {
        match self {
            ToolbarField::Place => "place",
            ToolbarField::Behaviour => "behaviour",
            ToolbarField::Style => "style",
            ToolbarField::Density => "density",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            ToolbarField::Place => "Place",
            ToolbarField::Behaviour => "When it shows",
            ToolbarField::Style => "Buttons show",
            ToolbarField::Density => "Size",
        }
    }

    /// Every value the field takes, as the file spells them.
    pub fn options(self) -> Vec<String> {
        match self {
            ToolbarField::Place => Place::ALL.map(choice_name).to_vec(),
            ToolbarField::Behaviour => Behaviour::ALL.map(choice_name).to_vec(),
            ToolbarField::Style => ButtonStyle::ALL.map(choice_name).to_vec(),
            ToolbarField::Density => Density::ALL.map(choice_name).to_vec(),
        }
    }

    /// The value `toolbar` has now.
    pub fn value(self, toolbar: &Toolbar) -> String {
        match self {
            ToolbarField::Place => choice_name(toolbar.place),
            ToolbarField::Behaviour => choice_name(toolbar.behaviour),
            ToolbarField::Style => choice_name(toolbar.style),
            ToolbarField::Density => choice_name(toolbar.density),
        }
    }

    /// Whether the field is a dropdown; the rest are segmented.
    pub fn is_dropdown(self) -> bool {
        matches!(self, ToolbarField::Place | ToolbarField::Behaviour)
    }
}

/// How a toolbar choice reads.
const TOOLBAR_LABELS: &[(&str, &str)] = &[
    ("status-bar", "Status bar"),
    ("editor-top", "Above the notes"),
    ("editor-bottom", "Below the notes"),
    ("window-left", "Left side of the window"),
    ("window-right", "Right side of the window"),
    ("selection", "By selected text"),
    ("cursor-line", "At the cursor’s line"),
    ("keyboard", "Above the iPhone keyboard"),
    ("always", "Always"),
    ("on-hover", "When the pointer comes near"),
    ("hide-while-typing", "Hidden while typing"),
    ("with-selection", "With a selection"),
    ("in-context", "In some kinds of text"),
    ("icons", "Icons"),
    ("icons-and-labels", "Icons and labels"),
    ("labels", "Labels"),
    ("compact", "Compact"),
    ("comfortable", "Comfortable"),
    ("text", "Text"),
    ("math", "Math"),
    ("code", "Code"),
    ("table", "Tables"),
];

pub fn toolbar_choice_label(value: &str) -> String {
    TOOLBAR_LABELS
        .iter()
        .find(|(known, _)| *known == value)
        .map_or_else(|| value.to_owned(), |(_, label)| (*label).to_owned())
}

/// Where a toolbar sits and when it shows, as a sentence.
fn toolbar_summary(toolbar: &Toolbar) -> String {
    if !toolbar.enabled {
        return "Turned off.".to_owned();
    }
    let place = match toolbar.place {
        Place::StatusBar => "In the status bar",
        Place::EditorTop => "Above the notes",
        Place::EditorBottom => "Below the notes",
        Place::WindowLeft => "Down the left side",
        Place::WindowRight => "Down the right side",
        Place::Selection => "By selected text",
        Place::CursorLine => "At the end of the cursor’s line",
        Place::Keyboard => "Above the iPhone’s keyboard",
    };
    let when = match toolbar.behaviour {
        Behaviour::Always => "always",
        Behaviour::OnHover => "when the pointer comes near",
        Behaviour::HideWhileTyping => "out of the way while you type",
        Behaviour::WithSelection => "while text is selected",
        Behaviour::InContext => "in the kinds of text below",
    };
    format!("{place}, {when}.")
}

/// What an item is called on its row and in the picker.
pub fn item_title(item: &ToolbarItem, toolbars: &Toolbars) -> String {
    match item {
        ToolbarItem::Command(id) => {
            command_spec(id).map_or_else(|| id.clone(), |spec| spec.title.to_owned())
        }
        ToolbarItem::Widget(widget) => widget.title().to_owned(),
        ToolbarItem::Separator => "Separator".to_owned(),
        ToolbarItem::Spacer => "Flexible space".to_owned(),
        ToolbarItem::Menu(id) => toolbars
            .menu(id)
            .map_or_else(|| id.clone(), |menu| format!("{} menu", menu.title)),
    }
}

/// An item's icon on its row and in the picker.
pub fn item_icon(item: &ToolbarItem, toolbars: &Toolbars) -> IconName {
    match item {
        ToolbarItem::Command(id) => IconName::for_command(id),
        ToolbarItem::Widget(widget) => IconName::from_name(widget.icon()).unwrap_or(IconName::Info),
        ToolbarItem::Separator => IconName::DotsSixVertical,
        ToolbarItem::Spacer => IconName::ArrowsInLineHorizontal,
        ToolbarItem::Menu(id) => toolbars
            .menu(id)
            .and_then(|menu| IconName::from_name(&menu.icon))
            .unwrap_or(IconName::DotsThree),
    }
}

/// A short line under an item's title, for the things that aren't commands.
pub fn item_description(item: &ToolbarItem) -> Option<&'static str> {
    match item {
        ToolbarItem::Separator => Some("A thin line between groups."),
        ToolbarItem::Spacer => Some("Pushes what follows to the far end."),
        ToolbarItem::Widget(Widget::Sync) => {
            Some("Shows how syncing is going; click it for details.")
        }
        ToolbarItem::Widget(_) => Some("Text about the note you’re in."),
        _ => None,
    }
}

impl SettingsView {
    // ---- Rows ----

    /// The page's cards: one per toolbar, then adding a toolbar and
    /// putting them all back.
    pub(super) fn toolbar_cards(&self, query: &str, layout: &mut PaneLayout) {
        for toolbar in &self.toolbars.toolbars {
            if query.is_empty() || self.toolbar_matches(toolbar, query) {
                layout.push_card(None, self.toolbar_rows(toolbar));
            }
        }
        let tail = [ControlRow::NewToolbar, ControlRow::ResetToolbars]
            .into_iter()
            .filter(|row| query.is_empty() || words_match(&row.title(), query))
            .collect();
        layout.push_card(None, tail);
    }

    fn toolbar_matches(&self, toolbar: &Toolbar, query: &str) -> bool {
        let items: Vec<String> = toolbar
            .items
            .iter()
            .map(|item| item_title(item, &self.toolbars))
            .collect();
        words_match(&format!("{} {}", toolbar.title, items.join(" ")), query)
    }

    /// A toolbar's rows: just its header while it's off.
    fn toolbar_rows(&self, toolbar: &Toolbar) -> Vec<ControlRow> {
        let id = toolbar.id.clone();
        let mut rows = vec![ControlRow::ToolbarHeader(id.clone())];
        if !toolbar.enabled {
            return rows;
        }
        rows.extend(
            ToolbarField::ALL
                .into_iter()
                .map(|field| ControlRow::ToolbarField {
                    toolbar: id.clone(),
                    field,
                }),
        );
        if toolbar.behaviour == Behaviour::InContext {
            rows.push(ControlRow::ToolbarContexts(id.clone()));
        }
        rows.extend(
            (0..toolbar.items.len()).map(|index| ControlRow::ToolbarItem {
                toolbar: id.clone(),
                index,
            }),
        );
        rows.push(ControlRow::ToolbarAdd(id));
        rows
    }

    pub(super) fn toolbar(&self, id: &str) -> Option<&Toolbar> {
        self.toolbars.get(id)
    }

    /// The title of a row on the Toolbars page.
    pub(super) fn toolbar_row_title(&self, row: &ControlRow) -> String {
        match row {
            ControlRow::ToolbarHeader(id) => self
                .toolbar(id)
                .map_or_else(|| id.clone(), |toolbar| toolbar.title.clone()),
            ControlRow::ToolbarField { field, .. } => field.title().to_owned(),
            ControlRow::ToolbarContexts(_) => "Kinds of text".to_owned(),
            ControlRow::ToolbarItem { toolbar, index } => self
                .toolbar(toolbar)
                .and_then(|toolbar| toolbar.items.get(*index))
                .map_or_else(String::new, |item| item_title(item, &self.toolbars)),
            ControlRow::ToolbarAdd(_) => "Add a button".to_owned(),
            ControlRow::NewToolbar => "Add a toolbar".to_owned(),
            ControlRow::ResetToolbars => "Put every toolbar back".to_owned(),
            _ => String::new(),
        }
    }

    /// The muted line under a row's title on the Toolbars page.
    pub(super) fn toolbar_row_description(&self, row: &ControlRow) -> String {
        match row {
            ControlRow::ToolbarHeader(id) => {
                self.toolbar(id).map(toolbar_summary).unwrap_or_default()
            }
            ControlRow::ToolbarContexts(_) => {
                "It shows while the cursor is in one of these.".to_owned()
            }
            ControlRow::ToolbarItem { toolbar, index } => self
                .toolbar(toolbar)
                .and_then(|toolbar| toolbar.items.get(*index))
                .and_then(item_description)
                .unwrap_or_default()
                .to_owned(),
            ControlRow::ToolbarAdd(_) => {
                "Any command, a status widget, a separator or a menu.".to_owned()
            }
            ControlRow::NewToolbar => "Starts empty, above the notes.".to_owned(),
            ControlRow::ResetToolbars => {
                "Back to the status bar, the selection bar and the keyboard bar as they came."
                    .to_owned()
            }
            _ => String::new(),
        }
    }

    // ---- Writing ----

    /// Applies a write's outcome: the toolbars now in effect, or why it
    /// failed, under `key`.
    fn after_toolbar_write(
        &mut self,
        result: Result<Toolbars, String>,
        key: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let written = result.is_ok();
        match result {
            Ok(toolbars) => {
                self.toolbars = toolbars;
                self.error = None;
                cx.emit(SettingsEvent::Changed(TOOLBARS_KEY.to_owned()));
            }
            Err(message) => self.error = Some((key.to_owned(), message)),
        }
        self.invalidate_layouts();
        cx.notify();
        written
    }

    pub(super) fn set_toolbar_field(
        &mut self,
        id: &str,
        field: &str,
        value: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        let result = toolbar_files::set_toolbar_field(&self.vault_root, id, field, value.as_ref());
        self.after_toolbar_write(result, &toolbar_error_key(id), cx);
    }

    pub(super) fn set_toolbar_items(
        &mut self,
        id: &str,
        items: Vec<ToolbarItem>,
        cx: &mut Context<Self>,
    ) {
        let result = toolbar_files::set_toolbar_items(&self.vault_root, id, &items);
        self.after_toolbar_write(result, &toolbar_error_key(id), cx);
    }

    pub(super) fn toggle_toolbar(&mut self, id: &str, cx: &mut Context<Self>) {
        let on = self.toolbar(id).is_some_and(|toolbar| toolbar.enabled);
        self.set_toolbar_field(id, "enabled", Some(Value::Bool(!on)), cx);
    }

    /// Moves a choice by `delta` options, stopping at the ends.
    pub(super) fn step_toolbar_field(
        &mut self,
        id: &str,
        field: ToolbarField,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(toolbar) = self.toolbar(id) else {
            return;
        };
        let options = field.options();
        let current = field.value(toolbar);
        let index = options
            .iter()
            .position(|option| *option == current)
            .unwrap_or(0);
        let next = (index as isize + delta).clamp(0, options.len() as isize - 1) as usize;
        if next != index {
            self.set_toolbar_field(
                id,
                field.key(),
                Some(Value::from(options[next].clone())),
                cx,
            );
        }
    }

    pub(super) fn choose_toolbar_field(
        &mut self,
        id: &str,
        field: ToolbarField,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        let unchanged = self
            .toolbar(id)
            .is_some_and(|toolbar| field.value(toolbar) == value);
        if !unchanged {
            self.set_toolbar_field(id, field.key(), Some(Value::from(value)), cx);
        }
    }

    /// Turns one kind of text on or off for a bar shown in context.
    pub(super) fn toggle_toolbar_context(
        &mut self,
        id: &str,
        context: ToolbarContext,
        cx: &mut Context<Self>,
    ) {
        let Some(toolbar) = self.toolbar(id) else {
            return;
        };
        let mut contexts = toolbar.contexts.clone();
        match contexts.iter().position(|known| *known == context) {
            Some(at) => {
                contexts.remove(at);
            }
            None => contexts.push(context),
        }
        let names: Vec<String> = ToolbarContext::ALL
            .into_iter()
            .filter(|known| contexts.contains(known))
            .map(choice_name)
            .collect();
        self.set_toolbar_field(id, "contexts", Some(Value::from(names)), cx);
    }

    fn items_of(&self, id: &str) -> Vec<ToolbarItem> {
        self.toolbar(id)
            .map(|toolbar| toolbar.items.clone())
            .unwrap_or_default()
    }

    /// Adds `item` to the end of toolbar `id`, then puts the keyboard on
    /// the add row again, ready for another.
    pub(super) fn add_toolbar_item(&mut self, id: &str, item: &str, cx: &mut Context<Self>) {
        let mut items = self.items_of(id);
        items.push(ToolbarItem::parse(item));
        self.set_toolbar_items(id, items, cx);
        let target = ControlRow::ToolbarAdd(id.to_owned());
        self.focus_row_where(|row| *row == target, cx);
    }

    pub(super) fn remove_toolbar_item(&mut self, id: &str, index: usize, cx: &mut Context<Self>) {
        let mut items = self.items_of(id);
        if index < items.len() {
            items.remove(index);
            self.set_toolbar_items(id, items, cx);
        }
    }

    /// Moves item `index` of toolbar `id` by `delta` places, keeping the
    /// keyboard on it.
    pub(super) fn move_toolbar_item(
        &mut self,
        id: &str,
        index: usize,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
        let mut items = self.items_of(id);
        let target = index as isize + delta;
        if target < 0 || target as usize >= items.len() {
            return;
        }
        items.swap(index, target as usize);
        self.set_toolbar_items(id, items, cx);
        let moved = ControlRow::ToolbarItem {
            toolbar: id.to_owned(),
            index: target as usize,
        };
        self.focus_row_where(|row| *row == moved, cx);
    }

    /// Drops a dragged item before item `to` of toolbar `into`, which may
    /// be another toolbar.
    pub(super) fn drop_toolbar_item(
        &mut self,
        from: (&str, usize),
        into: (&str, usize),
        cx: &mut Context<Self>,
    ) {
        let (source, index) = from;
        let (target, to) = into;
        let mut source_items = self.items_of(source);
        if index >= source_items.len() {
            return;
        }
        let item = source_items.remove(index);
        if source == target {
            let at = to.min(source_items.len());
            source_items.insert(at, item);
            self.set_toolbar_items(source, source_items, cx);
            return;
        }
        let mut target_items = self.items_of(target);
        target_items.insert(to.min(target_items.len()), item);
        self.set_toolbar_items(target, target_items, cx);
        self.set_toolbar_items(source, source_items, cx);
    }

    pub(super) fn new_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match toolbar_files::add_toolbar(&self.vault_root, Place::EditorTop) {
            Ok((id, toolbars)) => {
                self.after_toolbar_write(Ok(toolbars), TOOLBARS_KEY, cx);
                let header = ControlRow::ToolbarHeader(id);
                if let Some(index) = self.rows().iter().position(|row| *row == header) {
                    self.set_focus(SettingsFocus::Control(index), window, cx);
                }
            }
            Err(message) => {
                self.after_toolbar_write(Err(message), TOOLBARS_KEY, cx);
            }
        }
    }

    /// Removes a toolbar the vault added; a built-in one is turned off.
    pub(super) fn remove_toolbar(&mut self, id: &str, cx: &mut Context<Self>) {
        let result = toolbar_files::remove_toolbar(&self.vault_root, id);
        self.after_toolbar_write(result, TOOLBARS_KEY, cx);
    }

    pub(super) fn reset_toolbar(&mut self, id: &str, cx: &mut Context<Self>) {
        let result = toolbar_files::reset_toolbar(&self.vault_root, id);
        self.after_toolbar_write(result, &toolbar_error_key(id), cx);
    }

    pub(super) fn reset_toolbars(&mut self, cx: &mut Context<Self>) {
        let result = toolbar_files::reset_toolbars(&self.vault_root);
        self.after_toolbar_write(result, TOOLBARS_KEY, cx);
    }

    /// Whether a built-in toolbar differs from how it came.
    pub(super) fn toolbar_changed(&self, id: &str) -> bool {
        let built_in = Toolbars::defaults();
        match (built_in.get(id), self.toolbar(id)) {
            (Some(built), Some(now)) => built != now,
            _ => false,
        }
    }

    /// Shows the Toolbars page's picker for toolbar `id`, as its add
    /// button in the app asks.
    pub fn start_adding_to(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let target = ControlRow::ToolbarAdd(id.to_owned());
        let Some(index) = self.rows().iter().position(|row| *row == target) else {
            return;
        };
        self.set_focus(SettingsFocus::Control(index), window, cx);
        self.open_menu(index, window, cx);
    }

    /// The toolbars the page shows.
    pub fn toolbars(&self) -> &Toolbars {
        &self.toolbars
    }

    // ---- The picker ----

    /// Everything the picker offers toolbar `id`: commands not on it yet,
    /// the status widgets, a separator, a flexible space and the menus.
    pub(super) fn picker_options(&self, id: &str) -> Vec<String> {
        let Some(toolbar) = self.toolbar(id) else {
            return Vec::new();
        };
        let on_bar: Vec<String> = toolbar.items.iter().map(ToString::to_string).collect();
        let commands = BUILTIN_COMMANDS
            .iter()
            .filter(|spec| spec.palette)
            .map(|spec| spec.id.to_owned());
        let widgets = Widget::ALL.iter().map(|widget| widget.name().to_owned());
        let menus = self
            .toolbars
            .menus
            .iter()
            .map(|menu| format!("{MENU_PREFIX}{}", menu.id));
        let fixed = [SEPARATOR.to_owned(), SPACER.to_owned()];
        commands
            .chain(widgets)
            .chain(menus)
            .filter(|option| !on_bar.contains(option))
            .chain(fixed)
            .collect()
    }

    /// How an option reads in the picker.
    pub(super) fn picker_label(&self, option: &str) -> String {
        item_title(&ToolbarItem::parse(option), &self.toolbars)
    }

    /// The options whose title, category or id have every word typed.
    pub(super) fn filter_picker(&self, options: &[String], query: &str) -> Vec<String> {
        options
            .iter()
            .filter(|option| {
                let category = command_spec(option).map_or("", |spec| spec.category);
                let haystack = format!("{} {category} {option}", self.picker_label(option));
                words_match(&haystack, query)
            })
            .cloned()
            .collect()
    }
}

/// The key a toolbar's write errors are kept under.
pub fn toolbar_error_key(id: &str) -> String {
    format!("toolbar.{id}")
}
