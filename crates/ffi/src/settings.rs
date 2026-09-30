//! The vault's settings for the phone's settings screen: every setting the
//! schema describes, its value, and writing one back to
//! `.gasp/settings.toml` the way the desktop's settings screen does.

use gasp_config::schema::{SettingKind, setting_descriptors};
use gasp_config::setting_texts::{HIDDEN, setting_text};
use gasp_config::settings::SettingsIndex;
use gasp_config::store::write_setting;
use serde_json::Value as Json;
use toml::Value as Toml;

use crate::vault::{VaultError, VaultFolder};

/// The key `prose.toggle-sentence-highlighting` flips.
const SENTENCE_LENGTH_KEY: &str = "prose.sentence-length.enabled";

/// What kind of control a setting needs.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SettingControl {
    Switch,
    Integer,
    Number,
    Text,
    /// One of these.
    Choice {
        options: Vec<String>,
    },
    /// A list of text, one item per line.
    List,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum SettingValue {
    Bool { value: bool },
    Integer { value: i64 },
    Number { value: f64 },
    Text { value: String },
    List { values: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SettingItem {
    /// Dotted, such as `files.trash`.
    pub key: String,
    /// The group it's shown in, such as `Files`.
    pub section: String,
    pub title: String,
    pub description: String,
    pub control: SettingControl,
    pub value: SettingValue,
}

/// The setting that names the branch sync uses.
const BRANCH_KEY: &str = "sync.branch";

#[uniffi::export]
impl VaultFolder {
    /// Every setting a control can show, in the schema's order.
    pub fn settings(&self) -> Vec<SettingItem> {
        let index = SettingsIndex::new(&self.config().settings);
        setting_descriptors()
            .into_iter()
            .filter(|descriptor| {
                !HIDDEN
                    .iter()
                    .any(|prefix| descriptor.key.starts_with(prefix))
            })
            .filter_map(|descriptor| {
                let control = control(&descriptor.kind)?;
                let value = setting_value(index.get(&descriptor.key)?)?;
                let (title, description) = match setting_text(&descriptor.key) {
                    Some((title, description)) => (title.to_owned(), description.to_owned()),
                    None => (
                        title(&descriptor.key),
                        descriptor.description.unwrap_or_default(),
                    ),
                };
                Some(SettingItem {
                    section: humanize(descriptor.key.split('.').next().unwrap_or_default()),
                    title,
                    description,
                    key: descriptor.key,
                    control,
                    value,
                })
            })
            .collect()
    }

    /// Writes one setting and reloads the config. A value the app wouldn't
    /// load is refused with the reason.
    pub fn set_setting(&self, key: String, value: SettingValue) -> Result<(), VaultError> {
        let default = setting_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.key == key)
            .map(|descriptor| descriptor.default)
            .ok_or_else(|| VaultError::Refused {
                message: format!("There's no setting called {key}."),
            })?;
        let branch_before = self.config().settings.sync.branch.clone();
        write_setting(&self.root, &key, Some(&json_value(value)), &default)
            .map_err(|message| VaultError::Refused { message })?;
        self.reload_config();
        if key == BRANCH_KEY {
            let branch = self.config().settings.sync.branch.clone();
            gasp_sync::follow_branch(&self.root, crate::sync::REMOTE, &branch_before, &branch)
                .map_err(|error| VaultError::Refused {
                    message: format!("Sync stays on {branch_before}: {error}"),
                })?;
        }
        Ok(())
    }

    /// Turns sentence-length highlighting on or off by writing its
    /// setting, as the desktop does. Answers whether it's on now.
    pub fn toggle_sentence_highlighting(&self) -> Result<bool, VaultError> {
        let on = !self.config().settings.prose.sentence_length.enabled;
        self.set_setting(
            SENTENCE_LENGTH_KEY.to_owned(),
            SettingValue::Bool { value: on },
        )?;
        Ok(on)
    }
}

fn control(kind: &SettingKind) -> Option<SettingControl> {
    Some(match kind {
        SettingKind::Bool => SettingControl::Switch,
        SettingKind::Integer => SettingControl::Integer,
        SettingKind::Number => SettingControl::Number,
        SettingKind::Text => SettingControl::Text,
        SettingKind::Choice(options) => SettingControl::Choice {
            options: options.clone(),
        },
        SettingKind::List(_) => SettingControl::List,
        SettingKind::Map(_) => return None,
    })
}

fn setting_value(value: &Toml) -> Option<SettingValue> {
    Some(match value {
        Toml::Boolean(value) => SettingValue::Bool { value: *value },
        Toml::Integer(value) => SettingValue::Integer { value: *value },
        Toml::Float(value) => SettingValue::Number { value: *value },
        Toml::String(value) => SettingValue::Text {
            value: value.clone(),
        },
        Toml::Array(items) => SettingValue::List {
            values: items.iter().map(toml_text).collect(),
        },
        _ => return None,
    })
}

fn toml_text(value: &Toml) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

fn json_value(value: SettingValue) -> Json {
    match value {
        SettingValue::Bool { value } => Json::Bool(value),
        SettingValue::Integer { value } => Json::from(value),
        SettingValue::Number { value } => Json::from(value),
        SettingValue::Text { value } => Json::String(value),
        SettingValue::List { values } => {
            Json::Array(values.into_iter().map(Json::String).collect())
        }
    }
}

/// `files.update-links-on-rename` → `Update links on rename`.
fn title(key: &str) -> String {
    let rest: Vec<&str> = key.split('.').skip(1).collect();
    match rest.is_empty() {
        true => humanize(key),
        false => humanize(&rest.join(" ")),
    }
}

fn humanize(name: &str) -> String {
    let spaced = name.replace('-', " ");
    let mut characters = spaced.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests::vault_with;

    #[test]
    fn settings_have_titles_sections_and_values() {
        let (_dir, vault) = vault_with(&[]);
        let settings = vault.settings();
        let trash = settings
            .iter()
            .find(|item| item.key == "files.trash")
            .unwrap();
        assert_eq!(trash.section, "Files");
        assert_eq!(trash.title, "Deleted notes");
        assert_eq!(trash.description, "Where a note goes when you delete it.");
        assert!(matches!(trash.control, SettingControl::Choice { .. }));
        assert_eq!(
            trash.value,
            SettingValue::Text {
                value: "system".into()
            }
        );
    }

    #[test]
    fn a_written_setting_takes_effect() {
        let (_dir, vault) = vault_with(&[]);
        assert!(vault.toggle_sentence_highlighting().unwrap());
        let note = vault.document("One two. Three four five six seven eight nine.".into());
        assert_eq!(note.sentence_tints().len(), 2);
        let refused = vault.set_setting(
            "files.trash".into(),
            SettingValue::Text {
                value: "nowhere".into(),
            },
        );
        assert!(refused.is_err());
    }
}
