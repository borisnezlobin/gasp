//! A whole workspace window driven by a script, drawn to PNGs.
//!
//! The window is the one `gasp` opens on a vault: sidebars, tabs,
//! toolbars, the status bar, popovers, menus and the settings screen. It
//! opens on a copy of the vault in a temporary folder, with the app's
//! own folders there too, and is never shown. The script's input goes
//! through GPUI's own event path, as the system's would, and each `snap`
//! waits for the view to settle first.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use gasp_config::config_files::known_commands;
use gpui::{
    AnyWindowHandle, App, AppContext, Application, AsyncApp, Bounds, Entity, Modifiers,
    MouseButton, Pixels, PlatformInput, Point, WindowBounds, WindowOptions, point, px, size,
};
use serde_json::json;

use super::WindowSnapshotRequest;
use super::appkit;
use super::frames::{FRAME_INTERVAL, keep_drawing, save_last_frame, settle_or_warn};
use super::input;
use super::metal_capture::LayerCapture;
use super::native_input::NativePointer;
use super::scratch::ScratchFolder;
use super::script::{KeyInput, PointerAction, ScriptLine, Step, Target};
use crate::icons::Assets;
use crate::keymap::RunCommand;
use crate::sandbox::Sandbox;
use crate::ui::selector::{SelectorBounds, record_selectors, selectors_in_last_frame};
use crate::workspace::startup::VaultStart;
use crate::workspace::window::{build_started_workspace, titlebar};
use crate::workspace::{OpenIn, Workspace};

/// Copies the vault, opens the hidden window on the copy and follows the
/// script. Runs the app until the script ends and exits from inside it,
/// so it only returns an error.
pub(super) fn run(request: WindowSnapshotRequest, script: Vec<ScriptLine>) -> Result<(), String> {
    std::fs::create_dir_all(&request.out)
        .map_err(|error| format!("could not make {}: {error}", request.out.display()))?;
    let scratch = ScratchFolder::copy_vault(&request.vault)?;
    let note = match request.open.as_deref().map(|note| scratch.in_copy(note)) {
        Some(Err(error)) => {
            scratch.remove();
            return Err(error);
        }
        Some(Ok(note)) => Some(note),
        None => None,
    };
    crate::sandbox::enter(Sandbox {
        data_root: scratch.data_root(),
        allow_writes: request.allow_writes,
    });
    appkit::use_private_clipboard();
    appkit::draw_at_double_scale();
    let application = Application::new().with_assets(Assets);
    appkit::keep_app_in_background();
    application.run(move |cx| {
        let started = start(&request, note.as_deref(), &scratch, cx);
        match started {
            Ok((window, capture, native_pointer)) => {
                let runner = ScriptRunner {
                    window,
                    capture,
                    scratch,
                    out: request.out.clone(),
                    keep_temp: request.keep_temp,
                    native_pointer,
                    pointer_at: None,
                };
                cx.spawn(async move |cx| runner.follow(script, cx).await)
                    .detach();
            }
            Err(error) => finish(&scratch, request.keep_temp, Err(error)),
        }
    });
    Ok(())
}

/// Sets the app up as `gasp` does and opens the window, unshown.
fn start(
    request: &WindowSnapshotRequest,
    note: Option<&Path>,
    scratch: &ScratchFolder,
    cx: &mut App,
) -> Result<(AnyWindowHandle, LayerCapture, NativePointer), String> {
    crate::keymap::bind_keys(cx);
    crate::features::bind_view_keys(cx);
    crate::workspace::prompt::use_in_window_prompts(cx);
    let names = cx.text_system().all_font_names();
    crate::ui::set_installed_fonts(names, cx);
    appkit::set_appearance(request.dark);
    record_selectors();
    let start = VaultStart::load(scratch.vault());
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.), px(0.)),
            size(px(request.width as f32), px(request.height as f32)),
        ))),
        titlebar: Some(titlebar()),
        focus: false,
        show: false,
        is_movable: false,
        ..Default::default()
    };
    let note = note.map(Path::to_path_buf);
    let window = if request.tour {
        cx.open_window(options, |window, cx| {
            cx.new(|cx| crate::tour::Tour::whole(Vec::new(), window, cx))
        })
        .map(Into::into)
    } else {
        cx.open_window(options, move |window, cx| {
            cx.new(|cx| build_started_workspace(start, note.as_deref(), window, cx))
        })
        .map(Into::into)
    }
    .map_err(|error: anyhow::Error| error.to_string())?;
    let view = cx
        .update_window(window, |root, window, cx| {
            if let Ok(workspace) = root.downcast::<Workspace>() {
                workspace.update(cx, |workspace, cx| workspace.focus_active(window, cx));
            }
            crate::look_up::native_view(window)
        })
        .map_err(|error| error.to_string())?
        .ok_or("the window has no native view")?;
    Ok((
        window,
        LayerCapture::attach(view)?,
        NativePointer::new(view),
    ))
}

/// Saves what the app would save as it quits, when writes are allowed,
/// removes the temporary folder and exits.
fn finish(scratch: &ScratchFolder, keep_temp: bool, outcome: Result<(), String>) -> ! {
    appkit::release_private_clipboard();
    if keep_temp {
        eprintln!(
            "{} --snapshot: kept the vault's copy at {}",
            gasp_config::COMMAND_NAME,
            scratch.root().display()
        );
    } else {
        scratch.remove();
    }
    let _ = std::io::stdout().flush();
    match outcome {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("{} --snapshot: {error}", gasp_config::COMMAND_NAME);
            std::process::exit(1)
        }
    }
}

struct ScriptRunner {
    window: AnyWindowHandle,
    capture: LayerCapture,
    scratch: ScratchFolder,
    out: PathBuf,
    keep_temp: bool,
    native_pointer: NativePointer,
    /// Where the pointer is, once a step has moved it.
    pointer_at: Option<Point<Pixels>>,
}

impl ScriptRunner {
    async fn follow(mut self, script: Vec<ScriptLine>, cx: &mut AsyncApp) {
        settle_or_warn(&self.capture, cx).await;
        for line in &script {
            if let Err(error) = self.step(line, cx).await {
                self.close(Err(format!("line {}: {error}", line.line)), cx);
            }
        }
        self.close(Ok(()), cx);
    }

    fn close(&self, outcome: Result<(), String>, cx: &mut AsyncApp) -> ! {
        if crate::sandbox::writes_allowed() {
            let _ = self.update(cx, |workspace, _, cx| workspace.prepare_to_close(cx));
        }
        finish(&self.scratch, self.keep_temp, outcome)
    }

    async fn step(&mut self, line: &ScriptLine, cx: &mut AsyncApp) -> Result<(), String> {
        match &line.step {
            Step::Wait(duration) => keep_drawing(&self.capture, *duration, cx).await,
            Step::Settle => settle_or_warn(&self.capture, cx).await,
            Step::Snap(name) => return self.snap(line.line, name, cx).await,
            Step::SnapNow(name) => return self.snap_now(line.line, name, cx).await,
            Step::Bounds(selector) => return self.print_bounds(line.line, selector, cx).await,
            Step::Selectors(prefix) => self.print_selectors(line.line, prefix.as_deref(), cx).await,
            Step::Pointer {
                action,
                target,
                modifiers,
            } => return self.point(*action, target, *modifiers, cx).await,
            step => return self.input(step, cx).await,
        }
        Ok(())
    }

    /// Steps that change the window: its size, theme, notes, commands,
    /// drags, scrolls and keys.
    async fn input(&mut self, step: &Step, cx: &mut AsyncApp) -> Result<(), String> {
        let changed = match step {
            Step::Size { width, height } => {
                let size = size(px(*width as f32), px(*height as f32));
                self.with_window(cx, |window, _| window.resize(size))
            }
            Step::Theme { dark } => {
                appkit::set_appearance(*dark);
                Ok(())
            }
            Step::Open(note) => self.open(note, cx),
            Step::Command(id) => self.run_command(id, cx),
            Step::Drag {
                from,
                to,
                modifiers,
            } => {
                self.drag(
                    point(px(from.0), px(from.1)),
                    point(px(to.0), px(to.1)),
                    *modifiers,
                    cx,
                )
                .await
            }
            Step::Scroll { at, dy } => self.scroll(point(px(at.0), px(at.1)), *dy, cx).await,
            Step::Keys(keys) => self.press_keys(keys, cx).await,
            _ => Ok(()),
        };
        changed?;
        self.next_frame(cx).await;
        Ok(())
    }

    fn update<R>(
        &self,
        cx: &mut AsyncApp,
        change: impl FnOnce(&mut Workspace, &mut gpui::Window, &mut gpui::Context<Workspace>) -> R,
    ) -> Result<R, String> {
        let workspace = self.workspace(cx)?;
        cx.update_window(self.window, |_, window, cx| {
            workspace.update(cx, |workspace, cx| change(workspace, window, cx))
        })
        .map_err(|error| format!("the window closed: {error}"))
    }

    /// The window's workspace, once it shows a vault rather than the tour.
    fn workspace(&self, cx: &mut AsyncApp) -> Result<Entity<Workspace>, String> {
        cx.update_window(self.window, |root, _, _| root.downcast::<Workspace>().ok())
            .map_err(|error| format!("the window closed: {error}"))?
            .ok_or_else(|| "the window shows the welcome tour, not a vault".to_owned())
    }

    /// Runs `change` on the window without holding the workspace, as the
    /// system's events do, so handlers are free to update it.
    fn with_window<R>(
        &self,
        cx: &mut AsyncApp,
        change: impl FnOnce(&mut gpui::Window, &mut App) -> R,
    ) -> Result<R, String> {
        cx.update_window(self.window, |_, window, cx| change(window, cx))
            .map_err(|error| format!("the window closed: {error}"))
    }

    fn open(&self, note: &str, cx: &mut AsyncApp) -> Result<(), String> {
        let path = self.scratch.in_copy(Path::new(note))?;
        self.update(cx, |workspace, window, cx| {
            let opened = workspace.open_path(&path, OpenIn::ActiveTab, window, cx);
            workspace.focus_active(window, cx);
            opened.map_err(|error| format!("could not open {note}: {error}"))
        })?
    }

    /// Runs a command as its shortcut would: the workspace's own, or
    /// dispatched from where the keyboard is.
    fn run_command(&self, id: &str, cx: &mut AsyncApp) -> Result<(), String> {
        if self.workspace(cx).is_err() {
            let action = RunCommand {
                id: id.to_owned().into(),
            };
            return self.with_window(cx, |window, cx| {
                window.dispatch_action(Box::new(action), cx)
            });
        }
        self.update(cx, |workspace, window, cx| {
            if workspace.run_command(id, window, cx) {
                return Ok(());
            }
            if !known_commands().contains(&id) {
                return Err(format!("there's no command {id}"));
            }
            if window.focused(cx).is_none() {
                workspace.focus_active(window, cx);
            }
            window.dispatch_action(
                Box::new(RunCommand {
                    id: id.to_owned().into(),
                }),
                cx,
            );
            Ok(())
        })?
    }

    /// Moves the pointer to the target, then clicks there if asked.
    async fn point(
        &mut self,
        action: PointerAction,
        target: &Target,
        modifiers: Modifiers,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let at = self.locate(target, cx).await?;
        self.move_pointer(at, cx).await?;
        for event in input::clicks(action, at, modifiers) {
            self.send(event, cx).await?;
        }
        Ok(())
    }

    async fn locate(&self, target: &Target, cx: &mut AsyncApp) -> Result<Point<Pixels>, String> {
        match target {
            Target::At(x, y) => Ok(point(px(*x), px(*y))),
            Target::Element(selector) => {
                settle_or_warn(&self.capture, cx).await;
                Ok(find(selector, &selectors_in_last_frame())?.center())
            }
        }
    }

    async fn move_pointer(&mut self, at: Point<Pixels>, cx: &mut AsyncApp) -> Result<(), String> {
        let path = match self.pointer_at {
            Some(from) if from == at => Vec::new(),
            Some(from) => input::pointer_path(from, at, None, Modifiers::none()),
            None => vec![input::pointer_move(at, None, Modifiers::none())],
        };
        for event in path {
            self.send(event, cx).await?;
        }
        Ok(())
    }

    async fn scroll(
        &mut self,
        at: Point<Pixels>,
        dy: f32,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        self.move_pointer(at, cx).await?;
        self.send(input::scroll(at, dy), cx).await
    }

    async fn drag(
        &mut self,
        from: Point<Pixels>,
        to: Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        self.move_pointer(from, cx).await?;
        self.send(input::press(MouseButton::Left, from, modifiers, 1), cx)
            .await?;
        for event in input::pointer_path(from, to, Some(MouseButton::Left), modifiers) {
            self.send(event, cx).await?;
        }
        self.send(input::release(MouseButton::Left, to, modifiers, 1), cx)
            .await
    }

    /// Presses each key as GPUI's platform code does for a key from the
    /// system: the keymap first, then the focused text input for a
    /// character nothing bound.
    async fn press_keys(&mut self, keys: &[KeyInput], cx: &mut AsyncApp) -> Result<(), String> {
        for stroke in input::keystrokes(keys)? {
            self.with_window(cx, |window, cx| window.dispatch_keystroke(stroke, cx))?;
            self.next_frame(cx).await;
        }
        Ok(())
    }

    /// Sends one pointer event to the window as AppKit would, then draws
    /// the frame it calls for.
    async fn send(&mut self, event: PlatformInput, cx: &mut AsyncApp) -> Result<(), String> {
        if let Some(position) = event_position(&event) {
            self.pointer_at = Some(position);
        }
        let height = self.with_window(cx, |window, _| window.viewport_size().height)?;
        self.native_pointer.send(&event, height)?;
        self.next_frame(cx).await;
        Ok(())
    }

    async fn next_frame(&self, cx: &mut AsyncApp) {
        self.capture.request_frame();
        cx.background_executor().timer(FRAME_INTERVAL).await;
        self.capture.request_frame();
    }

    async fn snap(&self, line: usize, name: &str, cx: &mut AsyncApp) -> Result<(), String> {
        settle_or_warn(&self.capture, cx).await;
        self.save_snap(line, name)
    }

    async fn snap_now(&self, line: usize, name: &str, cx: &mut AsyncApp) -> Result<(), String> {
        self.next_frame(cx).await;
        self.save_snap(line, name)
    }

    fn save_snap(&self, line: usize, name: &str) -> Result<(), String> {
        let path = self.out.join(name);
        save_last_frame(&self.capture, &path)?;
        print_line(json!({ "line": line, "snap": path }));
        Ok(())
    }

    async fn print_bounds(
        &self,
        line: usize,
        selector: &str,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        settle_or_warn(&self.capture, cx).await;
        let found = find(selector, &selectors_in_last_frame())?;
        let mut answer = bounds_json(found);
        answer["line"] = json!(line);
        answer["selector"] = json!(selector);
        print_line(answer);
        Ok(())
    }

    async fn print_selectors(&self, line: usize, prefix: Option<&str>, cx: &mut AsyncApp) {
        settle_or_warn(&self.capture, cx).await;
        let selectors: serde_json::Map<String, serde_json::Value> = selectors_in_last_frame()
            .into_iter()
            .filter(|(name, _)| prefix.is_none_or(|prefix| name.starts_with(prefix)))
            .map(|(name, bounds)| (name, bounds_json(bounds)))
            .collect();
        print_line(json!({ "line": line, "selectors": selectors }));
    }
}

fn event_position(event: &PlatformInput) -> Option<Point<Pixels>> {
    match event {
        PlatformInput::MouseMove(event) => Some(event.position),
        PlatformInput::MouseDown(event) => Some(event.position),
        PlatformInput::MouseUp(event) => Some(event.position),
        PlatformInput::ScrollWheel(event) => Some(event.position),
        _ => None,
    }
}

/// The element named `selector` in the last frame, or an error that
/// says which names were there.
fn find(selector: &str, frame: &SelectorBounds) -> Result<Bounds<Pixels>, String> {
    if let Some(bounds) = frame.get(selector) {
        return Ok(*bounds);
    }
    let family = selector.split('-').next().unwrap_or(selector);
    let similar: Vec<&str> = frame
        .keys()
        .filter(|name| name.starts_with(family))
        .map(String::as_str)
        .take(20)
        .collect();
    Err(match similar.is_empty() {
        true => format!(
            "nothing on screen has the selector {selector}; `selectors` lists the {} that do",
            frame.len()
        ),
        false => format!(
            "nothing on screen has the selector {selector}; similar: {}",
            similar.join(", ")
        ),
    })
}

fn bounds_json(bounds: Bounds<Pixels>) -> serde_json::Value {
    json!({
        "x": f32::from(bounds.origin.x),
        "y": f32::from(bounds.origin.y),
        "width": f32::from(bounds.size.width),
        "height": f32::from(bounds.size.height),
    })
}

fn print_line(value: serde_json::Value) {
    println!("{value}");
}
