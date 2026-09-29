//! `gasp --snapshot`: draws a note's editor pane to a PNG without showing
//! a window, taking focus or activating the app, for agents and tests to
//! look at what the app draws.
//!
//! The note opens as it would in its vault: the vault's theme and
//! settings, its images, and the caret where `--cursor` puts it (the
//! editor has focus, so the caret and the table editor's marks show).
//! Nothing in the vault is written: its config is only read, and none of the
//! workspace's saving, syncing, watching or MCP bridge starts.
//!
//! Frames are drawn until the view stops changing (math and images
//! arrive a moment after the text), then the last one is read back at
//! the window's backing scale: a 900 by 700 window on a Retina display
//! gives an 1800 by 1400 image. Only macOS reads frames back so far.

#[cfg(target_os = "macos")]
mod metal_capture;

use std::path::{Path, PathBuf};
use std::time::Duration;

use gasp_config::{Config, ConfigLoader};

use crate::workspace::files::vault_for_note;

/// The window size when `--width` and `--height` are left out.
pub const DEFAULT_SIZE: (u32, u32) = (900, 700);

/// What `--snapshot` draws, and where.
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

/// How often a frame is asked for while the view settles.
const FRAME_INTERVAL: Duration = Duration::from_millis(30);
/// How long the view has to go without drawing to count as settled.
const SETTLE_TIME: Duration = Duration::from_millis(400);
/// The longest a snapshot waits for the view to settle before it takes
/// whatever was drawn last.
const SETTLE_LIMIT: Duration = Duration::from_secs(30);

/// A note read from disk with what its vault says about drawing it.
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

#[cfg(target_os = "macos")]
mod platform {
    use std::time::Instant;

    use gpui::{
        AppContext, Application, AsyncApp, Bounds, WindowBounds, WindowHandle, WindowOptions,
        point, px, size,
    };

    use super::metal_capture::{LayerCapture, keep_app_in_background};
    use super::{
        FRAME_INTERVAL, SETTLE_LIMIT, SETTLE_TIME, SnapshotNote, SnapshotRequest, offset_of,
    };
    use crate::editor::EditorView;
    use crate::icons::Assets;

    pub(super) fn run(request: SnapshotRequest, note: SnapshotNote) -> Result<(), String> {
        let application = Application::new().with_assets(Assets);
        keep_app_in_background();
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
            let outcome = settle_and_save(&capture, &request, cx).await;
            match outcome {
                Ok(()) => quit(window, cx),
                Err(error) => fail(&error),
            }
        })
        .detach();
        Ok(())
    }

    /// Puts the caret at a line and column, with a couple of lines above
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
        let top = offset_of(&text, line.saturating_sub(2).max(1), 1);
        editor.restore_position(at, top, cx);
    }

    /// Draws frames until the view stops changing, then writes the last.
    async fn settle_and_save(
        capture: &LayerCapture,
        request: &SnapshotRequest,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if !settle(capture, cx).await {
            eprintln!(
                "{} --snapshot: the view was still changing after {}s, so this is its last frame",
                gasp_config::COMMAND_NAME,
                SETTLE_LIMIT.as_secs()
            );
        }
        let image = capture.last_frame()?;
        image
            .save(&request.out)
            .map_err(|error| format!("could not write {}: {error}", request.out.display()))
    }

    /// Asks for frames until none has been needed for [`SETTLE_TIME`].
    /// Answers false when the view was still changing at [`SETTLE_LIMIT`].
    async fn settle(capture: &LayerCapture, cx: &mut AsyncApp) -> bool {
        let started = Instant::now();
        let mut last_change = Instant::now();
        let mut drawn = 0;
        while started.elapsed() < SETTLE_LIMIT {
            capture.request_frame();
            if capture.frames_drawn() != drawn {
                drawn = capture.frames_drawn();
                last_change = Instant::now();
            } else if drawn > 0 && last_change.elapsed() >= SETTLE_TIME {
                return true;
            }
            cx.background_executor().timer(FRAME_INTERVAL).await;
        }
        false
    }

    fn quit(window: WindowHandle<EditorView>, cx: &mut AsyncApp) {
        let _ = window.update(cx, |_, window, _| window.remove_window());
        let _ = cx.update(|cx| cx.quit());
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{SnapshotNote, SnapshotRequest};

    pub(super) fn run(_: SnapshotRequest, _: SnapshotNote) -> Result<(), String> {
        Err("snapshots only work on macOS so far".to_owned())
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
