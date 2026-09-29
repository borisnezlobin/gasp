//! State that belongs to this device and never syncs.
//!
//! - The last vault opened lives in `state.toml` in the platform's config
//!   folder (such as `~/.config/gasp` on Linux), because it's about the
//!   machine rather than any vault.
//! - Each vault's window size and position and its open tabs live in the
//!   vault's `.gasp/device.toml`, which the config crate defines and
//!   never syncs.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use gasp_config::device::{DeviceSettings, WindowState};
use gasp_config::migration::migrate_app_folder_and_log;
use gasp_config::{APP_FOLDER, CONFIG_DIR};
use gpui::{Bounds, Pixels, WindowBounds, point, px, size};
use serde::{Deserialize, Serialize};

use super::files::atomic_write;

const STATE_FILE: &str = "state.toml";
const DEVICE_FILE: &str = "device.toml";

/// Moves the app's folders from the names an earlier version used, in
/// the platform's config and data folders (one and the same on macOS).
pub fn migrate_app_folders() {
    let bases: BTreeSet<PathBuf> = [dirs::config_dir(), dirs::data_local_dir()]
        .into_iter()
        .flatten()
        .collect();
    for base in bases {
        migrate_app_folder_and_log(&base);
    }
}

/// Device-wide state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AppState {
    pub last_vault: Option<PathBuf>,
    /// Vaults opened on this device, most recent first.
    pub recent_vaults: Vec<PathBuf>,
}

/// Vaults the switcher remembers.
pub const MAX_RECENT_VAULTS: usize = 10;

impl AppState {
    /// Where the state file lives on this platform.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join(APP_FOLDER).join(STATE_FILE))
    }

    /// Reads the state, or the default when there's none or it's unreadable.
    pub fn load(path: &Path) -> AppState {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        atomic_write(path, &text)
    }

    /// Records `vault` as the last one opened, logging a failure.
    pub fn remember_vault(vault: &Path) {
        let Some(path) = AppState::default_path() else {
            return;
        };
        let mut state = AppState::load(&path);
        state.opened(vault);
        if let Err(error) = state.save(&path) {
            eprintln!("could not remember the vault: {error}");
        }
    }

    /// Puts `vault` first in the recent list and makes it the last one.
    pub fn opened(&mut self, vault: &Path) {
        self.last_vault = Some(vault.to_path_buf());
        self.recent_vaults.retain(|recent| recent != vault);
        self.recent_vaults.insert(0, vault.to_path_buf());
        self.recent_vaults.truncate(MAX_RECENT_VAULTS);
    }

    /// Recent vaults that still exist, most recent first.
    pub fn recent_vaults() -> Vec<PathBuf> {
        let Some(path) = AppState::default_path() else {
            return Vec::new();
        };
        let mut state = AppState::load(&path);
        if state.recent_vaults.is_empty() {
            state.recent_vaults.extend(state.last_vault.clone());
        }
        state.recent_vaults.retain(|vault| vault.is_dir());
        state.recent_vaults
    }

    /// The last vault, if it still exists.
    pub fn last_vault() -> Option<PathBuf> {
        let path = AppState::default_path()?;
        AppState::load(&path)
            .last_vault
            .filter(|vault| vault.is_dir())
    }
}

/// `<vault>/.gasp/device.toml`.
pub fn device_path(vault: &Path) -> PathBuf {
    vault.join(CONFIG_DIR).join(DEVICE_FILE)
}

/// Writes the vault's device file, creating `.gasp/` if needed.
pub fn save_device(vault: &Path, device: &DeviceSettings) -> io::Result<()> {
    let path = device_path(vault);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = device.to_toml();
    if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    atomic_write(&path, &text)
}

/// The saved window placement as GPUI bounds, when it has a position.
pub fn window_bounds(window: &WindowState) -> Option<WindowBounds> {
    let (x, y) = (window.x?, window.y?);
    let bounds = Bounds {
        origin: point(px(x as f32), px(y as f32)),
        size: size(px(window.width as f32), px(window.height as f32)),
    };
    Some(if window.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    })
}

/// The saved size alone, for centring a window with no saved position.
pub fn window_size(window: &WindowState) -> gpui::Size<Pixels> {
    size(px(window.width as f32), px(window.height as f32))
}

/// GPUI's window placement as device state.
pub fn window_state(bounds: WindowBounds) -> WindowState {
    let (restore, maximized) = match bounds {
        WindowBounds::Windowed(bounds) => (bounds, false),
        WindowBounds::Maximized(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, true),
    };
    WindowState {
        width: f32::from(restore.size.width).round().max(1.) as u32,
        height: f32::from(restore.size.height).round().max(1.) as u32,
        x: Some(f32::from(restore.origin.x).round() as i32),
        y: Some(f32::from(restore.origin.y).round() as i32),
        maximized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/state.toml");
        let state = AppState {
            last_vault: Some(PathBuf::from("vaults/notes")),
            recent_vaults: vec![PathBuf::from("vaults/notes")],
        };
        state.save(&path).unwrap();
        assert_eq!(AppState::load(&path), state);
        assert_eq!(
            AppState::load(&dir.path().join("missing")),
            AppState::default()
        );
    }

    #[test]
    fn opening_a_vault_moves_it_to_the_front() {
        let mut state = AppState::default();
        state.opened(Path::new("a"));
        state.opened(Path::new("b"));
        state.opened(Path::new("a"));
        assert_eq!(
            state.recent_vaults,
            [PathBuf::from("a"), PathBuf::from("b")]
        );
        assert_eq!(state.last_vault, Some(PathBuf::from("a")));
        let old: AppState = toml::from_str("last-vault = \"x\"\n").unwrap();
        assert!(old.recent_vaults.is_empty());
    }

    #[test]
    fn window_placement_round_trips() {
        let bounds = WindowBounds::Windowed(Bounds {
            origin: point(px(10.), px(20.)),
            size: size(px(800.), px(600.)),
        });
        let state = window_state(bounds);
        assert_eq!(window_bounds(&state), Some(bounds));
        assert_eq!(window_bounds(&WindowState::default()), None);
    }

    #[test]
    fn device_file_is_written_in_the_config_folder() {
        let vault = tempfile::tempdir().unwrap();
        let device = DeviceSettings {
            open_tabs: vec!["a.md".into()],
            active_tab: Some(0),
            ..DeviceSettings::default()
        };
        save_device(vault.path(), &device).unwrap();
        let text = std::fs::read_to_string(device_path(vault.path())).unwrap();
        assert!(text.contains("a.md"));
    }
}
