//! Moving the folders an earlier version named after "editor" to Gasp's
//! names: a vault's `.editor/` to `.gasp/`, and the app's folder in the
//! platform's config and data folders. A folder moves only when the new
//! one isn't there yet; when both are, the new one wins and the old one
//! stays untouched.

use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

use crate::names::{APP_FOLDER, CONFIG_DIR, LEGACY_APP_FOLDER, LEGACY_CONFIG_DIR};
use crate::store::{SettingsFile, save, settings_path};

const DEVICE_ONLY_KEY: &str = "sync.device-only";

/// What happened to a legacy folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderMigration {
    NoLegacyFolder,
    Moved,
    BothExist,
}

/// Renames `<vault>/.editor` to `<vault>/.gasp` in one step, so sync sees
/// every file in it as renamed. A settings file whose device-only list
/// names files in `.editor/` gets the same files in `.gasp/` added.
pub fn migrate_config_dir(vault: &Path) -> io::Result<FolderMigration> {
    let outcome = move_legacy_folder(&vault.join(LEGACY_CONFIG_DIR), &vault.join(CONFIG_DIR))?;
    if outcome == FolderMigration::Moved {
        carry_device_only_globs(vault)?;
    }
    Ok(outcome)
}

/// Renames `<base>/editor` to the app's folder in `base`, one of the
/// platform's config or data folders.
pub fn migrate_app_folder(base: &Path) -> io::Result<FolderMigration> {
    move_legacy_folder(&base.join(LEGACY_APP_FOLDER), &base.join(APP_FOLDER))
}

/// [`migrate_config_dir`], saying on stderr what it did.
pub fn migrate_config_dir_and_log(vault: &Path) {
    log_migration(
        migrate_config_dir(vault),
        &vault.join(LEGACY_CONFIG_DIR),
        &vault.join(CONFIG_DIR),
    );
}

/// [`migrate_app_folder`], saying on stderr what it did.
pub fn migrate_app_folder_and_log(base: &Path) {
    log_migration(
        migrate_app_folder(base),
        &base.join(LEGACY_APP_FOLDER),
        &base.join(APP_FOLDER),
    );
}

fn move_legacy_folder(legacy: &Path, current: &Path) -> io::Result<FolderMigration> {
    if !legacy.is_dir() {
        return Ok(FolderMigration::NoLegacyFolder);
    }
    if fs::symlink_metadata(current).is_ok() {
        return Ok(FolderMigration::BothExist);
    }
    fs::rename(legacy, current)?;
    Ok(FolderMigration::Moved)
}

fn log_migration(result: io::Result<FolderMigration>, legacy: &Path, current: &Path) {
    let (legacy, current) = (legacy.display(), current.display());
    match result {
        Ok(FolderMigration::NoLegacyFolder) => {}
        Ok(FolderMigration::Moved) => eprintln!("moved {legacy} to {current}"),
        Ok(FolderMigration::BothExist) => {
            eprintln!(
                "{legacy} and {current} both exist; using {current} and leaving {legacy} alone"
            )
        }
        Err(error) => eprintln!("could not move {legacy} to {current}: {error}"),
    }
}

fn carry_device_only_globs(vault: &Path) -> io::Result<()> {
    let path = settings_path(vault);
    let Ok(mut file) = SettingsFile::load(&path) else {
        return Ok(());
    };
    let Some(Value::Array(globs)) = file.get(DEVICE_ONLY_KEY) else {
        return Ok(());
    };
    let carried = with_current_config_dir(&globs);
    if carried.len() == globs.len() {
        return Ok(());
    }
    file.set(DEVICE_ONLY_KEY, &Value::Array(carried))
        .map_err(io::Error::other)?;
    save(&path, &file.to_string())
}

/// The globs, each one in `.editor/` followed by the same glob in `.gasp/`
/// unless the list already has it.
fn with_current_config_dir(globs: &[Value]) -> Vec<Value> {
    let legacy_prefix = format!("{LEGACY_CONFIG_DIR}/");
    let mut carried = Vec::with_capacity(globs.len());
    for glob in globs {
        carried.push(glob.clone());
        let Some(rest) = glob
            .as_str()
            .and_then(|glob| glob.strip_prefix(&legacy_prefix))
        else {
            continue;
        };
        let current = Value::String(format!("{CONFIG_DIR}/{rest}"));
        if !globs.contains(&current) {
            carried.push(current);
        }
    }
    carried
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_legacy_config_folder_moves_with_everything_in_it() {
        let vault = tempfile::tempdir().unwrap();
        let legacy = vault.path().join(LEGACY_CONFIG_DIR);
        write(&legacy.join("settings.toml"), "[files]\n");
        write(&legacy.join("stats/laptop.json"), "{}");

        assert_eq!(
            migrate_config_dir(vault.path()).unwrap(),
            FolderMigration::Moved
        );
        let current = vault.path().join(CONFIG_DIR);
        assert!(!legacy.exists());
        assert_eq!(
            fs::read_to_string(current.join("settings.toml")).unwrap(),
            "[files]\n"
        );
        assert!(current.join("stats/laptop.json").is_file());
        assert_eq!(
            migrate_config_dir(vault.path()).unwrap(),
            FolderMigration::NoLegacyFolder
        );
    }

    #[test]
    fn when_both_config_folders_exist_neither_changes() {
        let vault = tempfile::tempdir().unwrap();
        let legacy = vault.path().join(LEGACY_CONFIG_DIR).join("settings.toml");
        let current = vault.path().join(CONFIG_DIR).join("settings.toml");
        write(&legacy, "old");
        write(&current, "new");

        assert_eq!(
            migrate_config_dir(vault.path()).unwrap(),
            FolderMigration::BothExist
        );
        assert_eq!(fs::read_to_string(legacy).unwrap(), "old");
        assert_eq!(fs::read_to_string(current).unwrap(), "new");
    }

    #[test]
    fn a_vault_without_a_legacy_folder_is_left_alone() {
        let vault = tempfile::tempdir().unwrap();
        assert_eq!(
            migrate_config_dir(vault.path()).unwrap(),
            FolderMigration::NoLegacyFolder
        );
        assert!(!vault.path().join(CONFIG_DIR).exists());
    }

    #[test]
    fn device_only_globs_in_the_legacy_folder_follow_it() {
        let vault = tempfile::tempdir().unwrap();
        let legacy_device = format!("{LEGACY_CONFIG_DIR}/device.toml");
        write(
            &vault
                .path()
                .join(LEGACY_CONFIG_DIR)
                .join(crate::store::SETTINGS_FILE),
            &format!(
                "# mine\n[sync]\ndevice-only = [\"{legacy_device}\", \"**/.DS_Store\"] # kept\n"
            ),
        );

        migrate_config_dir(vault.path()).unwrap();
        let file = SettingsFile::load(&settings_path(vault.path())).unwrap();
        assert_eq!(
            file.get(DEVICE_ONLY_KEY),
            Some(serde_json::json!([
                legacy_device,
                format!("{CONFIG_DIR}/device.toml"),
                "**/.DS_Store"
            ]))
        );
        assert!(file.to_string().starts_with("# mine\n"));
    }

    #[test]
    fn a_legacy_app_folder_moves_to_the_new_name() {
        let base = tempfile::tempdir().unwrap();
        write(
            &base.path().join(LEGACY_APP_FOLDER).join("state.toml"),
            "last-vault = \"v\"\n",
        );

        assert_eq!(
            migrate_app_folder(base.path()).unwrap(),
            FolderMigration::Moved
        );
        assert!(!base.path().join(LEGACY_APP_FOLDER).exists());
        assert_eq!(
            fs::read_to_string(base.path().join(APP_FOLDER).join("state.toml")).unwrap(),
            "last-vault = \"v\"\n"
        );
    }

    #[test]
    fn an_existing_app_folder_is_kept_over_the_legacy_one() {
        let base = tempfile::tempdir().unwrap();
        write(
            &base.path().join(LEGACY_APP_FOLDER).join("state.toml"),
            "old",
        );
        write(&base.path().join(APP_FOLDER).join("state.toml"), "new");

        assert_eq!(
            migrate_app_folder(base.path()).unwrap(),
            FolderMigration::BothExist
        );
        assert_eq!(
            fs::read_to_string(base.path().join(LEGACY_APP_FOLDER).join("state.toml")).unwrap(),
            "old"
        );
        assert_eq!(
            fs::read_to_string(base.path().join(APP_FOLDER).join("state.toml")).unwrap(),
            "new"
        );
    }
}
