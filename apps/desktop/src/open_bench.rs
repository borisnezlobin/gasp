//! The note-opening benchmark: opens a vault's longest notes one after
//! another in the whole workspace window, then switches between tabs, and
//! reports how long each took to reach the screen and to finish drawing
//! its equations and code blocks.
//!
//! It runs on a copy of the vault in a temporary folder, with the app in a
//! [`crate::sandbox`], so nothing is saved, synced or watched.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::channel::oneshot;
use gpui::{
    App, AppContext, Application, AsyncApp, Bounds, WindowBounds, WindowHandle, WindowOptions, px,
    size,
};

use crate::icons::Assets;
use crate::sandbox::Sandbox;
use crate::snapshot::scratch::ScratchFolder;
use crate::stats::Samples;
use crate::workspace::files::notes_by_recency;
use crate::workspace::startup::VaultStart;
use crate::workspace::window::build_started_workspace;
use crate::workspace::{OpenIn, Workspace};

const WINDOW_SIZE: (f32, f32) = (1200., 800.);
/// How often the drawing of equations and code is checked on.
const SETTLE_POLL: Duration = Duration::from_millis(1);
/// The longest one note waits for its equations and code.
const SETTLE_LIMIT: Duration = Duration::from_secs(20);
/// Tabs opened for the switching phase.
const SWITCH_TABS: usize = 8;
/// Switches made between those tabs.
const SWITCHES: usize = 48;
/// Frames drawn after lingering, for [`crate::atlas`] to look a few times.
const SWEEP_FRAMES: usize = 100;

/// What to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenBenchConfig {
    /// How many of the vault's longest notes to open.
    pub notes: usize,
    /// Draw into a window that's never shown (macOS).
    pub hidden: bool,
    /// Keep the window open this long after switching, then report the
    /// memory again, as after hidden tabs let go of their pictures.
    pub linger: Option<Duration>,
}

impl Default for OpenBenchConfig {
    fn default() -> Self {
        Self {
            notes: 30,
            hidden: false,
            linger: None,
        }
    }
}

/// Measurements from a finished run.
#[derive(Default)]
struct OpenResults {
    /// From asking to open a note to its first frame on screen.
    open: Samples,
    /// From asking to open a note to its equations and code being drawn.
    settled: Samples,
    /// From switching to a tab to its frame on screen.
    switch: Samples,
}

impl OpenResults {
    fn report(&self) -> String {
        [
            ("open to first frame", &self.open),
            ("open to equations and code drawn", &self.settled),
            ("tab switch to frame", &self.switch),
        ]
        .iter()
        .filter_map(|(name, samples)| Some(format!("{name}: {}", samples.summary()?)))
        .collect::<Vec<_>>()
        .join("\n")
    }
}

/// Prints the app's memory, as `memory: 64.2 MB` plus `when`.
fn print_memory(when: &str) {
    if let Some(bytes) = crate::memory::footprint_bytes() {
        println!("memory{when}: {:.1} MB", bytes as f64 / 1e6);
    }
}

/// The `limit` longest notes in `vault`, longest first.
pub fn longest_notes(vault: &Path, limit: usize) -> Vec<PathBuf> {
    let mut notes: Vec<(u64, PathBuf)> = notes_by_recency(vault, usize::MAX)
        .into_iter()
        .map(|note| (std::fs::metadata(&note).map_or(0, |meta| meta.len()), note))
        .collect();
    notes.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    notes
        .into_iter()
        .take(limit)
        .map(|(_, note)| note)
        .collect()
}

/// Copies `vault`, opens it and runs the benchmark, printing the report.
/// Exits from inside the app.
pub fn run(vault: &Path, config: OpenBenchConfig) -> Result<(), String> {
    let scratch = ScratchFolder::copy_vault(vault)?;
    crate::sandbox::enter(Sandbox {
        data_root: scratch.data_root(),
        allow_writes: false,
    });
    let hidden = config.hidden && cfg!(target_os = "macos");
    #[cfg(target_os = "macos")]
    if hidden {
        crate::snapshot::hidden::prepare();
    }
    let application = Application::new().with_assets(Assets);
    #[cfg(target_os = "macos")]
    if hidden {
        crate::snapshot::hidden::keep_app_in_background();
    }
    application.run(move |cx| {
        let notes = longest_notes(scratch.vault(), config.notes);
        let outcome = open_window(&scratch, hidden, cx).map(|window| {
            cx.spawn(async move |cx| {
                let results = measure(window, notes, cx).await;
                println!("{}", results.report());
                print_memory("");
                if let Some(linger) = config.linger {
                    cx.background_executor().timer(linger).await;
                    // Enough frames for the atlas to let go of what
                    // nothing holds any more.
                    for _ in 0..SWEEP_FRAMES {
                        window.update(cx, |_, window, _| window.refresh()).ok();
                        next_frame(window, cx).await;
                    }
                    print_memory(&format!(" after {}s", linger.as_secs()));
                }
                scratch.remove();
                cx.update(|cx| cx.quit()).ok();
            })
            .detach();
        });
        if let Err(error) = outcome {
            eprintln!("{} --bench-open: {error}", gasp_config::COMMAND_NAME);
            std::process::exit(1);
        }
    });
    Ok(())
}

fn open_window(
    scratch: &ScratchFolder,
    hidden: bool,
    cx: &mut App,
) -> Result<WindowHandle<Workspace>, String> {
    crate::keymap::bind_keys(cx);
    crate::features::bind_view_keys(cx);
    let names = cx.text_system().all_font_names();
    crate::ui::set_installed_fonts(names, cx);
    let start = VaultStart::load(scratch.vault());
    let bounds = Bounds::centered(None, size(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(crate::workspace::window::titlebar()),
        show: !hidden,
        focus: !hidden,
        ..Default::default()
    };
    let window = cx
        .open_window(options, move |window, cx| {
            cx.new(|cx| build_started_workspace(start, None, window, cx))
        })
        .map_err(|error| error.to_string())?;
    window
        .update(cx, |workspace, window, cx| {
            workspace.focus_active(window, cx);
            keep_drawing(hidden, window, cx)
        })
        .map_err(|error| error.to_string())??;
    Ok(window)
}

#[cfg(target_os = "macos")]
fn keep_drawing(hidden: bool, window: &gpui::Window, cx: &mut App) -> Result<(), String> {
    if hidden {
        return crate::snapshot::hidden::keep_drawing(window, cx);
    }
    cx.activate(true);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn keep_drawing(_hidden: bool, _window: &gpui::Window, cx: &mut App) -> Result<(), String> {
    cx.activate(true);
    Ok(())
}

async fn measure(
    window: WindowHandle<Workspace>,
    notes: Vec<PathBuf>,
    cx: &mut AsyncApp,
) -> OpenResults {
    let mut results = OpenResults::default();
    next_frame(window, cx).await;
    for note in &notes {
        let started = Instant::now();
        open(window, note, OpenIn::ActiveTab, cx);
        next_frame(window, cx).await;
        results.open.push(started.elapsed());
        settle(window, cx).await;
        results.settled.push(started.elapsed());
    }
    for note in notes.iter().take(SWITCH_TABS) {
        open(window, note, OpenIn::NewTab, cx);
        settle(window, cx).await;
    }
    let tabs = notes.len().min(SWITCH_TABS) + 1;
    for switch in 0..SWITCHES {
        let started = Instant::now();
        window
            .update(cx, |workspace, window, cx| {
                workspace.activate_tab(switch % tabs, window, cx)
            })
            .ok();
        next_frame(window, cx).await;
        results.switch.push(started.elapsed());
    }
    results
}

fn open(window: WindowHandle<Workspace>, note: &Path, open_in: OpenIn, cx: &mut AsyncApp) {
    let opened = window.update(cx, |workspace, window, cx| {
        workspace.open_path(note, open_in, window, cx)
    });
    if let Ok(Err(error)) = opened {
        eprintln!("could not open {}: {error}", note.display());
    }
}

/// Waits until the window's next frame has been drawn and presented.
async fn next_frame(window: WindowHandle<Workspace>, cx: &mut AsyncApp) {
    let (drawn, frame) = oneshot::channel();
    window
        .update(cx, |_, window, _| {
            window.on_next_frame(move |_, cx| {
                // Tasks run once the frame callback returns, which is after
                // the frame is drawn and presented.
                cx.spawn(async move |_| drawn.send(()).ok()).detach();
            });
        })
        .ok();
    frame.await.ok();
}

/// Waits until no equation or code block is still being drawn, then for
/// the frame that shows them.
async fn settle(window: WindowHandle<Workspace>, cx: &mut AsyncApp) {
    let started = Instant::now();
    next_frame(window, cx).await;
    while crate::pending_renders::pending() > 0 && started.elapsed() < SETTLE_LIMIT {
        cx.background_executor().timer(SETTLE_POLL).await;
    }
    next_frame(window, cx).await;
}
