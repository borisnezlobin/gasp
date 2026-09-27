//! Checks on key bindings: OS-reserved shortcuts, conflicts and keyboard reachability.

use std::collections::BTreeMap;

use crate::commands::CommandInfo;
use crate::keys::{KeyChord, Modifiers};
use crate::platform::{InputContext, Platform};
use crate::rules::{Rule, RuleSet};

/// Shortcuts the OS keeps for itself on each platform.
const RESERVED: &[(Platform, &[&str])] = &[
    (
        Platform::Macos,
        &[
            "Cmd+H",
            "Cmd+M",
            "Cmd+Q",
            "Cmd+Alt+H",
            "Cmd+`",
            "Cmd+Space",
            "Cmd+Tab",
            "Cmd+Shift+Tab",
            "Cmd+Alt+Esc",
            "Ctrl+Cmd+Q",
            "Ctrl+Cmd+Space",
            "Cmd+Shift+3",
            "Cmd+Shift+4",
            "Cmd+Shift+5",
            "Ctrl+Space",
            "Ctrl+Up",
            "Ctrl+Down",
            "Ctrl+Left",
            "Ctrl+Right",
        ],
    ),
    (
        Platform::Windows,
        &[
            "Alt+F4",
            "Alt+Tab",
            "Alt+Shift+Tab",
            "Alt+Esc",
            "Alt+Space",
            "Ctrl+Esc",
            "Ctrl+Shift+Esc",
            "Ctrl+Alt+Delete",
        ],
    ),
    (
        Platform::Linux,
        &[
            "Alt+Tab",
            "Alt+Shift+Tab",
            "Alt+F4",
            "Alt+F2",
            "Alt+Space",
            "Ctrl+Alt+Delete",
            "Ctrl+Alt+T",
        ],
    ),
    (Platform::Ios, &["Cmd+H", "Cmd+Space", "Cmd+Tab"]),
];

/// Platforms where any chord using the OS key (Win or Super) belongs to the OS.
const META_RESERVED: &[Platform] = &[Platform::Windows, Platform::Linux];

/// Whether `chord` (already resolved) is reserved by the OS on `platform`.
pub fn is_reserved(chord: KeyChord, platform: Platform) -> bool {
    if META_RESERVED.contains(&platform) && chord.modifiers.contains(Modifiers::META) {
        return true;
    }
    reserved_chords(platform).contains(&chord)
}

/// The exact reserved chords for `platform`.
pub fn reserved_chords(platform: Platform) -> Vec<KeyChord> {
    RESERVED
        .iter()
        .filter(|(p, _)| *p == platform)
        .flat_map(|(_, chords)| chords.iter())
        .filter_map(|text| KeyChord::parse_for(text, platform).ok())
        .collect()
}

/// Key rules on `platform` whose chord the OS reserves.
pub fn reserved_bindings(rules: &RuleSet, platform: Platform) -> Vec<(&Rule, KeyChord)> {
    rules
        .key_rules(platform)
        .filter_map(|rule| Some((rule, rule.chord_for(platform)?)))
        .filter(|(_, chord)| is_reserved(*chord, platform))
        .collect()
}

/// Two or more key rules that fire on the same chord in the same context.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyConflict {
    pub chord: KeyChord,
    pub when: Option<InputContext>,
    pub commands: Vec<String>,
}

type ConflictKey = (KeyChord, Option<InputContext>, String);

/// Key rules on `platform` that share a chord, input context and setting conditions.
pub fn key_conflicts(rules: &RuleSet, platform: Platform) -> Vec<KeyConflict> {
    let mut groups: BTreeMap<ConflictKey, Vec<String>> = BTreeMap::new();
    for rule in rules.key_rules(platform) {
        let Some(chord) = rule.chord_for(platform) else {
            continue;
        };
        let conditions = format!("{:?}", rule.conditions);
        groups
            .entry((chord, rule.when, conditions))
            .or_default()
            .push(rule.command.clone());
    }
    groups
        .into_iter()
        .filter(|(_, commands)| commands.len() > 1)
        .map(|((chord, when, _), commands)| KeyConflict {
            chord,
            when,
            commands,
        })
        .collect()
}

/// Commands that can't be run from the keyboard on `platform`: they have no key
/// rule and aren't in the palette, or the palette itself has no key.
pub fn unreachable_commands<'a>(
    commands: impl IntoIterator<Item = &'a CommandInfo>,
    rules: &RuleSet,
    platform: Platform,
    palette_command: &str,
) -> Vec<String> {
    let palette_has_key = !rules.keys_for(palette_command, platform).is_empty();
    commands
        .into_iter()
        .filter(|info| {
            let has_key = !rules.keys_for(&info.id, platform).is_empty();
            !(has_key || (info.palette && palette_has_key))
        })
        .map(|info| info.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str, platform: Platform) -> KeyChord {
        KeyChord::parse_for(text, platform).unwrap()
    }

    #[test]
    fn mac_reserves_hide_and_quit() {
        assert!(is_reserved(
            chord("Mod+H", Platform::Macos),
            Platform::Macos
        ));
        assert!(is_reserved(
            chord("Mod+Q", Platform::Macos),
            Platform::Macos
        ));
        assert!(!is_reserved(
            chord("Mod+H", Platform::Linux),
            Platform::Linux
        ));
    }

    #[test]
    fn any_win_or_super_chord_is_reserved() {
        assert!(is_reserved(
            chord("Win+E", Platform::Windows),
            Platform::Windows
        ));
        assert!(is_reserved(
            chord("Super+Shift+X", Platform::Linux),
            Platform::Linux
        ));
        assert!(!is_reserved(
            chord("Cmd+E", Platform::Macos),
            Platform::Macos
        ));
    }

    #[test]
    fn every_reserved_entry_parses() {
        for (platform, chords) in RESERVED {
            assert_eq!(
                reserved_chords(*platform).len(),
                chords.len(),
                "{platform:?}"
            );
        }
    }

    #[test]
    fn finds_conflicts_in_the_same_context_only() {
        let text = "[[rule]]\non = \"key\"\nkeys = \"Mod+B\"\ndo = \"a.b\"\n\
                    [[rule]]\non = \"key\"\nkeys = \"Ctrl+B\"\ndo = \"c.d\"\n\
                    [[rule]]\non = \"key\"\nkeys = \"Mod+B\"\nwhen = \"math\"\ndo = \"e.f\"\n";
        let rules = RuleSet::from_toml("rules.toml", text).unwrap();
        let linux = key_conflicts(&rules, Platform::Linux);
        assert_eq!(linux.len(), 1);
        assert_eq!(linux[0].commands, ["a.b", "c.d"]);
        assert!(key_conflicts(&rules, Platform::Macos).is_empty());
    }
}
