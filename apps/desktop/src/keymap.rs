//! Key bindings built from the config crate's rules, so every shortcut the
//! editor answers to is a rule a vault can change.

use editor_config::keys::{Key, KeyChord, Modifiers, NamedKey};
use editor_config::{Platform, RuleSet};
use gpui::{Action, App, KeyBinding, SharedString, actions};

use crate::commands::handles;

/// Runs the command with this id, such as `edit.delete-word-backward`.
#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = editor, no_json)]
pub struct RunCommand {
    pub id: SharedString,
}

actions!(editor, [Quit]);

/// The key context the editor view sets. Commands the editor runs bind here.
pub const KEY_CONTEXT: &str = "Editor";

/// The key context of the window's root view. Every other command binds
/// here, so it works whatever has focus.
pub const WORKSPACE_CONTEXT: &str = "Workspace";

/// Binds every default key rule whose command the editor can run.
pub fn bind_keys(cx: &mut App) {
    bind_rules(&RuleSet::defaults(), cx);
}

/// Binds the key rules in `rules` for this platform.
pub fn bind_rules(rules: &RuleSet, cx: &mut App) {
    let bindings = all_bindings(rules, Platform::current())
        .into_iter()
        .map(|binding| {
            let action = RunCommand {
                id: binding.command.into(),
            };
            KeyBinding::new(&binding.keystroke, action, Some(binding.context))
        });
    cx.bind_keys(bindings);
    // The app menu normally owns Quit; without one, Cmd+Q or Ctrl+Q quits.
    cx.bind_keys([KeyBinding::new("secondary-q", Quit, None)]);
    cx.on_action(|_: &Quit, cx| cx.quit());
}

/// One key rule as a GPUI binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub keystroke: String,
    pub command: String,
    pub context: &'static str,
}

/// Every key rule on `platform` as a binding: editor commands in the
/// editor's context, the rest in the workspace's. Rules limited to an
/// input context are left for the pipeline.
pub fn all_bindings(rules: &RuleSet, platform: Platform) -> Vec<Binding> {
    rules
        .key_rules(platform)
        .filter(|rule| rule.when.is_none())
        .filter_map(|rule| {
            let chord = rule.chord_for(platform)?;
            let context = if handles(&rule.command) {
                KEY_CONTEXT
            } else {
                WORKSPACE_CONTEXT
            };
            Some(Binding {
                keystroke: keystroke_for(chord, platform),
                command: rule.command.clone(),
                context,
            })
        })
        .collect()
}

/// (GPUI keystroke, command id) for the key rules the editor view runs.
pub fn editor_bindings(rules: &RuleSet, platform: Platform) -> Vec<(String, String)> {
    all_bindings(rules, platform)
        .into_iter()
        .filter(|binding| binding.context == KEY_CONTEXT)
        .map(|binding| (binding.keystroke, binding.command))
        .collect()
}

/// (modifier, GPUI name). Meta is Cmd on Apple platforms and Super elsewhere.
const MODIFIERS: [(Modifiers, &str); 3] = [
    (Modifiers::CTRL, "ctrl"),
    (Modifiers::ALT, "alt"),
    (Modifiers::SHIFT, "shift"),
];

const NAMED_KEYS: [(NamedKey, &str); 14] = [
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

/// A resolved chord as a GPUI keystroke string, such as `alt-backspace`.
pub fn keystroke_for(chord: KeyChord, platform: Platform) -> String {
    let chord = chord.resolve(platform);
    let meta = if platform.is_apple() { "cmd" } else { "super" };
    let mut parts: Vec<&str> = MODIFIERS
        .iter()
        .filter(|(modifier, _)| chord.modifiers.contains(*modifier))
        .map(|(_, name)| *name)
        .collect();
    if chord.modifiers.contains(Modifiers::META) {
        parts.push(meta);
    }
    let key = key_name(chord.key);
    parts.push(&key);
    parts.join("-")
}

fn key_name(key: Key) -> String {
    match key {
        Key::Char(ch) => ch.to_ascii_lowercase().to_string(),
        Key::Function(number) => format!("f{number}"),
        Key::Named(named) => NAMED_KEYS
            .iter()
            .find(|(candidate, _)| *candidate == named)
            .map_or("", |(_, name)| name)
            .to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keystroke(text: &str, platform: Platform) -> String {
        keystroke_for(KeyChord::parse(text).unwrap(), platform)
    }

    #[test]
    fn chords_become_gpui_keystrokes() {
        assert_eq!(keystroke("Alt+Backspace", Platform::Macos), "alt-backspace");
        assert_eq!(keystroke("Mod+Shift+P", Platform::Macos), "shift-cmd-p");
        assert_eq!(keystroke("Mod+Shift+P", Platform::Linux), "ctrl-shift-p");
        assert_eq!(keystroke("Mod+\\", Platform::Windows), "ctrl-\\");
        assert_eq!(
            keystroke("Ctrl+Shift+Tab", Platform::Linux),
            "ctrl-shift-tab"
        );
        assert_eq!(
            keystroke("Shift+PageDown", Platform::Macos),
            "shift-pagedown"
        );
    }

    #[test]
    fn every_default_binding_parses_as_a_gpui_keystroke() {
        for platform in [Platform::Macos, Platform::Windows, Platform::Linux] {
            for binding in all_bindings(&RuleSet::defaults(), platform) {
                let parsed = gpui::Keystroke::parse(&binding.keystroke);
                assert!(parsed.is_ok(), "{binding:?} on {platform:?}");
            }
        }
    }

    #[test]
    fn option_delete_is_bound_on_macos() {
        let bindings = editor_bindings(&RuleSet::defaults(), Platform::Macos);
        let bound = |keys: &str, id: &str| {
            bindings
                .iter()
                .any(|(keystroke, command)| keystroke == keys && command == id)
        };
        assert!(bound("alt-backspace", "edit.delete-word-backward"));
        assert!(bound("cmd-backspace", "edit.delete-to-line-start"));
        assert!(bound("cmd-c", "edit.copy"));
        assert!(bound("cmd-v", "edit.paste"));
        assert!(bound("cmd-b", "format.bold"));
        assert!(bound("alt-0", "footnote.insert-or-jump"));
    }
}
