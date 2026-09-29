//! Sync, over two steps. The first shows how it works: the Mac and the
//! iPhone keep the vault in a private GitHub repository, drawn as what a
//! repository is, a history of commits. Each note travels from a device
//! to the end of that history and on to the other device. The second
//! shows the one question macOS will ask, the Keychain prompt, with the
//! button to press ringed. Setting up opens the vault with sync's form;
//! "Not now" on either step just opens it.

use std::time::Instant;

use gpui::{AnyElement, Context, Div, Pixels, Window, div, prelude::*, px, relative};

use super::history::{self, Line, Moment, Origin};
use super::sketch::{heading_bar, keychain_prompt, laptop, paper, phone};
use super::{AfterOpening, Step, Tour, explanation, heading, stage};
use crate::icons::{IconName, icon};
use crate::theme::UiTheme;
use crate::ui::{Button, ui_theme};

/// Set up sync, then not now, on the Keychain step.
pub const CHOICES: usize = 2;

/// The gap between commits on the repository's card.
const COMMIT_GAP: f32 = 40.;

impl Tour {
    pub fn take_sync_choice(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let after = if index == 0 {
            AfterOpening::SetUpSync
        } else {
            AfterOpening::Nothing
        };
        self.open_chosen_vault(after, window, cx);
    }

    /// Opens the vault chosen on the vault step, then does `after`.
    pub fn open_chosen_vault(
        &mut self,
        after: AfterOpening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.clone() else {
            self.skip_to_vault(window, cx);
            return;
        };
        self.open_vault(vault, after, window, cx);
    }
}

/// The buttons where Continue is on other steps: Not now and Continue on
/// the first sync step, Not now and Set up sync on the Keychain step.
pub fn walk_buttons(tour: &Tour, window: &Window, cx: &mut Context<Tour>) -> AnyElement {
    let ui = ui_theme(cx);
    let not_now = |index: usize| {
        Button::new("tour-sync-later", "Not now").on_click(
            cx.listener(move |tour, _, window, cx| tour.take_sync_choice(index, window, cx)),
        )
    };
    let buttons = if tour.step() == Step::Sync {
        let onward = Button::new("tour-continue", "Continue")
            .primary()
            .on_click(cx.listener(|tour, _, window, cx| tour.advance(window, cx)));
        (not_now(1), onward)
    } else {
        let focused = tour.focus_handle.is_focused(window);
        let ringed = |index: usize, cx: &gpui::App| {
            crate::ui::focus_visible::ring(focused && tour.selected == index, cx)
        };
        let set_up = Button::new("tour-set-up-sync", "Set up sync")
            .primary()
            .focused(ringed(0, cx))
            .on_click(cx.listener(|tour, _, window, cx| tour.take_sync_choice(0, window, cx)));
        (not_now(1).focused(ringed(1, cx)), set_up)
    };
    div()
        .flex()
        .flex_row()
        .gap(ui.space_md)
        .child(buttons.0)
        .child(buttons.1)
        .into_any_element()
}

/// How sync works: the devices, the repository and a note going round.
pub fn render_how(tour: &Tour, now: Instant, cx: &mut Context<Tour>) -> AnyElement {
    let ui = ui_theme(cx);
    let seconds = now.saturating_duration_since(tour.opened).as_secs_f32();
    let moment = history::moment(seconds);
    let content = div()
        .flex()
        .flex_col()
        .gap(ui.space_md)
        .child(heading(Step::Sync, &ui))
        .child(explanation(
            "Each device saves its changes to a private GitHub repository and picks up the others'.",
            &ui,
        ))
        .child(div().h(ui.space_xl * 3.))
        .child(diagram(&moment, &ui));
    stage(px(900.), &ui, content)
}

/// The Keychain's question, drawn large, before macOS asks it.
pub fn render_keychain(cx: &mut Context<Tour>) -> AnyElement {
    let ui = ui_theme(cx);
    let content = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(ui.space_md)
        .child(heading(Step::Keychain, &ui))
        .child(div().h(ui.space_xl * 2.))
        .child(keychain_prompt(px(420.), &ui))
        .child(div().h(ui.space_xl * 2.))
        .child(
            div().max_w(px(520.)).text_center().child(explanation(
                "Gasp keeps your GitHub key in the Keychain. When macOS asks, choose Always Allow and it won't ask again.",
                &ui,
            )),
        );
    stage(px(640.), &ui, content)
}

/// The Mac, the repository and the iPhone, joined by the lines notes
/// travel along.
fn diagram(moment: &Moment, ui: &UiTheme) -> impl IntoElement {
    let on = |line: Line| moment.travelling.filter(|note| note.line == line);
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .child(labelled(laptop(px(190.), ui), "This Mac", ui))
        .child(link(on(Line::Mac), ui))
        .child(labelled(
            repository(moment, ui),
            "Private GitHub repository",
            ui,
        ))
        .child(link(on(Line::Phone), ui))
        .child(labelled(phone(px(64.), ui), "iPhone", ui))
}

/// How tall the room each drawing stands in is, so their names line up.
const DRAWING_HEIGHT: f32 = 160.;

fn labelled(drawing: Div, name: &'static str, ui: &UiTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(ui.space_lg)
        .child(
            div()
                .h(px(DRAWING_HEIGHT))
                .flex()
                .items_center()
                .child(drawing),
        )
        .child(
            div()
                .h(label_height(ui))
                .text_color(ui.text_muted)
                .child(name),
        )
}

fn label_height(ui: &UiTheme) -> Pixels {
    ui.font_size * 1.4
}

/// A repository's card: its name, and its history as a line of commits,
/// the newest at the right. Commits from the Mac are filled, the phone's
/// are open rings.
fn repository(moment: &Moment, ui: &UiTheme) -> Div {
    let gap = px(COMMIT_GAP);
    let width = gap * history::SHOWN_COMMITS as f32;
    let last = moment.commits.len().saturating_sub(1);
    let commits = moment.commits.iter().enumerate().map(|(slot, origin)| {
        let shown = if slot == last {
            moment.newest_shown
        } else {
            1.
        };
        let left = gap * (slot as f32 + 0.5 + moment.slide);
        commit(*origin, ui)
            .absolute()
            .left(left - commit_size(ui) / 2.)
            .top(-commit_size(ui) / 2. + ui.tour.sea_line / 4.)
            .opacity(shown)
    });
    let line = div()
        .relative()
        .w(width)
        .h(ui.tour.sea_line / 2.)
        .bg(ui.fill_strong)
        .children(commits);
    paper(ui)
        .p(ui.space_lg)
        .gap(ui.space_xl)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(ui.space_md)
                .child(
                    icon(IconName::GitMerge)
                        .size(ui.icon_size)
                        .text_color(ui.icon),
                )
                .child(heading_bar(width * 0.4, ui)),
        )
        .child(div().py(ui.space_md).child(line))
}

fn commit_size(ui: &UiTheme) -> Pixels {
    ui.tour.sea_line * 2.2
}

/// A commit, or a note on its way: filled from the Mac, a ring from the
/// phone.
fn commit(origin: Origin, ui: &UiTheme) -> Div {
    let dot = div().size(commit_size(ui)).rounded_full();
    match origin {
        Origin::Mac => dot.bg(ui.text_muted),
        Origin::Phone => dot
            .bg(ui.note_background)
            .shadow(vec![ui.ring(ui.text_muted)]),
    }
}

/// A line between a device and the repository, with the note on it.
fn link(note: Option<history::Travelling>, ui: &UiTheme) -> impl IntoElement {
    let size = commit_size(ui);
    let dot = note.map(|note| {
        commit(note.origin, ui)
            .absolute()
            .left(relative(note.along))
            .ml(-size / 2.)
            .top(-size / 2. + ui.tour.sea_line / 4.)
    });
    div()
        .relative()
        .flex_1()
        .min_w(px(48.))
        .max_w(px(110.))
        .h(ui.tour.sea_line / 2.)
        .mx(ui.space_lg)
        .mb(label_height(ui) + ui.space_lg)
        .bg(ui.fill_strong)
        .children(dot)
}
