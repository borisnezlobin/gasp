//! Opening vault windows, and what a workspace remembers about its window
//! and tabs between runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt;
use gasp_config::device::DeviceSettings;
use gpui::{
    App, AppContext, Bounds, Context, PathPromptOptions, Pixels, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowOptions, point, px,
};

use super::files::{is_note, vault_for_note};
use super::startup::{PendingStart, VaultStart};
use super::state::{AppState, save_device, window_bounds, window_size, window_state};
use super::watcher::{DiskChange, normalize, watch};
use super::welcome::Welcome;
use super::{OpenIn, Workspace};
use crate::trace;

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

impl LaunchTarget {
    /// Starts reading what the target's window needs, on a background
    /// thread, so it's ready by the time the app can open the window.
    pub fn start_reading(&self) -> Option<PendingStart> {
        match self {
            LaunchTarget::Vault { vault, .. } => Some(VaultStart::spawn(vault.clone())),
            LaunchTarget::Welcome => None,
        }
    }
}

/// Opens a window for `target`, with what [`LaunchTarget::start_reading`]
/// read when given.
pub fn open_target(
    target: LaunchTarget,
    reading: Option<PendingStart>,
    cx: &mut App,
) -> anyhow::Result<()> {
    match (target, reading) {
        (LaunchTarget::Vault { note, .. }, Some(reading)) => {
            open_started_vault_window(reading.wait(), note, cx).map(|_| ())
        }
        (LaunchTarget::Vault { vault, note }, None) => {
            open_vault_window(&vault, note, cx).map(|_| ())
        }
        (LaunchTarget::Welcome, _) => open_welcome_window(cx),
    }
}

/// Opens `vault` in a new window, where it was last time, with its tabs
/// back, plus `note` when given.
pub fn open_vault_window(
    vault: &Path,
    note: Option<PathBuf>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<Workspace>> {
    open_started_vault_window(VaultStart::load(vault), note, cx)
}

fn open_started_vault_window(
    start: VaultStart,
    note: Option<PathBuf>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<Workspace>> {
    if crate::sandbox::blocks("opening another window") {
        anyhow::bail!("a snapshot run keeps to one window");
    }
    let options = window_options(&start.config.device, cx);
    let window = {
        let _span = trace::span("open-window");
        cx.open_window(options, move |window, cx| {
            cx.new(|cx| build_started_workspace(start, note.as_deref(), window, cx))
        })?
    };
    window.update(cx, |workspace, window, cx| {
        remember_vault(workspace.vault().to_path_buf(), cx);
        workspace.focus_active(window, cx);
        let _span = trace::span("activate");
        cx.activate(true);
        trace::on_first_frame(window);
        crate::first_frame::release_when_presented(window, cx);
    })?;
    Ok(window)
}

/// Records `vault` as the last one opened, off the main thread: nothing
/// on screen depends on it.
fn remember_vault(vault: PathBuf, cx: &mut App) {
    cx.background_spawn(async move { AppState::remember_vault(&vault) })
        .detach();
}

/// Opens the empty state, which asks for a folder.
pub fn open_welcome_window(cx: &mut App) -> anyhow::Result<()> {
    if crate::sandbox::blocks("opening another window") {
        anyhow::bail!("a snapshot run keeps to one window");
    }
    let options = window_options(&DeviceSettings::default(), cx);
    let window = cx.open_window(options, |window, cx| cx.new(|cx| Welcome::new(window, cx)))?;
    window.update(cx, |_, window, cx| {
        window.set_window_title("Open a vault");
        crate::first_frame::release_when_presented(window, cx);
        cx.activate(true);
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
    build_started_workspace(VaultStart::load(vault), note, window, cx)
}

pub(crate) fn build_started_workspace(
    start: VaultStart,
    note: Option<&Path>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Workspace {
    trace::mark("window-created");
    let mut workspace = {
        let _span = trace::span("workspace-new");
        Workspace::from_start(start, window, cx)
    };
    {
        let _span = trace::span("features-install");
        crate::features::install(&mut workspace, window, cx);
    }
    {
        let _span = trace::span("restore-session");
        workspace.restore_session(window, cx);
    }
    if let Some(note) = note
        && let Err(error) = open_or_create(&mut workspace, note, window, cx)
    {
        crate::notices::open_failed(note, error, cx);
    }
    if crate::sandbox::writes_allowed() {
        workspace.watch_vault(window, cx);
    }
    if crate::sandbox::reaches_outside() {
        workspace.start_mcp_bridge(window, cx);
    }
    trace::mark("workspace-built");
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

fn window_options(device: &DeviceSettings, cx: &App) -> WindowOptions {
    let bounds = window_bounds(&device.window).unwrap_or_else(|| {
        WindowBounds::Windowed(Bounds::centered(None, window_size(&device.window), cx))
    });
    WindowOptions {
        window_bounds: Some(bounds),
        titlebar: Some(titlebar()),
        // The tab bar sits where the hidden title bar was, and a movable
        // window moves from there whatever is pressed, tabs included. Its
        // empty space moves the window itself (`window_drag`).
        is_movable: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}

/// On macOS the app draws under a hidden title bar, with the window
/// buttons centred in the tab bar's row. Elsewhere the system's title bar
/// stays.
pub(crate) fn titlebar() -> TitlebarOptions {
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
        self.save_edit_time_now();
        if let Err(error) = save_device(&self.vault, &self.device_state(cx)) {
            eprintln!("could not save the window state: {error}");
        }
    }

    /// The window and open tabs as they are now.
    /// Notes that this device has offered the vault its Obsidian
    /// settings, so it doesn't again, and saves that now.
    pub fn mark_obsidian_import_offered(&mut self, cx: &App) {
        self.config.device.obsidian_import_offered = true;
        self.save_device_now(cx);
    }

    /// Writes this device's state for the vault now, rather than when the
    /// window next moves or closes.
    pub(crate) fn save_device_now(&self, cx: &App) {
        if let Err(error) = save_device(&self.vault, &self.device_state(cx)) {
            eprintln!("could not save the window state: {error}");
        }
    }

    pub fn device_state(&self, cx: &App) -> DeviceSettings {
        let mut device = self.config.device.clone();
        device.device_id = self.edit_time_device_id();
        if let Some(bounds) = self.window_bounds {
            device.window = window_state(bounds);
        }
        device.right_sidebar = self.right_panel.state(self.theme.workspace.sidebar_width);
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
        device.panes = Some(self.pane_layout(cx));
        device.positions = self.positions_now(cx);
        device
    }

    pub(crate) fn relative_name(&self, path: &Path) -> String {
        let canonical;
        let relative = match path.strip_prefix(&self.vault) {
            Ok(relative) => relative,
            Err(_) => {
                canonical = super::files::canonical_path(path);
                canonical
                    .as_deref()
                    .and_then(|real| real.strip_prefix(&self.vault).ok())
                    .unwrap_or(path)
            }
        };
        relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Reopens the tabs saved in `.gasp/device.toml`.
    pub fn restore_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let device = self.config.device.clone();
        if let Some(layout) = &device.panes {
            self.restore_layout(layout, window, cx);
            return;
        }
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

    /// Follows changes to the vault on disk. Watching a folder means
    /// visiting every folder in it, so that happens off the main thread.
    pub fn watch_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::first_frame::is_waiting() {
            let (this, handle) = (cx.weak_entity(), window.window_handle());
            crate::first_frame::defer(move |cx| {
                handle
                    .update(cx, |_, window, cx| {
                        this.update(cx, |workspace, cx| workspace.watch_vault(window, cx))
                    })
                    .ok();
            });
            return;
        }
        let vault = self.vault.clone();
        let texts = self.note_texts.clone();
        let starting = cx.background_spawn(async move {
            let _span = trace::span("watch-vault");
            let watching = watch(&vault).map_err(|error| (vault, error));
            texts.set_followed(watching.is_ok());
            watching
        });
        let task = cx.spawn_in(window, async move |workspace, cx| {
            let mut receiver = match starting.await {
                Ok((watcher, receiver)) => {
                    let stored =
                        workspace.update(cx, |workspace, _| workspace.watcher = Some(watcher));
                    if stored.is_err() {
                        return;
                    }
                    receiver
                }
                Err((vault, error)) => {
                    eprintln!("could not watch {}: {error}", vault.display());
                    return;
                }
            };
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
        let chosen = crate::sandbox::prompt_for_paths(folder_prompt(), cx);
        let task = cx.spawn_in(window, async move |_, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(vault) = paths.into_iter().next() else {
                return;
            };
            cx.update(|_, cx| {
                if let Err(error) = open_vault_window(&vault, None, cx) {
                    crate::notices::open_failed(&vault, error, cx);
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
