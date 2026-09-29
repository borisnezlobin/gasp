//! The shared command registry as the phone sees it: every command it can
//! run, the keys that run them with a hardware keyboard, and the bar above
//! the software keyboard.

use editor_config::commands::{BUILTIN_COMMANDS, CommandSpec};
use editor_config::keys::{Key, KeyChord, Modifiers, NamedKey};
use editor_config::rules::Rule;
use editor_config::{Config, Platform};

/// Commands the phone leaves out: it shows one note at a time, so there are
/// no panes to split or move tabs between, and it doesn't sync yet.
const NOT_ON_THE_PHONE: &[&str] = &["pane.", "sync."];

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

/// The toolbar's commands in order, leaving out ids the phone can't run.
pub(crate) fn toolbar(config: &Config) -> Vec<CommandInfo> {
    let commands = command_infos(config);
    config
        .settings
        .mobile
        .toolbar
        .iter()
        .filter_map(|id| commands.iter().find(|info| &info.id == id).cloned())
        .collect()
}

/// The keys to hand to UIKit. Plain typing and moving keys are left to
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

fn applies_anywhere(rule: &Rule) -> bool {
    rule.at.is_none() && rule.when.is_none() && rule.conditions.is_empty()
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
    fn panes_and_sync_stay_off_the_phone() {
        let commands = command_infos(&Config::defaults());
        assert!(commands.iter().any(|info| info.id == "format.bold"));
        assert!(!commands.iter().any(|info| info.id.starts_with("pane.")));
        assert!(!commands.iter().any(|info| info.id == "sync.now"));
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

    #[test]
    fn the_toolbar_starts_like_the_obsidian_one() {
        let ids: Vec<String> = toolbar(&Config::defaults())
            .into_iter()
            .map(|info| info.id)
            .collect();
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
