//! One-shot importer for .obsidian settings, hotkeys and snippets.
//!
//! [`migrate_obsidian`] reads a `.obsidian` folder and returns the new config files plus
//! a plain-text report of everything that couldn't be carried over.

pub mod app_settings;
pub mod hotkeys;
pub mod js;
pub mod latex_suite;
pub mod regex_convert;
pub mod replacements;
mod report;
mod toml_text;

use std::fs;
use std::path::Path;

pub use report::render_report;

/// One file the migration writes into the output folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputFile {
    pub name: &'static str,
    pub contents: String,
}

/// Everything read from a `.obsidian` folder; parts are `None` when their file is missing.
#[derive(Clone, Debug, Default)]
pub struct Migration {
    pub latex_suite: Option<latex_suite::LatexSuiteMigration>,
    pub replacements: Option<replacements::ReplacementsMigration>,
    pub hotkeys: Option<hotkeys::HotkeyMigration>,
    pub settings: Option<app_settings::SettingsMigration>,
    /// Source files that weren't found.
    pub missing: Vec<&'static str>,
}

pub const SNIPPETS_FILE: &str = "snippets.txt";
pub const REPLACEMENTS_FILE: &str = "replacements.toml";
pub const RULES_FILE: &str = "rules.toml";
pub const SETTINGS_FILE: &str = "settings.toml";
pub const REPORT_FILE: &str = "migration-report.txt";

const LATEX_SUITE: &str = "plugins/obsidian-latex-suite.json";
const SMART_TYPOGRAPHY: &str = "plugins/obsidian-smart-typography.json";
const PRETTIFIER: &str = "plugins/enhanced-symbols-prettifier.json";
const HOTKEYS: &str = "hotkeys.json";
const APP: &str = "app.json";
const APPEARANCE: &str = "appearance.json";

/// Reads a `.obsidian` folder and converts everything this tool knows about.
pub fn migrate_obsidian(dir: &Path) -> Result<Migration, String> {
    let mut migration = Migration::default();
    let mut read = |name: &'static str| -> Option<String> {
        let text = fs::read_to_string(dir.join(name)).ok();
        if text.is_none() {
            migration.missing.push(name);
        }
        text
    };
    let latex_suite = read(LATEX_SUITE);
    let smart_typography = read(SMART_TYPOGRAPHY);
    let prettifier = read(PRETTIFIER);
    let hotkeys = read(HOTKEYS);
    let app = read(APP);
    let appearance = read(APPEARANCE);
    migration.latex_suite = latex_suite
        .as_deref()
        .map(latex_suite::migrate)
        .transpose()?;
    if smart_typography.is_some() || prettifier.is_some() {
        migration.replacements = Some(replacements::migrate(
            smart_typography.as_deref(),
            prettifier.as_deref(),
        )?);
    }
    migration.hotkeys = hotkeys.as_deref().map(hotkeys::migrate).transpose()?;
    if app.is_some() || appearance.is_some() {
        migration.settings = Some(app_settings::migrate(
            app.as_deref(),
            appearance.as_deref(),
        )?);
    }
    Ok(migration)
}

impl Migration {
    /// The files to write, in a fixed order. The report is always last.
    pub fn output_files(&self) -> Vec<OutputFile> {
        let mut files = Vec::new();
        if let Some(latex_suite) = &self.latex_suite {
            files.push(OutputFile {
                name: SNIPPETS_FILE,
                contents: latex_suite.file.to_string(),
            });
        }
        if let Some(replacements) = &self.replacements {
            let header = "# Replacements merged from Smart Typography and Symbols Prettifier by editor-migrate.\n\n";
            files.push(OutputFile {
                name: REPLACEMENTS_FILE,
                contents: format!("{header}{}", replacements.table.to_toml()),
            });
        }
        if let Some(hotkeys) = &self.hotkeys {
            files.push(OutputFile {
                name: RULES_FILE,
                contents: hotkeys.to_toml(),
            });
        }
        if let Some(settings) = &self.settings {
            files.push(OutputFile {
                name: SETTINGS_FILE,
                contents: settings.to_toml(),
            });
        }
        files.push(OutputFile {
            name: REPORT_FILE,
            contents: render_report(self),
        });
        files
    }

    /// Writes every output file into `out`, creating it if needed.
    pub fn write_to(&self, out: &Path) -> Result<Vec<OutputFile>, String> {
        fs::create_dir_all(out)
            .map_err(|error| format!("can't create {}: {error}", out.display()))?;
        let files = self.output_files();
        for file in &files {
            let path = out.join(file.name);
            fs::write(&path, &file.contents)
                .map_err(|error| format!("can't write {}: {error}", path.display()))?;
        }
        Ok(files)
    }
}
