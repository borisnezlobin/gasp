//! Loading the `.gasp/` folder: built-in defaults with the user's files layered on top.
//!
//! Each file is loaded on its own. When one has an error, its diagnostics are reported
//! and that part of the config keeps its last good version.

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use toml::Table;

use crate::commands::{BUILTIN_COMMANDS, PLATFORM_COMMANDS};
use crate::device::DeviceSettings;
use crate::diagnostics::{Diagnostic, span_of_key};
use crate::layout::{LayoutNode, LayoutSpec};
use crate::merge::deep_merge;
use crate::rules::RuleSet;
use crate::settings::Settings;
use crate::store::SettingsFile;
use crate::theme::{Theme, TokenSet};
use crate::toolbars::{Toolbars, build_toolbars};
use crate::typing::{TypingTables, build_replacements, build_snippets};

pub use crate::names::CONFIG_DIR;

pub const DEFAULT_SETTINGS: &str = include_str!("../defaults/settings.toml");
pub const DEFAULT_THEME: &str = include_str!("../defaults/theme.toml");
pub const DEFAULT_LAYOUT: &str = include_str!("../defaults/layout.toml");

/// One file in the config folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConfigFile {
    Settings,
    Theme,
    Layout,
    Rules,
    Device,
    Snippets,
    Replacements,
    Toolbars,
}

const FILES: &[(ConfigFile, &str, bool)] = &[
    (ConfigFile::Settings, "settings.toml", true),
    (ConfigFile::Theme, "theme.toml", true),
    (ConfigFile::Layout, "layout.toml", true),
    (ConfigFile::Rules, "rules.toml", true),
    (ConfigFile::Device, "device.toml", false),
    (ConfigFile::Snippets, "snippets.txt", true),
    (ConfigFile::Replacements, "replacements.toml", true),
    (ConfigFile::Toolbars, "toolbars.toml", true),
];

impl ConfigFile {
    pub const ALL: [ConfigFile; 8] = [
        ConfigFile::Settings,
        ConfigFile::Theme,
        ConfigFile::Layout,
        ConfigFile::Rules,
        ConfigFile::Device,
        ConfigFile::Snippets,
        ConfigFile::Replacements,
        ConfigFile::Toolbars,
    ];

    pub fn file_name(self) -> &'static str {
        FILES
            .iter()
            .find(|(file, _, _)| *file == self)
            .map_or("", |(_, name, _)| name)
    }

    /// Whether the file syncs with the vault. `device.toml` never does.
    pub fn is_synced(self) -> bool {
        FILES
            .iter()
            .any(|(file, _, synced)| *file == self && *synced)
    }

    /// The config file a path refers to, by file name.
    pub fn from_path(path: &Path) -> Option<ConfigFile> {
        let name = path.file_name()?.to_str()?;
        if crate::device_file::is_device_file_name(name) {
            return Some(ConfigFile::Device);
        }
        FILES
            .iter()
            .find(|(_, file_name, _)| *file_name == name)
            .map(|(file, _, _)| *file)
    }
}

/// Everything loaded from the config folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub settings: Settings,
    pub theme: Theme,
    pub layout: LayoutNode,
    pub rules: RuleSet,
    pub device: DeviceSettings,
    /// Snippets and replacements.
    pub typing: TypingTables,
    pub toolbars: Toolbars,
}

/// The built-in config, parsed once: every editor and config load starts
/// from it.
static DEFAULTS: LazyLock<Config> = LazyLock::new(Config::parse_defaults);

impl Config {
    /// The built-in config with no user files.
    pub fn defaults() -> Config {
        DEFAULTS.clone()
    }

    fn parse_defaults() -> Config {
        Config {
            settings: built_or_default(build_settings("settings.toml", None)),
            theme: built_or_default(build_theme("theme.toml", None)),
            layout: build_layout("layout.toml", None)
                .map(|(layout, _)| layout)
                .expect("built-in layout is valid"),
            rules: RuleSet::defaults(),
            device: DeviceSettings::default(),
            typing: TypingTables::default(),
            toolbars: Toolbars::defaults(),
        }
    }
}

fn built_or_default<T: Default>(built: Result<(T, Vec<Diagnostic>), Vec<Diagnostic>>) -> T {
    built.map(|(value, _)| value).unwrap_or_default()
}

pub(crate) type Built<T> = Result<(T, Vec<Diagnostic>), Vec<Diagnostic>>;

fn parse_table(file: &str, text: &str) -> Result<Table, Vec<Diagnostic>> {
    toml::from_str(text).map_err(|error| vec![Diagnostic::from_toml(file, text, &error)])
}

/// Built-in settings with `user` layered on top.
pub fn build_settings(file: &str, user: Option<&str>) -> Built<Settings> {
    let mut merged = parse_table("defaults/settings.toml", DEFAULT_SETTINGS)?;
    if let Some(text) = user {
        let text = &*without_retired_settings(text);
        toml::from_str::<Settings>(text)
            .map_err(|error| vec![Diagnostic::from_toml(file, text, &error)])?;
        deep_merge(&mut merged, &parse_table(file, text)?);
    }
    let settings = merged.try_into().map_err(|error: toml::de::Error| {
        vec![Diagnostic::error(file, "", None, error.to_string())]
    })?;
    Ok((settings, Vec::new()))
}

/// Settings older versions wrote that no longer exist. Vaults sync between
/// devices running different versions, so a file that still sets one loads
/// as if it didn't.
const RETIRED_SETTINGS: &[&str] = &["sync.legacy-branch"];

fn without_retired_settings(text: &str) -> Cow<'_, str> {
    let Ok(mut file) = SettingsFile::parse(text) else {
        return Cow::Borrowed(text);
    };
    let removed = RETIRED_SETTINGS
        .iter()
        .filter(|key| file.remove(key))
        .count();
    if removed == 0 {
        return Cow::Borrowed(text);
    }
    Cow::Owned(file.to_string())
}

/// The built-in theme with `user` tokens layered on top, fully resolved.
pub fn build_theme(file: &str, user: Option<&str>) -> Built<Theme> {
    let mut tokens = TokenSet::from_table(&parse_table("defaults/theme.toml", DEFAULT_THEME)?);
    if let Some(text) = user {
        tokens.layer(TokenSet::from_table(&parse_table(file, text)?));
    }
    let text = user.unwrap_or(DEFAULT_THEME);
    tokens
        .resolve()
        .map(|theme| (theme, Vec::new()))
        .map_err(|error| {
            let span = error
                .tokens()
                .into_iter()
                .find_map(|token| span_of_key(text, token.rsplit('.').next().unwrap_or_default()));
            vec![Diagnostic::error(file, text, span, error.to_string())]
        })
}

/// The built-in layout with `user` slots layered on top.
pub fn build_layout(file: &str, user: Option<&str>) -> Built<LayoutNode> {
    let mut spec: LayoutSpec = toml::from_str(DEFAULT_LAYOUT)
        .map_err(|error| vec![Diagnostic::from_toml(file, DEFAULT_LAYOUT, &error)])?;
    if let Some(text) = user {
        let overlay: LayoutSpec = toml::from_str(text)
            .map_err(|error| vec![Diagnostic::from_toml(file, text, &error)])?;
        spec.layer(overlay);
    }
    let text = user.unwrap_or(DEFAULT_LAYOUT);
    spec.build()
        .map(|layout| (layout, Vec::new()))
        .map_err(|error| {
            let span = error
                .slot()
                .and_then(|slot| text.find(slot).map(|start| start..start + slot.len()));
            vec![Diagnostic::error(file, text, span, error.to_string())]
        })
}

/// The built-in rules with `user` rules layered on top. Warns about unknown commands.
pub fn build_rules(file: &str, user: Option<&str>, known_commands: &[&str]) -> Built<RuleSet> {
    let mut rules = RuleSet::defaults();
    let Some(text) = user else {
        return Ok((rules, Vec::new()));
    };
    let mut warnings = rules.layer(file, text)?;
    for rule in rules.unknown_commands(known_commands) {
        let needle = format!("\"{}\"", rule.command);
        let span = text.find(&needle).map(|start| start..start + needle.len());
        let message = format!("no command called `{}` is registered", rule.command);
        warnings.push(Diagnostic::warning(file, text, span, message));
    }
    Ok((rules, warnings))
}

/// Device settings from `user`, or the defaults.
pub fn build_device(file: &str, user: Option<&str>) -> Built<DeviceSettings> {
    let Some(text) = user else {
        return Ok((DeviceSettings::default(), Vec::new()));
    };
    toml::from_str(text)
        .map(|device| (device, Vec::new()))
        .map_err(|error| vec![Diagnostic::from_toml(file, text, &error)])
}

/// Loads and reloads one config folder, remembering the last good version of each part.
#[derive(Clone, Debug)]
pub struct ConfigLoader {
    dir: PathBuf,
    config: Config,
    known_commands: Vec<String>,
}

impl ConfigLoader {
    /// A loader for a config folder. Nothing is read until [`ConfigLoader::load_all`].
    pub fn new(dir: impl Into<PathBuf>) -> ConfigLoader {
        ConfigLoader {
            dir: dir.into(),
            config: Config::defaults(),
            known_commands: BUILTIN_COMMANDS
                .iter()
                .map(|c| c.id)
                .chain(PLATFORM_COMMANDS.iter().copied())
                .map(str::to_string)
                .collect(),
        }
    }

    /// A loader for `<vault>/.gasp`, where a legacy `.gasp` folder is
    /// moved first.
    pub fn for_vault(vault: &Path) -> ConfigLoader {
        crate::migration::migrate_config_dir_and_log(vault);
        crate::migration::migrate_mobile_toolbar_and_log(vault);
        ConfigLoader::new(vault.join(CONFIG_DIR))
    }

    /// Adds command ids (from apps or plugins) so rules naming them don't warn.
    pub fn add_known_commands<I: IntoIterator<Item = String>>(&mut self, ids: I) {
        self.known_commands.extend(ids);
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Reads every file.
    pub fn load_all(&mut self) -> Vec<Diagnostic> {
        ConfigFile::ALL
            .iter()
            .flat_map(|file| self.reload(*file))
            .collect()
    }

    /// Re-reads one file. On error, that part keeps its last good version.
    pub fn reload(&mut self, file: ConfigFile) -> Vec<Diagnostic> {
        let name = file.file_name();
        let path = match file {
            ConfigFile::Device => self.dir.join(crate::device_file::device_file_name(&self.dir)),
            _ => self.dir.join(name),
        };
        let text = match read_optional(&path) {
            Ok(text) => text,
            Err(error) => return vec![Diagnostic::error(name, "", None, error.to_string())],
        };
        let user = text.as_deref();
        match file {
            ConfigFile::Settings => store(build_settings(name, user), &mut self.config.settings),
            ConfigFile::Theme => store(build_theme(name, user), &mut self.config.theme),
            ConfigFile::Layout => store(build_layout(name, user), &mut self.config.layout),
            ConfigFile::Rules => {
                let known: Vec<&str> = self.known_commands.iter().map(String::as_str).collect();
                store(build_rules(name, user, &known), &mut self.config.rules)
            }
            ConfigFile::Device => store(build_device(name, user), &mut self.config.device),
            ConfigFile::Snippets => {
                store(build_snippets(name, user), &mut self.config.typing.snippets)
            }
            ConfigFile::Replacements => store(
                build_replacements(name, user),
                &mut self.config.typing.replacements,
            ),
            ConfigFile::Toolbars => {
                let known: Vec<&str> = self.known_commands.iter().map(String::as_str).collect();
                store(
                    build_toolbars(name, user, &known),
                    &mut self.config.toolbars,
                )
            }
        }
    }
}

fn store<T>(built: Built<T>, slot: &mut T) -> Vec<Diagnostic> {
    match built {
        Ok((value, warnings)) => {
            *slot = value;
            warnings
        }
        Err(errors) => errors,
    }
}

fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_round_trip() {
        for file in ConfigFile::ALL {
            let path = Path::new("vault").join(CONFIG_DIR).join(file.file_name());
            assert_eq!(ConfigFile::from_path(&path), Some(file));
        }
        assert_eq!(ConfigFile::from_path(Path::new("notes.md")), None);
    }

    #[test]
    fn only_device_settings_stay_local() {
        let local: Vec<_> = ConfigFile::ALL
            .into_iter()
            .filter(|f| !f.is_synced())
            .collect();
        assert_eq!(local, [ConfigFile::Device]);
    }

    #[test]
    fn settings_type_errors_carry_position() {
        let text = "[sidebar.files]\nreveal = \"sometimes\"\n";
        let errors = build_settings("settings.toml", Some(text)).unwrap_err();
        assert_eq!(errors[0].line, 2);
    }

    #[test]
    fn a_retired_setting_still_loads() {
        let text = "[sync]\nbranch = \"notes\"\nlegacy-branch = \"main\"\n";
        let (settings, _) = build_settings("settings.toml", Some(text)).unwrap();
        assert_eq!(settings.sync.branch, "notes");
    }

    #[test]
    fn theme_cycle_points_at_the_user_line() {
        let text = "[color]\naccent = \"{color.link}\"\nlink = \"{color.accent}\"\n";
        let errors = build_theme("theme.toml", Some(text)).unwrap_err();
        assert!(errors[0].message.contains("loop"));
        assert!(errors[0].line == 2 || errors[0].line == 3);
    }

    #[test]
    fn layout_errors_point_at_the_slot() {
        let text = "[slot.body]\nchildren = [\"center\", \"ribbon\"]\n";
        let errors = build_layout("layout.toml", Some(text)).unwrap_err();
        assert!(errors[0].message.contains("ribbon"));
        assert_eq!(errors[0].line, 2);
    }

    #[test]
    fn unknown_commands_warn_but_load() {
        let text = "[[rule]]\non = \"key\"\nkeys = \"Mod+Shift+Y\"\ndo = \"plugin.thing\"\n";
        let (rules, warnings) = build_rules("rules.toml", Some(text), &["format.bold"]).unwrap();
        assert!(rules.rules().iter().any(|r| r.command == "plugin.thing"));
        assert!(warnings.iter().any(|w| w.line == 4 && !w.is_error()));
    }
}
