//! Typed settings from `settings.toml`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use toml::Value;

use crate::merge::flatten;

/// Every synced setting. Missing keys take their default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Settings {
    pub sidebar: SidebarSettings,
    pub markdown: MarkdownSettings,
    pub prose: ProseSettings,
    pub files: FileSettings,
    pub editor: EditorSettings,
    pub math: MathSettings,
    pub appearance: AppearanceSettings,
    pub sync: SyncSettings,
    pub daily_notes: DailyNoteSettings,
    pub templates: TemplateSettings,
    pub recovery: RecoverySettings,
    pub mcp: McpSettings,
    pub telemetry: TelemetrySettings,
    pub updates: UpdateSettings,
    /// Only still read so an older file loads; see [`LegacyMobileSettings`].
    #[schemars(skip)]
    #[serde(skip_serializing_if = "LegacyMobileSettings::is_empty")]
    pub mobile: LegacyMobileSettings,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct SidebarSettings {
    /// The file sidebar.
    pub files: FileSidebarSettings,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct FileSidebarSettings {
    /// How the file sidebar appears.
    pub reveal: SidebarReveal,
    /// Whether the sidebar slides over the text or pushes it aside.
    pub mode: SidebarMode,
}

/// How a sidebar is revealed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SidebarReveal {
    Always,
    Toggle,
    #[default]
    Hover,
}

/// Whether a sidebar covers the text or moves it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SidebarMode {
    #[default]
    Overlay,
    Push,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct MarkdownSettings {
    /// When Markdown symbols such as `**` and `#` are visible.
    pub symbols: SymbolSettings,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct SymbolSettings {
    /// When symbols show.
    pub mode: SymbolMode,
    /// How much is revealed around the cursor.
    pub scope: RevealScope,
    /// Per-syntax modes that replace `mode`.
    pub overrides: BTreeMap<SyntaxKind, SymbolMode>,
}

impl SymbolSettings {
    /// The effective mode for one kind of syntax.
    pub fn mode_for(&self, syntax: SyntaxKind) -> SymbolMode {
        self.overrides.get(&syntax).copied().unwrap_or(self.mode)
    }
}

/// When Markdown symbols are visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolMode {
    AlwaysShown,
    #[default]
    AroundCursor,
    AlwaysHidden,
}

impl SymbolMode {
    const CYCLE: [SymbolMode; 3] = [
        SymbolMode::AlwaysShown,
        SymbolMode::AroundCursor,
        SymbolMode::AlwaysHidden,
    ];

    /// The next mode for `markdown.cycle-symbols`.
    pub fn next(self) -> SymbolMode {
        let index = Self::CYCLE.iter().position(|m| *m == self).unwrap_or(0);
        Self::CYCLE[(index + 1) % Self::CYCLE.len()]
    }
}

/// How much text is revealed around the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RevealScope {
    #[default]
    Element,
    Line,
    Block,
}

/// A kind of Markdown syntax that can override the symbol mode.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum SyntaxKind {
    Emphasis,
    Strong,
    Strikethrough,
    Highlight,
    Heading,
    LinkUrl,
    LinkText,
    Wikilink,
    InlineCode,
    CodeFence,
    Math,
    Blockquote,
    Callout,
    Footnote,
    Comment,
    Html,
    ListMarker,
    Frontmatter,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ProseSettings {
    /// Colouring sentences by length.
    pub sentence_length: SentenceLengthSettings,
    /// Underlining misspellings and mechanical problems.
    pub grammar: GrammarSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GrammarSettings {
    /// Whether problems are underlined as you write.
    pub enabled: bool,
    /// Whether misspelled words are underlined too.
    pub spelling: bool,
    /// Which English spelling follows.
    pub english: EnglishVariant,
}

impl Default for GrammarSettings {
    fn default() -> Self {
        GrammarSettings {
            enabled: true,
            spelling: true,
            english: EnglishVariant::American,
        }
    }
}

/// A variety of English, for spelling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EnglishVariant {
    #[default]
    American,
    British,
    Canadian,
    Australian,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct SentenceLengthSettings {
    /// Whether sentence-length highlighting is on.
    pub enabled: bool,
    /// Sentences with fewer words than this are short.
    pub short_below: u32,
    /// Sentences with more words than this are long.
    pub long_above: u32,
}

impl Default for SentenceLengthSettings {
    fn default() -> Self {
        SentenceLengthSettings {
            // Off, as the owner's Musical Text had it; Mod+J turns it on.
            enabled: false,
            short_below: 7,
            long_above: 18,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct FileSettings {
    /// Where pasted images go, relative to the note.
    pub attachments_folder: String,
    /// Whether renaming a note updates links to it.
    pub update_links_on_rename: bool,
    /// Where deleted files go.
    pub trash: TrashMode,
}

impl Default for FileSettings {
    fn default() -> Self {
        FileSettings {
            attachments_folder: "./images".to_string(),
            update_links_on_rename: true,
            trash: TrashMode::System,
        }
    }
}

/// Where deleted files go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TrashMode {
    #[default]
    System,
    Vault,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct EditorSettings {
    /// Whether the note's file name shows as a title above the text.
    pub show_inline_title: bool,
    /// Whether typed straight quotes become curly ones, outside code,
    /// math, frontmatter and links.
    pub smart_quotes: bool,
    /// Whether pasted text gets curly quotes too, while smart quotes are on.
    pub curl_pasted_quotes: bool,
    /// Whether typing an opening bracket adds its closing one, and typing
    /// a mark such as `*` over a selection wraps it.
    pub auto_pair: bool,
    /// Whether code blocks number their lines. A block's `ln:` option
    /// overrides it either way.
    pub code_line_numbers: bool,
    /// Whether numbered footnotes renumber in reading order, and `^[1]`
    /// typos become `[^1]`, once typing pauses.
    pub renumber_footnotes: bool,
    /// Whether snippets from `snippets.txt` expand as you type.
    pub snippets: bool,
    /// Whether the replacements in `replacements.toml`, such as `--` to an
    /// em dash, apply as you type.
    pub replacements: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        EditorSettings {
            show_inline_title: true,
            smart_quotes: true,
            curl_pasted_quotes: true,
            auto_pair: true,
            code_line_numbers: false,
            renumber_footnotes: true,
            snippets: true,
            replacements: true,
        }
    }
}

/// Latex Suite's typing helpers for math, besides snippets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct MathSettings {
    /// Whether typing `/` after a term in math makes a fraction of it.
    pub auto_fraction: bool,
    /// Whether Tab adds ` & ` and Enter ends the row inside matrices,
    /// `cases`, `align` and similar environments.
    pub matrix_shortcuts: bool,
    /// Whether Tab in math jumps past the next closing bracket, or out of
    /// the math at its end.
    pub tab_out: bool,
    /// Whether brackets around a sum, integral or fraction become
    /// `\left(` and `\right)` after a snippet expands.
    pub enlarge_brackets: bool,
    /// Whether matching brackets in math source share a colour.
    pub bracket_colours: bool,
}

impl Default for MathSettings {
    fn default() -> Self {
        MathSettings {
            auto_fraction: true,
            matrix_shortcuts: true,
            tab_out: true,
            enlarge_brackets: true,
            bracket_colours: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DailyNoteSettings {
    /// The folder daily notes go in, from the vault root. Empty is the root.
    pub folder: String,
    /// The note's name as a date pattern: `YYYY` year, `MM` month, `DD`
    /// day, such as `YYYY-MM-DD`.
    pub format: String,
    /// A note whose text starts each new daily note. Empty starts it blank.
    pub template: String,
}

impl Default for DailyNoteSettings {
    fn default() -> Self {
        DailyNoteSettings {
            folder: String::new(),
            format: "YYYY-MM-DD".to_string(),
            template: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TemplateSettings {
    /// The folder templates are kept in, from the vault root.
    pub folder: String,
    /// How `{{date}}` is written, as a date pattern such as `YYYY-MM-DD`.
    pub date_format: String,
    /// How `{{time}}` is written, as a time pattern such as `HH:mm`.
    pub time_format: String,
}

impl Default for TemplateSettings {
    fn default() -> Self {
        TemplateSettings {
            folder: "Templates".to_string(),
            date_format: "YYYY-MM-DD".to_string(),
            time_format: "HH:mm".to_string(),
        }
    }
}

/// Local snapshots of notes, kept outside the vault, for recovering an
/// earlier version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct RecoverySettings {
    /// Minutes between snapshots of a note while it's being edited.
    pub interval_minutes: u32,
    /// Days a snapshot is kept.
    pub keep_days: u32,
}

impl Default for RecoverySettings {
    fn default() -> Self {
        RecoverySettings {
            interval_minutes: 5,
            keep_days: 7,
        }
    }
}

/// Agents' access to the running app through `gasp mcp`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct McpSettings {
    /// Whether agents using `gasp mcp` can see and change what's open in
    /// the app. Reading and writing the vault's files works either way.
    pub enabled: bool,
}

impl Default for McpSettings {
    fn default() -> Self {
        McpSettings { enabled: true }
    }
}

/// The once-a-day count of who uses the app, which gaspmd.com/privacy
/// describes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TelemetrySettings {
    /// Whether the app sends its version, platform, system version and
    /// chip type once a day. Never notes, file names or an identifier.
    pub enabled: bool,
}

impl Default for TelemetrySettings {
    fn default() -> Self {
        TelemetrySettings { enabled: true }
    }
}

/// The Mac app's daily look at gaspmd.com for a newer version, which
/// gaspmd.com/privacy describes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct UpdateSettings {
    /// Whether the app asks once a day for the newest version number and
    /// offers it. It sends nothing about the person or the Mac.
    pub check: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        UpdateSettings { check: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct AppearanceSettings {
    /// The base text size in points. Theme font sizes scale from it.
    pub base_font_size: u32,
    /// Light or dark, or whichever the system uses.
    pub theme: ThemeChoice,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        AppearanceSettings {
            base_font_size: 12,
            theme: ThemeChoice::MatchSystem,
        }
    }
}

/// Which palette the app uses. `MatchSystem` follows the system's light
/// or dark appearance as it changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeChoice {
    Light,
    Dark,
    #[default]
    MatchSystem,
}

impl ThemeChoice {
    /// Whether the app is dark, given whether the system is.
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            ThemeChoice::Light => false,
            ThemeChoice::Dark => true,
            ThemeChoice::MatchSystem => system_dark,
        }
    }
}

/// Files that stay on the device where they're written, as globs relative
/// to the vault root. The legacy config folder's device file stays in the
/// list for one release, while other devices may still sync the old name.
pub const DEFAULT_DEVICE_ONLY: &[&str] = &[
    concat!(crate::config_dir!(), "/device.toml"),
    concat!(crate::names::legacy_config_dir!(), "/device.toml"),
    ".obsidian/workspace*.json",
    "**/.DS_Store",
    ".trash/**",
];

/// The iPhone app's old settings. Its keyboard bar is now the `keyboard`
/// toolbar in `toolbars.toml`, where
/// [`crate::migration::migrate_mobile_toolbar`] moves a vault's
/// `mobile.toolbar`; until it has, the key still loads and does nothing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct LegacyMobileSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolbar: Option<Vec<String>>,
}

impl LegacyMobileSettings {
    pub fn is_empty(&self) -> bool {
        self.toolbar.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct SyncSettings {
    /// Whether the vault syncs on its own after edits and every few minutes.
    pub auto: bool,
    /// Minutes between checks for changes from other devices.
    pub interval_minutes: u32,
    /// The branch this app commits to and pushes.
    pub branch: String,
    /// A branch older sync tools push to, merged in one way. Empty turns it off.
    pub legacy_branch: String,
    /// Files that never sync, as globs relative to the vault.
    pub device_only: Vec<String>,
}

impl Default for SyncSettings {
    fn default() -> Self {
        SyncSettings {
            auto: true,
            interval_minutes: 5,
            branch: "master".to_string(),
            legacy_branch: "main".to_string(),
            device_only: DEFAULT_DEVICE_ONLY
                .iter()
                .map(|glob| glob.to_string())
                .collect(),
        }
    }
}

/// Settings flattened to dotted keys, for rule conditions such as `sidebar.files.reveal`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsIndex(BTreeMap<String, Value>);

impl SettingsIndex {
    pub fn new(settings: &Settings) -> SettingsIndex {
        let table = toml::Table::try_from(settings).unwrap_or_default();
        SettingsIndex(flatten(&table).into_iter().collect())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_mode_cycles_through_all_three() {
        let start = SymbolMode::AlwaysShown;
        assert_eq!(start.next(), SymbolMode::AroundCursor);
        assert_eq!(start.next().next(), SymbolMode::AlwaysHidden);
        assert_eq!(start.next().next().next(), start);
    }

    #[test]
    fn overrides_replace_the_global_mode() {
        let mut symbols = SymbolSettings::default();
        symbols
            .overrides
            .insert(SyntaxKind::LinkUrl, SymbolMode::AlwaysHidden);
        assert_eq!(
            symbols.mode_for(SyntaxKind::LinkUrl),
            SymbolMode::AlwaysHidden
        );
        assert_eq!(
            symbols.mode_for(SyntaxKind::Emphasis),
            SymbolMode::AroundCursor
        );
    }

    #[test]
    fn partial_files_fill_in_defaults() {
        let settings: Settings = toml::from_str("[sidebar.files]\nmode = \"push\"\n").unwrap();
        assert_eq!(settings.sidebar.files.mode, SidebarMode::Push);
        assert_eq!(settings.sidebar.files.reveal, SidebarReveal::Hover);
        assert_eq!(settings.files, FileSettings::default());
    }

    #[test]
    fn overrides_parse_from_toml() {
        let text = "[markdown.symbols.overrides]\nlink-url = \"always-hidden\"\n";
        let settings: Settings = toml::from_str(text).unwrap();
        assert_eq!(
            settings.markdown.symbols.mode_for(SyntaxKind::LinkUrl),
            SymbolMode::AlwaysHidden
        );
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Settings>("[sidebar.files]\nrevel = \"hover\"\n").is_err());
    }

    #[test]
    fn index_exposes_dotted_keys() {
        let index = SettingsIndex::new(&Settings::default());
        assert_eq!(
            index.get("sidebar.files.reveal"),
            Some(&Value::String("hover".into()))
        );
        assert_eq!(
            index.get("appearance.base-font-size"),
            Some(&Value::Integer(12))
        );
    }
}
