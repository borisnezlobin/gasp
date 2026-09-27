//! State that belongs to this device and never syncs.
//!
//! - The last vault opened lives in `state.toml` in the platform's config
//!   folder (such as `~/.config/editor` on Linux), because it's about the
//!   machine rather than any vault.
//! - Each vault's window size and position and its open tabs live in the
//!   vault's `.editor/device.toml`, which the config crate defines and
//!   never syncs.

use std::io;
use std::path::{Path, PathBuf};

use editor_config::device::{DeviceSettings, WindowState};
use gpui::{Bounds, Pixels, WindowBounds, point, px, size};
use serde::{Deserialize, Serialize};

use super::files::atomic_write;

/// The app's folder inside the platform config folder.
const APP_DIR: &str = "editor";
const STATE_FILE: &str = "state.toml";
/// The vault's config folder and its device-local file.
const CONFIG_DIR: &str = ".editor";
const DEVICE_FILE: &str = "device.toml";

/// Device-wide state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AppState {
    pub last_vault: Option<PathBuf>,
}

impl AppState {
    /// Where the state file lives on this platform.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join(APP_DIR).join(STATE_FILE))
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
        let state = AppState {
            last_vault: Some(vault.to_path_buf()),
        };
        if let Err(error) = state.save(&path) {
            eprintln!("could not remember the vault: {error}");
        }
    }

    /// The last vault, if it still exists.
    pub fn last_vault() -> Option<PathBuf> {
        let path = AppState::default_path()?;
        AppState::load(&path)
            .last_vault
            .filter(|vault| vault.is_dir())
    }
}

/// `<vault>/.editor/device.toml`.
pub fn device_path(vault: &Path) -> PathBuf {
    vault.join(CONFIG_DIR).join(DEVICE_FILE)
}

/// Writes the vault's device file, creating `.editor/` if needed.
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
        };
        state.save(&path).unwrap();
        assert_eq!(AppState::load(&path), state);
        assert_eq!(
            AppState::load(&dir.path().join("missing")),
            AppState::default()
        );
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
