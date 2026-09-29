//! A vault folder on the phone: its notes, its config, and how its notes
//! are drawn. Sync will hang off this object when it comes to the phone.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::UNIX_EPOCH;

use editor_config::loader::CONFIG_DIR;
use editor_config::store::save;
use editor_config::{Config, ConfigLoader};
use editor_vault::build::{build_index, scan};
use editor_vault::entries::{EntryKind, display_name, is_hidden, natural_cmp};
use editor_vault::files::atomic_write;
use editor_vault::index::{LinkIndex, is_note_path};
use editor_vault::link_update::{file_name, parent_dir};

use crate::commands::{self, CommandInfo, KeyBinding};
use crate::display::{DisplayState, SharedDisplay, SymbolVisibility};
use crate::document::NoteDocument;
use crate::theme::{ThemeTokens, theme};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum VaultError {
    #[error("there's no folder at {path}")]
    NoFolder { path: String },
    #[error("{path} isn't a note in this vault")]
    NotANote { path: String },
    #[error("{message}")]
    Refused { message: String },
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

/// The notes open as tabs on this device, from `.gasp/device.toml`,
/// which never syncs.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct OpenTabs {
    pub paths: Vec<String>,
    pub active: Option<u32>,
}

#[derive(uniffi::Object)]
pub struct VaultFolder {
    pub(crate) root: PathBuf,
    config: Mutex<Config>,
    pub(crate) display: SharedDisplay,
    /// Built the first time links or tags are asked for.
    index: Mutex<Option<LinkIndex>>,
}

#[uniffi::export]
impl VaultFolder {
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Arc<Self>, VaultError> {
        let root = PathBuf::from(&path);
        if !root.is_dir() {
            return Err(VaultError::NoFolder { path });
        }
        let config = load_config(&root);
        Ok(Arc::new(Self {
            display: SharedDisplay::new(DisplayState::from_settings(&config.settings)),
            config: Mutex::new(config),
            index: Mutex::new(None),
            root,
        }))
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

    /// Every visible folder, `/`-separated, sorted as people count.
    pub fn folders(&self) -> Vec<String> {
        let mut folders = Vec::new();
        collect_folders(&self.root, "", &mut folders);
        folders.sort_by(|a, b| natural_cmp(a, b));
        folders
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
        atomic_write(&full, &text)?;
        if let Some(index) = self.index().as_mut() {
            index.set_note(&path, text);
        }
        Ok(())
    }

    /// A note's text, parsed and drawn with this vault's settings.
    pub fn document(&self, text: String) -> Arc<NoteDocument> {
        NoteDocument::with_display(text, self.display.clone())
    }

    /// Reads `.gasp/` again, after its files changed.
    pub fn reload_config(&self) {
        let config = load_config(&self.root);
        *self.display.lock() = DisplayState::from_settings(&config.settings);
        *self.config() = config;
    }

    /// The built-in theme with the vault's `.gasp/theme.toml` and
    /// `appearance.base-font-size` on top.
    pub fn theme(&self) -> ThemeTokens {
        theme(&self.config())
    }

    /// Every command the phone runs, in the registry's order.
    pub fn commands(&self) -> Vec<CommandInfo> {
        commands::command_infos(&self.config())
    }

    /// The keys that run commands with a hardware keyboard.
    pub fn key_bindings(&self) -> Vec<KeyBinding> {
        commands::key_bindings(&self.config())
    }

    /// The commands on the bar above the software keyboard, in order.
    pub fn toolbar(&self) -> Vec<CommandInfo> {
        commands::toolbar(&self.config())
    }

    pub fn symbol_visibility(&self) -> SymbolVisibility {
        self.display.lock().symbols.mode.into()
    }

    /// Moves every open note on to the next of always shown, shown around
    /// the cursor and always hidden, as `markdown.cycle-symbols` does.
    pub fn cycle_symbols(&self) -> SymbolVisibility {
        let mut display = self.display.lock();
        display.symbols.mode = display.symbols.mode.next();
        display.symbols.mode.into()
    }

    /// The open tabs saved on this device.
    pub fn open_tabs(&self) -> OpenTabs {
        let device = &self.config().device;
        OpenTabs {
            paths: device.open_tabs.clone(),
            active: device.active_tab.map(|index| index as u32),
        }
    }

    /// Saves the open tabs to `.gasp/device.toml`.
    pub fn save_open_tabs(&self, tabs: OpenTabs) -> Result<(), VaultError> {
        let mut config = self.config();
        config.device.open_tabs = tabs.paths;
        config.device.active_tab = tabs.active.map(|index| index as usize);
        let path = self.root.join(CONFIG_DIR).join("device.toml");
        Ok(save(&path, &config.device.to_toml())?)
    }
}

impl VaultFolder {
    pub(crate) fn config(&self) -> MutexGuard<'_, Config> {
        self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The link index, built on first use.
    pub(crate) fn index(&self) -> MutexGuard<'_, Option<LinkIndex>> {
        self.index.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn built_index(&self) -> MutexGuard<'_, Option<LinkIndex>> {
        let mut index = self.index();
        if index.is_none() {
            *index = Some(build_index(&self.root));
        }
        index
    }

    /// Forgets the link index after files move, so it's built again.
    pub(crate) fn forget_index(&self) {
        *self.index() = None;
    }

    pub(crate) fn summary(&self, path: String) -> NoteSummary {
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
    pub(crate) fn note_path(&self, path: &str) -> Result<PathBuf, VaultError> {
        if !is_inside(path) || !is_note_path(path) {
            return Err(VaultError::NotANote {
                path: path.to_owned(),
            });
        }
        Ok(self.root.join(path))
    }

    /// A folder's place on disk, with the same care as [`Self::note_path`].
    pub(crate) fn folder_path(&self, folder: &str) -> Result<PathBuf, VaultError> {
        if !folder.is_empty() && !is_inside(folder) {
            return Err(VaultError::Refused {
                message: format!("{folder} isn't a folder in this vault"),
            });
        }
        Ok(self.root.join(folder))
    }
}

fn load_config(root: &Path) -> Config {
    let mut loader = ConfigLoader::for_vault(root);
    loader.load_all();
    loader.config().clone()
}

/// Whether a relative path stays inside the vault and out of its hidden
/// folders.
fn is_inside(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|component| match component {
            Component::Normal(name) => !is_hidden(&name.to_string_lossy()),
            _ => false,
        })
}

fn collect_folders(root: &Path, relative: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root.join(relative)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_hidden(&name) || !entry.path().is_dir() {
            continue;
        }
        let path = match relative {
            "" => name,
            _ => format!("{relative}/{name}"),
        };
        collect_folders(root, &path, out);
        out.push(path);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn vault_with(notes: &[(&str, &str)]) -> (tempfile::TempDir, Arc<VaultFolder>) {
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
        let hidden = format!("{CONFIG_DIR}/theme.md");
        let (_dir, vault) = vault_with(&[
            ("Top.md", "a"),
            ("Physics/Waves.md", "b"),
            (&hidden, "c"),
            ("image.png", "d"),
        ]);
        let mut notes = vault.notes();
        notes.sort_by(|a, b| a.path.cmp(&b.path));
        let listed: Vec<(&str, &str)> = notes
            .iter()
            .map(|note| (note.title.as_str(), note.folder.as_str()))
            .collect();
        assert_eq!(listed, vec![("Waves", "Physics"), ("Top", "")]);
        assert_eq!(vault.folders(), ["Physics"]);
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
        let settings = format!("{CONFIG_DIR}/settings.toml");
        let (_dir, vault) = vault_with(&[(&settings, "[appearance]\nbase-font-size = 15\n")]);
        assert!((vault.theme().typography.body_size - 20.).abs() < 1e-9);
    }

    #[test]
    fn open_tabs_survive_a_reopen() {
        let (dir, vault) = vault_with(&[("A.md", ""), ("B.md", "")]);
        let tabs = OpenTabs {
            paths: vec!["A.md".into(), "B.md".into()],
            active: Some(1),
        };
        vault.save_open_tabs(tabs.clone()).unwrap();
        let reopened = VaultFolder::open(dir.path().to_string_lossy().into_owned()).unwrap();
        assert_eq!(reopened.open_tabs(), tabs);
    }

    #[test]
    fn cycling_symbols_changes_every_note() {
        let (_dir, vault) = vault_with(&[]);
        let note = vault.document("**bold** and more".into());
        let cursor_away = crate::offsets::TextRange { start: 17, end: 17 };
        assert!(!note.plan(cursor_away).lines[0].hidden.is_empty());
        assert_eq!(vault.cycle_symbols(), SymbolVisibility::AlwaysHidden);
        assert_eq!(vault.cycle_symbols(), SymbolVisibility::AlwaysShown);
        assert!(note.plan(cursor_away).lines[0].hidden.is_empty());
    }
}
