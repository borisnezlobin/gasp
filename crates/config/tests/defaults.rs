//! The built-in files match PLAN.md.

use std::collections::BTreeSet;

use editor_config::commands::BUILTIN_COMMANDS;
use editor_config::keys::KeyChord;
use editor_config::layout::SlotContent;
use editor_config::loader::DEFAULT_SETTINGS;
use editor_config::rules::EventKind;
use editor_config::settings::{RevealScope, SidebarMode, SidebarReveal, SymbolMode, TrashMode};
use editor_config::{CommandRegistry, Config, Platform, RuleSet, Settings};

/// Every row of PLAN.md's default keymap, both halves of paired rows included.
const KEYMAP: &[(&str, &str)] = &[
    ("Mod+B", "format.bold"),
    ("Mod+I", "format.italic"),
    ("Mod+U", "format.underline"),
    ("Mod+K", "format.link"),
    ("Mod+E", "format.code"),
    ("Mod+Shift+X", "format.strikethrough"),
    ("Mod+Shift+H", "format.highlight"),
    ("Mod+Shift+M", "format.math-inline"),
    ("Mod+/", "format.comment"),
    ("Mod+;", "markdown.cycle-symbols"),
    ("Alt+0", "footnote.insert-or-jump"),
    ("Mod+J", "prose.toggle-sentence-highlighting"),
    ("Mod+F", "find.open"),
    ("Mod+G", "find.next"),
    ("Mod+Shift+G", "find.previous"),
    ("Mod+Shift+R", "find.replace"),
    ("Mod+Shift+F", "search.open"),
    ("Mod+O", "switcher.open"),
    ("Mod+Shift+P", "palette.open"),
    ("Mod+N", "note.new"),
    ("Mod+Shift+O", "outline.jump-to-heading"),
    ("Mod+Shift+Y", "daily.open"),
    ("Mod+Shift+I", "template.insert"),
    ("Mod+Shift+J", "sidebar.right.toggle"),
    ("Mod+Shift+B", "sidebar.backlinks"),
    ("Mod+Shift+K", "sidebar.outgoing-links"),
    ("Mod+Shift+L", "sidebar.outline"),
    ("Mod+Shift+U", "sidebar.tags"),
    ("Mod+Enter", "link.follow"),
    ("Mod+[", "history.back"),
    ("Mod+]", "history.forward"),
    ("Mod+T", "tab.new"),
    ("Mod+W", "tab.close"),
    ("Mod+Shift+T", "tab.reopen"),
    ("Mod+1", "tab.go-1"),
    ("Mod+2", "tab.go-2"),
    ("Mod+3", "tab.go-3"),
    ("Mod+4", "tab.go-4"),
    ("Mod+5", "tab.go-5"),
    ("Mod+6", "tab.go-6"),
    ("Mod+7", "tab.go-7"),
    ("Mod+8", "tab.go-8"),
    ("Mod+9", "tab.go-9"),
    ("Ctrl+Tab", "tab.next"),
    ("Ctrl+Shift+Tab", "tab.previous"),
    ("Mod+\\", "sidebar.files.toggle"),
    ("Mod+Shift+E", "file-tree.focus"),
    ("Mod+Alt+Left", "pane.focus-left"),
    ("Mod+Alt+Right", "pane.focus-right"),
    ("Mod+Alt+Up", "pane.focus-up"),
    ("Mod+Alt+Down", "pane.focus-down"),
    ("Mod+Alt+Shift+Left", "pane.move-tab-left"),
    ("Mod+Alt+Shift+Right", "pane.move-tab-right"),
    ("Mod+Alt+Shift+Up", "pane.move-tab-up"),
    ("Mod+Alt+Shift+Down", "pane.move-tab-down"),
    ("Mod+Alt+W", "pane.close"),
    ("Mod+P", "app.print"),
    ("Mod+Shift+S", "app.export"),
    ("Mod+S", "sync.now"),
    ("Mod+,", "settings.open"),
    ("Mod+L", "settings.open"),
    ("Mod+Shift+N", "vault.open"),
];

/// Every command id named in the shared brief.
const BRIEF_COMMANDS: &[&str] = &[
    "format.bold",
    "format.italic",
    "format.underline",
    "format.link",
    "format.code",
    "format.strikethrough",
    "format.highlight",
    "format.math-inline",
    "format.comment",
    "markdown.cycle-symbols",
    "footnote.insert-or-jump",
    "prose.toggle-sentence-highlighting",
    "find.open",
    "find.next",
    "find.previous",
    "find.replace",
    "search.open",
    "switcher.open",
    "palette.open",
    "note.new",
    "outline.jump-to-heading",
    "link.follow",
    "history.back",
    "history.forward",
    "tab.new",
    "tab.close",
    "tab.reopen",
    "tab.go-1",
    "tab.go-2",
    "tab.go-3",
    "tab.go-4",
    "tab.go-5",
    "tab.go-6",
    "tab.go-7",
    "tab.go-8",
    "tab.go-9",
    "tab.next",
    "tab.previous",
    "sidebar.files.toggle",
    "sidebar.files.show",
    "sidebar.files.hide",
    "file-tree.focus",
    "pane.focus-left",
    "pane.focus-right",
    "app.print",
    "app.export",
    "sync.now",
    "settings.open",
    "vault.open",
];

#[test]
fn every_keymap_row_is_a_default_rule() {
    let rules = RuleSet::defaults();
    for (keys, command) in KEYMAP {
        let chord = KeyChord::parse(keys).unwrap();
        let found = rules
            .rules()
            .iter()
            .any(|rule| rule.keys == Some(chord) && rule.command == *command);
        assert!(found, "missing default rule {keys} -> {command}");
    }
}

/// Text-editing keys (cursor movement, selection, deletion, clipboard) are
/// defaults too, but they follow each platform's conventions rather than the
/// plan's keymap table. Zoom and split keys are additions beyond the table.
fn is_text_editing(command: &str) -> bool {
    ["cursor.", "select.", "edit.", "view.zoom-", "pane.split-"]
        .iter()
        .any(|prefix| command.starts_with(prefix))
}

#[test]
fn default_key_rules_outside_text_editing_are_exactly_the_keymap() {
    let key_rules = RuleSet::defaults()
        .rules()
        .iter()
        .filter(|rule| rule.is_key() && !is_text_editing(&rule.command))
        .count();
    assert_eq!(key_rules, KEYMAP.len());
}

#[test]
fn word_and_line_keys_follow_each_platform() {
    let rules = RuleSet::defaults();
    let cases = [
        (
            Platform::Macos,
            "Alt+Backspace",
            "edit.delete-word-backward",
        ),
        (
            Platform::Macos,
            "Cmd+Backspace",
            "edit.delete-to-line-start",
        ),
        (Platform::Macos, "Alt+Left", "cursor.word-left"),
        (Platform::Macos, "Cmd+Shift+Right", "select.line-end"),
        (Platform::Macos, "Ctrl+K", "edit.delete-to-line-end"),
        (
            Platform::Windows,
            "Ctrl+Backspace",
            "edit.delete-word-backward",
        ),
        (Platform::Linux, "Ctrl+Shift+Left", "select.word-left"),
        (Platform::Linux, "Ctrl+End", "cursor.doc-end"),
        (Platform::Windows, "Ctrl+Y", "edit.redo"),
        (Platform::Linux, "Ctrl+C", "edit.copy"),
        (Platform::Macos, "Cmd+V", "edit.paste"),
    ];
    for (platform, keys, command) in cases {
        let chord = KeyChord::parse_for(keys, platform).unwrap();
        let bound = rules.keys_for(command, platform);
        assert!(
            bound.contains(&chord),
            "{keys} should run {command} on {platform:?}, got {bound:?}"
        );
    }
    let apple_only = KeyChord::parse_for("Ctrl+K", Platform::Linux).unwrap();
    assert!(
        !rules
            .keys_for("edit.delete-to-line-end", Platform::Linux)
            .contains(&apple_only)
    );
}

#[test]
fn default_rules_have_unique_ids() {
    let rules = RuleSet::defaults();
    let mut ids = BTreeSet::new();
    for rule in rules.rules() {
        let id = rule.id.clone().expect("every default rule has an id");
        assert!(ids.insert(id.clone()), "duplicate rule id {id}");
    }
}

#[test]
fn hover_sidebar_rules_match_the_plan() {
    let rules = RuleSet::defaults();
    let show = rules.get("hover.sidebar.files.show").unwrap();
    assert_eq!(show.on, EventKind::PointerEnter);
    assert_eq!(show.at.as_deref(), Some("window.left-edge"));
    assert_eq!(show.command, "sidebar.files.show");
    assert!(show.applies_on(Platform::Macos) && !show.applies_on(Platform::Ios));
    let hide = rules.get("hover.sidebar.files.hide").unwrap();
    assert_eq!(hide.on, EventKind::PointerLeave);
    assert_eq!(hide.at.as_deref(), Some("sidebar.files"));
    assert_eq!(hide.after.as_millis(), 300);
    assert_eq!(hide.command, "sidebar.files.hide");
}

#[test]
fn registry_has_every_brief_command() {
    let registry = CommandRegistry::<()>::with_builtins();
    for id in BRIEF_COMMANDS {
        assert!(registry.contains(id), "registry is missing {id}");
    }
}

#[test]
fn every_default_rule_names_a_registered_command() {
    let registry = CommandRegistry::<()>::with_builtins();
    for rule in RuleSet::defaults().rules() {
        assert!(
            registry.contains(&rule.command),
            "{} isn't registered",
            rule.command
        );
    }
}

#[test]
fn command_titles_are_plain_sentence_case() {
    const PROPER: &[&str] = &["Markdown"];
    for spec in BUILTIN_COMMANDS {
        let mut words = spec.title.split(' ');
        let first = words.next().unwrap();
        assert!(
            first.chars().next().unwrap().is_uppercase(),
            "{}",
            spec.title
        );
        for word in words {
            let capitalised = word.chars().next().is_some_and(char::is_uppercase);
            assert!(!capitalised || PROPER.contains(&word), "{}", spec.title);
        }
        assert!(!spec.title.ends_with('.'), "{}", spec.title);
    }
}

#[test]
fn default_settings_file_matches_the_types() {
    let parsed: Settings = toml::from_str(DEFAULT_SETTINGS).unwrap();
    assert_eq!(parsed, Settings::default());
}

#[test]
fn default_settings_values() {
    let settings = Config::defaults().settings;
    assert_eq!(settings.sidebar.files.reveal, SidebarReveal::Hover);
    assert_eq!(settings.sidebar.files.mode, SidebarMode::Overlay);
    assert_eq!(settings.markdown.symbols.mode, SymbolMode::AroundCursor);
    assert_eq!(settings.markdown.symbols.scope, RevealScope::Element);
    assert_eq!(settings.prose.sentence_length.short_below, 7);
    assert_eq!(settings.prose.sentence_length.long_above, 18);
    assert_eq!(settings.files.attachments_folder, "./images");
    assert!(settings.files.update_links_on_rename);
    assert_eq!(settings.files.trash, TrashMode::System);
    assert!(settings.editor.show_inline_title);
    assert_eq!(settings.appearance.base_font_size, 12);
}

#[test]
fn default_theme_values() {
    let theme = Config::defaults().theme;
    assert_eq!(theme.text("font.text"), Some("Charter"));
    assert_eq!(theme.text("font.ui"), Some("Charter"));
    assert_eq!(theme.text("font.code"), Some("Courier New"));
    assert_eq!(theme.text("color.accent"), Some("#000000"));
    assert_eq!(theme.text("theme.mode"), Some("light"));
    assert_eq!(theme.text("color.sentence.short"), Some("#e3f2fd"));
    assert_eq!(theme.text("color.sentence.medium"), Some("#fff3e0"));
    assert_eq!(theme.text("color.sentence.long"), Some("#ffebee"));
}

#[test]
fn default_theme_covers_every_token_family() {
    let theme = Config::defaults().theme;
    for family in [
        "color.",
        "font.",
        "size.",
        "space.",
        "radius.",
        "shadow.",
        "curve.",
        "duration.",
    ] {
        assert!(
            theme.names().any(|name| name.starts_with(family)),
            "no {family} tokens"
        );
    }
}

#[test]
fn layout_size_tokens_exist_in_the_theme() {
    let config = Config::defaults();
    let mut stack = vec![&config.layout];
    while let Some(node) = stack.pop() {
        if let Some(size) = &node.size {
            assert!(
                config.theme.get(size).is_some(),
                "unknown size token {size}"
            );
        }
        stack.extend(node.children());
    }
}

#[test]
fn default_layout_has_every_slot_and_no_ribbon() {
    let layout = Config::defaults().layout;
    let slots = layout.slot_names();
    for slot in [
        "left-sidebar",
        "tab-bar",
        "editor",
        "right-sidebar",
        "status-bar",
        "toolbar",
    ] {
        assert!(slots.contains(&slot), "missing slot {slot}");
    }
    assert!(!slots.iter().any(|slot| slot.contains("ribbon")));
    assert!(!layout.components().iter().any(|c| c.contains("ribbon")));
    assert_eq!(
        layout.find("editor").unwrap().content,
        SlotContent::Component("editor".into())
    );
}
