//! Converts Obsidian's `app.json` and `appearance.json` into a settings TOML fragment.

use serde_json::Value;

use crate::toml_text::quote;

/// How a setting's value is converted.
#[derive(Clone, Copy)]
enum Convert {
    Text,
    Bool,
    Integer,
    Trash,
}

/// (source key, section, our key, conversion)
const APP_SETTINGS: &[(&str, &str, &str, Convert)] = &[
    (
        "attachmentFolderPath",
        "files",
        "attachments-folder",
        Convert::Text,
    ),
    (
        "alwaysUpdateLinks",
        "files",
        "update-links-on-rename",
        Convert::Bool,
    ),
    ("trashOption", "files", "trash", Convert::Trash),
    (
        "showInlineTitle",
        "editor",
        "show-inline-title",
        Convert::Bool,
    ),
];

/// Sections starting with `theme:` go to the theme file, because fonts and
/// colours are theme tokens rather than settings.
const APPEARANCE_SETTINGS: &[(&str, &str, &str, Convert)] = &[
    (
        "baseFontSize",
        "appearance",
        "base-font-size",
        Convert::Integer,
    ),
    ("textFontFamily", "theme:font", "text", Convert::Text),
    ("interfaceFontFamily", "theme:font", "ui", Convert::Text),
    ("monospaceFontFamily", "theme:font", "code", Convert::Text),
    ("accentColor", "theme:color", "accent", Convert::Text),
];

const THEME_PREFIX: &str = "theme:";

/// Settings that are deliberately not imported, and why.
const NOT_IMPORTED: &[(&str, &str)] = &[
    (
        "mobileToolbarCommands",
        "the iPhone toolbar is designed later (Phase 7)",
    ),
    (
        "settingsPopoutWindow",
        "settings always open in the main window",
    ),
    ("theme", "light mode is already the default"),
    (
        "cssTheme",
        "Obsidian community themes don't apply; the look comes from theme tokens",
    ),
    (
        "enabledCssSnippets",
        "CSS snippets don't apply; the look comes from theme tokens",
    ),
    ("showRibbon", "there is no ribbon"),
    (
        "showViewHeader",
        "the tab and header layout comes from the layout tree",
    ),
];

const SECTION_ORDER: [&str; 5] = ["files", "editor", "appearance", "theme:color", "theme:font"];

#[derive(Clone, Debug, Default)]
pub struct SettingsMigration {
    /// (section, key, TOML value) in output order.
    pub values: Vec<(String, String, String)>,
    pub notes: Vec<String>,
}

pub fn migrate(
    app_json: Option<&str>,
    appearance_json: Option<&str>,
) -> Result<SettingsMigration, String> {
    let mut migration = SettingsMigration::default();
    if let Some(text) = app_json {
        migration.add_file(text, "app.json", APP_SETTINGS)?;
    }
    if let Some(text) = appearance_json {
        migration.add_file(text, "appearance.json", APPEARANCE_SETTINGS)?;
    }
    migration.values.sort_by_key(|(section, _, _)| {
        SECTION_ORDER
            .iter()
            .position(|s| s == section)
            .unwrap_or(SECTION_ORDER.len())
    });
    Ok(migration)
}

impl SettingsMigration {
    fn add_file(
        &mut self,
        text: &str,
        name: &str,
        table: &[(&str, &str, &str, Convert)],
    ) -> Result<(), String> {
        let json: Value = serde_json::from_str(text)
            .map_err(|error| format!("{name} isn't valid JSON: {error}"))?;
        let object = json
            .as_object()
            .ok_or_else(|| format!("{name} isn't an object"))?;
        for (key, value) in object {
            self.add_setting(name, key, value, table);
        }
        Ok(())
    }

    fn add_setting(
        &mut self,
        file: &str,
        key: &str,
        value: &Value,
        table: &[(&str, &str, &str, Convert)],
    ) {
        if let Some((_, section, ours, convert)) = table.iter().find(|(source, ..)| *source == key)
        {
            match convert_value(value, *convert) {
                Some(toml_value) => {
                    self.values
                        .push((section.to_string(), ours.to_string(), toml_value))
                }
                None => self.notes.push(format!(
                    "{file}: `{key}` has an unexpected value {value}, so it wasn't imported."
                )),
            }
            return;
        }
        let reason = NOT_IMPORTED
            .iter()
            .find(|(source, _)| *source == key)
            .map_or("it has no equivalent setting", |(_, reason)| reason);
        self.notes
            .push(format!("{file}: `{key}` wasn't imported: {reason}."));
    }

    /// The settings fragment as TOML, grouped by section.
    pub fn to_toml(&self) -> String {
        let values = self
            .values
            .iter()
            .filter(|(section, ..)| !section.starts_with(THEME_PREFIX))
            .map(|(section, key, value)| (section.as_str(), key, value));
        render_sections(
            "# Settings imported from Obsidian by gasp-migrate.\n",
            values,
        )
    }

    /// The theme tokens (fonts and accent colour) as TOML, or `None` when there are none.
    pub fn theme_to_toml(&self) -> Option<String> {
        let values: Vec<_> = self
            .values
            .iter()
            .filter_map(|(section, key, value)| {
                section
                    .strip_prefix(THEME_PREFIX)
                    .map(|section| (section, key, value))
            })
            .collect();
        if values.is_empty() {
            return None;
        }
        Some(render_sections(
            "# Theme tokens imported from Obsidian by gasp-migrate.\n",
            values.into_iter(),
        ))
    }
}

fn render_sections<'a>(
    header: &str,
    values: impl Iterator<Item = (&'a str, &'a String, &'a String)>,
) -> String {
    let mut out = String::from(header);
    let mut current: Option<&str> = None;
    for (section, key, value) in values {
        if current != Some(section) {
            out.push_str(&format!("\n[{section}]\n"));
            current = Some(section);
        }
        out.push_str(&format!("{key} = {value}\n"));
    }
    out
}

fn convert_value(value: &Value, convert: Convert) -> Option<String> {
    match convert {
        Convert::Text => value.as_str().map(quote),
        Convert::Bool => value.as_bool().map(|b| b.to_string()),
        Convert::Integer => value.as_f64().map(|n| (n.round() as i64).to_string()),
        Convert::Trash => value.as_str().and_then(trash_mode).map(quote),
    }
}

fn trash_mode(obsidian: &str) -> Option<&'static str> {
    match obsidian {
        "system" => Some("system"),
        "local" => Some("vault"),
        "none" => Some("delete"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_settings_and_reports_the_rest() {
        let app = r#"{"trashOption": "local", "showInlineTitle": false, "attachmentFolderPath": "./img", "settingsPopoutWindow": false, "vimMode": true}"#;
        let appearance = r##"{"baseFontSize": 14, "accentColor": "#112233"}"##;
        let migration = migrate(Some(app), Some(appearance)).unwrap();
        let text = migration.to_toml();
        let parsed: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(parsed["files"]["trash"].as_str(), Some("vault"));
        assert_eq!(
            parsed["files"]["attachments-folder"].as_str(),
            Some("./img")
        );
        assert_eq!(parsed["editor"]["show-inline-title"].as_bool(), Some(false));
        assert_eq!(
            parsed["appearance"]["base-font-size"].as_integer(),
            Some(14)
        );
        assert!(parsed["appearance"].get("accent").is_none());
        let theme: toml::Table = toml::from_str(&migration.theme_to_toml().unwrap()).unwrap();
        assert_eq!(theme["color"]["accent"].as_str(), Some("#112233"));
        assert_eq!(migration.notes.len(), 2);
        assert!(migration.notes[1].contains("no equivalent"));
    }
}
