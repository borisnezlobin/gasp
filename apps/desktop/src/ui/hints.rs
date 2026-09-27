//! What a command is called and which keys run it, as buttons, menus and
//! tooltips show them: "New note" and `⌘N` on macOS, `Ctrl+N` elsewhere.

use editor_config::commands::BUILTIN_COMMANDS;
use editor_config::{Platform, RuleSet};
use gpui::{App, Global, SharedString};

use crate::picker::shortcut::shortcut_label;

/// The key rules shortcuts are read from.
struct Hints {
    rules: RuleSet,
    platform: Platform,
}

impl Global for Hints {}

/// Reads shortcuts from `rules` from now on, such as a vault's own rules.
pub fn set_rules(rules: RuleSet, cx: &mut App) {
    cx.set_global(Hints {
        rules,
        platform: Platform::current(),
    });
}

/// Shows shortcuts as `platform` would, for tests.
pub fn set_platform(platform: Platform, cx: &mut App) {
    if !cx.has_global::<Hints>() {
        set_rules(RuleSet::defaults(), cx);
    }
    cx.global_mut::<Hints>().platform = platform;
}

/// The command's title from the registry, or its id when it has none.
pub fn command_title(id: &str) -> SharedString {
    BUILTIN_COMMANDS
        .iter()
        .find(|spec| spec.id == id)
        .map_or_else(|| id.to_owned().into(), |spec| spec.title.into())
}

/// The first shortcut that runs `id`, as shown on this platform.
pub fn shortcut(id: &str, cx: &App) -> Option<SharedString> {
    let (rules, platform) = match cx.try_global::<Hints>() {
        Some(hints) => (&hints.rules, hints.platform),
        None => return shortcut_in(&RuleSet::defaults(), Platform::current(), id),
    };
    shortcut_in(rules, platform, id)
}

fn shortcut_in(rules: &RuleSet, platform: Platform, id: &str) -> Option<SharedString> {
    let chord = rules.keys_for(id, platform).into_iter().next()?;
    Some(shortcut_label(chord, platform).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_follow_the_platform() {
        let rules = RuleSet::defaults();
        assert_eq!(
            shortcut_in(&rules, Platform::Macos, "note.new")
                .map(|s| s.to_string())
                .as_deref(),
            Some("⌘N")
        );
        assert_eq!(
            shortcut_in(&rules, Platform::Linux, "note.new")
                .map(|s| s.to_string())
                .as_deref(),
            Some("Ctrl+N")
        );
        assert_eq!(shortcut_in(&rules, Platform::Linux, "vault.nope"), None);
    }

    #[test]
    fn titles_come_from_the_registry() {
        assert_eq!(command_title("tab.new"), "New tab");
        assert_eq!(command_title("unknown.command"), "unknown.command");
    }
}
