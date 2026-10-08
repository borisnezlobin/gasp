//! What every tool call can reach: the vault on disk, its settings as
//! they are now, and the app if it has the vault open.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use gasp_config::Settings;
use gasp_config::loader::build_settings;
use gasp_config::migration::migrate_config_dir_and_log;
use gasp_config::store::{SETTINGS_FILE, settings_path};
use gasp_search::engine::{Note, NoteCache};
use gasp_vault::index::LinkIndex;

use crate::bridge::client::AppLink;
use crate::bridge::endpoint::Endpoint;
use crate::paths::{self, VaultPath};
use crate::tool::ToolError;

/// One vault, for the life of the server.
pub struct Context {
    /// Canonical, so paths through links can be checked against it.
    root: PathBuf,
    /// Notes kept in memory between searches; each search rereads only
    /// what changed on disk.
    notes: Mutex<NoteCache>,
    app: AppLink,
}

impl Context {
    /// The vault at `vault`, and the app if it opens it. A legacy
    /// `.editor` folder in the vault moves to `.gasp` first.
    pub fn open(vault: &Path) -> io::Result<Context> {
        let root = canonical_vault(vault)?;
        migrate_config_dir_and_log(&root);
        let endpoint = Endpoint::for_vault(&root);
        Ok(Context::with_app(root, AppLink::new(endpoint)))
    }

    /// The vault at `vault` with a given way to reach the app.
    pub fn with_endpoint(vault: &Path, endpoint: Option<Endpoint>) -> io::Result<Context> {
        let root = canonical_vault(vault)?;
        Ok(Context::with_app(root, AppLink::new(endpoint)))
    }

    fn with_app(root: PathBuf, app: AppLink) -> Context {
        Context {
            root,
            notes: Mutex::default(),
            app,
        }
    }

    /// A vault with no app, for unit tests.
    #[cfg(test)]
    pub fn for_tests(root: &Path) -> Context {
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Context::with_app(root, AppLink::new(None))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn app(&self) -> &AppLink {
        &self.app
    }

    pub fn resolve(&self, path: &str) -> Result<VaultPath, ToolError> {
        paths::resolve(&self.root, path)
    }

    pub fn resolve_note(&self, path: &str) -> Result<VaultPath, ToolError> {
        paths::resolve_note(&self.root, path)
    }

    pub fn resolve_attachment(&self, path: &str) -> Result<VaultPath, ToolError> {
        paths::resolve_attachment(&self.root, path)
    }

    /// Fails in words when the vault folder can't be read, so a blocked
    /// vault isn't reported as an empty one. macOS keeps refusing a
    /// server that started before Gasp was allowed into the folder, or
    /// before Gasp was updated, until that server restarts.
    pub fn check_readable(&self) -> Result<(), ToolError> {
        match std::fs::read_dir(&self.root) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                Err(ToolError::new(format!(
                    "this server isn't allowed to read {}. Restart the app that started it, \
                     such as Claude, and allow {} into the folder if macOS asks.",
                    self.root.display(),
                    gasp_config::APP_NAME,
                )))
            }
            Err(error) => Err(ToolError::io(&self.root.display().to_string(), &error)),
        }
    }

    /// The vault's settings as its file says now, or the built-in ones
    /// when the file is missing or broken.
    pub fn settings(&self) -> Settings {
        let text = std::fs::read_to_string(settings_path(&self.root)).ok();
        build_settings(SETTINGS_FILE, text.as_deref())
            .map(|(settings, _)| settings)
            .unwrap_or_default()
    }

    /// The link index, read from disk now, so it's never stale. Reading
    /// the synthetic corpus's 200 notes takes a few milliseconds.
    pub fn link_index(&self) -> LinkIndex {
        gasp_vault::build::build_index(&self.root)
    }

    /// Every note, for searching, brought up to date with the disk.
    pub fn search_notes(&self) -> Vec<Note> {
        let mut cache = self.notes.lock().unwrap_or_else(PoisonError::into_inner);
        cache.refresh(&self.root);
        cache.notes()
    }
}

fn canonical_vault(vault: &Path) -> io::Result<PathBuf> {
    let root = std::fs::canonicalize(vault)?;
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} isn't a folder", vault.display()),
        ));
    }
    Ok(root)
}
