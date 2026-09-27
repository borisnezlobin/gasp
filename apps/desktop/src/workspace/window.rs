//! Opening vault windows, and what a workspace remembers about its window
//! and tabs between runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use editor_config::ConfigLoader;
use editor_config::device::DeviceSettings;
use editor_config::loader::ConfigFile;
use futures::StreamExt;
use gpui::{
    App, AppContext, Bounds, Context, PathPromptOptions, Pixels, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowOptions, point, px,
};

use super::files::{is_note, vault_for_note};
use super::state::{AppState, save_device, window_bounds, window_size, window_state};
use super::watcher::{DiskChange, normalize, watch};
use super::welcome::Welcome;
use super::{OpenIn, Workspace};

/// How long to gather a burst of file events before acting on them.
const WATCH_SETTLE: Duration = Duration::from_millis(100);

/// What `editor [PATH]` opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchTarget {
    /// A vault, and maybe one note in it.
    Vault {
        vault: PathBuf,
        note: Option<PathBuf>,
    },
    /// No vault yet: the empty state with "Open a folder".
    Welcome,
}

impl LaunchTarget {
    /// A folder opens as the vault, a note opens its vault with the note
    /// shown, and nothing reopens the last vault.
    pub fn resolve(
        path: Option<&Path>,
        last_vault: Option<PathBuf>,
    ) -> Result<LaunchTarget, String> {
        let Some(path) = path else {
            return Ok(
                last_vault.map_or(LaunchTarget::Welcome, |vault| LaunchTarget::Vault {
                    vault,
                    note: None,
                }),
            );
        };
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if path.is_dir() {
            return Ok(LaunchTarget::Vault {
                vault: path,
                note: None,
            });
        }
        if path.is_file() || is_note(&path) {
            return Ok(LaunchTarget::Vault {
                vault: vault_for_note(&path),
                note: Some(path),
            });
        }
        Err(format!("{} is not a folder or a note", path.display()))
    }
}

/// Opens a window for `target`.
pub fn open_target(target: LaunchTarget, cx: &mut App) -> anyhow::Result<()> {
    match target {
        LaunchTarget::Vault { vault, note } => open_vault_window(&vault, note, cx).map(|_| ()),
        LaunchTarget::Welcome => open_welcome_window(cx),
    }
}

/// Opens `vault` in a new window, where it was last time, with its tabs
/// back, plus `note` when given.
pub fn open_vault_window(
    vault: &Path,
    note: Option<PathBuf>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<Workspace>> {
    let device = load_device(vault);
    let options = window_options(&device, cx);
    let vault = vault.to_path_buf();
    let window = cx.open_window(options, move |window, cx| {
        cx.new(|cx| build_workspace(&vault, note.as_deref(), window, cx))
    })?;
    window.update(cx, |workspace, window, cx| {
        AppState::remember_vault(workspace.vault());
        workspace.focus_active(window, cx);
        cx.activate(true);
    })?;
    Ok(window)
}

/// Opens the empty state, which asks for a folder.
pub fn open_welcome_window(cx: &mut App) -> anyhow::Result<()> {
    let options = window_options(&DeviceSettings::default(), cx);
    let window = cx.open_window(options, |window, cx| cx.new(|cx| Welcome::new(window, cx)))?;
    window.update(cx, |_, window, cx| {
        window.set_window_title("Open a vault");
        cx.activate(true)
    })?;
    Ok(())
}

/// A workspace with its saved tabs and `note`, watching the disk.
pub fn build_workspace(
    vault: &Path,
    note: Option<&Path>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Workspace {
    let mut workspace = Workspace::new(vault, window, cx);
    crate::features::install(&mut workspace, window, cx);
    workspace.restore_session(window, cx);
    if let Some(note) = note
        && let Err(error) = open_or_create(&mut workspace, note, window, cx)
    {
        eprintln!("could not open {}: {error}", note.display());
    }
    workspace.watch_vault(window, cx);
    workspace
}

/// Opens `note`, creating it empty first when it doesn't exist yet, as
/// `editor new-idea.md` should.
fn open_or_create(
    workspace: &mut Workspace,
    note: &Path,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> std::io::Result<()> {
    if !note.exists() {
        super::files::atomic_write(note, "")?;
    }
    workspace.open_path(note, OpenIn::ActiveTab, window, cx)
}

fn load_device(vault: &Path) -> DeviceSettings {
    let mut loader = ConfigLoader::for_vault(vault);
    loader.reload(ConfigFile::Device);
    loader.config().device.clone()
}

fn window_options(device: &DeviceSettings, cx: &App) -> WindowOptions {
    let bounds = window_bounds(&device.window).unwrap_or_else(|| {
        WindowBounds::Windowed(Bounds::centered(None, window_size(&device.window), cx))
    });
    WindowOptions {
        window_bounds: Some(bounds),
        titlebar: Some(titlebar()),
        ..Default::default()
    }
}

/// On macOS the app draws under a hidden title bar, with the window
/// buttons centred in the tab bar's row. Elsewhere the system's title bar
/// stays.
fn titlebar() -> TitlebarOptions {
    let hidden = cfg!(target_os = "macos");
    TitlebarOptions {
        title: None,
        appears_transparent: hidden,
        traffic_light_position: hidden.then(|| point(px(14.), px(13.))),
    }
}

/// Room the window's own buttons take at its top-left: none in full
/// screen, where they hide.
pub(crate) fn window_buttons_inset(window: &Window, cx: &mut App) -> Pixels {
    if window.is_fullscreen() {
        px(0.)
    } else {
        crate::ui::ui_theme(cx).window_buttons_width
    }
}

impl Workspace {
    /// Saves on focus loss, tracks the window's placement, and saves
    /// everything when the window closes or the app quits.
    pub(crate) fn observe_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.window_bounds = Some(window.window_bounds());
        let bounds = cx.observe_window_bounds(window, |workspace, window, _| {
            workspace.window_bounds = Some(window.window_bounds());
        });
        let activation = cx.observe_window_activation(window, |workspace, window, cx| {
            if !window.is_window_active() {
                workspace.save_all(cx);
            }
        });
        let quit = cx.on_app_quit(|workspace, cx| {
            workspace.prepare_to_close(cx);
            async {}
        });
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |workspace, cx| {
                workspace.window_bounds = Some(window.window_bounds());
                workspace.prepare_to_close(cx);
            })
            .ok();
            true
        });
        self._subscriptions.extend([bounds, activation, quit]);
    }

    /// Saves notes and the device state.
    pub fn prepare_to_close(&mut self, cx: &mut Context<Self>) {
        self.save_for_close(cx);
        if let Err(error) = save_device(&self.vault, &self.device_state(cx)) {
            eprintln!("could not save the window state: {error}");
        }
    }

    /// The window and open tabs as they are now.
    pub fn device_state(&self, cx: &App) -> DeviceSettings {
        let mut device = self.config.device.clone();
        if let Some(bounds) = self.window_bounds {
            device.window = window_state(bounds);
        }
        let active = self.active_path(cx);
        device.open_tabs.clear();
        device.active_tab = None;
        for pane in self.panes.panes() {
            for tab in pane.read(cx).tabs() {
                let Some(path) = tab.path(cx) else {
                    continue;
                };
                if active.as_deref() == Some(path) && device.active_tab.is_none() {
                    device.active_tab = Some(device.open_tabs.len());
                }
                device.open_tabs.push(self.relative_name(path));
            }
        }
        device
    }

    fn relative_name(&self, path: &Path) -> String {
        let relative = path.strip_prefix(&self.vault).unwrap_or(path);
        relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Reopens the tabs saved in `.editor/device.toml`.
    pub fn restore_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let device = self.config.device.clone();
        let pane = self.active_pane.clone();
        let mut opened = Vec::new();
        for name in &device.open_tabs {
            let path = self.vault.join(name);
            if !path.is_file() {
                continue;
            }
            let replace = opened.is_empty();
            if self
                .show_path_in_pane(&pane, &path, replace, window, cx)
                .is_ok()
            {
                opened.push(path);
            }
        }
        let active = device
            .active_tab
            .and_then(|index| device.open_tabs.get(index));
        let active = active.map(|name| self.vault.join(name));
        if let Some(index) = active.and_then(|path| pane.read(cx).index_of_path(&path, cx)) {
            self.activate_tab(index, window, cx);
        }
    }

    /// Follows changes to the vault on disk.
    pub fn watch_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (watcher, mut receiver) = match watch(&self.vault) {
            Ok(watching) => watching,
            Err(error) => {
                eprintln!("could not watch {}: {error}", self.vault.display());
                return;
            }
        };
        self.watcher = Some(watcher);
        let task = cx.spawn_in(window, async move |workspace, cx| {
            while let Some(first) = receiver.next().await {
                cx.background_executor().timer(WATCH_SETTLE).await;
                let mut batch: Vec<DiskChange> = first;
                while let Ok(more) = receiver.try_recv() {
                    batch.extend(more);
                }
                let changes = normalize(batch);
                let updated = workspace.update_in(cx, |workspace, window, cx| {
                    workspace.apply_disk_changes(changes, window, cx)
                });
                if updated.is_err() {
                    break;
                }
            }
        });
        self.tasks.push(task);
    }

    /// `vault.open`: asks for a folder and opens it in a new window.
    pub(crate) fn prompt_for_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(folder_prompt());
        let task = cx.spawn_in(window, async move |_, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(vault) = paths.into_iter().next() else {
                return;
            };
            cx.update(|_, cx| {
                if let Err(error) = open_vault_window(&vault, None, cx) {
                    eprintln!("could not open {}: {error}", vault.display());
                }
            })
            .ok();
        });
        self.tasks.push(task);
    }
}

/// The native picker's options for choosing a vault folder.
pub fn folder_prompt() -> PathPromptOptions {
    PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_targets_follow_the_argument() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("a.md");
        std::fs::write(&note, "").unwrap();
        assert_eq!(
            LaunchTarget::resolve(Some(dir.path()), None),
            Ok(LaunchTarget::Vault {
                vault: dir.path().to_path_buf(),
                note: None
            })
        );
        assert_eq!(
            LaunchTarget::resolve(Some(&note), None),
            Ok(LaunchTarget::Vault {
                vault: dir.path().to_path_buf(),
                note: Some(note.clone())
            })
        );
        assert_eq!(LaunchTarget::resolve(None, None), Ok(LaunchTarget::Welcome));
        assert_eq!(
            LaunchTarget::resolve(None, Some(dir.path().to_path_buf())),
            Ok(LaunchTarget::Vault {
                vault: dir.path().to_path_buf(),
                note: None
            })
        );
        assert!(LaunchTarget::resolve(Some(&dir.path().join("nope")), None).is_err());
    }
}
