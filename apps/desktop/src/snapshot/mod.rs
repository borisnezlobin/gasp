//! `gasp --snapshot`: draws the app to PNGs without showing a window,
//! taking focus or activating the app, for agents and tests to look at
//! what the app draws.
//!
//! It has two forms:
//!
//! - `--snapshot NOTE OUT.png` draws one note's editor pane, as its
//!   vault's theme and settings show it, with the caret where `--cursor`
//!   puts it. Nothing is written: the vault's config is only read.
//! - `--snapshot --vault VAULT --script SCRIPT --out DIR` opens the whole
//!   workspace window on a copy of the vault and follows a script of
//!   pointer, keyboard and command steps (see [`script`]), writing a PNG
//!   at each `snap`. The copy and the app's own folders live in a
//!   temporary folder that's removed at the end, and the app is kept in a
//!   [`crate::sandbox`]: no syncing, no MCP bridge, and no saving or
//!   watching unless `--allow-writes` asks for them.
//!
//! Frames are drawn until the view stops changing and no equation, code
//! block or image is still on its way, then the last one is read back at
//! twice the window's size in points, whatever screen is attached. Only
//! macOS reads frames back so far.

#[cfg(target_os = "macos")]
mod appkit;
#[cfg(target_os = "macos")]
mod frames;
pub mod input;
#[cfg(target_os = "macos")]
mod metal_capture;
#[cfg(target_os = "macos")]
mod native_input;
pub mod scratch;
pub mod script;
#[cfg(target_os = "macos")]
mod window;

use std::path::{Path, PathBuf};

use gasp_config::{Config, ConfigLoader};

use crate::workspace::files::vault_for_note;

/// Windows the app never shows, drawn anyway: the layout bench's
/// `--hidden` uses these to run while the screen is locked or asleep,
/// when the display never asks a visible window for frames.
#[cfg(target_os = "macos")]
pub mod hidden {
    use std::time::Duration;

    use gpui::{App, Window};

    use super::appkit;
    use super::metal_capture::LayerCapture;

    /// How often a hidden window is asked for a frame.
    const FRAME_INTERVAL: Duration = Duration::from_millis(1);

    /// Makes windows draw at twice their size in points whatever screen
    /// is attached. Call before the application is made.
    pub fn prepare() {
        appkit::draw_at_double_scale();
    }

    /// Keeps the app out of the Dock and from becoming active. Call once
    /// the application is made, before it runs.
    pub fn keep_app_in_background() {
        appkit::keep_app_in_background();
    }

    /// Asks `window`, opened with `show: false`, for a frame every
    /// millisecond for as long as the app runs. It draws only when
    /// something changed.
    pub fn keep_drawing(window: &Window, cx: &mut App) -> Result<(), String> {
        let view = crate::look_up::native_view(window).ok_or("the window has no native view")?;
        let capture = LayerCapture::attach(view)?;
        cx.spawn(async move |cx| {
            loop {
                capture.request_frame();
                cx.background_executor().timer(FRAME_INTERVAL).await;
            }
        })
        .detach();
        Ok(())
    }
}

/// The window size when `--width` and `--height` are left out.
pub const DEFAULT_SIZE: (u32, u32) = (900, 700);
/// The whole window's size when `--window` is left out.
pub const DEFAULT_WINDOW_SIZE: (u32, u32) = (1200, 800);

/// What `--snapshot NOTE OUT.png` draws, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRequest {
    pub note: PathBuf,
    pub out: PathBuf,
    pub width: u32,
    pub height: u32,
    pub dark: bool,
    /// Where the caret goes: a line and a column, both counted from one.
    pub cursor: Option<(usize, usize)>,
}

impl SnapshotRequest {
    pub fn new(note: PathBuf, out: PathBuf) -> Self {
        Self {
            note,
            out,
            width: DEFAULT_SIZE.0,
            height: DEFAULT_SIZE.1,
            dark: false,
            cursor: None,
        }
    }
}

/// What `--snapshot --vault VAULT --script SCRIPT --out DIR` runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowSnapshotRequest {
    pub vault: PathBuf,
    /// The script's file, or `-` for standard input.
    pub script: PathBuf,
    /// The folder `snap` writes into.
    pub out: PathBuf,
    /// A note to open as the window opens, relative to the vault.
    pub open: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub dark: bool,
    /// Whether notes are saved and the vault watched (in the copy).
    pub allow_writes: bool,
    /// Whether the copy of the vault stays when the run ends.
    pub keep_temp: bool,
    /// Whether the window opens on the welcome tour instead of the vault.
    pub tour: bool,
}

impl WindowSnapshotRequest {
    pub fn new(vault: PathBuf, script: PathBuf, out: PathBuf) -> Self {
        Self {
            vault,
            script,
            out,
            open: None,
            width: DEFAULT_WINDOW_SIZE.0,
            height: DEFAULT_WINDOW_SIZE.1,
            dark: false,
            allow_writes: false,
            keep_temp: false,
            tour: false,
        }
    }
}

/// A note read from disk with what its vault says about drawing it.
/// Only macOS draws it so far.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct SnapshotNote {
    text: String,
    image_dirs: Vec<PathBuf>,
    config: Config,
}

/// The note the request names: the path as given, or with `.md` added.
fn note_path(requested: &Path) -> Result<PathBuf, String> {
    let with_extension = requested.with_extension("md");
    let path = [requested, with_extension.as_path()]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("{} isn't a note", requested.display()))?;
    std::path::absolute(path).map_err(|error| error.to_string())
}

/// Reads the note and its vault's config, without migrating or writing
/// anything.
fn read_note(requested: &Path) -> Result<SnapshotNote, String> {
    let path = note_path(requested)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let vault = vault_for_note(&path);
    let mut loader = ConfigLoader::new(vault.join(gasp_config::CONFIG_DIR));
    for diagnostic in loader.load_all() {
        eprintln!("{diagnostic}");
    }
    let config = loader.config().clone();
    let folder = path.parent().unwrap_or(&vault).to_path_buf();
    let image_dirs = vec![
        folder.clone(),
        folder.join(&config.settings.files.attachments_folder),
        vault,
    ];
    Ok(SnapshotNote {
        text,
        image_dirs,
        config,
    })
}

/// The byte offset of a one-based line and column in `text`, clamped to
/// the line's end and the text's.
pub fn offset_of(text: &str, line: usize, column: usize) -> usize {
    let line_start = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let rest = &text[line_start..];
    let line_text = rest.split('\n').next().unwrap_or_default();
    let within = line_text
        .char_indices()
        .nth(column.saturating_sub(1))
        .map_or(line_text.len(), |(at, _)| at);
    line_start + within
}

/// Draws the note the request names and writes the PNG. Runs the app
/// until then and exits from inside it, so it only returns an error.
pub fn run(request: SnapshotRequest) -> Result<(), String> {
    let note = read_note(&request.note)?;
    platform::run(request, note)
}

/// Follows the request's script in a whole window. Runs the app until the
/// script ends and exits from inside it, so it only returns an error.
pub fn run_window(request: WindowSnapshotRequest) -> Result<(), String> {
    let source = read_script(&request.script)?;
    let script = script::parse(&source)?;
    platform::run_window(request, script)
}

fn read_script(path: &Path) -> Result<String, String> {
    let read = match path.as_os_str() == "-" {
        true => std::io::read_to_string(std::io::stdin()),
        false => std::fs::read_to_string(path),
    };
    read.map_err(|error| format!("could not read the script {}: {error}", path.display()))
}

#[cfg(target_os = "macos")]
mod platform {
    use gpui::{
        AppContext, Application, AsyncApp, Bounds, WindowBounds, WindowHandle, WindowOptions,
        point, px, size,
    };

    use super::appkit;
    use super::frames::{save_last_frame, settle_or_warn};
    use super::metal_capture::LayerCapture;
    use super::script::ScriptLine;
    use super::{SnapshotNote, SnapshotRequest, WindowSnapshotRequest, offset_of};
    use crate::editor::EditorView;
    use crate::icons::Assets;

    pub(super) fn run_window(
        request: WindowSnapshotRequest,
        script: Vec<ScriptLine>,
    ) -> Result<(), String> {
        super::window::run(request, script)
    }

    pub(super) fn run(request: SnapshotRequest, note: SnapshotNote) -> Result<(), String> {
        appkit::draw_at_double_scale();
        let application = Application::new().with_assets(Assets);
        appkit::keep_app_in_background();
        application.run(move |cx| {
            if let Err(error) = start(request, note, cx) {
                fail(&error);
            }
        });
        Ok(())
    }

    fn fail(error: &str) -> ! {
        eprintln!("{} --snapshot: {error}", gasp_config::COMMAND_NAME);
        std::process::exit(1);
    }

    /// Opens the hidden window on the note and starts drawing it.
    fn start(
        request: SnapshotRequest,
        note: SnapshotNote,
        cx: &mut gpui::App,
    ) -> Result<(), String> {
        let names = cx.text_system().all_font_names();
        crate::ui::set_installed_fonts(names, cx);
        crate::ui::set_theme(&note.config.theme, request.dark, cx);
        let bounds = Bounds::new(
            point(px(0.), px(0.)),
            size(px(request.width as f32), px(request.height as f32)),
        );
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            show: false,
            ..Default::default()
        };
        let window = cx
            .open_window(options, |_, cx| {
                cx.new(|cx| EditorView::with_config(&note.text, note.image_dirs, &note.config, cx))
            })
            .map_err(|error| error.to_string())?;
        let view = window
            .update(cx, |editor, window, cx| {
                window.focus(&editor.focus_handle);
                place_caret(editor, request.cursor, cx);
                crate::look_up::native_view(window)
            })
            .map_err(|error| error.to_string())?
            .ok_or("the window has no native view")?;
        let capture = LayerCapture::attach(view)?;
        cx.spawn(async move |cx| {
            settle_or_warn(&capture, cx).await;
            match save_last_frame(&capture, &request.out) {
                Ok(()) => quit(window, cx),
                Err(error) => fail(&error),
            }
        })
        .detach();
        Ok(())
    }

    /// Puts the caret at a line and column, with a few lines above
    /// it in view, or after the frontmatter as a note opens.
    fn place_caret(
        editor: &mut EditorView,
        cursor: Option<(usize, usize)>,
        cx: &mut gpui::Context<EditorView>,
    ) {
        let Some((line, column)) = cursor else {
            editor.place_cursor_after_frontmatter(cx);
            return;
        };
        let text = editor.text();
        let at = offset_of(&text, line, column);
        let top = offset_of(&text, line.saturating_sub(4).max(1), 1);
        editor.restore_position(at, top, cx);
    }

    fn quit(window: WindowHandle<EditorView>, cx: &mut AsyncApp) {
        let _ = window.update(cx, |_, window, _| window.remove_window());
        let _ = cx.update(|cx| cx.quit());
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::script::ScriptLine;
    use super::{SnapshotNote, SnapshotRequest, WindowSnapshotRequest};

    const MACOS_ONLY: &str = "snapshots only work on macOS so far";

    pub(super) fn run(_: SnapshotRequest, _: SnapshotNote) -> Result<(), String> {
        Err(MACOS_ONLY.to_owned())
    }

    pub(super) fn run_window(_: WindowSnapshotRequest, _: Vec<ScriptLine>) -> Result<(), String> {
        Err(MACOS_ONLY.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_columns_count_from_one() {
        let text = "ab\ncdé\n\nlast";
        assert_eq!(offset_of(text, 1, 1), 0);
        assert_eq!(offset_of(text, 2, 3), 5);
        assert_eq!(offset_of(text, 2, 9), 7);
        assert_eq!(offset_of(text, 3, 1), 8);
        assert_eq!(offset_of(text, 4, 2), 10);
        assert_eq!(offset_of(text, 99, 1), text.len());
    }
}
