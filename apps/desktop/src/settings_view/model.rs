//! What the settings screen shows: its pages, the rows on each page
//! (generated from the settings schema, so none drifts from the files),
//! titles and descriptions, search, and the list of keyboard shortcuts.

use std::borrow::Cow;
use std::sync::OnceLock;

use gasp_config::commands::BUILTIN_COMMANDS;
use gasp_config::keys::KeyChord;
use gasp_config::schema::{SettingKind, setting_descriptors};
use gasp_config::setting_texts::setting_text;
use gasp_config::{Platform, RuleSet};
use serde_json::Value;

use crate::icons::IconName;
use crate::picker::shortcut::{KeyQuery, Shortcut};

/// Settings the desktop app doesn't read yet, by key prefix. Showing them
/// would be controls that do nothing, so they stay hidden until their
/// feature lands.
const UNWIRED: &[&str] = &["mobile."];

/// Settings that only apply while another (a switch) is on, as
/// (setting, the switch it needs). Their rows fade and stop taking input
/// while the switch is off.
const REQUIRES: &[(&str, &str)] = &[
    ("editor.curl-pasted-quotes", "editor.smart-quotes"),
    (
        "prose.sentence-length.short-below",
        "prose.sentence-length.enabled",
    ),
    (
        "prose.sentence-length.long-above",
        "prose.sentence-length.enabled",
    ),
    ("prose.grammar.spelling", "prose.grammar.enabled"),
    ("prose.grammar.english", "prose.grammar.spelling"),
];

/// The switch `key` needs on to apply, if any.
pub fn required_switch(key: &str) -> Option<&'static str> {
    REQUIRES
        .iter()
        .find(|(setting, _)| *setting == key)
        .map(|(_, switch)| *switch)
}

/// The smallest value a number setting takes, when it isn't zero.
const MINIMUMS: &[(&str, i64)] = &[
    ("appearance.base-font-size", 6),
    ("sync.interval-minutes", 1),
    ("recovery.interval-minutes", 1),
    ("recovery.keep-days", 1),
    ("prose.sentence-length.short-below", 1),
    ("prose.sentence-length.long-above", 1),
];

/// One page of the settings screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    General,
    Sync,
    Appearance,
    Sidebar,
    Shortcuts,
    Toolbars,
    Editor,
    Files,
    DailyNotes,
    Prose,
    Snippets,
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

/// The theme token the accent row edits in light mode.
pub const ACCENT_TOKEN: &str = "color.accent";
/// The theme token the accent row edits in dark mode, which has its own.
pub const DARK_ACCENT_TOKEN: &str = "dark.color.accent";
pub const ACCENT_TITLE: &str = "Accent colour";
pub const ACCENT_DESCRIPTION: &str = "Used for the cursor, links and switches that are on.";

/// The App icon row's credit. The whale's source model is CC BY 4.0, which asks for a credit people can find.
pub const ICON_CREDIT: &str = "The humpback is drawn from a 3D model by Gutarra Díaz, Stubbs, Moon, Palmer and Benton, shared under CC BY 4.0.";

/// The Obsidian import row: what it brings in, and that nothing set here
/// is overwritten.
pub const OBSIDIAN_IMPORT_TITLE: &str = "Settings from Obsidian";
pub const OBSIDIAN_IMPORT_DESCRIPTION: &str = "Brings in Latex Suite snippets, typing replacements, hotkeys and app settings from the vault’s .obsidian folder. Anything already set here stays as it is.";

/// The archive the whale's source model is published in.
pub const ICON_SOURCE_URL: &str = "https://doi.org/10.5281/zenodo.5979631";

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
    /// Credit for the whale in the app icon, which its licence requires.
    IconCredit,
    /// Importing the settings of an Obsidian vault.
    ObsidianImport,
    /// The address of the repository the vault syncs with.
    SyncRemote,
    /// Signing in to it with a token.
    SyncAccount,
    /// The AI apps on this Mac, each with a button that connects it.
    AgentApps,
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
        cards: &[
            &[RowSpec::Vault, RowSpec::Version],
            &[RowSpec::ObsidianImport],
            &[setting("mcp.enabled"), RowSpec::AgentApps],
            &[setting("telemetry.enabled"), setting("updates.check")],
            &[RowSpec::IconCredit],
        ],
    },
    PageSpec {
        page: Page::Sync,
        id: SYNC_SECTION,
        title: "Sync",
        icon: IconName::CloudCheck,
        group: "App",
        cards: &[
            &[RowSpec::SyncRemote, RowSpec::SyncAccount],
            &[setting("sync.auto"), setting("sync.interval-minutes")],
            &[setting("sync.branch"), setting("sync.legacy-branch")],
            &[setting("sync.device-only")],
        ],
    },
    PageSpec {
        page: Page::Appearance,
        id: "appearance",
        title: "Appearance",
        icon: IconName::Palette,
        group: "App",
        cards: &[
            &[setting("appearance.theme"), RowSpec::Accent],
            &[
                RowSpec::Font(FontSlot::Text),
                RowSpec::Font(FontSlot::Interface),
                RowSpec::Font(FontSlot::Code),
                setting("appearance.base-font-size"),
            ],
            &[
                setting("theme.font.line-height.body"),
                setting("theme.size.editor-max-width"),
                setting("theme.font.scale.title"),
                setting("theme.font.scale.h1"),
                setting("theme.font.scale.h2"),
                setting("theme.font.line-height.code"),
            ],
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
        page: Page::Toolbars,
        id: TOOLBARS_SECTION,
        title: "Toolbars",
        icon: IconName::AppWindow,
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
                setting("editor.smart-quotes"),
                setting("editor.curl-pasted-quotes"),
                setting("editor.auto-pair"),
            ],
            &[setting("editor.renumber-footnotes")],
            &[setting("editor.code-line-numbers")],
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
            &[setting("files.open-in-new-tab")],
            &[
                setting("files.attachments-folder"),
                setting("files.update-links-on-rename"),
            ],
            &[setting("files.trash")],
            &[
                setting("recovery.interval-minutes"),
                setting("recovery.keep-days"),
            ],
        ],
    },
    PageSpec {
        page: Page::DailyNotes,
        id: DAILY_NOTES_SECTION,
        title: "Daily notes and templates",
        icon: IconName::CalendarBlank,
        group: "Writing",
        cards: &[
            &[
                setting("daily-notes.folder"),
                setting("daily-notes.format"),
                setting("daily-notes.template"),
            ],
            &[
                setting("templates.folder"),
                setting("templates.date-format"),
                setting("templates.time-format"),
            ],
        ],
    },
    PageSpec {
        page: Page::Prose,
        id: "prose",
        title: "Prose",
        icon: IconName::Article,
        group: "Writing",
        cards: &[
            &[
                setting("prose.sentence-length.enabled"),
                setting("prose.sentence-length.short-below"),
                setting("prose.sentence-length.long-above"),
            ],
            &[
                setting("prose.grammar.enabled"),
                setting("prose.grammar.spelling"),
                setting("prose.grammar.english"),
            ],
        ],
    },
    PageSpec {
        page: Page::Snippets,
        id: SNIPPETS_SECTION,
        title: "Snippets and replacements",
        icon: IconName::Function,
        group: "Writing",
        cards: &[
            &[setting("editor.snippets"), setting("editor.replacements")],
            &[
                setting("math.auto-fraction"),
                setting("math.matrix-shortcuts"),
                setting("math.tab-out"),
                setting("math.enlarge-brackets"),
                setting("math.bracket-colours"),
            ],
        ],
    },
];

/// The id of the Daily notes and templates page.
pub const DAILY_NOTES_SECTION: &str = "daily-notes";

/// The id of the Snippets and replacements page.
pub const SNIPPETS_SECTION: &str = "snippets";

/// The id of the Sync page.
pub const SYNC_SECTION: &str = "sync";

/// The id of the Toolbars page.
pub const TOOLBARS_SECTION: &str = "toolbars";

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
        "files" | "recovery" => Page::Files,
        "sidebar" => Page::Sidebar,
        "prose" => Page::Prose,
        "daily-notes" | "templates" => Page::DailyNotes,
        "sync" => Page::Sync,
        "math" => Page::Snippets,
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

/// A number in the theme that the screen edits like a setting, under the
/// key `theme.<token>`.
pub struct ThemeNumber {
    pub token: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// How far one press of − or + moves it.
    pub step: f64,
    pub min: f64,
    pub max: f64,
}

pub const THEME_NUMBERS: &[ThemeNumber] = &[
    ThemeNumber {
        token: "font.line-height.body",
        title: "Line height",
        description: "Space from one line of a note to the next, as a multiple of the text size.",
        step: 0.05,
        min: 1.,
        max: 3.,
    },
    ThemeNumber {
        token: "size.editor-max-width",
        title: "Readable line length",
        description: "How wide the text column grows, in pixels, while readable line length is on.",
        step: 20.,
        min: 320.,
        max: 2000.,
    },
    ThemeNumber {
        token: "font.scale.title",
        title: "Title size",
        description: "The note’s title above its text, as a multiple of the text size.",
        step: 0.1,
        min: 1.,
        max: 4.,
    },
    ThemeNumber {
        token: "font.scale.h1",
        title: "Heading 1 size",
        description: "As a multiple of the text size.",
        step: 0.05,
        min: 0.5,
        max: 4.,
    },
    ThemeNumber {
        token: "font.scale.h2",
        title: "Heading 2 size",
        description: "As a multiple of the text size.",
        step: 0.05,
        min: 0.5,
        max: 4.,
    },
    ThemeNumber {
        token: "font.line-height.code",
        title: "Code line height",
        description: "Line spacing inside code blocks, as a multiple of the code size.",
        step: 0.05,
        min: 1.,
        max: 3.,
    },
];

/// The theme number a setting key names, if it names one.
pub fn theme_number(key: &str) -> Option<&'static ThemeNumber> {
    let token = key.strip_prefix("theme.")?;
    THEME_NUMBERS.iter().find(|number| number.token == token)
}

impl ThemeNumber {
    /// `value` moved by `steps` steps, kept in range and rounded to the
    /// step so repeated presses don't gather float error.
    pub fn stepped(&self, value: f64, steps: i64) -> f64 {
        let moved = value + steps as f64 * self.step;
        self.clamp((moved / self.step).round() * self.step)
    }

    pub fn clamp(&self, value: f64) -> f64 {
        let rounded = (value * 100.).round() / 100.;
        rounded.clamp(self.min, self.max)
    }

    fn item(&self, default: Option<f64>) -> SettingItem {
        SettingItem {
            key: format!("theme.{}", self.token),
            title: self.title.to_string(),
            description: self.description.to_string(),
            kind: SettingKind::Number,
            default: default.map_or(Value::Null, Value::from),
        }
    }
}

/// A setting item for each theme number, with the built-in value from
/// `default_of`.
pub fn theme_number_items(default_of: impl Fn(&str) -> Option<f64>) -> Vec<SettingItem> {
    THEME_NUMBERS
        .iter()
        .map(|number| number.item(default_of(number.token)))
        .collect()
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
        let (title, description) = match setting_text(&key) {
            Some((title, description)) => (title.to_string(), description.to_string()),
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
    gasp_config::setting_texts::choice_label(value).map_or_else(|| humanize(value), str::to_owned)
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

pub use gasp_config::config_files::{is_user_rule, user_rule_id};

/// One key that runs a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutKey {
    /// As this platform writes it, such as `Ctrl+Shift+P` or `⇧⌘P`.
    pub label: String,
    pub shortcut: Shortcut,
    /// The id of the rule binding it, which removing it goes by.
    pub rule: Option<String>,
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
    /// The keys the command comes with, when the user changed them.
    pub changed_from: Option<Vec<Shortcut>>,
    /// What text searches look in, lowercased once.
    haystack: String,
}

/// A search as the shortcut rows read it: words, and the same text read
/// as keys when it can be.
pub struct ShortcutQuery {
    words: String,
    keys: Option<KeyQuery>,
}

impl ShortcutQuery {
    pub fn new(query: &str) -> ShortcutQuery {
        ShortcutQuery {
            words: query.trim().to_lowercase(),
            keys: KeyQuery::parse(query, Platform::current()),
        }
    }

    /// The keys searched for, when the search reads as keys.
    pub fn keys(&self) -> Option<&KeyQuery> {
        self.keys.as_ref()
    }
}

impl ShortcutRow {
    /// Whether the row matches a search: by its words, or by its keys
    /// when the search reads as keys ("cmd f", "ctrl shift p").
    pub fn matches(&self, query: &str) -> bool {
        self.matches_query(&ShortcutQuery::new(query))
    }

    pub fn matches_query(&self, query: &ShortcutQuery) -> bool {
        if let Some(keys) = &query.keys {
            if self.keys.iter().any(|key| keys.matches(key.shortcut.chord)) {
                return true;
            }
            if keys.only_keys {
                return false;
            }
        }
        query
            .words
            .split_whitespace()
            .all(|word| self.haystack.contains(word))
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
    conflicts_in(&bound_keys(rules, platform), platform, command, chord)
}

fn conflicts_in(
    bound: &[Bound<'_>],
    platform: Platform,
    command: &str,
    chord: KeyChord,
) -> Vec<String> {
    let chord = chord.resolve(platform);
    let context = key_context(command);
    let mut titles: Vec<String> = bound
        .iter()
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

/// The built-in rules, parsed once.
pub fn default_rules() -> &'static RuleSet {
    static DEFAULTS: OnceLock<RuleSet> = OnceLock::new();
    DEFAULTS.get_or_init(RuleSet::defaults)
}

/// The ids of the built-in key rules that run `command`, on any platform.
pub fn default_rule_ids(command: &str) -> Vec<String> {
    default_rules()
        .rules()
        .iter()
        .filter(|rule| rule.is_key() && rule.command == command)
        .filter_map(|rule| rule.id.clone())
        .collect()
}

/// Every built-in command with its keys, in registry order.
pub fn shortcut_rows(rules: &RuleSet, platform: Platform) -> Vec<ShortcutRow> {
    let bound = bound_keys(rules, platform);
    BUILTIN_COMMANDS
        .iter()
        .map(|spec| {
            let mut row = ShortcutRow {
                id: spec.id.to_string(),
                title: spec.title.to_string(),
                category: spec.category.to_string(),
                keys: Vec::new(),
                conflicts: Vec::new(),
                changed_from: None,
                haystack: String::new(),
            };
            fill_keys(&mut row, rules, &bound, platform);
            row
        })
        .collect()
}

fn fill_keys(row: &mut ShortcutRow, rules: &RuleSet, bound: &[Bound<'_>], platform: Platform) {
    for rule in rules
        .key_rules(platform)
        .filter(|rule| rule.command == row.id)
    {
        let Some(chord) = rule.keys else {
            continue;
        };
        let shortcut = Shortcut::new(chord, platform);
        let label = shortcut.label();
        if rule.when.is_none() {
            for other in conflicts_in(bound, platform, &row.id, chord) {
                row.conflicts.push((label.clone(), other));
            }
        }
        let user_rule = rule
            .id
            .clone()
            .filter(|rule_id| is_user_rule(rule_id, &row.id));
        row.keys.push(ShortcutKey {
            label,
            shortcut,
            rule: rule.id.clone(),
            user_rule,
        });
    }
    let defaults: Vec<Shortcut> = default_rules()
        .keys_for(&row.id, platform)
        .into_iter()
        .map(|chord| Shortcut::new(chord, platform))
        .collect();
    let mut now: Vec<KeyChord> = row.keys.iter().map(|key| key.shortcut.chord).collect();
    let mut before: Vec<KeyChord> = defaults.iter().map(|shortcut| shortcut.chord).collect();
    now.sort();
    before.sort();
    row.changed_from = (now != before).then_some(defaults);
    row.haystack = format!(
        "{} {} {} {}",
        row.title,
        row.id,
        row.category,
        row.labels().join(" ")
    )
    .to_lowercase();
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
            let known =
                setting_descriptors().iter().any(|d| d.key == *key) || theme_number(key).is_some();
            assert!(known, "{key} isn't a setting");
        }
    }

    #[test]
    fn prose_and_recovery_settings_show() {
        let keys: Vec<String> = setting_items().into_iter().map(|item| item.key).collect();
        for key in [
            "prose.sentence-length.enabled",
            "prose.grammar.enabled",
            "recovery.keep-days",
            "files.trash",
        ] {
            assert!(keys.contains(&key.to_string()), "{key}");
        }
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
                    gasp_config::setting_texts::choice_label(&option).is_some(),
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
    fn theme_numbers_step_cleanly_and_stay_in_range() {
        let line_height = theme_number("theme.font.line-height.body").unwrap();
        assert_eq!(line_height.stepped(1.6, 1), 1.65);
        assert_eq!(line_height.stepped(1.6, 3), 1.75);
        assert_eq!(line_height.stepped(1.0, -1), 1.0);
        let width = theme_number("theme.size.editor-max-width").unwrap();
        assert_eq!(width.stepped(720., 1), 740.);
        assert!(theme_number("font.line-height.body").is_none());
        let defaults =
            theme_number_items(|token| (token == "font.line-height.body").then_some(1.6));
        assert_eq!(defaults.len(), THEME_NUMBERS.len());
        assert_eq!(defaults[0].default, Value::from(1.6));
    }

    #[test]
    fn every_syntax_name_is_one_the_config_takes() {
        use gasp_config::settings::SyntaxKind;
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
