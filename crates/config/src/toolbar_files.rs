//! Writing the vault's `.gasp/toolbars.toml` a field at a time, the way
//! the settings screen and `gasp mcp` change toolbars: comments and
//! everything else in the file stay as the user wrote them, a built-in
//! toolbar's field is written only while it differs from the built-in
//! one, and nothing is written unless the file still loads.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config_files::known_commands;
use crate::loader::CONFIG_DIR;
use crate::store::{SettingsFile, save};
use crate::toolbars::{Place, ToolbarItem, Toolbars, build_toolbars, choice_name, is_valid_id};

pub const TOOLBARS_FILE: &str = "toolbars.toml";

/// The fields of a `[toolbar.<id>]` table that the writers change.
pub const TOOLBAR_FIELDS: [&str; 9] = [
    "title",
    "enabled",
    "place",
    "behaviour",
    "contexts",
    "style",
    "density",
    "surface",
    "items",
];

pub fn toolbars_path(vault_root: &Path) -> PathBuf {
    vault_root.join(CONFIG_DIR).join(TOOLBARS_FILE)
}

/// The vault's toolbars layered on the built-in ones, or the built-in
/// ones when the file is missing or broken.
pub fn load_toolbars(vault_root: &Path) -> Toolbars {
    let text = std::fs::read_to_string(toolbars_path(vault_root)).ok();
    build_toolbars(TOOLBARS_FILE, text.as_deref(), &known_commands())
        .map(|(toolbars, _)| toolbars)
        .unwrap_or_else(|_| Toolbars::defaults())
}

fn field_key(id: &str, field: &str) -> String {
    format!("toolbar.{id}.{field}")
}

/// A built-in toolbar's field as the built-in file resolves it, in the
/// form the file writes it.
fn built_in_value(id: &str, field: &str) -> Option<Value> {
    let toolbar = Toolbars::defaults().get(id)?.clone();
    let value = match field {
        "title" => Value::from(toolbar.title),
        "enabled" => Value::from(toolbar.enabled),
        "place" => Value::from(choice_name(toolbar.place)),
        "behaviour" => Value::from(choice_name(toolbar.behaviour)),
        "contexts" => Value::from(
            toolbar
                .contexts
                .iter()
                .map(|c| choice_name(*c))
                .collect::<Vec<_>>(),
        ),
        "style" => Value::from(choice_name(toolbar.style)),
        "density" => Value::from(choice_name(toolbar.density)),
        "surface" => Value::from(choice_name(toolbar.surface)),
        "items" => Value::from(item_names(&toolbar.items)),
        _ => return None,
    };
    Some(value)
}

/// Items as a toolbar's `items` list spells them.
pub fn item_names(items: &[ToolbarItem]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

/// Checks the file still loads, saves it and returns the toolbars in
/// effect.
fn save_toolbars(path: &Path, file: &SettingsFile) -> Result<Toolbars, String> {
    let text = file.to_string();
    let (toolbars, _) =
        build_toolbars(TOOLBARS_FILE, Some(&text), &known_commands()).map_err(|errors| {
            errors.first().map_or_else(
                || "the toolbars file doesn’t load".to_string(),
                |d| d.message.clone(),
            )
        })?;
    save(path, &text).map_err(|error| error.to_string())?;
    Ok(toolbars)
}

/// Sets one field of toolbar `id`, such as `place` = "editor-top". `None`
/// removes it; on a built-in toolbar, so does the built-in value.
pub fn set_toolbar_field(
    vault_root: &Path,
    id: &str,
    field: &str,
    value: Option<&Value>,
) -> Result<Toolbars, String> {
    if !TOOLBAR_FIELDS.contains(&field) {
        return Err(format!("toolbars have no `{field}`"));
    }
    let path = toolbars_path(vault_root);
    let mut file = SettingsFile::load(&path)?;
    let key = field_key(id, field);
    let built_in = built_in_value(id, field);
    match value {
        Some(value) if built_in.as_ref() != Some(value) => file.set(&key, value)?,
        _ => {
            file.remove(&key);
        }
    }
    save_toolbars(&path, &file)
}

/// Replaces toolbar `id`'s items.
pub fn set_toolbar_items(
    vault_root: &Path,
    id: &str,
    items: &[ToolbarItem],
) -> Result<Toolbars, String> {
    let names = Value::from(item_names(items));
    set_toolbar_field(vault_root, id, "items", Some(&names))
}

/// `toolbar`, then `toolbar-2`, `toolbar-3`… whichever is free first.
fn free_id(toolbars: &Toolbars) -> String {
    std::iter::once("toolbar".to_owned())
        .chain((2..).map(|n| format!("toolbar-{n}")))
        .find(|id| toolbars.get(id).is_none())
        .unwrap_or_default()
}

/// Adds an empty toolbar at `place`. Returns its id and the toolbars in
/// effect.
pub fn add_toolbar(vault_root: &Path, place: Place) -> Result<(String, Toolbars), String> {
    let path = toolbars_path(vault_root);
    let mut file = SettingsFile::load(&path)?;
    let id = free_id(&load_toolbars(vault_root));
    let number = id.strip_prefix("toolbar-").unwrap_or("1");
    file.set(
        &field_key(&id, "title"),
        &Value::from(format!("Toolbar {number}")),
    )?;
    file.set(&field_key(&id, "place"), &Value::from(choice_name(place)))?;
    file.set(&field_key(&id, "items"), &Value::Array(Vec::new()))?;
    let toolbars = save_toolbars(&path, &file)?;
    Ok((id, toolbars))
}

/// Removes toolbar `id`: a built-in one is turned off, since the built-in
/// file would bring it back, and one the vault added is deleted.
pub fn remove_toolbar(vault_root: &Path, id: &str) -> Result<Toolbars, String> {
    if Toolbars::defaults().get(id).is_some() {
        return set_toolbar_field(vault_root, id, "enabled", Some(&Value::Bool(false)));
    }
    let path = toolbars_path(vault_root);
    let mut file = SettingsFile::load(&path)?;
    file.remove(&format!("toolbar.{id}"));
    save_toolbars(&path, &file)
}

/// Puts toolbar `id` back as the built-in file has it.
pub fn reset_toolbar(vault_root: &Path, id: &str) -> Result<Toolbars, String> {
    let path = toolbars_path(vault_root);
    let mut file = SettingsFile::load(&path)?;
    file.remove(&format!("toolbar.{id}"));
    save_toolbars(&path, &file)
}

/// Puts every toolbar, menu and timing back to the built-in ones. The
/// file keeps any comments that aren't inside those tables.
pub fn reset_toolbars(vault_root: &Path) -> Result<Toolbars, String> {
    let path = toolbars_path(vault_root);
    let mut file = SettingsFile::load(&path)?;
    for table in ["toolbar", "menu", "timing"] {
        file.remove(table);
    }
    save_toolbars(&path, &file)
}

/// Checks a new id for a toolbar or menu.
pub fn check_id(id: &str) -> Result<(), String> {
    if is_valid_id(id) {
        Ok(())
    } else {
        Err(format!(
            "`{id}` can only use lowercase letters, digits and dashes"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const FILE: &str = "\
# My bars.
[toolbar.status]
items = [\"word-count\"]   # just the count
";

    fn vault(text: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if let Some(text) = text {
            fs::create_dir_all(dir.path().join(CONFIG_DIR)).unwrap();
            fs::write(toolbars_path(dir.path()), text).unwrap();
        }
        dir
    }

    fn read(dir: &tempfile::TempDir) -> String {
        fs::read_to_string(toolbars_path(dir.path())).unwrap_or_default()
    }

    #[test]
    fn fields_are_written_beside_comments_and_built_in_values_removed() {
        let dir = vault(Some(FILE));
        let place = Value::from("editor-bottom");
        let toolbars = set_toolbar_field(dir.path(), "status", "place", Some(&place)).unwrap();
        assert_eq!(toolbars.get("status").unwrap().place, Place::EditorBottom);
        let text = read(&dir);
        assert!(text.starts_with("# My bars.\n[toolbar.status]\n"), "{text}");
        assert!(
            text.contains("items = [\"word-count\"]   # just the count"),
            "{text}"
        );
        let back = Value::from("status-bar");
        set_toolbar_field(dir.path(), "status", "place", Some(&back)).unwrap();
        assert!(!read(&dir).contains("place"));
        let error = set_toolbar_field(dir.path(), "status", "place", Some(&Value::from("roof")));
        assert!(error.is_err());
        assert!(!read(&dir).contains("roof"));
    }

    #[test]
    fn a_bar_is_set_to_float_over_the_note_and_back() {
        let dir = vault(Some(FILE));
        let (id, _) = add_toolbar(dir.path(), Place::EditorTop).unwrap();
        let overlay = Value::from("overlay");
        let toolbars = set_toolbar_field(dir.path(), &id, "surface", Some(&overlay)).unwrap();
        assert!(toolbars.get(&id).unwrap().floats_over_note());
        assert!(read(&dir).contains("surface = \"overlay\""));
        let strip = Value::from("strip");
        set_toolbar_field(dir.path(), "status", "surface", Some(&strip)).unwrap();
        assert!(
            !read(&dir).contains("surface = \"strip\""),
            "the built-in value isn't written"
        );
        let toolbars = set_toolbar_field(dir.path(), &id, "surface", None).unwrap();
        assert!(!toolbars.get(&id).unwrap().floats_over_note());
        assert!(!read(&dir).contains("surface"));
    }

    #[test]
    fn toolbars_are_added_removed_and_reset() {
        let dir = vault(None);
        let (id, toolbars) = add_toolbar(dir.path(), Place::EditorTop).unwrap();
        assert_eq!(id, "toolbar");
        assert_eq!(toolbars.get("toolbar").unwrap().title, "Toolbar 1");
        let (second, _) = add_toolbar(dir.path(), Place::WindowLeft).unwrap();
        assert_eq!(second, "toolbar-2");
        let items = [
            ToolbarItem::Command("export.html".into()),
            ToolbarItem::Separator,
        ];
        let toolbars = set_toolbar_items(dir.path(), &second, &items).unwrap();
        assert_eq!(toolbars.get(&second).unwrap().items, items);
        let toolbars = remove_toolbar(dir.path(), "toolbar").unwrap();
        assert!(toolbars.get("toolbar").is_none());
        let toolbars = remove_toolbar(dir.path(), "status").unwrap();
        assert!(!toolbars.get("status").unwrap().enabled);
        let toolbars = reset_toolbars(dir.path()).unwrap();
        assert_eq!(toolbars, Toolbars::defaults());
        assert_eq!(read(&dir).trim(), "");
    }

    #[test]
    fn a_built_in_toolbar_resets_alone() {
        let dir = vault(Some(FILE));
        add_toolbar(dir.path(), Place::EditorBottom).unwrap();
        let toolbars = reset_toolbar(dir.path(), "status").unwrap();
        assert_eq!(
            toolbars.get("status").unwrap(),
            Toolbars::defaults().get("status").unwrap()
        );
        assert!(toolbars.get("toolbar").is_some());
    }
}
