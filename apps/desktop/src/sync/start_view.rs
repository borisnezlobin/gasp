//! How "Set up sync" draws each stage. The choice puts iCloud first and
//! biggest, on a card of its own; GitHub sits below it, quieter, with the
//! way to make an account. Lines that report progress or a problem keep
//! their room whether or not they say anything, so nothing moves.

use std::path::Path;

use gpui::{
    AnyElement, ClickEvent, Context, Div, IntoElement, Stateful, Window, div, prelude::*, px,
    relative,
};

use super::icloud::{ICloudReadiness, shown_location, shown_path};
use super::setup::done_sentences;
use super::start::{Picking, Retry, Stage, StartAction, SyncStart};
use crate::icons::{IconName, icon};
use crate::settings_view::controls::{button, icon_label_button, inert_button};
use crate::theme::SettingsTheme;
use crate::ui::Selectable;

/// The code's placeholder while GitHub hands one out, the same width.
const CODE_PLACEHOLDER: &str = "····-····";

pub fn render(
    dialog: &mut SyncStart,
    window: &mut Window,
    cx: &mut Context<SyncStart>,
) -> impl IntoElement {
    dialog.style = crate::ui::settings_theme(cx);
    let style = dialog.style.clone();
    let ui = crate::ui::ui_theme(cx);
    let children = stage_children(dialog, window, cx);
    div()
        .id("sync-start")
        .selector(|| "sync-start".to_owned())
        .key_context("SyncStart")
        .track_focus(&dialog.focus_handle)
        .on_key_down(cx.listener(SyncStart::on_key_down))
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

fn stage_children(
    dialog: &SyncStart,
    window: &Window,
    cx: &mut Context<SyncStart>,
) -> Vec<AnyElement> {
    let view = View { dialog, window };
    match &dialog.stage {
        Stage::Choosing => view.choosing(cx),
        Stage::ICloudConfirm {
            folder,
            notes_there,
        } => view.icloud(folder, *notes_there, false, cx),
        Stage::ICloudMoving {
            folder,
            notes_there,
        } => view.icloud(folder, *notes_there, true, cx),
        Stage::GettingCode => view.code(None, false, cx),
        Stage::Code { code, opened, .. } => view.code(Some(code), *opened, cx),
        Stage::Picking(picking) => view.picking(picking, cx),
        Stage::SettingUp { repository } => view.setting_up(repository),
        Stage::Done(done) => view.done(done, cx),
        Stage::Problem { message, retry } => view.problem(message, *retry, cx),
    }
}

struct View<'a> {
    dialog: &'a SyncStart,
    window: &'a Window,
}

impl View<'_> {
    fn style(&self) -> &SettingsTheme {
        &self.dialog.style
    }

    fn ringed(&self, action: StartAction, cx: &gpui::App) -> bool {
        self.dialog.ringed(action, self.window, cx)
    }

    /// A button that runs `action`.
    fn button(
        &self,
        action: StartAction,
        label: &'static str,
        primary: bool,
        cx: &mut Context<SyncStart>,
    ) -> Stateful<Div> {
        let name = selector_name(action);
        button(
            gpui::SharedString::from(name.clone()),
            label,
            primary,
            self.ringed(action, cx),
            self.style(),
        )
        .selector(move || name.clone())
        .on_click(
            cx.listener(move |dialog, _: &ClickEvent, window, cx| dialog.press(action, window, cx)),
        )
    }

    fn header(&self, title: &'static str, intro: Option<String>) -> AnyElement {
        let style = self.style();
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

    fn footer(&self, left: Option<AnyElement>, right: Vec<AnyElement>) -> AnyElement {
        let style = self.style();
        div()
            .flex()
            .items_center()
            .gap(style.control_gap)
            .child(div().flex_1().min_w_0().children(left))
            .children(right)
            .into_any_element()
    }

    /// A line that keeps two lines' room: what's happening, with a
    /// turning mark, or nothing yet.
    fn status(&self, working: bool, text: String, cx: &mut gpui::App) -> AnyElement {
        let style = self.style();
        let ui = crate::ui::ui_theme(cx);
        let lines = style.small_text_size * style.line_height_factor * 2.;
        let mark = working
            .then(|| super::indicator::turning_icon(style.small_icon_size, style.text_muted, &ui));
        div()
            .selector(|| "sync-start-status".to_owned())
            .h(lines)
            .flex()
            .items_start()
            .gap(style.gap_sm * 1.5)
            .text_size(style.small_text_size)
            .text_color(style.text_muted)
            .children(mark.map(|mark| div().flex_none().mt(style.gap_xs).child(mark)))
            .child(div().flex_1().min_w_0().child(text))
            .into_any_element()
    }

    // ---- The choice ----

    fn choosing(&self, cx: &mut Context<SyncStart>) -> Vec<AnyElement> {
        let advanced = self.link(
            StartAction::Advanced,
            "Use a repository address and token",
            None,
            cx,
        );
        let cancel = self.button(StartAction::Cancel, "Cancel", false, cx);
        vec![
            self.header("Sync this vault", None),
            self.icloud_choice(cx),
            self.github_choice(cx),
            self.footer(Some(advanced), vec![cancel.into_any_element()]),
        ]
    }

    fn icloud_choice(&self, cx: &mut Context<SyncStart>) -> AnyElement {
        let detail = match &self.dialog.readiness {
            ICloudReadiness::Ready { .. } => {
                "Your notes go in iCloud Drive, in a folder called Gasp. Your iPhone and other Macs pick them up from there."
            }
            ICloudReadiness::NoDrive => {
                "Turn on iCloud Drive in System Settings, under your Apple Account, then come back here."
            }
            ICloudReadiness::SyncsWithGit => "This vault syncs with git, so it stays on git.",
            ICloudReadiness::AlreadyThere => "This vault is in iCloud already.",
        };
        let enabled = matches!(self.dialog.readiness, ICloudReadiness::Ready { .. });
        let choice = Choice {
            action: StartAction::ICloud,
            icon: IconName::Cloud,
            title: "Sync with iCloud",
            detail,
            enabled,
            dominant: true,
        };
        self.choice(choice, cx)
    }

    fn github_choice(&self, cx: &mut Context<SyncStart>) -> AnyElement {
        let ready = self.dialog.github_ready;
        let detail = if ready {
            "Keeps every version in a private repository. Works on Windows and Linux too."
        } else {
            "GitHub sign-in isn’t available in this build yet. A repository address and token still work."
        };
        let choice = Choice {
            action: StartAction::GitHub,
            icon: IconName::GithubLogo,
            title: "Sign in with GitHub",
            detail,
            enabled: ready,
            dominant: false,
        };
        let sign_up = self.link(
            StartAction::SignUp,
            "No GitHub account? Make one",
            Some(IconName::ArrowSquareOut),
            cx,
        );
        let style = self.style();
        div()
            .flex()
            .flex_col()
            .gap(style.gap_sm)
            .child(self.choice(choice, cx))
            .child(div().pl(choice_indent(style)).child(sign_up))
            .into_any_element()
    }

    /// One way to sync, as a row that's pressed as a whole: its icon, what
    /// it does, and a sentence on what that means.
    fn choice(&self, choice: Choice, cx: &mut Context<SyncStart>) -> AnyElement {
        let style = self.style();
        let focused = self.ringed(choice.action, cx);
        let name = selector_name(choice.action);
        let (icon_size, title_size) = if choice.dominant {
            (style.icon_size * 2., style.text_size * 1.2)
        } else {
            (style.icon_size * 1.4, style.text_size)
        };
        let text = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(style.gap_xs)
            .child(
                div()
                    .text_size(title_size)
                    .font_weight(style.strong_weight)
                    .child(choice.title),
            )
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child(choice.detail),
            );
        let mark = div()
            .flex_none()
            .w(choice_icon_room(style))
            .flex()
            .justify_center()
            .child(icon(choice.icon).size(icon_size).text_color(style.text));
        let row = div()
            .id(gpui::SharedString::from(name.clone()))
            .selector(move || name.clone())
            .flex()
            .items_center()
            .gap(style.control_gap * 1.5)
            .px(style.card_padding_x)
            .py(style.row_padding_y * if choice.dominant { 1.5 } else { 1. })
            .rounded(style.card_radius)
            .when(choice.dominant, |row| {
                row.bg(style.card_background).shadow(vec![style.outline()])
            })
            .when(focused, |row| row.shadow(vec![style.focus()]))
            .child(mark)
            .child(text);
        if !choice.enabled {
            return row.opacity(style.inactive_opacity).into_any_element();
        }
        let action = choice.action;
        let hover = if choice.dominant {
            style.card_hover
        } else {
            style.hover_fill
        };
        row.cursor_pointer()
            .hover(move |row| row.bg(hover))
            .on_click(cx.listener(move |dialog, _: &ClickEvent, window, cx| {
                dialog.press(action, window, cx)
            }))
            .into_any_element()
    }

    /// A quiet text button, with an icon after it when it leaves the app.
    fn link(
        &self,
        action: StartAction,
        label: &'static str,
        trailing: Option<IconName>,
        cx: &mut Context<SyncStart>,
    ) -> AnyElement {
        let style = self.style();
        let name = selector_name(action);
        let focused = self.ringed(action, cx);
        div()
            .id(gpui::SharedString::from(name.clone()))
            .selector(move || name.clone())
            .flex()
            .items_center()
            .gap(style.gap_sm)
            .px(style.gap_sm)
            .py(style.gap_xs)
            .mx(-style.gap_sm)
            .rounded(style.radius)
            .text_size(style.small_text_size)
            .text_color(style.text_muted)
            .cursor_pointer()
            .hover(|link| link.text_color(style.text))
            .when(focused, |link| link.shadow(vec![style.focus()]))
            .child(label)
            .children(trailing.map(|name| {
                icon(name)
                    .size(style.small_icon_size)
                    .text_color(style.text_muted)
            }))
            .on_click(cx.listener(move |dialog, _: &ClickEvent, window, cx| {
                dialog.press(action, window, cx)
            }))
            .into_any_element()
    }

    // ---- iCloud ----

    fn icloud(
        &self,
        folder: &Path,
        notes_there: usize,
        moving: bool,
        cx: &mut Context<SyncStart>,
    ) -> Vec<AnyElement> {
        let joining = (notes_there > 0).then(|| {
            format!(
                "{} already has {}. This vault’s notes join them, and a note in both with different text keeps both versions.",
                shown_location(folder),
                super::state::count(notes_there, "note"),
            )
        });
        let status = if moving {
            "Copying every file and checking each copy…".to_owned()
        } else {
            "Gasp copies every file and checks each copy before it opens the vault from iCloud. This folder stays as it is.".to_owned()
        };
        let buttons = if moving {
            vec![
                inert_button("sync-start-back", "Back", self.style()).into_any_element(),
                inert_button("sync-start-move", "Moving…", self.style()).into_any_element(),
            ]
        } else {
            vec![
                self.button(StartAction::Back, "Back", false, cx)
                    .into_any_element(),
                self.button(StartAction::Move, "Move to iCloud", true, cx)
                    .into_any_element(),
            ]
        };
        vec![
            self.header("Move this vault to iCloud", joining),
            self.journey(folder, moving, cx),
            self.status(moving, status, cx),
            self.footer(None, buttons),
        ]
    }

    /// Where the vault is now, and where it's going.
    fn journey(&self, folder: &Path, moving: bool, cx: &mut gpui::App) -> AnyElement {
        let style = self.style();
        let ui = crate::ui::ui_theme(cx);
        let root = &self.dialog.root;
        let name = root
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let arrow = if moving {
            super::indicator::turning_icon(style.icon_size, style.text_muted, &ui)
        } else {
            icon(IconName::ArrowRight)
                .size(style.icon_size)
                .text_color(style.text_muted)
                .into_any_element()
        };
        div()
            .flex()
            .items_center()
            .gap(style.control_gap)
            .child(self.place(IconName::FolderOpen, name, shown_path(root)))
            .child(div().flex_none().child(arrow))
            .child(self.place(
                IconName::Cloud,
                shown_location(folder),
                "iCloud Drive".to_owned(),
            ))
            .into_any_element()
    }

    fn place(&self, mark: IconName, name: String, location: String) -> AnyElement {
        let style = self.style();
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(style.gap_sm)
            .px(style.card_padding_x)
            .py(style.row_padding_y * 1.5)
            .rounded(style.card_radius)
            .bg(style.card_background)
            .shadow(vec![style.outline()])
            .child(
                icon(mark)
                    .size(style.icon_size * 1.6)
                    .text_color(style.text),
            )
            .child(
                div()
                    .font_weight(style.strong_weight)
                    .truncate()
                    .child(name),
            )
            .child(
                div()
                    .max_w_full()
                    .truncate()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child(location),
            )
            .into_any_element()
    }

    // ---- GitHub ----

    fn code(
        &self,
        code: Option<&String>,
        opened: bool,
        cx: &mut Context<SyncStart>,
    ) -> Vec<AnyElement> {
        let style = self.style();
        let shown = code.map_or(CODE_PLACEHOLDER.to_owned(), Clone::clone);
        let plate = div()
            .selector(|| "sync-start-code".to_owned())
            .flex()
            .justify_center()
            .py(style.row_padding_y * 2.)
            .rounded(style.card_radius)
            .bg(style.card_background)
            .shadow(vec![style.outline()])
            .font_family(style.code_font_family.clone())
            .text_size(style.page_title_size * 1.6)
            .font_weight(style.strong_weight)
            .when(code.is_none(), |plate| plate.text_color(style.text_faint))
            .child(shown);
        let status = match (code, opened) {
            (None, _) => "Asking GitHub for a code…",
            (Some(_), false) => "Waiting for you to approve Gasp on GitHub.",
            (Some(_), true) => {
                "The code is on your clipboard. Waiting for you to approve Gasp on GitHub."
            }
        };
        let open = if code.is_some() {
            self.icon_button(
                StartAction::CopyAndOpen,
                IconName::ArrowSquareOut,
                "Copy code and open GitHub",
                cx,
            )
        } else {
            inert_button(
                "sync-start-copy-and-open",
                "Copy code and open GitHub",
                style,
            )
            .into_any_element()
        };
        let cancel = self.button(StartAction::Cancel, "Cancel", false, cx);
        vec![
            self.header(
                "Sign in with GitHub",
                Some(
                    "Type this code on GitHub’s page to let Gasp keep your notes in a repository."
                        .to_owned(),
                ),
            ),
            plate.into_any_element(),
            self.status(true, status.to_owned(), cx),
            self.footer(None, vec![cancel.into_any_element(), open]),
        ]
    }

    fn icon_button(
        &self,
        action: StartAction,
        mark: IconName,
        label: impl Into<gpui::SharedString>,
        cx: &mut Context<SyncStart>,
    ) -> AnyElement {
        let name = selector_name(action);
        icon_label_button(
            gpui::SharedString::from(name.clone()),
            mark,
            label,
            true,
            self.ringed(action, cx),
            self.style(),
        )
        .selector(move || name.clone())
        .on_click(
            cx.listener(move |dialog, _: &ClickEvent, window, cx| dialog.press(action, window, cx)),
        )
        .into_any_element()
    }

    fn picking(&self, picking: &Picking, cx: &mut Context<SyncStart>) -> Vec<AnyElement> {
        let style = self.style();
        let make = self.icon_button(
            StartAction::MakeRepository,
            IconName::Plus,
            format!("Make a private repository called {}", picking.new_name),
            cx,
        );
        let existing = (!picking.repositories.is_empty()).then(|| self.repositories(picking, cx));
        let keychain = div()
            .text_size(style.small_text_size)
            .text_color(style.text_muted)
            .child("Gasp keeps your GitHub sign-in in the Keychain. If macOS asks, choose Always Allow.");
        let cancel = self.button(StartAction::Cancel, "Cancel", false, cx);
        vec![
            self.header(
                "Where your notes go",
                Some(format!("Signed in to GitHub as {}.", picking.login)),
            ),
            div().flex().child(make).into_any_element(),
            existing.unwrap_or_else(|| div().into_any_element()),
            keychain.into_any_element(),
            self.footer(None, vec![cancel.into_any_element()]),
        ]
    }

    fn repositories(&self, picking: &Picking, cx: &mut Context<SyncStart>) -> AnyElement {
        let style = self.style();
        let rows: Vec<AnyElement> = picking
            .repositories
            .iter()
            .enumerate()
            .map(|(index, repository)| {
                self.repository_row(index, &repository.full_name, repository.private, cx)
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(style.gap_sm)
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child("Or use one you already have"),
            )
            .child(
                div()
                    .id("sync-start-repositories")
                    .max_h(px(200.))
                    .overflow_y_scroll()
                    .p(style.ring_width * 2.)
                    .flex()
                    .flex_col()
                    .children(rows),
            )
            .into_any_element()
    }

    fn repository_row(
        &self,
        index: usize,
        name: &str,
        private: bool,
        cx: &mut Context<SyncStart>,
    ) -> AnyElement {
        let style = self.style();
        let action = StartAction::UseRepository(index);
        let focused = self.ringed(action, cx);
        let lock = div()
            .flex_none()
            .w(style.small_icon_size)
            .children(private.then(|| {
                icon(IconName::LockSimple)
                    .size(style.small_icon_size)
                    .text_color(style.text_muted)
            }));
        div()
            .id(("sync-start-repository", index))
            .selector(move || format!("sync-start-repository-{index}"))
            .flex()
            .items_center()
            .gap(style.gap_sm * 1.5)
            .px(style.gap_sm * 1.5)
            .py(style.list_row_padding_y)
            .rounded(style.radius)
            .cursor_pointer()
            .hover(|row| row.bg(style.hover_fill))
            .when(focused, |row| row.shadow(vec![style.focus()]))
            .child(lock)
            .child(div().flex_1().min_w_0().truncate().child(name.to_owned()))
            .on_click(cx.listener(move |dialog, _: &ClickEvent, window, cx| {
                dialog.press(action, window, cx)
            }))
            .into_any_element()
    }

    fn setting_up(&self, repository: &str) -> Vec<AnyElement> {
        let style = self.style();
        let line = format!(
            "Bringing your notes and {repository} together. A big vault can take a minute."
        );
        vec![
            self.header("Setting up sync", None),
            div()
                .text_color(style.text_muted)
                .child(line)
                .into_any_element(),
        ]
    }

    fn done(&self, done: &super::setup::SetupDone, cx: &mut Context<SyncStart>) -> Vec<AnyElement> {
        let style = self.style();
        let lines = done_sentences(done)
            .into_iter()
            .map(|line| div().child(line).into_any_element());
        let mut buttons = Vec::new();
        if !done.report.waiting.is_empty() {
            buttons.push(
                self.button(StartAction::Resolve, "Resolve conflicts", false, cx)
                    .into_any_element(),
            );
        }
        buttons.push(
            self.button(StartAction::Done, "Done", true, cx)
                .into_any_element(),
        );
        vec![
            self.header("Sync is set up", None),
            div()
                .flex()
                .flex_col()
                .gap(style.control_gap)
                .children(lines)
                .into_any_element(),
            self.footer(None, buttons),
        ]
    }

    fn problem(&self, message: &str, retry: Retry, cx: &mut Context<SyncStart>) -> Vec<AnyElement> {
        let style = self.style();
        let title = match retry {
            Retry::ICloud => "Moving to iCloud stopped",
            Retry::GitHub => "Signing in with GitHub stopped",
        };
        let line = div()
            .flex()
            .items_start()
            .gap(style.gap_sm * 1.5)
            .child(
                div().flex_none().mt(style.gap_xs).child(
                    icon(IconName::WarningCircle)
                        .size(style.small_icon_size)
                        .text_color(style.warning),
                ),
            )
            .child(div().flex_1().min_w_0().child(message.to_owned()));
        let buttons = vec![
            self.button(StartAction::Back, "Back", false, cx)
                .into_any_element(),
            self.button(StartAction::TryAgain, "Try again", true, cx)
                .into_any_element(),
        ];
        vec![
            self.header(title, None),
            line.into_any_element(),
            self.footer(None, buttons),
        ]
    }
}

/// A way to sync on the choice.
struct Choice {
    action: StartAction,
    icon: IconName,
    title: &'static str,
    detail: &'static str,
    enabled: bool,
    dominant: bool,
}

/// The room a choice's icon stands in, so both choices' words line up.
fn choice_icon_room(style: &SettingsTheme) -> gpui::Pixels {
    style.icon_size * 2.
}

/// How far the GitHub choice's words start from its edge, for the line
/// under it to line up with them.
fn choice_indent(style: &SettingsTheme) -> gpui::Pixels {
    style.card_padding_x + choice_icon_room(style) + style.control_gap * 1.5
}

fn selector_name(action: StartAction) -> String {
    let name = match action {
        StartAction::ICloud => "icloud",
        StartAction::GitHub => "github",
        StartAction::SignUp => "sign-up",
        StartAction::Advanced => "advanced",
        StartAction::Cancel => "cancel",
        StartAction::Back => "back",
        StartAction::Move => "move",
        StartAction::CopyAndOpen => "copy-and-open",
        StartAction::MakeRepository => "make-repository",
        StartAction::UseRepository(index) => return format!("sync-start-repository-{index}"),
        StartAction::TryAgain => "try-again",
        StartAction::Resolve => "resolve",
        StartAction::Done => "done",
    };
    format!("sync-start-{name}")
}
