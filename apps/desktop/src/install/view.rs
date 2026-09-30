//! The install window: the page with the whale, a line saying where Gasp
//! is running from, and "Move to Applications". The whale breaches when
//! the window opens (any key or click skips it), dives as the copy
//! starts, and the page's first line is written again as the copy runs.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, App, AppContext, Bounds, Context, ElementId, FocusHandle,
    Focusable, KeyDownEvent, MouseButton, SharedString, Task, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowOptions, div, ease_in_out, point, prelude::*, px, size,
};

use super::location::{Placement, Source};
use super::paint;
use super::scene::Moment;
use super::{InstallError, Installer, Prepared, Progress};
use crate::theme::InstallTheme;
use crate::ui::{Button, ButtonKind, ui_theme};

/// How often the copy's progress is looked at while it runs.
const TICK: Duration = Duration::from_millis(16);
/// The first line takes at least this long to write, so a fast copy
/// still reads as one; it never runs ahead of the copy.
const SHORTEST_WRITE: f32 = 0.6;
/// The words and buttons fade in over this part of the breach.
const FOOTER_FADE: (f32, f32) = (0.5, 0.85);

/// Something the window asks of the app around it.
pub type WindowHook = Rc<dyn Fn(&mut Window, &mut App)>;

/// What the window does besides moving the app, so tests can stand in.
#[derive(Clone)]
pub struct InstallHooks {
    /// Opens Gasp from where it is, and closes this window.
    pub open_here: WindowHook,
    /// Remembers "Not now" for this copy.
    pub remember_not_now: Rc<dyn Fn(&Placement)>,
    /// Runs once the moved copy is open: the app quits.
    pub finished: WindowHook,
}

/// Where the window is up to.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Breaching,
    Ready,
    /// Finding where the copy goes.
    Checking,
    /// A Gasp open from `target` has to quit first.
    AskToQuit(PathBuf),
    Quitting(PathBuf),
    Moving(PathBuf),
    Opening(PathBuf),
    Failed(InstallError),
}

/// What a button in the footer does.
#[derive(Clone, Debug, PartialEq)]
enum Action {
    Move,
    NotNow,
    QuitAndReplace(PathBuf),
    Cancel,
    TryAgain,
    OpenHere,
}

pub struct InstallView {
    focus_handle: FocusHandle,
    placement: Placement,
    installer: Arc<dyn Installer>,
    hooks: InstallHooks,
    theme: InstallTheme,
    step: Step,
    /// Holds the page at one moment, for screenshots.
    frozen: Option<Moment>,
    /// Counts attempts, so an old attempt's work is ignored.
    attempt: u32,
    progress: Arc<Progress>,
    /// How much of the first line is written: follows `progress`, no
    /// faster than [`SHORTEST_WRITE`] allows.
    written: f32,
    dived: bool,
    copied: Option<Result<(), InstallError>>,
    tasks: Vec<Task<()>>,
}

impl Focusable for InstallView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl InstallView {
    pub fn new(
        placement: Placement,
        installer: Arc<dyn Installer>,
        hooks: InstallHooks,
        dark: bool,
        frozen: Option<Moment>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        let theme = InstallTheme::new(dark);
        let mut view = InstallView {
            focus_handle,
            placement,
            installer,
            hooks,
            step: if frozen.is_some() {
                Step::Ready
            } else {
                Step::Breaching
            },
            frozen,
            attempt: 0,
            progress: Arc::default(),
            written: 0.,
            dived: false,
            copied: None,
            tasks: Vec::new(),
            theme,
        };
        if view.frozen.is_none() {
            view.settle_after_breach(window, cx);
        }
        view
    }

    pub fn step(&self) -> &Step {
        &self.step
    }

    /// The moment the page shows, or None while it's animating.
    pub fn moment(&self) -> Option<Moment> {
        if let Some(frozen) = self.frozen {
            return Some(frozen);
        }
        match &self.step {
            Step::Breaching => None,
            Step::Moving(_) if !self.dived => None,
            Step::Moving(_) => Some(Moment::Writing(self.written)),
            Step::Opening(_) => Some(Moment::Writing(1.)),
            _ => Some(Moment::REST),
        }
    }

    fn settle_after_breach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let breach = Duration::from_secs_f32(self.theme.stage.breach_seconds);
        let executor = cx.background_executor().clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            executor.timer(breach).await;
            this.update(cx, |view, cx| view.skip(cx)).ok();
        });
        self.tasks.push(task);
    }

    /// Ends the breach where it would have ended.
    pub fn skip(&mut self, cx: &mut Context<Self>) {
        if self.step == Step::Breaching {
            self.step = Step::Ready;
            cx.notify();
        }
    }

    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::Move | Action::TryAgain => self.start_move(window, cx),
            Action::QuitAndReplace(target) => self.quit_and_replace(target, window, cx),
            Action::Cancel => {
                self.step = Step::Ready;
                cx.notify();
            }
            Action::NotNow => {
                (self.hooks.remember_not_now)(&self.placement);
                (self.hooks.open_here)(window, cx);
            }
            Action::OpenHere => (self.hooks.open_here)(window, cx),
        }
    }

    fn next_attempt(&mut self, step: Step, cx: &mut Context<Self>) -> u32 {
        self.attempt += 1;
        self.step = step;
        cx.notify();
        self.attempt
    }

    /// Finds where the copy goes, then moves it or asks to quit the Gasp
    /// already there.
    pub fn start_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let attempt = self.next_attempt(Step::Checking, cx);
        let (installer, placement) = (self.installer.clone(), self.placement.clone());
        let executor = cx.background_executor().clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let prepared = executor
                .spawn(async move { installer.prepare(&placement) })
                .await;
            this.update_in(cx, |view, window, cx| {
                if view.attempt == attempt {
                    view.prepared(prepared, window, cx);
                }
            })
            .ok();
        });
        self.tasks.push(task);
    }

    fn prepared(
        &mut self,
        prepared: Result<Prepared, InstallError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match prepared {
            Ok(Prepared {
                target,
                running: true,
            }) => {
                self.step = Step::AskToQuit(target);
                cx.notify();
            }
            Ok(Prepared { target, .. }) => self.begin_moving(target, window, cx),
            Err(error) => self.fail(error, cx),
        }
    }

    fn quit_and_replace(&mut self, target: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let attempt = self.next_attempt(Step::Quitting(target.clone()), cx);
        let installer = self.installer.clone();
        let executor = cx.background_executor().clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let quitting = target.clone();
            let quit = executor
                .spawn(async move { installer.quit_running(&quitting) })
                .await;
            this.update_in(cx, |view, window, cx| match quit {
                _ if view.attempt != attempt => {}
                Ok(()) => view.begin_moving(target, window, cx),
                Err(error) => view.fail(error, cx),
            })
            .ok();
        });
        self.tasks.push(task);
    }

    /// Starts the copy and the dive together; the first line is written
    /// as the copy goes once the whale is under.
    fn begin_moving(&mut self, target: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let attempt = self.next_attempt(Step::Moving(target.clone()), cx);
        self.progress = Arc::default();
        (self.written, self.dived, self.copied) = (0., false, None);
        let (installer, placement) = (self.installer.clone(), self.placement.clone());
        let progress = self.progress.clone();
        let executor = cx.background_executor().clone();
        let copying = cx.spawn_in(window, async move |this, cx| {
            let copied = executor
                .spawn(async move { installer.copy(&placement, &target, &progress) })
                .await;
            this.update(cx, |view, _| {
                if view.attempt == attempt {
                    view.copied = Some(copied);
                }
            })
            .ok();
        });
        let dive = Duration::from_secs_f32(self.theme.stage.dive_seconds);
        let executor = cx.background_executor().clone();
        let ticking = cx.spawn_in(window, async move |this, cx| {
            executor.timer(dive).await;
            loop {
                let going = this.update_in(cx, |view, window, cx| view.tick(attempt, window, cx));
                if !matches!(going, Ok(true)) {
                    break;
                }
                executor.timer(TICK).await;
            }
        });
        self.tasks.extend([copying, ticking]);
    }

    /// Writes more of the first line, and moves on once the copy is done
    /// and the line is written. False once there's nothing more to do.
    fn tick(&mut self, attempt: u32, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Step::Moving(target) = &self.step else {
            return false;
        };
        if self.attempt != attempt {
            return false;
        }
        let target = target.clone();
        self.dived = true;
        if matches!(self.copied, Some(Ok(()))) {
            self.progress.set(1.);
        }
        let most = self.written + TICK.as_secs_f32() / SHORTEST_WRITE;
        self.written = self.progress.get().min(most);
        cx.notify();
        match self.copied.clone() {
            Some(Err(error)) => {
                self.fail(error, cx);
                false
            }
            Some(Ok(())) if self.written >= 1. => {
                self.open_moved(target, window, cx);
                false
            }
            _ => true,
        }
    }

    fn open_moved(&mut self, target: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let attempt = self.next_attempt(Step::Opening(target.clone()), cx);
        let (installer, placement) = (self.installer.clone(), self.placement.clone());
        let executor = cx.background_executor().clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let opened = executor
                .spawn(async move { installer.open_and_tidy(&placement, &target) })
                .await;
            this.update_in(cx, |view, window, cx| match opened {
                _ if view.attempt != attempt => {}
                Ok(()) => (view.hooks.finished.clone())(window, cx),
                Err(error) => view.fail(error, cx),
            })
            .ok();
        });
        self.tasks.push(task);
    }

    fn fail(&mut self, error: InstallError, cx: &mut Context<Self>) {
        self.step = Step::Failed(error);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.step == Step::Breaching {
            self.skip(cx);
            cx.stop_propagation();
            return;
        }
        let (_, buttons) = self.footer();
        let chosen = match event.keystroke.key.as_str() {
            "enter" => buttons.last(),
            "escape" => buttons.first(),
            _ => None,
        };
        if let Some((action, ..)) = chosen.cloned() {
            cx.stop_propagation();
            self.run(action, window, cx);
        }
    }

    /// The words under the page and its buttons, the primary one last.
    fn footer(&self) -> (String, Vec<(Action, &'static str, ButtonKind)>) {
        match &self.step {
            Step::Breaching | Step::Ready => (
                running_from(&self.placement),
                vec![
                    (Action::NotNow, "Not now", ButtonKind::Quiet),
                    (Action::Move, "Move to Applications", ButtonKind::Primary),
                ],
            ),
            Step::Checking => ("Getting ready…".into(), Vec::new()),
            Step::AskToQuit(target) => (
                format!(
                    "Gasp is open from {}. It has to quit first.",
                    folder(target)
                ),
                vec![
                    (Action::Cancel, "Cancel", ButtonKind::Quiet),
                    (
                        Action::QuitAndReplace(target.clone()),
                        "Quit it and replace",
                        ButtonKind::Primary,
                    ),
                ],
            ),
            Step::Quitting(_) => ("Waiting for the other Gasp to quit…".into(), Vec::new()),
            Step::Moving(target) => (format!("Moving to {}…", folder(target)), Vec::new()),
            Step::Opening(target) => (format!("Opening Gasp from {}…", folder(target)), Vec::new()),
            Step::Failed(error) if error.can_retry() => (
                error.to_string(),
                vec![
                    (Action::OpenHere, "Open from here", ButtonKind::Quiet),
                    (Action::TryAgain, "Try again", ButtonKind::Primary),
                ],
            ),
            Step::Failed(error) => (
                error.to_string(),
                vec![(Action::OpenHere, "Open Gasp", ButtonKind::Primary)],
            ),
        }
    }

    fn render_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let font = ui_theme(cx).font_family;
        if let Some(moment) = self.moment() {
            return paint::stage(&self.theme, moment, font).into_any_element();
        }
        let (id, seconds, moment): (ElementId, f32, fn(f32) -> Moment) = match self.step {
            Step::Breaching => (
                "breach".into(),
                self.theme.stage.breach_seconds,
                Moment::Breach,
            ),
            _ => (
                ("dive", self.attempt as usize).into(),
                self.theme.stage.dive_seconds,
                Moment::Dive,
            ),
        };
        let theme = self.theme.clone();
        div()
            .flex_none()
            .with_animation(
                id,
                Animation::new(Duration::from_secs_f32(seconds)),
                move |page, t| page.child(paint::stage(&theme, moment(t), font.clone())),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let (message, buttons) = self.footer();
        let theme = &self.theme;
        let row = buttons.into_iter().map(|(action, label, kind)| {
            Button::new(SharedString::from(label), label)
                .kind(kind)
                .on_click(
                    cx.listener(move |view, _, window, cx| view.run(action.clone(), window, cx)),
                )
        });
        let footer = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(theme.footer_gap)
            .px(theme.footer_padding)
            .child(
                div()
                    .max_w(theme.message_width)
                    .text_center()
                    .text_size(theme.text_size)
                    .text_color(theme.text_muted)
                    .child(message),
            )
            .child(div().flex().flex_row().gap(theme.button_gap).children(row));
        if self.step != Step::Breaching {
            return footer.into_any_element();
        }
        let breach = Duration::from_secs_f32(theme.stage.breach_seconds);
        footer
            .with_animation(
                "footer",
                Animation::new(breach).with_easing(ease_in_out),
                |footer, t| {
                    let (from, to) = FOOTER_FADE;
                    footer.opacity(((t - from) / (to - from)).clamp(0., 1.))
                },
            )
            .into_any_element()
    }
}

impl Render for InstallView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        div()
            .id("install")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _, window, cx| {
                    // A press on a button is for the button.
                    if !window.default_prevented() {
                        view.skip(cx);
                    }
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(theme.paper)
            .font_family(ui_theme(cx).font_family)
            .child(self.render_page(cx))
            .child(self.render_footer(cx))
    }
}

/// Where Gasp is running from, in a few words.
fn running_from(placement: &Placement) -> String {
    match &placement.source {
        Source::DiskImage { .. } => "Gasp is running from its disk image.".into(),
        Source::Downloads => "Gasp is running from your Downloads folder.".into(),
        // Gatekeeper runs it from a hidden copy; where the original is
        // couldn't be found, only that it isn't in Applications.
        Source::Translocated => "Gasp is running from outside Applications.".into(),
        Source::Elsewhere => {
            format!("Gasp is running from {}.", folder(&placement.bundle))
        }
        Source::Development => "This is a development build of Gasp.".into(),
    }
}

/// The name of the folder `path` is in, such as "Applications".
fn folder(path: &Path) -> String {
    let parent = path.parent().unwrap_or(path);
    parent.file_name().map_or_else(
        || parent.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Opens the install window for `placement`, in the middle of the screen.
pub fn open_install_window(
    placement: Placement,
    installer: Arc<dyn Installer>,
    hooks: InstallHooks,
    dark: Option<bool>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<InstallView>> {
    let dark = dark.unwrap_or_else(|| crate::ui::is_dark_appearance(cx.window_appearance()));
    crate::ui::set_theme(&gasp_config::Config::defaults().theme, dark, cx);
    let theme = InstallTheme::new(dark);
    let bounds = Bounds::centered(None, size(theme.window_width, theme.window_height), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(gasp_config::APP_NAME.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.), px(14.))),
        }),
        is_resizable: false,
        is_minimizable: false,
        ..Default::default()
    };
    let frozen = super::frozen_moment();
    let window = cx.open_window(options, move |window, cx| {
        cx.new(|cx| InstallView::new(placement, installer, hooks, dark, frozen, window, cx))
    })?;
    window.update(cx, |_, window, cx| {
        window.set_window_title(gasp_config::APP_NAME);
        cx.activate(true);
    })?;
    Ok(window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_footer_names_the_folder() {
        assert_eq!(folder(Path::new("/Applications/Gasp.app")), "Applications");
        assert_eq!(
            folder(Path::new("/Users/ada/Applications/Gasp.app")),
            "Applications"
        );
        let elsewhere = Placement {
            bundle: PathBuf::from("/Users/ada/Tools/Gasp.app"),
            original: None,
            source: Source::Elsewhere,
        };
        assert_eq!(running_from(&elsewhere), "Gasp is running from Tools.");
    }

    #[test]
    fn each_place_reads_naturally() {
        let from = |source: Source| {
            running_from(&Placement {
                bundle: PathBuf::from("/private/var/AppTranslocation/A/d/Gasp.app"),
                original: None,
                source,
            })
        };
        let mount = PathBuf::from("/Volumes/Gasp");
        assert_eq!(
            from(Source::DiskImage { mount }),
            "Gasp is running from its disk image."
        );
        assert_eq!(
            from(Source::Downloads),
            "Gasp is running from your Downloads folder."
        );
        assert_eq!(
            from(Source::Translocated),
            "Gasp is running from outside Applications."
        );
        assert_eq!(
            from(Source::Development),
            "This is a development build of Gasp."
        );
    }
}
