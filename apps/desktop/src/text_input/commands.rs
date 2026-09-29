//! The editing commands a text input runs, by command id, and their key
//! bindings from the config rules. The ids are the editor's, so a rule
//! that changes a key in the editor changes it in every input too.

use gasp_config::{Platform, RuleSet};
use gpui::{App, KeyBinding};

use super::state::Motion;
use super::{Cancel, Submit, TEXT_INPUT_CONTEXT};
use crate::keymap::{RunCommand, keystroke_for, keystroke_variants};

/// What a command id does in an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputCommand {
    Move(Motion),
    Select(Motion),
    Delete(Motion),
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
}

use InputCommand::{Delete, Move, Select};

/// Every command an input runs. A one-line input has one line, so line
/// and note motions both go to its ends.
const COMMANDS: [(&str, InputCommand); 29] = [
    ("cursor.left", Move(Motion::Left)),
    ("cursor.right", Move(Motion::Right)),
    ("cursor.word-left", Move(Motion::WordLeft)),
    ("cursor.word-right", Move(Motion::WordRight)),
    ("cursor.line-start", Move(Motion::Start)),
    ("cursor.line-end", Move(Motion::End)),
    ("cursor.doc-start", Move(Motion::Start)),
    ("cursor.doc-end", Move(Motion::End)),
    ("select.left", Select(Motion::Left)),
    ("select.right", Select(Motion::Right)),
    ("select.word-left", Select(Motion::WordLeft)),
    ("select.word-right", Select(Motion::WordRight)),
    ("select.line-start", Select(Motion::Start)),
    ("select.line-end", Select(Motion::End)),
    ("select.doc-start", Select(Motion::Start)),
    ("select.doc-end", Select(Motion::End)),
    ("edit.delete-backward", Delete(Motion::Left)),
    ("edit.delete-forward", Delete(Motion::Right)),
    ("edit.delete-word-backward", Delete(Motion::WordLeft)),
    ("edit.delete-word-forward", Delete(Motion::WordRight)),
    ("edit.delete-to-line-start", Delete(Motion::Start)),
    ("edit.delete-to-line-end", Delete(Motion::End)),
    ("select.all", InputCommand::SelectAll),
    ("edit.copy", InputCommand::Copy),
    ("edit.cut", InputCommand::Cut),
    ("edit.paste", InputCommand::Paste),
    ("edit.paste-plain", InputCommand::Paste),
    ("edit.undo", InputCommand::Undo),
    ("edit.redo", InputCommand::Redo),
];

/// What command `id` does in an input, if it runs there.
pub fn input_command(id: &str) -> Option<InputCommand> {
    COMMANDS
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, command)| *command)
}

/// Whether an input runs command `id`.
pub fn handles(id: &str) -> bool {
    input_command(id).is_some()
}

/// (GPUI keystroke, command id) for every key rule an input runs.
pub fn input_bindings(rules: &RuleSet, platform: Platform) -> Vec<(String, String)> {
    rules
        .key_rules(platform)
        .filter(|rule| rule.when.is_none() && handles(&rule.command))
        .filter_map(|rule| {
            let chord = rule.chord_for(platform)?;
            Some((keystroke_for(chord, platform), rule.command.clone()))
        })
        .flat_map(|(keystroke, id)| {
            keystroke_variants(&keystroke)
                .into_iter()
                .map(move |variant| (variant, id.clone()))
        })
        .collect()
}

/// Binds, in [`TEXT_INPUT_CONTEXT`], the editing rules in `rules` as
/// `RunCommand`, plus Enter to submit and Escape to cancel. Called by
/// `keymap::bind_rules`, so every input edits like the editor.
pub fn bind_keys(rules: &RuleSet, cx: &mut App) {
    let context = Some(TEXT_INPUT_CONTEXT);
    let editing = input_bindings(rules, Platform::current())
        .into_iter()
        .map(|(keystroke, id)| KeyBinding::new(&keystroke, RunCommand { id: id.into() }, context));
    cx.bind_keys(editing);
    cx.bind_keys([
        KeyBinding::new("enter", Submit, context),
        KeyBinding::new("escape", Cancel, context),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_commands_are_handled_and_others_are_not() {
        assert!(handles("edit.paste"));
        assert!(handles("select.word-left"));
        assert!(handles("edit.undo"));
        assert!(!handles("edit.newline"));
        assert!(!handles("cursor.up"));
        assert!(!handles("format.bold"));
        assert_eq!(
            input_command("cursor.doc-end"),
            Some(InputCommand::Move(Motion::End))
        );
    }

    #[test]
    fn input_keys_come_from_the_editing_rules() {
        let keys = input_bindings(&RuleSet::defaults(), Platform::Macos);
        let bound = |keystroke: &str, id: &str| {
            keys.iter()
                .any(|(candidate, command)| candidate == keystroke && command == id)
        };
        assert!(bound("backspace", "edit.delete-backward"));
        assert!(bound("alt-backspace", "edit.delete-word-backward"));
        assert!(bound("cmd-v", "edit.paste"));
        assert!(bound("cmd-z", "edit.undo"));
        assert!(!keys.iter().any(|(_, id)| id == "cursor.up"));
        assert!(!keys.iter().any(|(_, id)| id == "edit.newline"));
    }
}
