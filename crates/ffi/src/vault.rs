//! A vault folder on the phone: its notes, its config, and how its notes
//! are drawn. Sync will hang off this object when it comes to the phone.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::UNIX_EPOCH;

use gasp_config::loader::CONFIG_DIR;
use gasp_config::settings::ThemeChoice;
use gasp_config::store::save;
use gasp_config::{Config, ConfigLoader};
use gasp_vault::build::{build_index, scan};
use gasp_vault::entries::{EntryKind, display_name, is_hidden, natural_cmp};
use gasp_vault::files::atomic_write;
use gasp_vault::index::{LinkIndex, is_note_path};
use gasp_vault::link_update::{file_name, parent_dir};

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
    pub(crate) search_notes: Mutex<gasp_search::engine::NoteCache>,
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
            search_notes: Mutex::default(),
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

    /// Reads the notes and files at `paths` again after something other
    /// than this vault changed them, such as a sync.
    pub fn files_changed(&self, paths: Vec<String>) {
        self.reindex(&paths);
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

    /// The look `appearance.theme` asks for: light, dark, or whichever the
    /// phone uses. The theme's palettes hold both; this picks one.
    pub fn appearance(&self) -> Appearance {
        match self.config().settings.appearance.theme {
            ThemeChoice::Light => Appearance::Light,
            ThemeChoice::Dark => Appearance::Dark,
            ThemeChoice::MatchSystem => Appearance::System,
        }
    }

    /// Every command the phone runs, in the registry's order.
    pub fn commands(&self) -> Vec<CommandInfo> {
        commands::command_infos(&self.config())
    }

    /// The keys that run commands with a hardware keyboard.
    pub fn key_bindings(&self) -> Vec<KeyBinding> {
        commands::key_bindings(&self.config())
    }

    /// Whether a note's name shows as an editable title above its text.
    pub fn shows_inline_title(&self) -> bool {
        self.config().settings.editor.show_inline_title
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

    /// Reads the files at `paths` into the link index again, or forgets
    /// the ones that are gone, if the index has been built. Only what
    /// changed is read, so making, renaming or trashing a note costs the
    /// notes it touches rather than the whole vault.
    pub(crate) fn reindex(&self, paths: &[String]) {
        let mut index = self.index();
        let Some(index) = index.as_mut() else {
            return;
        };
        for path in paths {
            let full = self.root.join(path);
            if !full.is_file() {
                index.remove(path);
            } else if !is_note_path(path) {
                index.add_file(path);
            } else if let Ok(text) = std::fs::read_to_string(&full) {
                index.set_note(path, text);
            }
        }
    }

    /// Drops the link index, to be built again when next asked for: for
    /// folder changes, which move or remove every note inside.
    pub(crate) fn forget_index(&self) {
        *self.index() = None;
    }

    /// Follows a note moving from `from` to `to` in the link index, and
    /// reads again the notes whose links the move rewrote.
    pub(crate) fn reindex_move(&self, from: &str, to: &str, rewritten: &[String]) {
        if let Some(index) = self.index().as_mut() {
            index.rename(from, to);
        }
        self.reindex(rewritten);
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
    fn appearance_follows_the_theme_setting() {
        let (dir, vault) = vault_with(&[]);
        assert_eq!(vault.appearance(), Appearance::System);
        std::fs::create_dir_all(dir.path().join(CONFIG_DIR)).unwrap();
        std::fs::write(
            dir.path().join(CONFIG_DIR).join("settings.toml"),
            "[appearance]\ntheme = \"light\"\n",
        )
        .unwrap();
        vault.reload_config();
        assert_eq!(vault.appearance(), Appearance::Light);
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

/// Which palette the app shows, from `appearance.theme`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Appearance {
    Light,
    Dark,
    /// Follow the phone's light or dark mode.
    System,
}
