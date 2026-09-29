//! The phone's two toolbars from `toolbars.toml`: the bar above the
//! software keyboard (place `keyboard`) and the bar at the bottom of the
//! screen (place `browser-bar`), as the phone draws them, and changing
//! them from the phone's settings with the same writers the desktop's
//! Toolbars page uses, so the change syncs like any setting.

use gasp_config::commands::command_spec;
use gasp_config::toolbar_files;
use gasp_config::toolbars::{
    ButtonStyle, Place, Toolbar, ToolbarItem, Toolbars, Widget, choice_name,
};
use gasp_config::{CommandSpec, Config};
use serde_json::Value as Json;

use crate::commands::{CommandInfo, command_info, command_infos, on_the_phone};
use crate::vault::{VaultError, VaultFolder};

/// The command the keyboard bar keeps at its right end, whatever its
/// items say.
const HIDE_KEYBOARD: &str = "keyboard.hide";

/// The settings screen's group for lines and gaps.
const LAYOUT: &str = "Layout";

/// One thing on a phone toolbar.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ToolbarEntry {
    Command {
        command: CommandInfo,
    },
    /// A thin line between groups of buttons.
    Separator,
    /// A wider gap on the keyboard bar; on the bottom bar, where the note's
    /// title sits.
    Spacer,
    /// A button that opens a menu of commands.
    Menu {
        title: String,
        commands: Vec<CommandInfo>,
    },
    /// A status widget the phone draws, such as `sync`.
    Widget {
        name: String,
        title: String,
    },
}

/// How a toolbar's buttons read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ToolbarLabels {
    Icons,
    IconsAndLabels,
    Labels,
}

/// A phone toolbar: whether it shows, how its buttons read and what it
/// holds.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PhoneToolbar {
    pub enabled: bool,
    pub labels: ToolbarLabels,
    pub entries: Vec<ToolbarEntry>,
}

/// What an item on a toolbar is, for the settings screen.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ToolbarItemKind {
    Command {
        id: String,
    },
    Separator,
    /// On the bottom bar, the note's title.
    Spacer,
    Menu,
    Widget,
}

/// One item as the settings screen lists it, or offers it to add.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ToolbarItemChoice {
    /// The item as `toolbars.toml` spells it, such as `format.bold`.
    pub item: String,
    pub title: String,
    /// The registry's group for a command; `Layout`, `Menus` or `Status`
    /// for the rest.
    pub category: String,
    pub kind: ToolbarItemKind,
}

/// A phone toolbar as its settings page shows it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ToolbarSetup {
    /// Its name in `toolbars.toml`, such as `keyboard`.
    pub id: String,
    pub title: String,
    /// Whether it sits at the bottom of the screen rather than above the
    /// keyboard.
    pub is_browser_bar: bool,
    pub enabled: bool,
    pub labels: ToolbarLabels,
    pub items: Vec<ToolbarItemChoice>,
    /// Whether it differs from the built-in toolbar, so it can be reset.
    pub changed: bool,
}

/// The bars placed at `keyboard`, their items one after the other,
/// leaving out commands the phone can't run, the status widgets and the
/// hide button the bar always has.
pub(crate) fn keyboard_toolbar(config: &Config) -> PhoneToolbar {
    let entries = phone_toolbar(config, Place::Keyboard);
    PhoneToolbar {
        entries: entries
            .entries
            .into_iter()
            .filter(|entry| !matches!(entry, ToolbarEntry::Widget { .. }))
            .collect(),
        ..entries
    }
}

/// The bars placed at `browser-bar`, with a spacer for the note's title
/// at the end when they name none. It always shows, since tabs are
/// reached through it.
pub(crate) fn browser_bar(config: &Config) -> PhoneToolbar {
    // Tabs are reached through this bar, so turning it off never hides it.
    let bars: Vec<&Toolbar> = config
        .toolbars
        .toolbars
        .iter()
        .filter(|toolbar| toolbar.place == Place::BrowserBar)
        .collect();
    let mut bar = toolbar_from(&bars, config);
    if !bar.entries.contains(&ToolbarEntry::Spacer) {
        bar.entries.push(ToolbarEntry::Spacer);
    }
    PhoneToolbar {
        enabled: true,
        ..bar
    }
}

fn phone_toolbar(config: &Config, place: Place) -> PhoneToolbar {
    let bars: Vec<&Toolbar> = config.toolbars.at(place).collect();
    toolbar_from(&bars, config)
}

fn toolbar_from(bars: &[&Toolbar], config: &Config) -> PhoneToolbar {
    let entries = bars
        .iter()
        .flat_map(|bar| bar.items.iter())
        .filter_map(|item| entry(item, &config.toolbars, config))
        .collect();
    PhoneToolbar {
        enabled: !bars.is_empty(),
        labels: bars
            .first()
            .map_or(ToolbarLabels::Icons, |bar| labels_for(bar.style)),
        entries,
    }
}

/// A command the phone runs and may put on a bar, by id.
fn phone_command(id: &str, config: &Config) -> Option<CommandInfo> {
    let spec = command_spec(id).filter(|spec| on_the_phone(spec.id) && spec.id != HIDE_KEYBOARD)?;
    Some(command_info(spec, config))
}

fn entry(item: &ToolbarItem, toolbars: &Toolbars, config: &Config) -> Option<ToolbarEntry> {
    match item {
        ToolbarItem::Command(id) => {
            phone_command(id, config).map(|command| ToolbarEntry::Command { command })
        }
        ToolbarItem::Separator => Some(ToolbarEntry::Separator),
        ToolbarItem::Spacer => Some(ToolbarEntry::Spacer),
        ToolbarItem::Menu(id) => {
            let menu = toolbars.menu(id)?;
            Some(ToolbarEntry::Menu {
                title: menu.title.clone(),
                commands: menu
                    .items
                    .iter()
                    .filter_map(|id| phone_command(id, config))
                    .collect(),
            })
        }
        ToolbarItem::Widget(Widget::Sync) => Some(ToolbarEntry::Widget {
            name: Widget::Sync.name().to_owned(),
            title: Widget::Sync.title().to_owned(),
        }),
        ToolbarItem::Widget(_) => None,
    }
}

fn labels_for(style: ButtonStyle) -> ToolbarLabels {
    match style {
        ButtonStyle::Icons => ToolbarLabels::Icons,
        ButtonStyle::IconsAndLabels => ToolbarLabels::IconsAndLabels,
        ButtonStyle::Labels => ToolbarLabels::Labels,
    }
}

fn style_for(labels: ToolbarLabels) -> ButtonStyle {
    match labels {
        ToolbarLabels::Icons => ButtonStyle::Icons,
        ToolbarLabels::IconsAndLabels => ButtonStyle::IconsAndLabels,
        ToolbarLabels::Labels => ButtonStyle::Labels,
    }
}

// MARK: Settings

fn is_phone_place(place: Place) -> bool {
    matches!(place, Place::Keyboard | Place::BrowserBar)
}

/// An item as the settings screen shows it on `place`'s bar.
fn item_choice(item: &ToolbarItem, place: Place, toolbars: &Toolbars) -> ToolbarItemChoice {
    let (title, category, kind) = match item {
        ToolbarItem::Command(id) => command_spec(id).map_or_else(
            || {
                (
                    id.clone(),
                    String::new(),
                    ToolbarItemKind::Command { id: id.clone() },
                )
            },
            command_choice_parts,
        ),
        ToolbarItem::Separator => (
            "Line".to_owned(),
            LAYOUT.to_owned(),
            ToolbarItemKind::Separator,
        ),
        ToolbarItem::Spacer if place == Place::BrowserBar => (
            "Note title".to_owned(),
            LAYOUT.to_owned(),
            ToolbarItemKind::Spacer,
        ),
        ToolbarItem::Spacer => ("Gap".to_owned(), LAYOUT.to_owned(), ToolbarItemKind::Spacer),
        ToolbarItem::Menu(id) => (
            toolbars
                .menu(id)
                .map_or_else(|| id.clone(), |menu| menu.title.clone()),
            "Menus".to_owned(),
            ToolbarItemKind::Menu,
        ),
        ToolbarItem::Widget(widget) => (
            widget.title().to_owned(),
            "Status".to_owned(),
            ToolbarItemKind::Widget,
        ),
    };
    ToolbarItemChoice {
        item: item.to_string(),
        title,
        category,
        kind,
    }
}

fn command_choice_parts(spec: &CommandSpec) -> (String, String, ToolbarItemKind) {
    (
        spec.title.to_owned(),
        spec.category.to_owned(),
        ToolbarItemKind::Command {
            id: spec.id.to_owned(),
        },
    )
}

fn setup(toolbar: &Toolbar, toolbars: &Toolbars) -> ToolbarSetup {
    let mut items: Vec<ToolbarItemChoice> = toolbar
        .items
        .iter()
        .filter(|item| **item != ToolbarItem::Command(HIDE_KEYBOARD.to_owned()))
        .map(|item| item_choice(item, toolbar.place, toolbars))
        .collect();
    let is_browser_bar = toolbar.place == Place::BrowserBar;
    if is_browser_bar && !toolbar.items.contains(&ToolbarItem::Spacer) {
        items.push(item_choice(&ToolbarItem::Spacer, toolbar.place, toolbars));
    }
    ToolbarSetup {
        id: toolbar.id.clone(),
        title: toolbar.title.clone(),
        is_browser_bar,
        enabled: toolbar.enabled,
        labels: labels_for(toolbar.style),
        items,
        changed: Toolbars::defaults().get(&toolbar.id) != Some(toolbar),
    }
}

/// What can be added to a bar at `place`: every command the phone runs,
/// then a line and a gap (the bottom bar has one title, so no gap), the
/// menus, and on the bottom bar sync's status.
fn choices(place: Place, config: &Config) -> Vec<ToolbarItemChoice> {
    let toolbars = &config.toolbars;
    let commands = command_infos(config)
        .into_iter()
        .filter(|info| info.in_palette && info.id != HIDE_KEYBOARD)
        .map(|info| ToolbarItem::Command(info.id));
    let mut extras = vec![ToolbarItem::Separator];
    if place == Place::Keyboard {
        extras.push(ToolbarItem::Spacer);
    }
    extras.extend(
        toolbars
            .menus
            .iter()
            .map(|menu| ToolbarItem::Menu(menu.id.clone())),
    );
    if place == Place::BrowserBar {
        extras.push(ToolbarItem::Widget(Widget::Sync));
    }
    commands
        .chain(extras)
        .map(|item| item_choice(&item, place, toolbars))
        .collect()
}

fn refused(message: String) -> VaultError {
    VaultError::Refused { message }
}

#[uniffi::export]
impl VaultFolder {
    /// The bar above the software keyboard: the `keyboard` toolbar in
    /// `toolbars.toml`.
    pub fn keyboard_toolbar(&self) -> PhoneToolbar {
        keyboard_toolbar(&self.config())
    }

    /// The bar at the bottom of the screen: the `browser-bar` toolbar.
    pub fn browser_bar(&self) -> PhoneToolbar {
        browser_bar(&self.config())
    }

    /// The toolbars the phone draws, turned on or off, for its settings.
    pub fn phone_toolbars(&self) -> Vec<ToolbarSetup> {
        let config = self.config();
        let toolbars = &config.toolbars;
        toolbars
            .toolbars
            .iter()
            .filter(|toolbar| is_phone_place(toolbar.place))
            .map(|toolbar| setup(toolbar, toolbars))
            .collect()
    }

    /// What can be added to toolbar `id`.
    pub fn toolbar_choices(&self, id: String) -> Vec<ToolbarItemChoice> {
        let config = self.config();
        let place = config
            .toolbars
            .get(&id)
            .map_or(Place::Keyboard, |bar| bar.place);
        choices(place, &config)
    }

    /// Replaces toolbar `id`'s items, spelled as `toolbars.toml` spells
    /// them.
    pub fn set_toolbar_items(&self, id: String, items: Vec<String>) -> Result<(), VaultError> {
        let items: Vec<ToolbarItem> = items.iter().map(|item| ToolbarItem::parse(item)).collect();
        toolbar_files::set_toolbar_items(&self.root, &id, &items).map_err(refused)?;
        self.reload_config();
        Ok(())
    }

    /// Sets how toolbar `id`'s buttons read.
    pub fn set_toolbar_labels(&self, id: String, labels: ToolbarLabels) -> Result<(), VaultError> {
        let style = Json::from(choice_name(style_for(labels)));
        self.set_toolbar_field(&id, "style", &style)
    }

    /// Turns toolbar `id` on or off.
    pub fn set_toolbar_enabled(&self, id: String, enabled: bool) -> Result<(), VaultError> {
        self.set_toolbar_field(&id, "enabled", &Json::Bool(enabled))
    }

    /// Puts toolbar `id` back as the built-in file has it.
    pub fn reset_toolbar(&self, id: String) -> Result<(), VaultError> {
        toolbar_files::reset_toolbar(&self.root, &id).map_err(refused)?;
        self.reload_config();
        Ok(())
    }
}

impl VaultFolder {
    fn set_toolbar_field(&self, id: &str, field: &str, value: &Json) -> Result<(), VaultError> {
        toolbar_files::set_toolbar_field(&self.root, id, field, Some(value)).map_err(refused)?;
        self.reload_config();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
