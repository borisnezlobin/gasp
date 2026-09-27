//! What the settings screen shows: its pages, the rows on each page
//! (generated from the settings schema, so none drifts from the files),
//! titles and descriptions, search, and the list of keyboard shortcuts.

use std::borrow::Cow;

use editor_config::commands::BUILTIN_COMMANDS;
use editor_config::keys::KeyChord;
use editor_config::schema::{SettingKind, setting_descriptors};
use editor_config::{Platform, RuleSet};
use serde_json::Value;

use crate::icons::IconName;
use crate::picker::shortcut::shortcut_label;

/// Title and description for each setting. A setting missing here gets a
/// title made from its key and the schema's description, so a new one
/// always shows.
const TEXTS: &[(&str, &str, &str)] = &[
    (
        "sidebar.files.reveal",
        "Show the file sidebar",
        "Keep it open, open it from its shortcut, or show it when the pointer reaches the left edge.",
    ),
    (
        "sidebar.files.mode",
        "Sidebar placement",
        "Slide over the note, or push the note aside to make room.",
    ),
    (
        "markdown.symbols.mode",
        "Markdown symbols",
        "When to show the symbols that mark up text, such as ** and #.",
    ),
    (
        "markdown.symbols.scope",
        "Reveal near the cursor",
        "How much markup to show around the cursor when symbols appear near it.",
    ),
    (
        "markdown.symbols.overrides",
        "Symbols for one kind of syntax",
        "Give one kind of syntax its own rule, such as always hiding link addresses.",
    ),
    (
        "prose.sentence-length.enabled",
        "Colour sentences by length",
        "Tint short, medium and long sentences so the rhythm of a paragraph shows.",
    ),
    (
        "prose.sentence-length.short-below",
        "Short sentences",
        "Sentences with fewer words than this count as short.",
    ),
    (
        "prose.sentence-length.long-above",
        "Long sentences",
        "Sentences with more words than this count as long.",
    ),
    (
        "files.attachments-folder",
        "Attachments folder",
        "Where pasted and dropped images are saved, relative to the note.",
    ),
    (
        "files.update-links-on-rename",
        "Update links when renaming",
        "Rewrite links to a note when you rename or move it.",
    ),
    (
        "files.trash",
        "Deleted files",
        "Where a note goes when you delete it.",
    ),
    (
        "editor.show-inline-title",
        "Show the note's title",
        "Show the file name as an editable title above the text.",
    ),
    (
        "appearance.base-font-size",
        "Font size",
        "The size of body text in points. Headings scale with it.",
    ),
];

/// How each option of a choice reads. Values are unique across settings.
const CHOICE_LABELS: &[(&str, &str)] = &[
    ("always", "Always open"),
    ("toggle", "From its shortcut"),
    ("hover", "On hover"),
    ("overlay", "Over the note"),
    ("push", "Beside the note"),
    ("always-shown", "Always"),
    ("around-cursor", "Near the cursor"),
    ("always-hidden", "Never"),
    ("element", "Just the element"),
    ("line", "The whole line"),
    ("block", "The whole block"),
    ("system", "System trash"),
    ("vault", "The vault's .trash folder"),
    ("delete", "Delete for good"),
];

/// Settings the desktop app doesn't read yet, by key prefix. Showing them
/// would be controls that do nothing, so they stay hidden until their
/// feature lands.
const UNWIRED: &[&str] = &["prose."];

/// The smallest value a number setting takes, when it isn't zero.
const MINIMUMS: &[(&str, i64)] = &[("appearance.base-font-size", 6)];

/// One page of the settings screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    General,
    Appearance,
    Sidebar,
    Shortcuts,
    Editor,
    Files,
    Prose,
}

/// A theme font the Appearance page edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontSlot {
    Text,
    Interface,
    Code,
}

impl FontSlot {
    pub const ALL: [FontSlot; 3] = [FontSlot::Text, FontSlot::Interface, FontSlot::Code];

    /// The theme token, such as `font.text`.
    pub fn token(self) -> &'static str {
        match self {
            FontSlot::Text => "font.text",
            FontSlot::Interface => "font.ui",
            FontSlot::Code => "font.code",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            FontSlot::Text => "Text font",
            FontSlot::Interface => "Interface font",
            FontSlot::Code => "Code font",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            FontSlot::Text => "Used for the text of your notes.",
            FontSlot::Interface => "Used for menus, tabs, the sidebar and this screen.",
            FontSlot::Code => "Used for code blocks and inline code.",
        }
    }
}

/// The theme token the accent row edits.
pub const ACCENT_TOKEN: &str = "color.accent";
pub const ACCENT_TITLE: &str = "Accent colour";
pub const ACCENT_DESCRIPTION: &str = "Used for the cursor, links and switches that are on.";

/// Where a row on a page comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowSpec {
    /// A setting, by key.
    Setting(Cow<'static, str>),
    Font(FontSlot),
    Accent,
    /// The vault's folder, with a button to open another.
    Vault,
    Version,
}

const fn setting(key: &'static str) -> RowSpec {
    RowSpec::Setting(Cow::Borrowed(key))
}

/// A page, its place in the section list and its rows, grouped in cards.
#[derive(Clone, Copy, Debug)]
pub struct PageSpec {
    pub page: Page,
    /// Stable id for [`super::SettingsView::show_section`].
    pub id: &'static str,
    pub title: &'static str,
    pub icon: IconName,
    /// The small label above the group the page sits in.
    pub group: &'static str,
    pub cards: &'static [&'static [RowSpec]],
}

/// Every page, in the order the section list shows them.
pub const PAGES: &[PageSpec] = &[
    PageSpec {
        page: Page::General,
        id: "general",
        title: "General",
        icon: IconName::SlidersHorizontal,
        group: "App",
        cards: &[&[RowSpec::Vault, RowSpec::Version]],
    },
    PageSpec {
        page: Page::Appearance,
        id: "appearance",
        title: "Appearance",
        icon: IconName::Palette,
        group: "App",
        cards: &[
            &[
                RowSpec::Font(FontSlot::Text),
                RowSpec::Font(FontSlot::Interface),
                RowSpec::Font(FontSlot::Code),
                setting("appearance.base-font-size"),
            ],
            &[RowSpec::Accent],
        ],
    },
    PageSpec {
        page: Page::Sidebar,
        id: "sidebar",
        title: "Sidebar",
        icon: IconName::SidebarSimple,
        group: "App",
        cards: &[&[
            setting("sidebar.files.reveal"),
            setting("sidebar.files.mode"),
        ]],
    },
    PageSpec {
        page: Page::Shortcuts,
        id: SHORTCUTS_SECTION,
        title: "Keyboard shortcuts",
        icon: IconName::Keyboard,
        group: "App",
        cards: &[],
    },
    PageSpec {
        page: Page::Editor,
        id: "editor",
        title: "Editor",
        icon: IconName::PencilSimple,
        group: "Writing",
        cards: &[
            &[setting("editor.show-inline-title")],
            &[
                setting("markdown.symbols.mode"),
                setting("markdown.symbols.scope"),
                setting("markdown.symbols.overrides"),
            ],
        ],
    },
    PageSpec {
        page: Page::Files,
        id: "files",
        title: "Files and links",
        icon: IconName::Folder,
        group: "Writing",
        cards: &[
            &[
                setting("files.attachments-folder"),
                setting("files.update-links-on-rename"),
            ],
            &[setting("files.trash")],
        ],
    },
    PageSpec {
        page: Page::Prose,
        id: "prose",
        title: "Prose",
        icon: IconName::Article,
        group: "Writing",
        cards: &[&[
            setting("prose.sentence-length.enabled"),
            setting("prose.sentence-length.short-below"),
            setting("prose.sentence-length.long-above"),
        ]],
    },
];

/// The id of the page listing every command's keys.
pub const SHORTCUTS_SECTION: &str = "keyboard-shortcuts";

impl PageSpec {
    pub fn get(page: Page) -> &'static PageSpec {
        PAGES
            .iter()
            .find(|spec| spec.page == page)
            .expect("every page is listed")
    }
}

/// The page a setting that no page lists lands on, by its key's first part.
fn fallback_page(key: &str) -> Page {
    match key.split('.').next().unwrap_or_default() {
        "appearance" => Page::Appearance,
        "editor" | "markdown" => Page::Editor,
        "files" => Page::Files,
        "sidebar" => Page::Sidebar,
        "prose" => Page::Prose,
        _ => Page::General,
    }
}

/// Whether the app reads this setting yet.
pub fn is_wired(key: &str) -> bool {
    !UNWIRED.iter().any(|prefix| key.starts_with(prefix))
}

/// The lowest value a number setting can step down to.
pub fn minimum_for(key: &str) -> i64 {
    MINIMUMS
        .iter()
        .find(|(known, _)| *known == key)
        .map_or(0, |(_, minimum)| *minimum)
}

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
    fn from_parts(key: String, kind: SettingKind, default: Value, schema_text: String) -> Self {
        let (title, description) = match TEXTS.iter().find(|(known, ..)| *known == key) {
            Some((_, title, description)) => (title.to_string(), description.to_string()),
            None => (title_for(&key), schema_text),
        };
        SettingItem {
            title,
            description,
            key,
            kind,
            default,
        }
    }

    /// Whether every word of `query` appears in the title, key or description.
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!("{} {} {}", self.title, self.key, self.description);
        words_match(&haystack, query)
    }
}

/// Whether every word of `query` appears somewhere in `haystack`,
/// ignoring case.
pub fn words_match(haystack: &str, query: &str) -> bool {
    let haystack = haystack.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

/// Every setting in the schema that the app reads.
pub fn setting_items() -> Vec<SettingItem> {
    setting_descriptors()
        .into_iter()
        .filter(|descriptor| is_wired(&descriptor.key))
        .map(|descriptor| {
            SettingItem::from_parts(
                descriptor.key,
                descriptor.kind,
                descriptor.default,
                descriptor.description.unwrap_or_default(),
            )
        })
        .collect()
}

/// A page's rows grouped in cards, with every setting no page lists
/// added to the page its key points at.
pub fn page_cards(page: Page, items: &[SettingItem]) -> Vec<Vec<RowSpec>> {
    let spec = PageSpec::get(page);
    let listed: Vec<&str> = PAGES
        .iter()
        .flat_map(|spec| spec.cards.iter().flat_map(|card| card.iter()))
        .filter_map(|row| match row {
            RowSpec::Setting(key) => Some(key.as_ref()),
            _ => None,
        })
        .collect();
    let mut cards: Vec<Vec<RowSpec>> = spec.cards.iter().map(|card| card.to_vec()).collect();
    let extra: Vec<RowSpec> = items
        .iter()
        .filter(|item| !listed.contains(&item.key.as_str()) && fallback_page(&item.key) == page)
        .map(|item| RowSpec::Setting(Cow::Owned(item.key.clone())))
        .collect();
    if !extra.is_empty() {
        cards.push(extra);
    }
    cards
}

/// The title for a setting key without one in the table.
pub fn title_for(key: &str) -> String {
    let rest: Vec<&str> = key.split('.').skip(1).collect();
    let rest = if rest.is_empty() {
        key.to_string()
    } else {
        rest.join(" ")
    };
    humanize(&rest)
}

/// `update-links-on-rename` → `Update links on rename`.
/// The kinds of syntax `markdown.symbols.overrides` takes, as the config
/// names them and as the screen shows them.
const SYNTAX_NAMES: &[(&str, &str)] = &[
    ("emphasis", "Italic"),
    ("strong", "Bold"),
    ("strikethrough", "Strikethrough"),
    ("highlight", "Highlight"),
    ("heading", "Headings"),
    ("link-text", "Link text"),
    ("link-url", "Link addresses"),
    ("wikilink", "Wiki links"),
    ("inline-code", "Inline code"),
    ("code-fence", "Code block fences"),
    ("math", "Math"),
    ("blockquote", "Block quotes"),
    ("callout", "Callouts"),
    ("footnote", "Footnotes"),
    ("comment", "Comments"),
    ("html", "HTML"),
    ("list-marker", "List markers"),
    ("frontmatter", "Frontmatter"),
];

/// Map settings whose names come from a fixed list, so the screen offers
/// the list rather than a field to type a name into.
const MAP_NAMES: &[(&str, &[(&str, &str)])] = &[("markdown.symbols.overrides", SYNTAX_NAMES)];

/// The names map setting `map_key` takes, when they're a fixed list.
pub fn map_names(map_key: &str) -> Option<&'static [(&'static str, &'static str)]> {
    MAP_NAMES
        .iter()
        .find(|(key, _)| *key == map_key)
        .map(|(_, names)| *names)
}

/// How entry `name` of map setting `map_key` reads.
pub fn map_name_label(map_key: &str, name: &str) -> String {
    map_names(map_key)
        .and_then(|names| names.iter().find(|(known, _)| *known == name))
        .map_or_else(|| humanize(name), |(_, label)| (*label).to_string())
}

pub fn humanize(text: &str) -> String {
    let spaced = text.replace(['-', '_', '.'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// How one option of a choice reads.
pub fn choice_label(value: &str) -> String {
    CHOICE_LABELS
        .iter()
        .find(|(known, _)| *known == value)
        .map_or_else(|| humanize(value), |(_, label)| label.to_string())
}

// ---- Fonts ----

/// The fonts a font menu offers: `current` first, then the theme's
/// built-in font, then every family the system has, sorted and without
/// duplicates or hidden system names.
pub fn font_choices(system: &[String], current: &str, built_in: &str) -> Vec<String> {
    let mut names: Vec<String> = system
        .iter()
        .filter(|name| !name.starts_with('.') && !name.trim().is_empty())
        .filter(|name| *name != current && *name != built_in)
        .cloned()
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    let mut choices = vec![current.to_string()];
    if built_in != current && !built_in.is_empty() {
        choices.push(built_in.to_string());
    }
    choices.extend(names);
    choices
}

/// The fonts matching a typed filter, names that start with it first.
pub fn filter_fonts(choices: &[String], query: &str) -> Vec<String> {
    let query = query.trim().to_lowercase();
    let (mut starts, rest): (Vec<String>, Vec<String>) = choices
        .iter()
        .filter(|name| words_match(name, &query))
        .cloned()
        .partition(|name| name.to_lowercase().starts_with(&query));
    starts.extend(rest);
    starts
}

// ---- Keyboard shortcuts ----

/// The id of the rule a shortcut added from the settings screen or the
/// palette gets. A second one for the same command adds `~2`, and so on.
pub fn user_rule_id(command: &str) -> String {
    format!("user.key.{command}")
}

/// Whether a rule id is one the user added for `command`.
pub fn is_user_rule(id: &str, command: &str) -> bool {
    let base = user_rule_id(command);
    id == base
        || id
            .strip_prefix(&base)
            .is_some_and(|rest| rest.starts_with('~'))
}

/// One key that runs a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutKey {
    /// As this platform writes it, such as `Ctrl+Shift+P` or `⇧⌘P`.
    pub label: String,
    /// The id of the user's rule, when the user added this key.
    pub user_rule: Option<String>,
}

/// A command and the keys bound to it on this platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutRow {
    pub id: String,
    pub title: String,
    pub category: String,
    pub keys: Vec<ShortcutKey>,
    /// (key, other command's title) for each key another command in the
    /// same key context also uses.
    pub conflicts: Vec<(String, String)>,
}

impl ShortcutRow {
    pub fn matches(&self, query: &str) -> bool {
        let keys: Vec<&str> = self.keys.iter().map(|key| key.label.as_str()).collect();
        let haystack = format!(
            "{} {} {} {}",
            self.title,
            self.id,
            self.category,
            keys.join(" ")
        );
        words_match(&haystack, query)
    }

    pub fn labels(&self) -> Vec<String> {
        self.keys.iter().map(|key| key.label.clone()).collect()
    }
}

/// The key context a command's keys bind in: the editor's for commands
/// the editor view runs, the workspace's for the rest.
pub fn key_context(command: &str) -> &'static str {
    if crate::commands::handles(command) {
        crate::keymap::KEY_CONTEXT
    } else {
        crate::keymap::WORKSPACE_CONTEXT
    }
}

/// A key rule reduced to what conflicts compare.
struct Bound<'a> {
    command: &'a str,
    chord: KeyChord,
    context: &'static str,
}

fn bound_keys(rules: &RuleSet, platform: Platform) -> Vec<Bound<'_>> {
    rules
        .key_rules(platform)
        .filter(|rule| rule.when.is_none())
        .filter_map(|rule| {
            Some(Bound {
                command: rule.command.as_str(),
                chord: rule.chord_for(platform)?,
                context: key_context(&rule.command),
            })
        })
        .collect()
}

/// Titles of the other commands that `chord` runs in `command`'s context.
pub fn conflicts_for(
    rules: &RuleSet,
    platform: Platform,
    command: &str,
    chord: KeyChord,
) -> Vec<String> {
    let chord = chord.resolve(platform);
    let context = key_context(command);
    let mut titles: Vec<String> = bound_keys(rules, platform)
        .into_iter()
        .filter(|bound| bound.command != command && bound.chord == chord)
        .filter(|bound| bound.context == context)
        .map(|bound| command_title(bound.command))
        .collect();
    titles.dedup();
    titles
}

/// A command's title, or its id when it isn't built in.
pub fn command_title(command: &str) -> String {
    BUILTIN_COMMANDS
        .iter()
        .find(|spec| spec.id == command)
        .map_or_else(|| command.to_string(), |spec| spec.title.to_string())
}

/// Every built-in command with its keys, in registry order.
pub fn shortcut_rows(rules: &RuleSet, platform: Platform) -> Vec<ShortcutRow> {
    BUILTIN_COMMANDS
        .iter()
        .map(|spec| shortcut_row(rules, platform, spec.id, spec.title, spec.category))
        .collect()
}

fn shortcut_row(
    rules: &RuleSet,
    platform: Platform,
    id: &str,
    title: &str,
    category: &str,
) -> ShortcutRow {
    let mut keys = Vec::new();
    let mut conflicts = Vec::new();
    for rule in rules.key_rules(platform).filter(|rule| rule.command == id) {
        let Some(chord) = rule.keys else {
            continue;
        };
        let label = shortcut_label(chord, platform);
        if rule.when.is_none() {
            for other in conflicts_for(rules, platform, id, chord) {
                conflicts.push((label.clone(), other));
            }
        }
        let user_rule = rule.id.clone().filter(|rule_id| is_user_rule(rule_id, id));
        keys.push(ShortcutKey { label, user_rule });
    }
    ShortcutRow {
        id: id.to_string(),
        title: title.to_string(),
        category: category.to_string(),
        keys,
        conflicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_wired_setting_is_on_a_page_once() {
        let items = setting_items();
        let mut placed = Vec::new();
        for spec in PAGES {
            for card in page_cards(spec.page, &items) {
                for row in card {
                    if let RowSpec::Setting(key) = row {
                        placed.push(key.to_string());
                    }
                }
            }
        }
        for item in &items {
            let count = placed.iter().filter(|key| **key == item.key).count();
            assert_eq!(count, 1, "{} is placed {count} times", item.key);
        }
        for key in &placed {
            let known = setting_descriptors().iter().any(|d| d.key == *key);
            assert!(known, "{key} isn't a setting");
        }
    }

    #[test]
    fn unwired_settings_are_hidden() {
        let keys: Vec<String> = setting_items().into_iter().map(|item| item.key).collect();
        assert!(!keys.iter().any(|key| key.starts_with("prose.")));
        assert!(keys.contains(&"files.trash".to_string()));
    }

    #[test]
    fn settings_have_titles_and_descriptions() {
        for item in setting_items() {
            assert!(!item.title.is_empty(), "{}", item.key);
            assert!(
                !item.description.is_empty(),
                "{} has no description",
                item.key
            );
        }
    }

    #[test]
    fn every_choice_option_has_a_label() {
        for descriptor in setting_descriptors() {
            let options = match &descriptor.kind {
                SettingKind::Choice(options) => options.clone(),
                SettingKind::Map(inner) => match inner.as_ref() {
                    SettingKind::Choice(options) => options.clone(),
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            };
            for option in options {
                assert!(
                    CHOICE_LABELS.iter().any(|(known, _)| *known == option),
                    "{} has no label for {option}",
                    descriptor.key
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
        assert_eq!(choice_label("hover"), "On hover");
        assert_eq!(choice_label("sideways"), "Sideways");
    }

    #[test]
    fn titles_in_the_table_are_real_keys() {
        let keys: Vec<String> = setting_descriptors().into_iter().map(|d| d.key).collect();
        for (key, ..) in TEXTS {
            assert!(keys.iter().any(|k| k == key), "{key} isn't a setting");
        }
    }

    #[test]
    fn search_matches_title_key_and_description() {
        let all = setting_items();
        let found = |query: &str| -> Vec<&str> {
            all.iter()
                .filter(|item| item.matches(query))
                .map(|item| item.key.as_str())
                .collect()
        };
        assert_eq!(found("trash"), ["files.trash"]);
        assert_eq!(found("ATTACHMENTS"), ["files.attachments-folder"]);
        assert!(found("pasted images").contains(&"files.attachments-folder"));
        assert!(found("no such thing").is_empty());
    }

    #[test]
    fn font_choices_put_the_current_font_first() {
        let system: Vec<String> = [
            "Noto Serif",
            ".Hidden",
            "DejaVu Sans",
            "Noto Serif",
            "Arial",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            font_choices(&system, "Charter", "Charter"),
            ["Charter", "Arial", "DejaVu Sans", "Noto Serif"]
        );
        assert_eq!(
            font_choices(&system, "DejaVu Sans", "Charter"),
            ["DejaVu Sans", "Charter", "Arial", "Noto Serif"]
        );
        assert_eq!(
            font_choices(&system, "Arial", "Noto Serif"),
            ["Arial", "Noto Serif", "DejaVu Sans"]
        );
    }

    #[test]
    fn font_filter_ranks_prefix_matches_first() {
        let choices: Vec<String> = ["Charter", "Noto Serif", "Serif Pro", "Noto Sans Mono"]
            .map(String::from)
            .to_vec();
        assert_eq!(filter_fonts(&choices, "serif"), ["Serif Pro", "Noto Serif"]);
        assert_eq!(filter_fonts(&choices, "noto mono"), ["Noto Sans Mono"]);
        assert_eq!(filter_fonts(&choices, "  "), choices);
        assert!(filter_fonts(&choices, "zzz").is_empty());
    }

    #[test]
    fn every_syntax_name_is_one_the_config_takes() {
        use editor_config::settings::SyntaxKind;
        for (name, _) in SYNTAX_NAMES {
            let parsed: Result<SyntaxKind, _> = serde_json::from_value(Value::from(*name));
            assert!(parsed.is_ok(), "{name}");
        }
        let names: std::collections::HashSet<_> = SYNTAX_NAMES.iter().map(|(n, _)| n).collect();
        assert_eq!(names.len(), SYNTAX_NAMES.len());
        assert_eq!(
            map_name_label("markdown.symbols.overrides", "link-url"),
            "Link addresses"
        );
        assert_eq!(map_name_label("other", "link-url"), "Link url");
    }

    #[test]
    fn shortcuts_list_every_command_with_its_keys() {
        let rows = shortcut_rows(&RuleSet::defaults(), Platform::Linux);
        assert_eq!(rows.len(), BUILTIN_COMMANDS.len());
        let settings = rows.iter().find(|row| row.id == "settings.open").unwrap();
        assert!(settings.labels().contains(&"Ctrl+,".to_string()));
        assert!(settings.matches("ctrl+,"));
        assert!(settings.keys.iter().all(|key| key.user_rule.is_none()));
        let mac = shortcut_rows(&RuleSet::defaults(), Platform::Macos);
        let bold = mac.iter().find(|row| row.id == "format.bold").unwrap();
        assert_eq!(bold.labels(), ["⌘B"]);
    }

    #[test]
    fn user_rules_are_marked_and_conflicts_found() {
        let mut rules = RuleSet::defaults();
        let user = "[[rule]]\nid = \"user.key.tab.new\"\non = \"key\"\nkeys = \"Mod+B\"\ndo = \"tab.new\"\n\n\
                    [[rule]]\nid = \"user.key.note.new~2\"\non = \"key\"\nkeys = \"Mod+Shift+B\"\ndo = \"note.new\"\n";
        rules.layer("rules.toml", user).unwrap();
        let rows = shortcut_rows(&rules, Platform::Linux);
        let tab = rows.iter().find(|row| row.id == "tab.new").unwrap();
        let added = tab.keys.iter().find(|key| key.label == "Ctrl+B").unwrap();
        assert_eq!(added.user_rule.as_deref(), Some("user.key.tab.new"));
        // Bold runs in the editor and New tab in the workspace: no clash.
        assert!(tab.conflicts.is_empty(), "{:?}", tab.conflicts);
        let note = rows.iter().find(|row| row.id == "note.new").unwrap();
        assert!(
            note.keys
                .iter()
                .any(|key| key.user_rule.as_deref() == Some("user.key.note.new~2"))
        );
        let chord = KeyChord::parse("Mod+,").unwrap();
        let clash = conflicts_for(&rules, Platform::Linux, "note.new", chord);
        assert_eq!(clash, ["Open settings"]);
    }

    #[test]
    fn default_keys_do_not_clash() {
        for row in shortcut_rows(&RuleSet::defaults(), Platform::Linux) {
            assert!(row.conflicts.is_empty(), "{}: {:?}", row.id, row.conflicts);
        }
    }

    #[test]
    fn user_rule_ids_match_only_their_command() {
        assert!(is_user_rule("user.key.tab.go-1", "tab.go-1"));
        assert!(is_user_rule("user.key.tab.go-1~3", "tab.go-1"));
        assert!(!is_user_rule("user.key.tab.go-10", "tab.go-1"));
        assert!(!is_user_rule("key.tab.go-1", "tab.go-1"));
    }
}
