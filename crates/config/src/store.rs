//! Reading and writing the vault's `.gasp/settings.toml` one key at a
//! time, keeping the user's comments, layout and unrelated keys. Only
//! values that differ from the defaults are written.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::loader::{CONFIG_DIR, build_settings};
use serde_json::Value;
use toml_edit::{DocumentMut, Item, Table, TableLike};

pub const SETTINGS_FILE: &str = "settings.toml";

/// Where a vault's settings live.
pub fn settings_path(vault_root: &Path) -> PathBuf {
    vault_root.join(CONFIG_DIR).join(SETTINGS_FILE)
}

/// A parsed settings file that edits in place.
#[derive(Clone, Debug, Default)]
pub struct SettingsFile {
    doc: DocumentMut,
}

impl SettingsFile {
    pub fn parse(text: &str) -> Result<SettingsFile, String> {
        let doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
        Ok(SettingsFile { doc })
    }

    /// Reads the file; a missing file is an empty one.
    pub fn load(path: &Path) -> Result<SettingsFile, String> {
        match fs::read_to_string(path) {
            Ok(text) => SettingsFile::parse(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(SettingsFile::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    /// The value the file sets for a dotted key, if any.
    pub fn get(&self, key: &str) -> Option<Value> {
        let (tables, leaf) = split_key(key)?;
        let table = find_table(self.doc.as_table(), &tables)?;
        to_json(table.get(leaf)?)
    }

    /// The names set under a map setting, such as the syntax kinds in
    /// `markdown.symbols.overrides`, with their values.
    pub fn entries(&self, key: &str) -> Vec<(String, Value)> {
        let parts: Vec<&str> = key.split('.').collect();
        let Some(table) = find_table(self.doc.as_table(), &parts) else {
            return Vec::new();
        };
        table
            .iter()
            .filter_map(|(name, item)| Some((name.to_string(), to_json(item)?)))
            .collect()
    }

    /// Sets a key, creating tables as needed and keeping the comment that
    /// follows an existing value.
    pub fn set(&mut self, key: &str, value: &Value) -> Result<(), String> {
        let (tables, leaf) = split_key(key).ok_or("empty key")?;
        let new_value = to_toml(value).ok_or("that kind of value can’t be saved")?;
        let table = table_mut(self.doc.as_table_mut(), &tables)?;
        match table.get_mut(leaf).and_then(Item::as_value_mut) {
            Some(existing) => {
                let decor = existing.decor().clone();
                *existing = new_value;
                *existing.decor_mut() = decor;
            }
            None => {
                table.insert(leaf, Item::Value(new_value));
            }
        }
        Ok(())
    }

    /// Removes a key and any tables it leaves empty. Returns whether the
    /// key was there.
    pub fn remove(&mut self, key: &str) -> bool {
        let Some((tables, leaf)) = split_key(key) else {
            return false;
        };
        remove_in(self.doc.as_table_mut(), &tables, leaf)
    }
}

impl std::fmt::Display for SettingsFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.doc)
    }
}

fn split_key(key: &str) -> Option<(Vec<&str>, &str)> {
    let mut parts: Vec<&str> = key.split('.').filter(|p| !p.is_empty()).collect();
    let leaf = parts.pop()?;
    Some((parts, leaf))
}

fn find_table<'a>(root: &'a dyn TableLike, path: &[&str]) -> Option<&'a dyn TableLike> {
    let mut current = root;
    for part in path {
        current = current.get(part)?.as_table_like()?;
    }
    Some(current)
}

fn table_mut<'a>(
    root: &'a mut dyn TableLike,
    path: &[&str],
) -> Result<&'a mut dyn TableLike, String> {
    let mut current = root;
    for part in path {
        if current.get(part).is_none() {
            let mut table = Table::new();
            table.set_implicit(true);
            current.insert(part, Item::Table(table));
        }
        current = current
            .get_mut(part)
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| format!("`{part}` isn’t a table in settings.toml"))?;
    }
    Ok(current)
}

/// Removes `leaf` under `path`, then drops tables left with nothing in
/// them and no comments.
fn remove_in(table: &mut dyn TableLike, path: &[&str], leaf: &str) -> bool {
    let Some((first, rest)) = path.split_first() else {
        return table.remove(leaf).is_some();
    };
    let Some(child) = table.get_mut(first).and_then(Item::as_table_like_mut) else {
        return false;
    };
    let removed = remove_in(child, rest, leaf);
    let prunable = table.get(first).is_some_and(is_prunable);
    if removed && prunable {
        table.remove(first);
    }
    removed
}

fn is_prunable(item: &Item) -> bool {
    let Some(table) = item.as_table() else {
        return item.as_table_like().is_some_and(|t| t.is_empty());
    };
    let decor = table.decor();
    let commented = [decor.prefix(), decor.suffix()]
        .into_iter()
        .flatten()
        .any(|raw| raw.as_str().is_some_and(|text| text.contains('#')));
    table.is_empty() && !commented
}

fn to_toml(value: &Value) -> Option<toml_edit::Value> {
    match value {
        Value::Bool(flag) => Some((*flag).into()),
        Value::Number(number) => number
            .as_i64()
            .map(toml_edit::Value::from)
            .or_else(|| number.as_f64().map(toml_edit::Value::from)),
        Value::String(text) => Some(text.as_str().into()),
        Value::Array(items) => {
            let items: Option<Vec<toml_edit::Value>> = items.iter().map(to_toml).collect();
            Some(toml_edit::Value::Array(items?.into_iter().collect()))
        }
        _ => None,
    }
}

fn to_json(item: &Item) -> Option<Value> {
    let value = item.as_value()?;
    if let Some(flag) = value.as_bool() {
        return Some(Value::Bool(flag));
    }
    if let Some(number) = value.as_integer() {
        return Some(Value::from(number));
    }
    if let Some(number) = value.as_float() {
        return Some(Value::from(number));
    }
    if let Some(items) = value.as_array() {
        let items: Option<Vec<Value>> = items
            .iter()
            .map(|item| to_json(&Item::Value(item.clone())))
            .collect();
        return items.map(Value::Array);
    }
    value.as_str().map(Value::from)
}

/// Writes one setting to the vault's settings file: `None`, or a value
/// equal to `default`, removes the key. The result must still load, or
/// nothing is written and the reason comes back.
pub fn write_setting(
    vault_root: &Path,
    key: &str,
    value: Option<&Value>,
    default: &Value,
) -> Result<SettingsFile, String> {
    let path = settings_path(vault_root);
    let mut file = SettingsFile::load(&path)?;
    match value {
        Some(value) if value != default => file.set(key, value)?,
        _ => {
            file.remove(key);
        }
    }
    let text = file.to_string();
    build_settings(SETTINGS_FILE, Some(&text)).map_err(|diagnostics| {
        diagnostics.first().map_or_else(
            || "that value isn’t allowed".to_string(),
            |d| d.message.clone(),
        )
    })?;
    save(&path, &text).map_err(|error| error.to_string())?;
    Ok(file)
}

/// Writes through a temporary file so a crash can't leave half a file.
pub fn save(path: &Path, text: &str) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_FILE: &str = "\
# My settings, tuned for the laptop.

[sidebar.files]
reveal = \"always\"   # I like it visible

[files]
# Keep deletions inside the vault.
trash = \"vault\"
attachments-folder = \"./assets\"
";

    fn vault_with(text: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if let Some(text) = text {
            fs::create_dir_all(dir.path().join(CONFIG_DIR)).unwrap();
            fs::write(settings_path(dir.path()), text).unwrap();
        }
        dir
    }

    fn read(dir: &tempfile::TempDir) -> String {
        fs::read_to_string(settings_path(dir.path())).unwrap()
    }

    #[test]
    fn changing_a_value_keeps_comments_and_other_keys() {
        let dir = vault_with(Some(USER_FILE));
        let default = Value::from("hover");
        write_setting(
            dir.path(),
            "sidebar.files.reveal",
            Some(&Value::from("toggle")),
            &default,
        )
        .unwrap();
        let text = read(&dir);
        assert!(text.contains("# My settings, tuned for the laptop."));
        assert!(
            text.contains("reveal = \"toggle\"   # I like it visible"),
            "{text}"
        );
        assert!(text.contains("# Keep deletions inside the vault.\ntrash = \"vault\""));
        assert!(text.contains("attachments-folder = \"./assets\""));
    }

    #[test]
    fn new_keys_go_in_new_tables() {
        let dir = vault_with(Some(USER_FILE));
        write_setting(
            dir.path(),
            "appearance.base-font-size",
            Some(&Value::from(14)),
            &Value::from(12),
        )
        .unwrap();
        write_setting(
            dir.path(),
            "prose.sentence-length.enabled",
            Some(&Value::from(false)),
            &Value::from(true),
        )
        .unwrap();
        let text = read(&dir);
        assert!(text.starts_with(USER_FILE), "{text}");
        assert!(
            text.contains("[appearance]\nbase-font-size = 14\n"),
            "{text}"
        );
        assert!(
            text.contains("[prose.sentence-length]\nenabled = false\n"),
            "{text}"
        );
        assert!(!text.contains("[prose]\n"), "{text}");
    }

    #[test]
    fn a_missing_file_is_created() {
        let dir = vault_with(None);
        write_setting(
            dir.path(),
            "files.trash",
            Some(&Value::from("delete")),
            &Value::from("system"),
        )
        .unwrap();
        assert_eq!(read(&dir), "[files]\ntrash = \"delete\"\n");
    }

    #[test]
    fn setting_the_default_removes_the_key() {
        let dir = vault_with(Some(USER_FILE));
        write_setting(
            dir.path(),
            "files.trash",
            Some(&Value::from("system")),
            &Value::from("system"),
        )
        .unwrap();
        let text = read(&dir);
        assert!(!text.contains("trash ="), "{text}");
        assert!(text.contains("attachments-folder"));
        assert!(text.contains("reveal = \"always\""));
    }

    #[test]
    fn resetting_the_last_key_drops_its_table() {
        let dir = vault_with(Some(
            "[files]\ntrash = \"vault\"\n\n[editor]\nshow-inline-title = false\n",
        ));
        write_setting(
            dir.path(),
            "editor.show-inline-title",
            None,
            &Value::from(true),
        )
        .unwrap();
        assert_eq!(read(&dir), "[files]\ntrash = \"vault\"\n");
    }

    #[test]
    fn a_table_carrying_comments_stays_when_emptied() {
        let dir = vault_with(Some(USER_FILE));
        write_setting(
            dir.path(),
            "sidebar.files.reveal",
            None,
            &Value::from("hover"),
        )
        .unwrap();
        let text = read(&dir);
        assert!(!text.contains("reveal"), "{text}");
        assert!(text.contains("# My settings, tuned for the laptop."));
        assert!(text.contains("# Keep deletions inside the vault."));
    }

    #[test]
    fn invalid_values_are_not_written() {
        let dir = vault_with(Some(USER_FILE));
        let result = write_setting(
            dir.path(),
            "files.trash",
            Some(&Value::from("shred")),
            &Value::from("system"),
        );
        assert!(result.is_err());
        assert_eq!(read(&dir), USER_FILE);
        let unknown = write_setting(
            dir.path(),
            "markdown.symbols.overrides.not-a-syntax",
            Some(&Value::from("always-hidden")),
            &Value::Null,
        );
        assert!(unknown.is_err());
        assert_eq!(read(&dir), USER_FILE);
    }

    #[test]
    fn map_entries_are_listed_and_removable() {
        let dir = vault_with(None);
        let key = "markdown.symbols.overrides.link-url";
        write_setting(
            dir.path(),
            key,
            Some(&Value::from("always-hidden")),
            &Value::Null,
        )
        .unwrap();
        let file = SettingsFile::load(&settings_path(dir.path())).unwrap();
        assert_eq!(
            file.entries("markdown.symbols.overrides"),
            [("link-url".to_string(), Value::from("always-hidden"))]
        );
        assert_eq!(file.get(key), Some(Value::from("always-hidden")));
        let file = write_setting(dir.path(), key, None, &Value::Null).unwrap();
        assert!(file.entries("markdown.symbols.overrides").is_empty());
    }

    #[test]
    fn values_read_back_as_json() {
        let file = SettingsFile::parse(USER_FILE).unwrap();
        assert_eq!(file.get("files.trash"), Some(Value::from("vault")));
        assert_eq!(file.get("files.update-links-on-rename"), None);
        assert_eq!(file.get("sidebar.files"), None);
        let numbers = SettingsFile::parse("a = 3\nb = 1.5\nc = true\n").unwrap();
        assert_eq!(numbers.get("a"), Some(Value::from(3)));
        assert_eq!(numbers.get("b"), Some(Value::from(1.5)));
        assert_eq!(numbers.get("c"), Some(Value::from(true)));
    }

    #[test]
    fn inline_tables_are_edited_in_place() {
        let dir = vault_with(Some("sidebar = { files = { mode = \"push\" } }\n"));
        write_setting(
            dir.path(),
            "sidebar.files.reveal",
            Some(&Value::from("always")),
            &Value::from("hover"),
        )
        .unwrap();
        let file = SettingsFile::load(&settings_path(dir.path())).unwrap();
        assert_eq!(file.get("sidebar.files.mode"), Some(Value::from("push")));
        assert_eq!(
            file.get("sidebar.files.reveal"),
            Some(Value::from("always"))
        );
    }
}
