//! Setting up sync for a vault that isn't a git clone: the repository,
//! its branch and a GitHub token, then `gasp_sync::set_up_in_place` on the
//! background executor, which makes the folder a clone and merges its
//! notes with the repository's. The token goes to the credential store
//! and the window's sync starts, so the status bar shows it at once.
//!
//! Tab and Shift+Tab walk the fields and buttons, Enter sets up (or
//! presses the button the keyboard is on), Escape closes. While setting
//! up runs, nothing closes it: the work finishes either way.

use std::path::PathBuf;

use gasp_config::settings::SyncSettings;
use gasp_sync::{InPlaceSetup, SetupReport, SyncError, Token, VaultConfig};
use gpui::{
    AnyElement, App, AsyncWindowContext, ClickEvent, Context, DismissEvent, Entity, EventEmitter,
    FocusHandle, Focusable, KeyDownEvent, SharedString, Subscription, WeakEntity, Window, div,
    prelude::*, relative,
};
use serde_json::Value;

use super::engine::vault_config;
use super::state::count;
use crate::icons::{IconName, icon};
use crate::settings_view::controls::{
    FieldState, Fills, button, button_in, field_box_in, icon_label_button, inert_button, widest_of,
};
use crate::text_input::{TextInput, TextInputEvent, TextInputStyle};
use crate::theme::SettingsTheme;
use crate::ui::Selectable;
use crate::workspace::Workspace;

/// Where GitHub makes fine-grained tokens.
pub const NEW_TOKEN_URL: &str = "https://github.com/settings/personal-access-tokens/new";

const SUBMIT_LABEL: &str = "Set up sync";
const WORKING_LABEL: &str = "Setting up…";

/// The places Tab stops at, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupStop {
    Repository,
    Branch,
    Token,
    CreateToken,
    Cancel,
    Submit,
    /// On the finished screen.
    Resolve,
    Done,
}

/// The fields a refusal can point at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupField {
    Repository,
    Branch,
    Token,
}

/// Where setting up is, as tests and the screen read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupPhase {
    Editing,
    Working,
    /// Stopped, with why and what to do.
    Refused {
        field: Option<SetupField>,
        message: String,
    },
    Done(SetupDone),
}

/// What the finished screen says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupDone {
    pub repository: String,
    pub report: SetupReport,
    /// Something that went wrong after the vault was set up, such as a
    /// token that couldn't be kept.
    pub caveat: Option<String>,
}

/// What the dialog asks its host to do once it has closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncSetupEvent {
    RunCommand(String),
}

/// Everything the background work needs, taken from the form.
struct SetupJob {
    root: PathBuf,
    repository: String,
    url: String,
    branch: String,
    token: Option<Token>,
    config: VaultConfig,
    settings_branch: String,
}

/// The "Set up sync" dialog.
pub struct SyncSetup {
    workspace: WeakEntity<Workspace>,
    root: PathBuf,
    settings: SyncSettings,
    repository: Entity<TextInput>,
    branch: Entity<TextInput>,
    token: Entity<TextInput>,
    focus_handle: FocusHandle,
    /// The button the keyboard is on while no field has it.
    button: SetupStop,
    phase: SetupPhase,
    style: SettingsTheme,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<DismissEvent> for SyncSetup {}
impl EventEmitter<SyncSetupEvent> for SyncSetup {}

impl Focusable for SyncSetup {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.phase {
            SetupPhase::Done(_) => self.focus_handle.clone(),
            _ => self.repository.focus_handle(cx),
        }
    }
}

fn field(placeholder: &str, window: &mut Window, cx: &mut Context<SyncSetup>) -> Entity<TextInput> {
    let placeholder = SharedString::from(placeholder.to_owned());
    cx.new(|cx| {
        TextInput::new(window, cx)
            .with_placeholder(placeholder)
            .with_style(TextInputStyle::Query)
    })
}

impl SyncSetup {
    /// The dialog for the vault at `root`, whose sync settings are `settings`.
    pub fn new(
        workspace: WeakEntity<Workspace>,
        root: PathBuf,
        settings: SyncSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let repository = field("you/notes", window, cx);
        let branch = field(&SyncSettings::default().branch, window, cx);
        branch.update(cx, |branch, cx| branch.set_text(&settings.branch, cx));
        let token = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Paste your token")
                .with_style(TextInputStyle::Query)
                .secure()
        });
        let subscriptions = [&repository, &branch, &token]
            .map(|input| cx.subscribe_in(input, window, Self::on_field_event))
            .into();
        SyncSetup {
            workspace,
            root,
            settings,
            repository,
            branch,
            token,
            focus_handle: cx.focus_handle(),
            button: SetupStop::Submit,
            phase: SetupPhase::Editing,
            style: crate::ui::settings_theme(cx),
            _subscriptions: subscriptions,
        }
    }

    pub fn phase(&self) -> &SetupPhase {
        &self.phase
    }

    /// Fills the form, as typing would.
    pub fn fill(&mut self, repository: &str, branch: &str, token: &str, cx: &mut Context<Self>) {
        self.repository
            .update(cx, |field, cx| field.set_text(repository, cx));
        self.branch
            .update(cx, |field, cx| field.set_text(branch, cx));
        self.token.update(cx, |field, cx| field.set_text(token, cx));
    }

    /// What's in the token field, which only shows dots.
    pub fn token_field(&self) -> &Entity<TextInput> {
        &self.token
    }

    fn is_working(&self) -> bool {
        self.phase == SetupPhase::Working
    }

    fn is_done(&self) -> bool {
        matches!(self.phase, SetupPhase::Done(_))
    }

    fn on_field_event(
        &mut self,
        input: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Submitted => self.submit(window, cx),
            TextInputEvent::Cancelled => self.cancel(cx),
            TextInputEvent::Changed => self.clear_refusal_of(input, cx),
            TextInputEvent::Blurred => {}
        }
    }

    /// Typing into the field a refusal pointed at takes the refusal away.
    fn clear_refusal_of(&mut self, input: &Entity<TextInput>, cx: &mut Context<Self>) {
        let SetupPhase::Refused {
            field: Some(field), ..
        } = &self.phase
        else {
            return;
        };
        if self.input(*field) == input {
            self.phase = SetupPhase::Editing;
            cx.notify();
        }
    }

    fn input(&self, field: SetupField) -> &Entity<TextInput> {
        match field {
            SetupField::Repository => &self.repository,
            SetupField::Branch => &self.branch,
            SetupField::Token => &self.token,
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.is_working() {
            cx.emit(DismissEvent);
        }
    }

    // ---- Setting up ----

    /// Checks the form and starts setting up, or says what's missing.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_working() || self.is_done() {
            return;
        }
        match self.job(cx) {
            Ok(job) => self.start(job, window, cx),
            Err((field, message)) => self.refuse(Some(field), message, window, cx),
        }
    }

    fn refuse(
        &mut self,
        field: Option<SetupField>,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(field) = field {
            window.focus(&self.input(field).focus_handle(cx));
        }
        self.phase = SetupPhase::Refused { field, message };
        cx.notify();
    }

    fn job(&self, cx: &App) -> Result<SetupJob, (SetupField, String)> {
        let repository = self.repository.read(cx).text().trim().to_owned();
        let Some(url) = gasp_sync::repository_url(&repository) else {
            let message =
                "Enter the repository as you/notes, github.com/you/notes or its full address.";
            return Err((SetupField::Repository, message.to_owned()));
        };
        if !crate::sandbox::allows_repository(&url) {
            let message = "A snapshot run only sets up sync with a repository on this computer.";
            return Err((SetupField::Repository, message.to_owned()));
        }
        let token = self.token.read(cx).text().trim().to_owned();
        if token.is_empty() && gasp_sync::url_takes_token(&url) {
            let message = "Paste a GitHub token that can read and write this repository.";
            return Err((SetupField::Token, message.to_owned()));
        }
        let branch = match self.branch.read(cx).text().trim() {
            "" => SyncSettings::default().branch,
            typed => typed.to_owned(),
        };
        let settings = SyncSettings {
            branch: branch.clone(),
            ..self.settings.clone()
        };
        let config = vault_config(&settings).map_err(|message| (SetupField::Branch, message))?;
        Ok(SetupJob {
            root: self.root.clone(),
            repository,
            url,
            branch,
            token: (!token.is_empty()).then(|| Token::new(token)),
            config,
            settings_branch: self.settings.branch.clone(),
        })
    }

    /// Saves open notes, so the folder holds what's on screen, then sets
    /// up on the background executor. The work carries on if the dialog
    /// goes away; it only reports back if it's still there.
    fn start(&mut self, job: SetupJob, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| workspace.save_all(cx));
        }
        self.phase = SetupPhase::Working;
        cx.notify();
        let device = crate::edit_time::device_name();
        let work = cx.background_spawn(async move {
            let result = gasp_sync::set_up_in_place(&InPlaceSetup {
                root: &job.root,
                url: &job.url,
                config: job.config.clone(),
                token: job.token.clone(),
                author: gasp_sync::default_author(&device),
                device: &device,
            });
            (job, result)
        });
        let workspace = self.workspace.clone();
        cx.spawn_in(window, async move |this, cx| {
            let (job, result) = work.await;
            let phase = finish(job, result, &workspace, cx);
            this.update_in(cx, |this, window, cx| this.finished(phase, window, cx))
                .ok();
        })
        .detach();
    }

    /// Shows how it ended, with the keyboard on what comes next: the
    /// finished screen's first button, or the field a refusal points at.
    fn finished(&mut self, phase: SetupPhase, window: &mut Window, cx: &mut Context<Self>) {
        match &phase {
            SetupPhase::Done(done) => {
                self.button = if done.report.waiting.is_empty() {
                    SetupStop::Done
                } else {
                    SetupStop::Resolve
                };
                window.focus(&self.focus_handle);
            }
            SetupPhase::Refused {
                field: Some(field), ..
            } => window.focus(&self.input(*field).focus_handle(cx)),
            _ => {}
        }
        self.phase = phase;
        cx.notify();
    }

    /// Closes, then runs `command`.
    fn close_and_run(&mut self, command: &str, cx: &mut Context<Self>) {
        cx.emit(SyncSetupEvent::RunCommand(command.to_owned()));
        cx.emit(DismissEvent);
    }

    // ---- Keys ----

    fn stops(&self) -> Vec<SetupStop> {
        match &self.phase {
            SetupPhase::Done(done) if done.report.waiting.is_empty() => vec![SetupStop::Done],
            SetupPhase::Done(_) => vec![SetupStop::Resolve, SetupStop::Done],
            _ => vec![
                SetupStop::Repository,
                SetupStop::Branch,
                SetupStop::Token,
                SetupStop::CreateToken,
                SetupStop::Cancel,
                SetupStop::Submit,
            ],
        }
    }

    /// Where the keyboard is: a field that has focus, else the button.
    pub fn current_stop(&self, window: &Window, cx: &App) -> SetupStop {
        let fields = [
            (SetupStop::Repository, &self.repository),
            (SetupStop::Branch, &self.branch),
            (SetupStop::Token, &self.token),
        ];
        fields
            .into_iter()
            .find(|(_, input)| input.focus_handle(cx).is_focused(window))
            .map_or(self.button, |(stop, _)| stop)
    }

    fn focus_stop(&mut self, stop: SetupStop, window: &mut Window, cx: &mut Context<Self>) {
        let handle = match stop {
            SetupStop::Repository => self.repository.focus_handle(cx),
            SetupStop::Branch => self.branch.focus_handle(cx),
            SetupStop::Token => self.token.focus_handle(cx),
            button => {
                self.button = button;
                self.focus_handle.clone()
            }
        };
        window.focus(&handle);
        cx.notify();
    }

    fn move_focus(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        let stops = self.stops();
        let at = stops
            .iter()
            .position(|stop| *stop == self.current_stop(window, cx))
            .unwrap_or(0);
        let count = stops.len();
        let next = if backwards {
            (at + count - 1) % count
        } else {
            (at + 1) % count
        };
        self.focus_stop(stops[next], window, cx);
    }

    fn press(&mut self, stop: SetupStop, window: &mut Window, cx: &mut Context<Self>) {
        match stop {
            SetupStop::CreateToken => crate::sandbox::open_url(NEW_TOKEN_URL, cx),
            SetupStop::Cancel => self.cancel(cx),
            SetupStop::Resolve => self.close_and_run("sync.resolve-conflicts", cx),
            SetupStop::Done => cx.emit(DismissEvent),
            _ => self.submit(window, cx),
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt {
            return;
        }
        let on_button = !matches!(
            self.current_stop(window, cx),
            SetupStop::Repository | SetupStop::Branch | SetupStop::Token
        );
        match keystroke.key.as_str() {
            "tab" => self.move_focus(modifiers.shift, window, cx),
            "enter" | "space" if on_button => self.press(self.button, window, cx),
            // Nothing closes the dialog while it works.
            "escape" if self.is_working() => {}
            "escape" => cx.emit(DismissEvent),
            _ => return,
        }
        cx.stop_propagation();
    }

    // ---- Drawing ----

    fn ringed(&self, stop: SetupStop, window: &Window, cx: &App) -> bool {
        let on_it = self.focus_handle.is_focused(window) && self.button == stop;
        crate::ui::focus_visible::ring(on_it, cx)
    }

    fn render_header(&self, title: &'static str, intro: Option<String>) -> AnyElement {
        let style = &self.style;
        div()
            .flex()
            .flex_col()
            .gap(style.text_gap * 2.)
            .child(
                div()
                    .text_size(style.page_title_size)
                    .font_weight(style.strong_weight)
                    .child(title),
            )
            .children(intro.map(|intro| div().text_color(style.text_muted).child(intro)))
            .into_any_element()
    }

    fn field_state(&self, field: SetupField, window: &Window, cx: &App) -> FieldState {
        let refused = matches!(
            &self.phase,
            SetupPhase::Refused { field: Some(refused), .. } if *refused == field
        );
        if refused {
            return FieldState::Refused;
        }
        if self.input(field).focus_handle(cx).is_focused(window) {
            FieldState::Focused
        } else {
            FieldState::Idle
        }
    }

    fn render_field(
        &self,
        field: SetupField,
        label: &'static str,
        under: Option<AnyElement>,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let style = &self.style;
        let name = match field {
            SetupField::Repository => "repository",
            SetupField::Branch => "branch",
            SetupField::Token => "token",
        };
        div()
            .flex()
            .flex_col()
            .gap(style.gap_sm * 1.5)
            .child(div().child(label))
            .child(
                field_box_in(
                    self.input(field).clone(),
                    None,
                    self.field_state(field, window, cx),
                    style,
                )
                .w_full()
                .selector(move || format!("sync-setup-{name}")),
            )
            .children(under)
            .into_any_element()
    }

    /// The line under the token field: what the token needs, and a button
    /// that makes one on GitHub.
    fn render_token_help(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let create = icon_label_button(
            "sync-setup-create-token",
            IconName::ArrowSquareOut,
            "Create a token",
            false,
            self.ringed(SetupStop::CreateToken, window, cx),
            style,
        )
        .selector(|| "sync-setup-create-token".to_owned())
        .on_click(|_: &ClickEvent, _, cx| crate::sandbox::open_url(NEW_TOKEN_URL, cx));
        div()
            .flex()
            .items_center()
            .gap(style.control_gap)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child("A fine-grained token with read and write access to the repository’s contents."),
            )
            .child(create)
            .into_any_element()
    }

    /// The line above the buttons, which always keeps its room: why
    /// setting up stopped, or that it's running.
    fn render_status(&self) -> AnyElement {
        let style = &self.style;
        let lines = style.small_text_size * style.line_height_factor * 2.;
        let (mark, color, text): (Option<AnyElement>, _, String) = match &self.phase {
            SetupPhase::Refused { message, .. } => (
                Some(
                    icon(IconName::WarningCircle)
                        .flex_none()
                        .size(style.small_icon_size)
                        .text_color(style.warning)
                        .into_any_element(),
                ),
                style.text,
                message.clone(),
            ),
            SetupPhase::Working => (
                None,
                style.text_muted,
                "Bringing your notes and the repository’s together. A big vault can take a minute."
                    .to_owned(),
            ),
            _ => (None, style.text_muted, String::new()),
        };
        div()
            .selector(|| "sync-setup-status".to_owned())
            .h(lines)
            .flex()
            .items_start()
            .gap(style.gap_sm * 1.5)
            .text_size(style.small_text_size)
            .text_color(color)
            .children(mark.map(|mark| div().flex_none().mt(style.gap_xs).child(mark)))
            .child(div().flex_1().min_w_0().child(text))
            .into_any_element()
    }

    fn render_form_buttons(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let cancel = if self.is_working() {
            inert_button("sync-setup-cancel", "Cancel", style)
        } else {
            button(
                "sync-setup-cancel",
                "Cancel",
                false,
                self.ringed(SetupStop::Cancel, window, cx),
                style,
            )
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.cancel(cx)))
        }
        .selector(|| "sync-setup-cancel".to_owned());
        div()
            .flex()
            .justify_end()
            .gap(style.control_gap)
            .child(cancel)
            .child(self.render_submit(window, cx))
            .into_any_element()
    }

    /// The main button. While setting up runs it stays pressed in, with a
    /// turning mark and "Setting up…", at the same width.
    fn render_submit(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let ui = crate::ui::ui_theme(cx);
        let working = self.is_working();
        let (mark, label) = if working {
            let spinning =
                super::indicator::turning_icon(style.small_icon_size, style.on_accent, &ui);
            (spinning, WORKING_LABEL)
        } else {
            let still = icon(IconName::CloudArrowUp)
                .flex_none()
                .size(style.small_icon_size)
                .text_color(style.on_accent)
                .into_any_element();
            (still, SUBMIT_LABEL)
        };
        let content = div()
            .flex()
            .items_center()
            .gap(style.gap_sm * 1.5)
            .text_color(style.on_accent)
            .child(mark)
            .child(widest_of(
                div().child(label),
                [SUBMIT_LABEL.into(), WORKING_LABEL.into()],
            ));
        let submit = if working {
            button_in(
                "sync-setup-submit",
                content,
                Fills::held(style.accent_pressed, style),
                false,
                style,
            )
        } else {
            button(
                "sync-setup-submit",
                content,
                true,
                self.ringed(SetupStop::Submit, window, cx),
                style,
            )
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| view.submit(window, cx)))
        };
        submit
            .pl(style.control_padding_x * 0.75)
            .selector(|| "sync-setup-submit".to_owned())
            .into_any_element()
    }

    fn render_form(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let style = &self.style;
        let intro = "Keep this vault the same on every device through a GitHub repository. The notes here stay, and anything the repository has joins them.";
        let token_help = self.render_token_help(window, cx);
        let fields = div()
            .flex()
            .flex_col()
            .gap(style.row_gap * 0.75)
            .child(self.render_field(SetupField::Repository, "Repository", None, window, cx))
            .child(self.render_field(SetupField::Branch, "Branch", None, window, cx))
            .child(self.render_field(
                SetupField::Token,
                "GitHub token",
                Some(token_help),
                window,
                cx,
            ));
        vec![
            self.render_header("Set up sync", Some(intro.to_owned())),
            fields.into_any_element(),
            self.render_status(),
            self.render_form_buttons(window, cx),
        ]
    }

    fn render_done(
        &self,
        done: &SetupDone,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let style = &self.style;
        let lines = done_sentences(done)
            .into_iter()
            .map(|line| div().child(line).into_any_element());
        let resolve = (!done.report.waiting.is_empty()).then(|| {
            button(
                "sync-setup-resolve",
                "Resolve conflicts",
                false,
                self.ringed(SetupStop::Resolve, window, cx),
                style,
            )
            .selector(|| "sync-setup-resolve".to_owned())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                view.close_and_run("sync.resolve-conflicts", cx)
            }))
        });
        let finish = button(
            "sync-setup-done",
            "Done",
            true,
            self.ringed(SetupStop::Done, window, cx),
            style,
        )
        .selector(|| "sync-setup-done".to_owned())
        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(DismissEvent)));
        vec![
            self.render_header("Sync is set up", None),
            div()
                .flex()
                .flex_col()
                .gap(style.control_gap)
                .children(lines)
                .into_any_element(),
            div()
                .flex()
                .justify_end()
                .gap(style.control_gap)
                .children(resolve)
                .child(finish)
                .into_any_element(),
        ]
    }
}

/// What the finished screen says, one sentence to a line.
pub fn done_sentences(done: &SetupDone) -> Vec<String> {
    let report = &done.report;
    let mut lines = vec![if report.remote_was_empty {
        format!(
            "Your notes are in {} now, and they sync on their own from here.",
            done.repository
        )
    } else {
        format!(
            "{} came in from {} and {} went out. From here, sync runs on its own.",
            capitalized(&count(report.brought_in.len(), "file")),
            done.repository,
            count(report.sent.len(), "file"),
        )
    }];
    if !report.waiting.is_empty() {
        lines.push(format!(
            "{} differ between this vault and the repository. Each shows both versions until you pick what to keep.",
            capitalized(&count(report.waiting.len(), "note")),
        ));
    }
    lines.extend(done.caveat.clone());
    lines
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Which field a failure points at.
fn field_for(error: &SyncError) -> Option<SetupField> {
    match error {
        SyncError::Auth(_) | SyncError::PushRejected(_) => Some(SetupField::Token),
        SyncError::Offline(message) if message.contains("401") || message.contains("403") => {
            Some(SetupField::Token)
        }
        SyncError::Offline(_) => Some(SetupField::Repository),
        _ => None,
    }
}

/// After the work: on success, keeps the token, names the branch in the
/// vault's settings if it isn't the one they name, and starts the
/// window's sync on the new clone.
fn finish(
    job: SetupJob,
    result: Result<SetupReport, SyncError>,
    workspace: &WeakEntity<Workspace>,
    cx: &mut AsyncWindowContext,
) -> SetupPhase {
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            return SetupPhase::Refused {
                field: field_for(&error),
                message: gasp_sync::setup_problem(&error, &job.url),
            };
        }
    };
    let caveat = cx
        .update(|_, cx| keep_setup(&job, workspace, cx))
        .ok()
        .flatten();
    SetupPhase::Done(SetupDone {
        repository: job.repository,
        report,
        caveat,
    })
}

/// Keeps what setting up needs from now on. Returns what didn't work.
fn keep_setup(job: &SetupJob, workspace: &WeakEntity<Workspace>, cx: &mut App) -> Option<String> {
    let mut caveat = None;
    if let Some(token) = &job.token
        && crate::sandbox::keeps_credentials()
        && let Err(error) = super::credential_store(cx).save(&job.url, token)
    {
        caveat = Some(format!(
            "The token couldn’t be kept ({error}). Paste it on the Sync page in settings."
        ));
    }
    if job.branch != job.settings_branch {
        let default = Value::from(SyncSettings::default().branch);
        let written = gasp_config::store::write_setting(
            &job.root,
            "sync.branch",
            Some(&Value::from(job.branch.clone())),
            &default,
        );
        if let Err(error) = written {
            caveat.get_or_insert(format!(
                "The branch couldn’t be saved in settings ({error}). Set it on the Sync page."
            ));
        }
    }
    if let Some(workspace) = workspace.upgrade() {
        workspace.update(cx, |workspace, cx| {
            workspace.reload_config(cx);
            let settings = workspace.config().settings.sync.clone();
            if let Some(sync) = workspace.sync().cloned() {
                sync.update(cx, |sync, cx| sync.reopen(settings, cx));
            }
        });
    }
    caveat
}

impl Render for SyncSetup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.style = crate::ui::settings_theme(cx);
        let style = self.style.clone();
        let ui = crate::ui::ui_theme(cx);
        let children = match &self.phase {
            SetupPhase::Done(done) => {
                let done = done.clone();
                self.render_done(&done, window, cx)
            }
            _ => self.render_form(window, cx),
        };
        div()
            .id("sync-setup")
            .selector(|| "sync-setup".to_owned())
            .key_context("SyncSetup")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .w(ui.dialog_width)
            .max_w_full()
            .flex()
            .flex_col()
            .gap(style.card_gap)
            .px(style.content_padding_x)
            .py(style.content_padding_y)
            .rounded(style.modal_radius)
            .bg(style.background)
            .shadow(vec![style.outline(), style.popover_shadow()])
            .font_family(style.font_family.clone())
            .text_size(style.text_size)
            .line_height(relative(style.line_height_factor))
            .text_color(style.text)
            .children(children)
    }
}
