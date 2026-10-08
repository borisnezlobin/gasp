//! "Set up sync" for a vault that doesn't sync yet: one choice between
//! iCloud and GitHub, then either path to the end.
//!
//! iCloud moves the vault into iCloud Drive's Gasp folder (copying every
//! file and checking each copy, the original left where it was) and
//! opens it from there. GitHub signs in with a short code typed on
//! github.com, then makes a private repository for the notes or takes
//! one the person has, and sets sync up in place as the form does. The
//! form itself, for a repository address and a token, is a quiet link.
//!
//! Tab and the arrows walk the buttons, Enter or Space presses the one
//! the keyboard is on, Escape closes, except while work runs.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use gasp_config::settings::SyncSettings;
use gasp_sync::Token;
use gasp_sync::github::{GitHub, GitHubError, HttpClient, PollState, Repository, SignIn};
use gpui::{
    App, AppContext, AsyncWindowContext, Context, DismissEvent, EventEmitter, FocusHandle,
    Focusable, KeyDownEvent, Task, WeakEntity, Window,
};

use super::github_client::{client_id, github_client};
use super::icloud::{ICloudReadiness, readiness, shown_location, shown_path};
use super::setup::{SetupDone, SetupJob, SetupPhase, finish};
use crate::notices::Notice;
use crate::theme::SettingsTheme;
use crate::workspace::Workspace;

/// Where the dialog opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartAt {
    /// The choice between iCloud and GitHub.
    Choice,
    /// Straight into moving to iCloud, as the tour's "Sync with iCloud".
    ICloud,
    /// Straight into signing in, as the tour's "Use GitHub instead".
    GitHub,
}

/// What a button does. The keyboard walks them in the order a stage
/// lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartAction {
    ICloud,
    GitHub,
    SignUp,
    Advanced,
    Cancel,
    Back,
    Move,
    CopyAndOpen,
    MakeRepository,
    UseRepository(usize),
    TryAgain,
    Resolve,
    Done,
}

/// What the dialog asks its host to do once it has closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncStartEvent {
    /// Open the form for a repository address and token.
    OpenForm,
    RunCommand(String),
}

/// What to try again after something went wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retry {
    ICloud,
    GitHub,
}

/// Where the dialog is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    Choosing,
    /// Moving to iCloud, waiting for a yes. `notes_there` are notes the
    /// Gasp folder already has, from another device.
    ICloudConfirm {
        folder: PathBuf,
        notes_there: usize,
    },
    ICloudMoving {
        folder: PathBuf,
        notes_there: usize,
    },
    /// Asking GitHub for a code.
    GettingCode,
    /// Waiting for the person to type `code` on GitHub.
    Code {
        code: String,
        page: String,
        opened: bool,
    },
    /// Signed in; choosing where the notes go.
    Picking(Picking),
    /// Making the repository, or setting sync up with it.
    SettingUp {
        repository: String,
    },
    Done(SetupDone),
    Problem {
        message: String,
        retry: Retry,
    },
}

/// The person's account and repositories, once signed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picking {
    pub login: String,
    pub repositories: Vec<Repository>,
    /// What a new repository would be called.
    pub new_name: String,
    token: Token,
}

/// The dialog.
pub struct SyncStart {
    pub(super) workspace: WeakEntity<Workspace>,
    pub(super) root: PathBuf,
    pub(super) settings: SyncSettings,
    pub(super) stage: Stage,
    /// The button the keyboard is on, in [`Self::actions`].
    pub(super) focused: usize,
    pub(super) readiness: ICloudReadiness,
    pub(super) github_ready: bool,
    pub(super) focus_handle: FocusHandle,
    pub(super) style: SettingsTheme,
    github: Arc<dyn HttpClient>,
    /// The poll for the code, or other GitHub work under way. Dropping it
    /// stops it.
    work: Option<Task<()>>,
}

impl EventEmitter<DismissEvent> for SyncStart {}
impl EventEmitter<SyncStartEvent> for SyncStart {}

impl Focusable for SyncStart {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SyncStart {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        root: PathBuf,
        settings: SyncSettings,
        start: StartAt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Starting touches the workspace, which may be mid-update opening
        // this dialog, so it waits for that to finish.
        let first = match start {
            StartAt::Choice => None,
            StartAt::ICloud => Some(StartAction::ICloud),
            StartAt::GitHub => Some(StartAction::GitHub),
        };
        if let Some(action) = first {
            cx.defer_in(window, move |dialog, window, cx| {
                dialog.press(action, window, cx)
            });
        }
        SyncStart {
            workspace,
            readiness: readiness(&root, cx),
            root,
            settings,
            stage: Stage::Choosing,
            focused: 0,
            github_ready: client_id(cx).is_some(),
            focus_handle: cx.focus_handle(),
            style: crate::ui::settings_theme(cx),
            github: github_client(cx),
            work: None,
        }
    }

    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    /// Whether work runs that nothing should interrupt.
    fn is_busy(&self) -> bool {
        matches!(
            self.stage,
            Stage::ICloudMoving { .. } | Stage::SettingUp { .. }
        )
    }

    fn go(&mut self, stage: Stage, cx: &mut Context<Self>) {
        self.stage = stage;
        self.focused = self.primary_index();
        cx.notify();
    }

    fn fail(&mut self, message: String, retry: Retry, cx: &mut Context<Self>) {
        self.work = None;
        self.go(Stage::Problem { message, retry }, cx);
    }

    // ---- Buttons ----

    /// The buttons the stage shows, in the order Tab walks them.
    pub fn actions(&self) -> Vec<StartAction> {
        use StartAction::*;
        match &self.stage {
            Stage::Choosing => self.choosing_actions(),
            Stage::ICloudConfirm { .. } => vec![Back, Move],
            Stage::GettingCode => vec![Cancel],
            Stage::Code { .. } => vec![CopyAndOpen, Cancel],
            Stage::Picking(picking) => std::iter::once(MakeRepository)
                .chain((0..picking.repositories.len()).map(UseRepository))
                .chain([Cancel])
                .collect(),
            Stage::Done(done) if done.report.waiting.is_empty() => vec![Done],
            Stage::Done(_) => vec![Resolve, Done],
            Stage::Problem { .. } => vec![Back, TryAgain],
            Stage::ICloudMoving { .. } | Stage::SettingUp { .. } => Vec::new(),
        }
    }

    fn choosing_actions(&self) -> Vec<StartAction> {
        let icloud = matches!(self.readiness, ICloudReadiness::Ready { .. });
        let mut actions = Vec::new();
        actions.extend(icloud.then_some(StartAction::ICloud));
        actions.extend(self.github_ready.then_some(StartAction::GitHub));
        actions.extend([
            StartAction::SignUp,
            StartAction::Advanced,
            StartAction::Cancel,
        ]);
        actions
    }

    /// Where the keyboard starts on a stage: its main button.
    fn primary_index(&self) -> usize {
        use StartAction::*;
        let actions = self.actions();
        let primary = [
            ICloud,
            GitHub,
            Move,
            CopyAndOpen,
            MakeRepository,
            Done,
            TryAgain,
        ];
        actions
            .iter()
            .position(|action| primary.contains(action))
            .unwrap_or(0)
    }

    /// Whether the keyboard is on `action`, with its ring showing.
    pub(super) fn ringed(&self, action: StartAction, window: &Window, cx: &App) -> bool {
        let on_it = self.actions().get(self.focused) == Some(&action);
        crate::ui::focus_visible::ring(on_it && self.focus_handle.is_focused(window), cx)
    }

    pub fn press(&mut self, action: StartAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            StartAction::ICloud => self.start_icloud(window, cx),
            StartAction::GitHub => self.start_github(window, cx),
            StartAction::SignUp => crate::sandbox::open_url(gasp_sync::github::SIGN_UP_PAGE, cx),
            StartAction::Advanced => self.close_with(SyncStartEvent::OpenForm, cx),
            StartAction::Cancel => self.cancel(cx),
            StartAction::Back => self.back(cx),
            StartAction::Move => self.move_to_icloud(window, cx),
            StartAction::CopyAndOpen => self.copy_and_open(cx),
            StartAction::MakeRepository => self.make_repository(window, cx),
            StartAction::UseRepository(index) => self.use_repository(index, window, cx),
            StartAction::TryAgain => self.try_again(window, cx),
            StartAction::Resolve => self.close_with(
                SyncStartEvent::RunCommand("sync.resolve-conflicts".to_owned()),
                cx,
            ),
            StartAction::Done => cx.emit(DismissEvent),
        }
    }

    fn close_with(&mut self, event: SyncStartEvent, cx: &mut Context<Self>) {
        cx.emit(event);
        cx.emit(DismissEvent);
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.is_busy() {
            self.work = None;
            cx.emit(DismissEvent);
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.work = None;
        self.readiness = readiness(&self.root, cx);
        self.go(Stage::Choosing, cx);
    }

    fn try_again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.stage {
            Stage::Problem {
                retry: Retry::ICloud,
                ..
            } => self.start_icloud(window, cx),
            _ => self.start_github(window, cx),
        }
    }

    // ---- iCloud ----

    /// Moves at once when the vault has no notes to lose track of, and
    /// otherwise shows where they're going first.
    fn start_icloud(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.readiness = readiness(&self.root, cx);
        let ICloudReadiness::Ready { folder, notes } = self.readiness.clone() else {
            self.go(Stage::Choosing, cx);
            return;
        };
        let stage = Stage::ICloudConfirm {
            folder,
            notes_there: notes,
        };
        self.go(stage, cx);
        if super::icloud::count_notes(&self.root) == 0 {
            self.move_to_icloud(window, cx);
        }
    }

    fn move_to_icloud(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Stage::ICloudConfirm {
            folder,
            notes_there,
        } = self.stage.clone()
        else {
            return;
        };
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| workspace.save_all(cx));
        }
        self.go(
            Stage::ICloudMoving {
                folder: folder.clone(),
                notes_there,
            },
            cx,
        );
        let from = self.root.clone();
        let device = crate::edit_time::device_name();
        let target = folder.clone();
        let old = super::icloud::old_folder_with_notes(cx);
        let moving = cx.background_spawn(async move {
            super::icloud::move_into_icloud(&from, &target, old.as_deref(), &device)
        });
        self.work = Some(cx.spawn_in(window, async move |this, cx| {
            let moved = moving.await;
            this.update_in(cx, |this, window, cx| match moved {
                Ok(report) => this.moved(folder, report, window, cx),
                Err(error) => this.fail(error.to_string(), Retry::ICloud, cx),
            })
            .ok();
        }));
    }

    /// Opens the vault from iCloud in this window, and says where the
    /// original still is.
    fn moved(
        &mut self,
        folder: PathBuf,
        report: gasp_sync::icloud::MoveReport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let notice = moved_message(&report, &shown_location(&folder), &shown_path(&self.root));
        cx.emit(DismissEvent);
        window.defer(cx, move |window, cx| {
            crate::tour::open_here(folder, crate::tour::AfterOpening::Nothing, window, cx);
            crate::notices::show(Notice::offer(notice), cx);
        });
    }

    // ---- GitHub ----

    fn start_github(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = client_id(cx) else {
            self.fail(GitHubError::NotConfigured.to_string(), Retry::GitHub, cx);
            return;
        };
        self.go(Stage::GettingCode, cx);
        let http = self.github.clone();
        let asking = cx.background_spawn(async move { SignIn::start(&*http, &client) });
        self.work = Some(cx.spawn_in(window, async move |this, cx| {
            let started = asking.await;
            let Some(sign_in) = this
                .update(cx, |this, cx| this.show_code(started, cx))
                .ok()
                .flatten()
            else {
                return;
            };
            wait_for_approval(this, sign_in, cx).await;
        }));
    }

    /// Shows the code GitHub handed out, or why it didn't.
    fn show_code(
        &mut self,
        started: Result<SignIn, GitHubError>,
        cx: &mut Context<Self>,
    ) -> Option<Arc<Mutex<SignIn>>> {
        let sign_in = match started {
            Ok(sign_in) => sign_in,
            Err(error) => {
                self.fail(error.to_string(), Retry::GitHub, cx);
                return None;
            }
        };
        let code = sign_in.code();
        let stage = Stage::Code {
            code: code.user_code.clone(),
            page: code.verification_uri.clone(),
            opened: false,
        };
        self.go(stage, cx);
        Some(Arc::new(Mutex::new(sign_in)))
    }

    fn copy_and_open(&mut self, cx: &mut Context<Self>) {
        let Stage::Code { code, page, opened } = &mut self.stage else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(code.clone()));
        crate::sandbox::open_url(page, cx);
        *opened = true;
        cx.notify();
    }

    /// Signed in: reads who they are and what they have.
    fn signed_in(&mut self, token: Token, window: &mut Window, cx: &mut Context<Self>) {
        let http = self.github.clone();
        let reading = cx.background_spawn(async move {
            let github = GitHub::new(&*http, &token);
            let account = github.account()?;
            let repositories = github.repositories()?;
            Ok::<_, GitHubError>((account, repositories, token))
        });
        self.work = Some(cx.spawn_in(window, async move |this, cx| {
            let read = reading.await;
            this.update(cx, |this, cx| match read {
                Ok((account, repositories, token)) => {
                    let taken: Vec<String> =
                        repositories.iter().map(|repo| repo.name.clone()).collect();
                    let picking = Picking {
                        login: account.login,
                        new_name: gasp_sync::github::notes_repository_name(&taken),
                        repositories,
                        token,
                    };
                    this.go(Stage::Picking(picking), cx);
                }
                Err(error) => this.fail(error.to_string(), Retry::GitHub, cx),
            })
            .ok();
        }));
    }

    fn make_repository(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Stage::Picking(picking) = self.stage.clone() else {
            return;
        };
        self.go(
            Stage::SettingUp {
                repository: format!("{}/{}", picking.login, picking.new_name),
            },
            cx,
        );
        let http = self.github.clone();
        let token = picking.token.clone();
        let making = cx.background_spawn(async move {
            GitHub::new(&*http, &token).make_notes_repository(&picking.repositories)
        });
        let branch = self.settings.branch.clone();
        self.work = Some(cx.spawn_in(window, async move |this, cx| {
            let made = making.await;
            this.update_in(cx, |this, window, cx| match made {
                Ok(repository) => this.set_up(repository, branch, picking.token, window, cx),
                Err(error) => this.fail(error.to_string(), Retry::GitHub, cx),
            })
            .ok();
        }));
    }

    fn use_repository(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Stage::Picking(picking) = self.stage.clone() else {
            return;
        };
        let Some(repository) = picking.repositories.get(index).cloned() else {
            return;
        };
        let branch = match repository.default_branch.trim() {
            "" => self.settings.branch.clone(),
            branch => branch.to_owned(),
        };
        self.set_up(repository, branch, picking.token, window, cx);
    }

    /// Sets sync up in place with `repository`, as the form does.
    fn set_up(
        &mut self,
        repository: Repository,
        branch: String,
        token: Token,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let job = SetupJob::new(
            self.root.clone(),
            repository.full_name.clone(),
            repository.clone_url.clone(),
            branch,
            Some(token),
            &self.settings,
        );
        let job = match job {
            Ok(job) => job,
            Err(message) => return self.fail(message, Retry::GitHub, cx),
        };
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| workspace.save_all(cx));
        }
        self.go(
            Stage::SettingUp {
                repository: repository.full_name,
            },
            cx,
        );
        let device = crate::edit_time::device_name();
        let running = cx.background_spawn(async move { job.run(&device) });
        let workspace = self.workspace.clone();
        self.work = Some(cx.spawn_in(window, async move |this, cx| {
            let (job, result) = running.await;
            let phase = finish(job, result, &workspace, cx);
            this.update(cx, |this, cx| this.set_up_ended(phase, cx))
                .ok();
        }));
    }

    fn set_up_ended(&mut self, phase: SetupPhase, cx: &mut Context<Self>) {
        self.work = None;
        match phase {
            SetupPhase::Done(done) => self.go(Stage::Done(done), cx),
            SetupPhase::Refused { message, .. } => self.fail(message, Retry::GitHub, cx),
            _ => {}
        }
    }

    // ---- Keys ----

    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt {
            return;
        }
        let count = self.actions().len().max(1);
        match keystroke.key.as_str() {
            "tab" if modifiers.shift => self.focused = (self.focused + count - 1) % count,
            "up" | "left" => self.focused = (self.focused + count - 1) % count,
            "tab" | "down" | "right" => self.focused = (self.focused + 1) % count,
            "enter" | "space" => self.press_focused(window, cx),
            "escape" => self.cancel(cx),
            _ => return,
        }
        cx.notify();
        cx.stop_propagation();
    }

    fn press_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(action) = self.actions().get(self.focused).copied() {
            self.press(action, window, cx);
        }
    }
}

/// Polls GitHub until the person approves the code, then reads their
/// account. Stops when the dialog closes or starts over, which drops the
/// task this runs in.
async fn wait_for_approval(
    this: WeakEntity<SyncStart>,
    sign_in: Arc<Mutex<SignIn>>,
    cx: &mut AsyncWindowContext,
) {
    let started = Instant::now();
    let mut wait = lock(&sign_in).first_wait();
    let Some(http) = this.read_with(cx, |this, _| this.github.clone()).ok() else {
        return;
    };
    loop {
        cx.background_executor().timer(wait).await;
        let (sign_in, http) = (sign_in.clone(), http.clone());
        let state = cx
            .background_spawn(async move { lock(&sign_in).poll_once(&*http, started.elapsed()) })
            .await;
        match state {
            PollState::Waiting { next_poll } => wait = next_poll,
            PollState::SignedIn(token) => {
                this.update_in(cx, |this, window, cx| this.signed_in(token, window, cx))
                    .ok();
                return;
            }
            PollState::Failed(failure) => {
                this.update(cx, |this, cx| {
                    this.fail(failure.sentence(), Retry::GitHub, cx)
                })
                .ok();
                return;
            }
        }
    }
}

fn lock(sign_in: &Mutex<SignIn>) -> std::sync::MutexGuard<'_, SignIn> {
    sign_in.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the notice says once the vault is in iCloud.
pub fn moved_message(
    report: &gasp_sync::icloud::MoveReport,
    location: &str,
    original: &str,
) -> String {
    let mut message = format!("Your notes are in {location} now, and iCloud keeps them in step.");
    match report.kept_beside.len() {
        0 => {}
        1 => message
            .push_str(" One note was already there with other text, so both versions are kept."),
        many => message.push_str(&format!(
            " {many} notes were already there with other text, so both versions of each are kept."
        )),
    }
    if report.notes() > 0 {
        message.push_str(&format!(
            " The old folder is still at {original}; delete it once you’ve checked everything’s here."
        ));
    }
    message
}

impl gpui::Render for SyncStart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        super::start_view::render(self, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gasp_sync::icloud::MoveReport;

    use super::*;

    #[test]
    fn the_notice_says_where_the_notes_went_and_where_the_old_folder_is() {
        let report = MoveReport {
            copied: vec![PathBuf::from("Plan.md")],
            kept_beside: vec![(PathBuf::from("A.md"), PathBuf::from("A (from mac).md"))],
            ..MoveReport::default()
        };
        let message = moved_message(&report, "iCloud Drive › Gasp", "~/Documents/Notes");
        assert_eq!(
            message,
            "Your notes are in iCloud Drive › Gasp now, and iCloud keeps them in step. One note was already there with other text, so both versions are kept. The old folder is still at ~/Documents/Notes; delete it once you’ve checked everything’s here."
        );
        let empty = moved_message(&MoveReport::default(), "iCloud Drive › Gasp", "~/Notes");
        assert!(!empty.contains("old folder"));
    }
}
