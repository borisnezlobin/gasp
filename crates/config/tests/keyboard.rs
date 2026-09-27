//! Keyboard-first checks from PLAN.md: reachability, reserved shortcuts and conflicts.

use editor_config::keymap::{key_conflicts, reserved_bindings, unreachable_commands};
use editor_config::{CommandRegistry, Platform, RuleSet};

const DESKTOP: [Platform; 3] = [Platform::Macos, Platform::Windows, Platform::Linux];

#[test]
fn every_command_is_reachable_from_the_keyboard() {
    let registry = CommandRegistry::<()>::with_builtins();
    let rules = RuleSet::defaults();
    for platform in Platform::ALL {
        let unreachable =
            unreachable_commands(registry.commands(), &rules, platform, "palette.open");
        assert!(unreachable.is_empty(), "{platform:?}: {unreachable:?}");
    }
}

#[test]
fn the_palette_has_a_key_everywhere() {
    let rules = RuleSet::defaults();
    for platform in Platform::ALL {
        assert!(
            !rules.keys_for("palette.open", platform).is_empty(),
            "{platform:?}"
        );
    }
}

#[test]
fn commands_left_out_of_the_palette_have_keys() {
    let registry = CommandRegistry::<()>::with_builtins();
    let rules = RuleSet::defaults();
    for info in registry.commands().filter(|info| !info.palette) {
        for platform in Platform::ALL {
            assert!(
                !rules.keys_for(&info.id, platform).is_empty(),
                "{}",
                info.id
            );
        }
    }
}

#[test]
fn a_command_with_neither_key_nor_palette_entry_is_caught() {
    let mut registry = CommandRegistry::<()>::with_builtins();
    registry.declare(editor_config::CommandInfo {
        id: "hidden.thing".into(),
        title: "Hidden thing".into(),
        category: "Test".into(),
        palette: false,
    });
    let rules = RuleSet::defaults();
    let unreachable =
        unreachable_commands(registry.commands(), &rules, Platform::Linux, "palette.open");
    assert_eq!(unreachable, ["hidden.thing"]);
}

#[test]
fn no_default_binding_uses_a_reserved_shortcut() {
    let rules = RuleSet::defaults();
    for platform in Platform::ALL {
        let reserved: Vec<String> = reserved_bindings(&rules, platform)
            .iter()
            .map(|(rule, chord)| format!("{} -> {}", chord.display_for(platform), rule.command))
            .collect();
        assert!(reserved.is_empty(), "{platform:?}: {reserved:?}");
    }
}

#[test]
fn reserved_check_catches_cmd_h_on_macos() {
    let text = "[[rule]]\non = \"key\"\nkeys = \"Mod+H\"\ndo = \"format.highlight\"\n";
    let rules = RuleSet::from_toml("rules.toml", text).unwrap();
    assert_eq!(reserved_bindings(&rules, Platform::Macos).len(), 1);
    assert!(reserved_bindings(&rules, Platform::Windows).is_empty());
}

#[test]
fn no_two_default_key_rules_conflict() {
    let rules = RuleSet::defaults();
    for platform in DESKTOP.into_iter().chain([Platform::Ios]) {
        let conflicts = key_conflicts(&rules, platform);
        assert!(conflicts.is_empty(), "{platform:?}: {conflicts:?}");
    }
}
