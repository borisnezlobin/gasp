//! What the settings screen shows, generated from the settings schema so
//! it never drifts from the files: sections, titles, search and the list
//! of keyboard shortcuts.

use editor_config::commands::BUILTIN_COMMANDS;
use editor_config::schema::{SettingKind, setting_descriptors};
use editor_config::{Platform, RuleSet};
use serde_json::Value;

/// Titles that read better than the key. Keys missing here get a title
/// made from the key itself, so a new setting always shows.
const TITLES: &[(&str, &str)] = &[
    ("sidebar.files.reveal", "Show the file sidebar"),
    ("sidebar.files.mode", "File sidebar layout"),
    ("markdown.symbols.mode", "Show Markdown symbols"),
    ("markdown.symbols.scope", "Reveal around the cursor"),
    (
        "markdown.symbols.overrides",
        "Symbols for each kind of syntax",
    ),
    (
        "prose.sentence-length.enabled",
        "Sentence-length highlighting",
    ),
    (
        "prose.sentence-length.short-below",
        "Short sentences are under",
    ),
    (
        "prose.sentence-length.long-above",
        "Long sentences are over",
    ),
    ("files.attachments-folder", "Attachments folder"),
    ("files.update-links-on-rename", "Update links when renaming"),
    ("files.trash", "Deleted files go to"),
    ("editor.show-inline-title", "Show the note's title"),
    ("appearance.base-font-size", "Base font size"),
];

/// The id of the section listing every command's keys.
pub const SHORTCUTS_SECTION: &str = "keyboard-shortcuts";
const SHORTCUTS_TITLE: &str = "Keyboard shortcuts";

/// One setting as the screen shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingItem {
    /// Dotted key, such as `files.trash`.
    pub key: String,
    pub title: String,
    pub description: String,
    pub kind: SettingKind,
    pub default: Value,
}

impl SettingItem {
    fn from_parts(key: String, kind: SettingKind, default: Value, description: String) -> Self {
        SettingItem {
            title: title_for(&key),
            key,
            description,
            kind,
            default,
        }
    }

    /// Whether every word of `query` appears in the title, key or description.
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!("{} {} {}", self.title, self.key, self.description).to_lowercase();
        words_match(&haystack, query)
    }
}

fn words_match(haystack: &str, query: &str) -> bool {
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

/// A group of settings that share their key's first part.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub items: Vec<SettingItem>,
}

/// Every setting in the schema, grouped by the first part of its key, in
/// alphabetical order of keys.
pub fn settings_sections() -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for descriptor in setting_descriptors() {
        let id = descriptor
            .key
            .split('.')
            .next()
            .unwrap_or_default()
            .to_string();
        let item = SettingItem::from_parts(
            descriptor.key,
            descriptor.kind,
            descriptor.default,
            descriptor.description.unwrap_or_default(),
        );
        match sections.iter_mut().find(|section| section.id == id) {
            Some(section) => section.items.push(item),
            None => sections.push(Section {
                title: humanize(&id),
                id,
                items: vec![item],
            }),
        }
    }
    sections
}

/// The title for a setting key.
pub fn title_for(key: &str) -> String {
    if let Some((_, title)) = TITLES.iter().find(|(known, _)| *known == key) {
        return title.to_string();
    }
    let rest: Vec<&str> = key.split('.').skip(1).collect();
    let rest = if rest.is_empty() {
        key.to_string()
    } else {
        rest.join(" ")
    };
    humanize(&rest)
}

/// `update-links-on-rename` → `Update links on rename`.
pub fn humanize(text: &str) -> String {
    let spaced = text.replace(['-', '_', '.'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A command and the keys bound to it on this platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutRow {
    pub id: String,
    pub title: String,
    pub category: String,
    /// Each chord as the platform writes it, such as `Ctrl+Shift+P`.
    pub keys: Vec<String>,
}

impl ShortcutRow {
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {} {}",
            self.title,
            self.id,
            self.category,
            self.keys.join(" ")
        )
        .to_lowercase();
        words_match(&haystack, query)
    }
}

/// Every built-in command with its keys, in registry order.
pub fn shortcut_rows(rules: &RuleSet, platform: Platform) -> Vec<ShortcutRow> {
    BUILTIN_COMMANDS
        .iter()
        .map(|spec| ShortcutRow {
            id: spec.id.to_string(),
            title: spec.title.to_string(),
            category: spec.category.to_string(),
            keys: rules
                .keys_for(spec.id, platform)
                .into_iter()
                .map(|chord| chord.display_for(platform))
                .collect(),
        })
        .collect()
}

/// The title of the shortcuts section, for the section list.
pub fn shortcuts_title() -> &'static str {
    SHORTCUTS_TITLE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_lands_in_a_section() {
        let sections = settings_sections();
        let count: usize = sections.iter().map(|s| s.items.len()).sum();
        assert_eq!(count, setting_descriptors().len());
        let ids: Vec<&str> = sections.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "appearance",
                "editor",
                "files",
                "markdown",
                "prose",
                "sidebar"
            ]
        );
        assert_eq!(sections[0].title, "Appearance");
    }

    #[test]
    fn settings_have_titles_and_descriptions() {
        for section in settings_sections() {
            for item in section.items {
                assert!(!item.title.is_empty(), "{}", item.key);
                assert!(
                    !item.description.is_empty(),
                    "{} has no description",
                    item.key
                );
            }
        }
    }

    #[test]
    fn unknown_keys_get_a_readable_title() {
        assert_eq!(title_for("files.keep-backups"), "Keep backups");
        assert_eq!(
            title_for("prose.grammar.check-style"),
            "Grammar check style"
        );
        assert_eq!(humanize("always-shown"), "Always shown");
    }

    #[test]
    fn titles_in_the_table_are_real_keys() {
        let keys: Vec<String> = setting_descriptors().into_iter().map(|d| d.key).collect();
        for (key, _) in TITLES {
            assert!(keys.iter().any(|k| k == key), "{key} isn't a setting");
        }
    }

    #[test]
    fn search_matches_title_key_and_description() {
        let sections = settings_sections();
        let all: Vec<&SettingItem> = sections.iter().flat_map(|s| &s.items).collect();
        let found = |query: &str| -> Vec<&str> {
            all.iter()
                .filter(|item| item.matches(query))
                .map(|item| item.key.as_str())
                .collect()
        };
        assert_eq!(found("trash"), ["files.trash"]);
        assert_eq!(found("ATTACHMENTS"), ["files.attachments-folder"]);
        assert!(found("sentence long").contains(&"prose.sentence-length.long-above"));
        assert!(found("pasted images").contains(&"files.attachments-folder"));
        assert!(found("no such thing").is_empty());
    }

    #[test]
    fn shortcuts_list_every_command_with_its_keys() {
        let rows = shortcut_rows(&RuleSet::defaults(), Platform::Linux);
        assert_eq!(rows.len(), BUILTIN_COMMANDS.len());
        let settings = rows.iter().find(|row| row.id == "settings.open").unwrap();
        assert!(
            settings.keys.contains(&"Ctrl+,".to_string()),
            "{:?}",
            settings.keys
        );
        assert!(settings.matches("ctrl+l"));
        let mac = shortcut_rows(&RuleSet::defaults(), Platform::Macos);
        let bold = mac.iter().find(|row| row.id == "format.bold").unwrap();
        assert_eq!(bold.keys, ["Cmd+B"]);
    }
}
