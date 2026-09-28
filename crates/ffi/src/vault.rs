//! A vault folder on the phone: its notes, reading and saving them, and its
//! theme. Sync will hang off this object when it comes to the phone.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use editor_config::ConfigLoader;
use editor_vault::build::scan;
use editor_vault::entries::{EntryKind, display_name, is_hidden, natural_cmp};
use editor_vault::files::atomic_write;
use editor_vault::index::is_note_path;
use editor_vault::link_update::{file_name, parent_dir};

use crate::theme::{ThemeTokens, theme};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum VaultError {
    #[error("there's no folder at {path}")]
    NoFolder { path: String },
    #[error("{path} isn't a note in this vault")]
    NotANote { path: String },
    #[error("{message}")]
    Io { message: String },
}

impl From<std::io::Error> for VaultError {
    fn from(error: std::io::Error) -> Self {
        VaultError::Io {
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NoteSummary {
    /// Relative to the vault, with `/` between folders.
    pub path: String,
    /// The file name without `.md`.
    pub title: String,
    /// The folder it's in, empty at the vault's top.
    pub folder: String,
    /// When it last changed, in seconds since 1970.
    pub modified: i64,
}

#[derive(uniffi::Object)]
pub struct VaultFolder {
    root: PathBuf,
}

#[uniffi::export]
impl VaultFolder {
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Arc<Self>, VaultError> {
        let root = PathBuf::from(&path);
        if !root.is_dir() {
            return Err(VaultError::NoFolder { path });
        }
        Ok(Arc::new(Self { root }))
    }

    /// The folder's name.
    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Every note, most recently changed first.
    pub fn notes(&self) -> Vec<NoteSummary> {
        let (_, paths) = scan(&self.root, &self.root);
        let mut notes: Vec<NoteSummary> =
            paths.into_iter().map(|path| self.summary(path)).collect();
        notes.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| natural_cmp(&a.path, &b.path))
        });
        notes
    }

    pub fn read_note(&self, path: String) -> Result<String, VaultError> {
        let full = self.note_path(&path)?;
        Ok(std::fs::read_to_string(full)?)
    }

    /// Writes the note in one step, so a crash never leaves half of it.
    pub fn save_note(&self, path: String, text: String) -> Result<(), VaultError> {
        let full = self.note_path(&path)?;
        if let Some(folder) = full.parent() {
            std::fs::create_dir_all(folder)?;
        }
        Ok(atomic_write(&full, &text)?)
    }

    /// The built-in theme with the vault's `.editor/theme.toml` and
    /// `appearance.base-font-size` on top.
    pub fn theme(&self) -> ThemeTokens {
        let mut loader = ConfigLoader::for_vault(&self.root);
        loader.load_all();
        theme(loader.config())
    }
}

impl VaultFolder {
    fn summary(&self, path: String) -> NoteSummary {
        let modified = std::fs::metadata(self.root.join(&path))
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs() as i64);
        NoteSummary {
            title: display_name(file_name(&path), EntryKind::Note).to_owned(),
            folder: parent_dir(&path).to_owned(),
            modified,
            path,
        }
    }

    /// The note's place on disk, refusing anything that could reach
    /// outside the vault or into its hidden folders.
    fn note_path(&self, path: &str) -> Result<PathBuf, VaultError> {
        let refused = || VaultError::NotANote {
            path: path.to_owned(),
        };
        let relative = Path::new(path);
        let inside = relative.components().all(|component| match component {
            Component::Normal(name) => !is_hidden(&name.to_string_lossy()),
            _ => false,
        });
        if !inside || !is_note_path(path) {
            return Err(refused());
        }
        Ok(self.root.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_with(notes: &[(&str, &str)]) -> (tempfile::TempDir, Arc<VaultFolder>) {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in notes {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        let vault = VaultFolder::open(dir.path().to_string_lossy().into_owned()).unwrap();
        (dir, vault)
    }

    #[test]
    fn notes_list_titles_and_folders_and_skip_hidden_files() {
        let (_dir, vault) = vault_with(&[
            ("Top.md", "a"),
            ("Physics/Waves.md", "b"),
            (".editor/theme.md", "c"),
            ("image.png", "d"),
        ]);
        let mut notes = vault.notes();
        notes.sort_by(|a, b| a.path.cmp(&b.path));
        let listed: Vec<(&str, &str)> = notes
            .iter()
            .map(|note| (note.title.as_str(), note.folder.as_str()))
            .collect();
        assert_eq!(listed, vec![("Waves", "Physics"), ("Top", "")]);
    }

    #[test]
    fn a_saved_note_reads_back() {
        let (_dir, vault) = vault_with(&[("Plan.md", "old")]);
        vault
            .save_note("Plan.md".into(), "new text".into())
            .unwrap();
        assert_eq!(vault.read_note("Plan.md".into()).unwrap(), "new text");
    }

    #[test]
    fn paths_outside_the_vault_are_refused() {
        let (_dir, vault) = vault_with(&[("Plan.md", "old")]);
        for path in ["../Plan.md", "/etc/Plan.md", ".git/Plan.md", "Plan.txt"] {
            assert!(
                matches!(
                    vault.save_note(path.into(), String::new()),
                    Err(VaultError::NotANote { .. })
                ),
                "{path}"
            );
        }
    }

    #[test]
    fn a_missing_folder_does_not_open() {
        assert!(matches!(
            VaultFolder::open("/no/such/vault".into()),
            Err(VaultError::NoFolder { .. })
        ));
    }

    #[test]
    fn the_theme_reads_the_vault_s_base_size() {
        let (_dir, vault) = vault_with(&[(
            ".editor/settings.toml",
            "[appearance]\nbase-font-size = 15\n",
        )]);
        assert!((vault.theme().typography.body_size - 20.).abs() < 1e-9);
    }
}
