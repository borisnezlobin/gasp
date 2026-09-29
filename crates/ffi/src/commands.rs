//! The shared command registry as the phone sees it: every command it can
//! run, the keys that run them with a hardware keyboard, and the bar above
//! the software keyboard.

use gasp_config::commands::{BUILTIN_COMMANDS, CommandSpec};
use gasp_config::keys::{Key, KeyChord, Modifiers, NamedKey};
use gasp_config::rules::Rule;
use gasp_config::toolbars::{ButtonStyle, Place, ToolbarItem, Toolbars};
use gasp_config::{Config, Platform};

/// Commands the phone leaves out: it shows one note at a time, so there are
/// no panes to split or move tabs between.
const NOT_ON_THE_PHONE: &[&str] = &["pane."];

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CommandInfo {
    pub id: String,
    /// Sentence case, for the palette.
    pub title: String,
    pub category: String,
    /// Whether the palette lists it; the rest only run from a key.
    pub in_palette: bool,
    /// Its key with a hardware keyboard, such as `Cmd+Shift+P`.
    pub shortcut: Option<String>,
}

/// A key that runs a command with a hardware keyboard.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct KeyBinding {
    pub command: String,
    /// A character in lowercase, or one of `up`, `down`, `left`, `right`,
    /// `escape`, `tab`, `enter`, `space`, `backspace`, `delete`, `home`,
    /// `end`, `pageup`, `pagedown`, or `f1` to `f24`.
    pub input: String,
    pub command_key: bool,
    pub shift: bool,
    pub option: bool,
    pub control: bool,
}

pub(crate) fn on_the_phone(id: &str) -> bool {
    !NOT_ON_THE_PHONE.iter().any(|prefix| id.starts_with(prefix))
}

pub(crate) fn command_infos(config: &Config) -> Vec<CommandInfo> {
    BUILTIN_COMMANDS
        .iter()
        .filter(|spec| on_the_phone(spec.id))
        .map(|spec| command_info(spec, config))
        .collect()
}

fn command_info(spec: &CommandSpec, config: &Config) -> CommandInfo {
    CommandInfo {
        id: spec.id.to_owned(),
        title: spec.title.to_owned(),
        category: spec.category.to_owned(),
        in_palette: spec.palette,
        shortcut: config
            .rules
            .keys_for(spec.id, Platform::Ios)
            .first()
            .map(|chord| chord.display_for(Platform::Ios)),
    }
}

/// One thing on the bar above the software keyboard.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ToolbarEntry {
    Command {
        command: CommandInfo,
    },
    /// A thin line between groups of buttons.
    Separator,
    /// A wider gap; the bar scrolls, so there's no far end to push to.
    Spacer,
    /// A button that opens a menu of commands.
    Menu {
        title: String,
        commands: Vec<CommandInfo>,
    },
}

/// How the keyboard bar's buttons read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ToolbarLabels {
    Icons,
    IconsAndLabels,
    Labels,
}

/// The `keyboard` toolbar from `toolbars.toml`: what the bar above the
/// software keyboard holds and how it reads.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct KeyboardToolbar {
    /// Whether the bar shows at all.
    pub enabled: bool,
    pub labels: ToolbarLabels,
    pub entries: Vec<ToolbarEntry>,
}

/// The bars placed at `keyboard`, their items one after the other,
/// leaving out commands the phone can't run and the status widgets,
/// which the phone shows elsewhere.
pub(crate) fn keyboard_toolbar(config: &Config) -> KeyboardToolbar {
    let commands = command_infos(config);
    let find = |id: &str| commands.iter().find(|info| info.id == id).cloned();
    let toolbars = &config.toolbars;
    let bars: Vec<_> = toolbars.at(Place::Keyboard).collect();
    let entries = bars
        .iter()
        .flat_map(|bar| bar.items.iter())
        .filter_map(|item| keyboard_entry(item, toolbars, &find))
        .collect();
    KeyboardToolbar {
        enabled: !bars.is_empty(),
        labels: bars
            .first()
            .map_or(ToolbarLabels::Icons, |bar| labels_for(bar.style)),
        entries,
    }
}

fn labels_for(style: ButtonStyle) -> ToolbarLabels {
    match style {
        ButtonStyle::Icons => ToolbarLabels::Icons,
        ButtonStyle::IconsAndLabels => ToolbarLabels::IconsAndLabels,
        ButtonStyle::Labels => ToolbarLabels::Labels,
    }
}

fn keyboard_entry(
    item: &ToolbarItem,
    toolbars: &Toolbars,
    find: &dyn Fn(&str) -> Option<CommandInfo>,
) -> Option<ToolbarEntry> {
    match item {
        ToolbarItem::Command(id) => find(id).map(|command| ToolbarEntry::Command { command }),
        ToolbarItem::Separator => Some(ToolbarEntry::Separator),
        ToolbarItem::Spacer => Some(ToolbarEntry::Spacer),
        ToolbarItem::Menu(id) => {
            let menu = toolbars.menu(id)?;
            Some(ToolbarEntry::Menu {
                title: menu.title.clone(),
                commands: menu.items.iter().filter_map(|id| find(id)).collect(),
            })
        }
        ToolbarItem::Widget(_) => None,
    }
}

/// The keys to hand to UIKit. Moving, selecting and deleting are left to
/// the text view, which already handles them; what's left has a modifier,
/// or is Tab or Escape.
pub(crate) fn key_bindings(config: &Config) -> Vec<KeyBinding> {
    config
        .rules
        .key_rules(Platform::Ios)
        .filter(|rule| applies_anywhere(rule) && on_the_phone(&rule.command))
        .filter_map(|rule| {
            let chord = rule.chord_for(Platform::Ios)?;
            binding(&rule.command, chord)
        })
        .collect()
}

/// Commands a text view already runs from its own keys, the Emacs ones
/// Apple's rules bind (Ctrl+B, Ctrl+F…) included.
const TEXT_VIEW_KEYS: &[&str] = &[
    "cursor.",
    "select.left",
    "select.right",
    "select.up",
    "select.down",
    "select.word-",
    "select.line-",
    "select.doc-",
    "select.page-",
    "edit.delete-backward",
    "edit.delete-forward",
    "edit.delete-word-",
    "edit.newline",
];

fn applies_anywhere(rule: &Rule) -> bool {
    let text_view_key = TEXT_VIEW_KEYS
        .iter()
        .any(|prefix| rule.command.starts_with(prefix));
    !text_view_key && rule.at.is_none() && rule.when.is_none() && rule.conditions.is_empty()
}

fn binding(command: &str, chord: KeyChord) -> Option<KeyBinding> {
    let has = |modifier| chord.modifiers.contains(modifier);
    let modified = has(Modifiers::META) || has(Modifiers::CTRL) || has(Modifiers::ALT);
    let own_key = matches!(chord.key, Key::Named(NamedKey::Tab | NamedKey::Escape));
    if !modified && !own_key {
        return None;
    }
    Some(KeyBinding {
        command: command.to_owned(),
        input: key_input(chord.key),
        command_key: has(Modifiers::META),
        shift: has(Modifiers::SHIFT),
        option: has(Modifiers::ALT),
        control: has(Modifiers::CTRL),
    })
}

const NAMED_INPUTS: &[(NamedKey, &str)] = &[
    (NamedKey::Enter, "enter"),
    (NamedKey::Tab, "tab"),
    (NamedKey::Space, "space"),
    (NamedKey::Escape, "escape"),
    (NamedKey::Backspace, "backspace"),
    (NamedKey::Delete, "delete"),
    (NamedKey::Left, "left"),
    (NamedKey::Right, "right"),
    (NamedKey::Up, "up"),
    (NamedKey::Down, "down"),
    (NamedKey::Home, "home"),
    (NamedKey::End, "end"),
    (NamedKey::PageUp, "pageup"),
    (NamedKey::PageDown, "pagedown"),
];

fn key_input(key: Key) -> String {
    match key {
        Key::Char(character) => character.to_lowercase().to_string(),
        Key::Function(number) => format!("f{number}"),
        Key::Named(named) => NAMED_INPUTS
            .iter()
            .find(|(candidate, _)| *candidate == named)
            .map_or("", |(_, input)| input)
            .to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_stay_off_the_phone_and_sync_is_on_it() {
        let commands = command_infos(&Config::defaults());
        assert!(commands.iter().any(|info| info.id == "format.bold"));
        assert!(!commands.iter().any(|info| info.id.starts_with("pane.")));
        assert!(commands.iter().any(|info| info.id == "sync.now"));
    }

    #[test]
    fn the_palette_key_uses_command() {
        let bindings = key_bindings(&Config::defaults());
        let palette = bindings
            .iter()
            .find(|binding| binding.command == "palette.open")
            .unwrap();
        assert_eq!(palette.input, "p");
        assert!(palette.command_key && !palette.control);
    }

    #[test]
    fn arrows_without_modifiers_stay_with_the_text_view() {
        let bindings = key_bindings(&Config::defaults());
        assert!(
            !bindings
                .iter()
                .any(|binding| binding.command == "cursor.left")
        );
    }

    fn command_ids(toolbar: KeyboardToolbar) -> Vec<String> {
        toolbar
            .entries
            .into_iter()
            .filter_map(|entry| match entry {
                ToolbarEntry::Command { command } => Some(command.id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_keyboard_bar_follows_toolbars_toml() {
        let mut config = Config::defaults();
        let text = "[toolbar.keyboard]\nstyle = \"labels\"\nitems = [\"format.bold\", \"separator\", \"menu:insert\", \"word-count\", \"pane.close\"]\n";
        config.toolbars = gasp_config::toolbars::build_toolbars("toolbars.toml", Some(text), &[])
            .unwrap()
            .0;
        let bar = keyboard_toolbar(&config);
        assert_eq!(bar.labels, ToolbarLabels::Labels);
        assert_eq!(bar.entries.len(), 3, "no widget and no pane command");
        assert_eq!(bar.entries[1], ToolbarEntry::Separator);
        assert!(
            matches!(&bar.entries[2], ToolbarEntry::Menu { title, commands } if title == "Insert" && !commands.is_empty())
        );
        let off = "[toolbar.keyboard]\nenabled = false\n";
        config.toolbars = gasp_config::toolbars::build_toolbars("toolbars.toml", Some(off), &[])
            .unwrap()
            .0;
        assert!(!keyboard_toolbar(&config).enabled);
    }

    #[test]
    fn the_toolbar_starts_like_the_obsidian_one() {
        let ids = command_ids(keyboard_toolbar(&Config::defaults()));
        // keyboard.hide comes first on the phone; other hosts leave it out.
        let start = ids.iter().position(|id| id == "note.import-image").unwrap();
        assert!(start <= 1);
        assert_eq!(
            ids[start..start + 3],
            ["note.import-image", "edit.indent", "edit.outdent"]
        );
        assert_eq!(ids.last().map(String::as_str), Some("palette.open"));
    }
}
