//! The welcome tour, the first thing a new install shows: hello, writing
//! in Markdown, getting around by keyboard, choosing where notes live and
//! how sync works. With no vault to reopen and some opened before, the
//! window starts at choosing a vault instead.
//!
//! The app icon's whale carries it: it breaches through a paragraph on
//! the first step, then swims under the lines at the bottom of the window
//! to where the red caret marks how far along the tour is.
//!
//! Enter or the right arrow moves on and the left arrow goes back, except
//! while the practice note has the keyboard. On the vault and sync steps
//! the arrows and Tab walk the choices and Enter takes one.

mod art;
mod hello;
mod motion;
mod sample;
mod sea;
mod shortcuts;
mod sketch;
mod sync_step;
mod vault_step;
mod writing;

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, KeyDownEvent, MouseMoveEvent, Pixels,
    Point, Subscription, Task, Window, div, point, prelude::*, px,
};

use crate::editor::EditorView;
use crate::icons::IconName;
use crate::keymap::{RunCommand, WORKSPACE_CONTEXT};
use crate::theme::UiTheme;
use crate::ui::{Button, Selectable, ui_theme};

pub use art::WhaleArt;

/// A still whale, `width` wide.
pub fn drawn_still(still: &std::sync::Arc<gpui::RenderImage>, width: Pixels) -> AnyElement {
    art::drawn(still, 0, width).into_any_element()
}
pub use sample::{FIRST_NOTE, SAMPLE_VAULT_NAME, write_sample_vault};
pub use shortcuts::SHORTCUT_COMMANDS;
pub use vault_step::{NEW_VAULT_NAME, VaultChoice};

/// One screen of the tour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Hello,
    Writing,
    Shortcuts,
    Vault,
    Sync,
}

impl Step {
    /// Every step, for a first launch.
    pub const WHOLE_TOUR: [Step; 5] = [
        Step::Hello,
        Step::Writing,
        Step::Shortcuts,
        Step::Vault,
        Step::Sync,
    ];

    /// Only choosing a vault, for a launch with none to reopen.
    pub const PICK_A_VAULT: [Step; 2] = [Step::Vault, Step::Sync];

    fn title(self) -> &'static str {
        match self {
            Step::Hello => "Welcome to Gasp",
            Step::Writing => "Write in Markdown",
            Step::Shortcuts => "Get around with the keyboard",
            Step::Vault => "Where your notes live",
            Step::Sync => "Sync with your other devices",
        }
    }

    /// Whether this step moves on by its own choices rather than a
    /// Continue button.
    fn chooses(self) -> bool {
        matches!(self, Step::Vault | Step::Sync)
    }
}

/// What happens once a vault is chosen and the tour hands the window
/// over to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AfterOpening {
    Nothing,
    OpenNote(PathBuf),
    SetUpSync,
}

/// A move from one step to another, for the motion between them.
#[derive(Clone, Copy, Debug)]
struct StepChange {
    from: usize,
    at: Instant,
}

pub struct Tour {
    focus_handle: FocusHandle,
    steps: Vec<Step>,
    at: usize,
    change: Option<StepChange>,
    opened: Instant,
    art: Option<Rc<WhaleArt>>,
    art_is_dark: bool,
    decoding: Option<Task<()>>,
    playground: Option<Entity<EditorView>>,
    /// The shortcut last pressed on the shortcuts step, and when.
    pressed: Option<(usize, Instant)>,
    /// The whale leaping when clicked on the first step.
    leapt: Option<Instant>,
    recent: Vec<PathBuf>,
    /// The vault chosen, waiting on the sync step.
    vault: Option<PathBuf>,
    /// The choice the keyboard is on, on the vault and sync steps.
    selected: usize,
    /// Where the pointer is, from -1 to 1 across and down the window.
    pointer: Point<f32>,
    ticking: Option<Task<()>>,
    _notices: Subscription,
    _activation: Subscription,
    _appearance: Subscription,
}

impl Focusable for Tour {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Tour {
    /// The whole tour, for a first launch.
    pub fn whole(recent: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_steps(Step::WHOLE_TOUR.to_vec(), recent, window, cx)
    }

    /// Only choosing a vault, then sync.
    pub fn pick_a_vault(recent: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_steps(Step::PICK_A_VAULT.to_vec(), recent, window, cx)
    }

    fn with_steps(
        steps: Vec<Step>,
        recent: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        let activation = cx.observe_window_activation(window, |_, _, cx| cx.notify());
        follow_appearance(window, cx);
        let appearance = cx.observe_window_appearance(window, |tour, window, cx| {
            if follow_appearance(window, cx) {
                tour.restyle(cx);
            }
        });
        let mut tour = Tour {
            focus_handle,
            steps,
            at: 0,
            change: None,
            opened: Instant::now(),
            art: None,
            art_is_dark: false,
            decoding: None,
            playground: None,
            pressed: None,
            leapt: None,
            recent,
            vault: None,
            selected: 0,
            pointer: point(0., 0.),
            ticking: None,
            _notices: crate::notices::observe(cx),
            _activation: activation,
            _appearance: appearance,
        };
        tour.decode_art(cx);
        tour
    }

    /// Redraws the practice note and the whales for the theme in effect.
    fn restyle(&mut self, cx: &mut Context<Self>) {
        if let Some(playground) = &self.playground {
            let config = gasp_config::Config::defaults();
            playground.update(cx, |editor, cx| editor.apply_config(&config, cx));
        }
        self.decode_art(cx);
        cx.notify();
    }

    pub fn step(&self) -> Step {
        self.steps[self.at]
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn chosen_vault(&self) -> Option<&PathBuf> {
        self.vault.as_ref()
    }

    /// When a shortcut was last pressed on the shortcuts step.
    pub fn pressed_shortcut(&self) -> Option<usize> {
        self.pressed.map(|(index, _)| index)
    }

    /// Decodes the whales for the theme in effect, off the main thread.
    fn decode_art(&mut self, cx: &mut Context<Self>) {
        let dark = crate::ui::is_dark(cx);
        if self.decoding.is_some() || (self.art.is_some() && self.art_is_dark == dark) {
            return;
        }
        let decoding = cx.background_spawn(async move { WhaleArt::decode(dark) });
        self.decoding = Some(cx.spawn(async move |tour, cx| {
            let art = decoding.await;
            tour.update(cx, |tour, cx| {
                tour.art = art.map(Rc::new);
                tour.art_is_dark = dark;
                tour.decoding = None;
                cx.notify();
            })
            .ok();
        }));
    }

    /// Goes to step `index`, if there is one.
    pub fn go_to(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.steps.len() || index == self.at {
            return;
        }
        self.change = Some(StepChange {
            from: self.at,
            at: Instant::now(),
        });
        self.at = index;
        self.selected = 0;
        self.arrive(window, cx);
        cx.notify();
    }

    pub fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.go_to(self.at + 1, window, cx);
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.at > 0 {
            self.go_to(self.at - 1, window, cx);
        }
    }

    /// Skips to choosing a vault.
    pub fn skip_to_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.steps.iter().position(|step| *step == Step::Vault) {
            self.go_to(index, window, cx);
        }
    }

    /// Sets up what a step needs as it shows, and gives it the keyboard.
    fn arrive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.step() == Step::Writing {
            let playground = self.playground(cx);
            window.focus(&playground.focus_handle(cx));
            return;
        }
        window.focus(&self.focus_handle);
    }

    fn playground(&mut self, cx: &mut Context<Self>) -> Entity<EditorView> {
        if let Some(playground) = &self.playground {
            return playground.clone();
        }
        let playground = writing::new_playground(cx);
        self.playground = Some(playground.clone());
        playground
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.modified() && !keystroke.modifiers.shift {
            return;
        }
        let handled = if self.step().chooses() {
            self.on_choice_key(&keystroke.key, keystroke.modifiers.shift, window, cx)
        } else {
            self.on_walk_key(&keystroke.key, window, cx)
        };
        if handled {
            cx.stop_propagation();
        }
    }

    /// Keys that walk the tour on the steps without choices.
    fn on_walk_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match key {
            "enter" | "right" | "space" => self.advance(window, cx),
            "left" => self.back(window, cx),
            "escape" => self.skip_to_vault(window, cx),
            _ => return false,
        }
        true
    }

    /// Keys that walk and take the choices on the vault and sync steps.
    fn on_choice_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let count = self.choice_count();
        let step =
            |by: isize| (self.selected as isize + by).rem_euclid(count.max(1) as isize) as usize;
        self.selected = match (key, shift) {
            ("down", _) | ("tab", false) => step(1),
            ("up", _) | ("tab", true) => step(-1),
            ("enter" | "space", _) => {
                self.take_choice(self.selected, window, cx);
                return true;
            }
            ("left" | "escape", _) => {
                self.back(window, cx);
                return true;
            }
            _ => return false,
        };
        cx.notify();
        true
    }

    fn choice_count(&self) -> usize {
        match self.step() {
            Step::Vault => self.vault_choices().len(),
            Step::Sync => sync_step::CHOICES,
            _ => 0,
        }
    }

    fn take_choice(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match self.step() {
            Step::Vault => {
                if let Some(choice) = self.vault_choices().get(index).cloned() {
                    self.take_vault_choice(choice, window, cx);
                }
            }
            Step::Sync => self.take_sync_choice(index, window, cx),
            _ => {}
        }
    }

    /// A shortcut's command, run from anywhere in the tour: on the
    /// shortcuts step it plays that shortcut's picture.
    fn on_run_command(&mut self, action: &RunCommand, _: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = SHORTCUT_COMMANDS
            .iter()
            .position(|id| *id == action.id.as_ref())
        else {
            return;
        };
        if self.step() == Step::Shortcuts {
            self.press_shortcut(index, cx);
        }
    }

    pub fn press_shortcut(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pressed = Some((index, Instant::now()));
        cx.notify();
    }

    fn leap(&mut self, cx: &mut Context<Self>) {
        let leaping = self
            .leapt
            .is_some_and(|at| at.elapsed() < ui_theme(cx).tour.press * 2);
        if !leaping {
            self.leapt = Some(Instant::now());
            cx.notify();
        }
    }

    fn on_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.step() != Step::Hello {
            return;
        }
        let size = window.viewport_size();
        let across = event.position.x / size.width.max(px(1.)) * 2. - 1.;
        let down = event.position.y / size.height.max(px(1.)) * 2. - 1.;
        self.pointer = point(across.clamp(-1., 1.), down.clamp(-1., 1.));
        cx.notify();
    }

    /// Hands the window to `vault`, then does `after` there.
    pub fn open_vault(
        &mut self,
        vault: PathBuf,
        after: AfterOpening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(art) = &self.art {
            art.release(window);
        }
        vault_step::open_here(vault, after, window, cx);
    }

    /// Asks for the next frame while something moves smoothly, so a step
    /// always finishes arriving. Otherwise it asks for a wake-up when the
    /// swimming whale's frame or the caret's blink next changes, but not
    /// while the window is in the background.
    fn schedule_next_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self.moves_smoothly(now, cx) {
            window.request_animation_frame();
            return;
        }
        if !window.is_window_active() {
            self.ticking = None;
            return;
        }
        if self.step() == Step::Sync {
            window.request_animation_frame();
            return;
        }
        if self.ticking.is_some() {
            return;
        }
        let wait = art::SWIM_FRAME_TIME.min(motion::until_caret_flips(self.opened, now));
        self.ticking = Some(cx.spawn_in(window, async move |tour, cx| {
            cx.background_executor()
                .timer(wait.max(Duration::from_millis(16)))
                .await;
            tour.update(cx, |tour, cx| {
                tour.ticking = None;
                cx.notify();
            })
            .ok();
        }));
    }

    fn moves_smoothly(&self, now: Instant, cx: &mut App) -> bool {
        let tour = ui_theme(cx).tour;
        let changing = self
            .change
            .is_some_and(|change| now < change.at + tour.swim.max(tour.step_enter));
        let pressing = self.pressed.is_some_and(|(_, at)| now < at + tour.press);
        let leaping = self.leapt.is_some_and(|at| now < at + tour.press * 2);
        let rising = self.step() == Step::Hello && now < self.opened + tour.breach_rise;
        changing || pressing || leaping || rising
    }

    /// How far into its entrance the step showing is, from 0 to 1.
    fn entrance(&self, now: Instant, ui: &UiTheme) -> f32 {
        self.change.map_or(1., |change| {
            motion::ease_out(motion::progress(change.at, now, ui.tour.step_enter))
        })
    }

    /// Where along the sea the caret is, from 0 at the first step to 1 at
    /// the last, moving with the whale between steps.
    fn tide(&self, now: Instant, ui: &UiTheme) -> f32 {
        let at = |index: usize| index as f32 / (self.steps.len().max(2) - 1) as f32;
        let Some(change) = self.change else {
            return at(self.at);
        };
        let swum = motion::ease_in_out(motion::progress(change.at, now, ui.tour.swim));
        motion::lerp(at(change.from), at(self.at), swum)
    }

    fn render_step(
        &mut self,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.step() {
            Step::Hello => hello::render(self, now, window, cx),
            Step::Writing => {
                let playground = self.playground(cx);
                writing::render(playground, cx)
            }
            Step::Shortcuts => shortcuts::render(self, now, window, cx),
            Step::Vault => vault_step::render(self, window, cx),
            Step::Sync => sync_step::render(self, now, cx),
        }
    }

    /// The strip along the top: it moves the window, as the title bar it
    /// stands in for would, and holds the way to skip ahead.
    fn render_top(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = ui_theme(cx);
        let skip = self
            .steps
            .iter()
            .position(|step| *step == Step::Vault)
            .filter(|vault| self.at < *vault);
        div()
            .id("tour-top")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(ui.tab_height + ui.space_lg)
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .px(ui.space_xl)
            .on_mouse_down(gpui::MouseButton::Left, |event, window, _| {
                if event.click_count == 2 {
                    crate::window_drag::double_click(window);
                } else {
                    crate::window_drag::start(window);
                }
            })
            .children(skip.map(|_| {
                Button::new("tour-skip", "Skip to your notes")
                    .quiet()
                    .on_click(cx.listener(|tour, _, window, cx| tour.skip_to_vault(window, cx)))
            }))
            .into_any_element()
    }

    /// Back and Continue, above the sea.
    fn render_walk(&self, ui: &UiTheme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let step = self.step();
        let back = (self.at > 0).then(|| {
            Button::new("tour-back", "Back")
                .quiet()
                .on_click(cx.listener(|tour, _, window, cx| tour.back(window, cx)))
        });
        let onward: Option<AnyElement> = match step {
            Step::Hello | Step::Vault => None,
            Step::Sync => Some(sync_step::choices(self, window, cx)),
            _ => Some(
                Button::new("tour-continue", "Continue")
                    .primary()
                    .on_click(cx.listener(|tour, _, window, cx| tour.advance(window, cx)))
                    .into_any_element(),
            ),
        };
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(sea::band_height(ui) + ui.space_lg)
            .px(sea::margin(ui))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(div().children(back))
            .child(div().children(onward))
            .into_any_element()
    }
}

/// Puts the built-in theme in effect in the system's light or dark, as a
/// vault with the default settings would. Answers whether it changed.
fn follow_appearance(window: &Window, cx: &mut App) -> bool {
    let dark = crate::ui::is_dark_appearance(window.appearance());
    crate::ui::set_system_dark(dark, cx);
    crate::ui::set_theme(&gasp_config::Config::defaults().theme, dark, cx)
}

/// A step's heading, in the tour's heading size.
fn heading(step: Step, ui: &UiTheme) -> impl IntoElement {
    div()
        .text_size(ui.tour.heading_size)
        .line_height(ui.tour.heading_size * 1.2)
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(ui.text_strong)
        .child(step.title())
}

/// A step's one sentence of explanation under its heading.
fn explanation(text: &'static str, ui: &UiTheme) -> impl IntoElement {
    div()
        .max_w(px(640.))
        .text_color(ui.text_detail)
        .line_height(ui.font_size * 1.5)
        .child(text)
}

/// A step's content, centred in the room between the top strip and the
/// Back and Continue buttons, at most `width` wide.
fn stage(width: Pixels, ui: &UiTheme, content: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .top(ui.tab_height + ui.space_xl)
        .bottom(sea::band_height(ui) + ui.button_height + ui.space_xl * 2.)
        .left_0()
        .right_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(sea::margin(ui))
        .child(div().w_full().max_w(width).child(content))
        .into_any_element()
}

/// Where an icon sits in a choice row, and how big.
fn choice_icon(name: IconName, ui: &UiTheme) -> impl IntoElement {
    crate::icons::icon(name)
        .flex_none()
        .size(ui.icon_size)
        .text_color(ui.icon)
}

impl Render for Tour {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.decode_art(cx);
        let ui = ui_theme(cx);
        let now = Instant::now();
        let entrance = self.entrance(now, &ui);
        let step = self.render_step(now, window, cx);
        let sea = (self.step() != Step::Hello).then(|| {
            let tide = self.tide(now, &ui);
            sea::band(self.art.clone(), tide, self.opened, now, window, &ui)
        });
        let walk = self.render_walk(&ui, window, cx);
        let top = self.render_top(cx);
        self.schedule_next_frame(window, cx);
        div()
            .id("tour")
            .selector(|| "tour".to_owned())
            .key_context(WORKSPACE_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_action(cx.listener(Self::on_run_command))
            .on_mouse_move(cx.listener(Self::on_pointer_move))
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(ui.app_background)
            .font_family(ui.font_family.clone())
            .text_size(ui.font_size)
            .text_color(ui.text)
            .child(crate::ui::focus_visible::pointer_watch())
            .children(sea)
            .child(
                div()
                    .absolute()
                    .size_full()
                    .opacity(entrance)
                    .top((1. - entrance) * ui.space_xl)
                    .child(step),
            )
            .child(walk)
            .child(top)
            .children(crate::notices::render(window, ui.space_xl, cx))
    }
}

/// The distance the whale on the first step leans towards the pointer.
fn lean(pointer: Point<f32>, reach: Pixels) -> Point<Pixels> {
    point(reach * pointer.x, reach * 0.5 * pointer.y)
}
