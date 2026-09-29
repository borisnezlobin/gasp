//! How sync works, before anything asks for a password: the Mac and the
//! iPhone each keep the vault in step with a private GitHub repository,
//! with notes moving along the lines between them. Under that, the
//! Keychain prompt macOS will show, with the button to press ringed.
//! Setting up opens the vault with sync's form; "Not now" just opens it.

use std::time::Instant;

use gpui::{AnyElement, Context, Window, div, prelude::*, px, relative};

use super::sketch::{keychain_prompt, laptop, phone, repository};
use super::{AfterOpening, Step, Tour, explanation, heading, stage};
use crate::theme::UiTheme;
use crate::ui::{Button, ui_theme};

/// Set up sync, then not now.
pub const CHOICES: usize = 2;

/// How long a note takes to cross from one device to the next.
const CROSSING_SECONDS: f32 = 2.4;

impl Tour {
    pub fn take_sync_choice(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            self.back(window, cx);
            return;
        };
        let after = if index == 0 {
            AfterOpening::SetUpSync
        } else {
            AfterOpening::Nothing
        };
        self.open_vault(vault, after, window, cx);
    }
}

/// Not now, then Set up sync, where Continue is on the other steps.
pub fn choices(tour: &Tour, window: &Window, cx: &mut Context<Tour>) -> AnyElement {
    let ui = ui_theme(cx);
    let focused = tour.focus_handle.is_focused(window);
    let ringed = |index: usize, cx: &gpui::App| {
        crate::ui::focus_visible::ring(focused && tour.selected == index, cx)
    };
    let set_up = Button::new("tour-set-up-sync", "Set up sync")
        .primary()
        .focused(ringed(0, cx))
        .on_click(cx.listener(|tour, _, window, cx| tour.take_sync_choice(0, window, cx)));
    let not_now = Button::new("tour-sync-later", "Not now")
        .focused(ringed(1, cx))
        .on_click(cx.listener(|tour, _, window, cx| tour.take_sync_choice(1, window, cx)));
    div()
        .flex()
        .flex_row()
        .gap(ui.space_md)
        .child(not_now)
        .child(set_up)
        .into_any_element()
}

pub fn render(tour: &Tour, now: Instant, cx: &mut Context<Tour>) -> AnyElement {
    let ui = ui_theme(cx);
    let flow = now.saturating_duration_since(tour.opened).as_secs_f32() / CROSSING_SECONDS;
    let content = div()
        .flex()
        .flex_col()
        .gap(ui.space_md)
        .child(heading(Step::Sync, &ui))
        .child(explanation(
            "Your vault lives in a private GitHub repository. Each device saves its changes there and picks up the others'. You'll need a GitHub account, and the next screen links to the page where GitHub makes a key for Gasp.",
            &ui,
        ))
        .child(div().h(ui.space_xl))
        .child(diagram(flow.fract(), &ui))
        .child(div().h(ui.space_xl))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(ui.space_xl * 2.)
                .child(keychain_prompt(px(300.), &ui).flex_none())
                .child(div().flex_1().min_w_0().child(explanation(
                    "Gasp keeps that key in your Mac's Keychain. The first time sync runs, macOS asks whether Gasp may use it. Choose Always Allow and it won't ask again.",
                    &ui,
                ))),
        )
;
    stage(px(900.), &ui, content)
}

/// The Mac, the repository and the iPhone, with notes crossing between
/// them `flow` of the way along.
fn diagram(flow: f32, ui: &UiTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .child(labelled(laptop(px(150.), ui), "This Mac", ui))
        .child(link(flow, ui))
        .child(labelled(repository(px(96.), ui), "GitHub", ui))
        .child(link((flow + 0.5).fract(), ui))
        .child(labelled(phone(px(52.), ui), "iPhone", ui))
}

fn labelled(drawing: gpui::Div, name: &'static str, ui: &UiTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(ui.space_md)
        .child(drawing)
        .child(div().text_color(ui.text_muted).child(name))
}

/// A line between two devices, with one note going each way.
fn link(flow: f32, ui: &UiTheme) -> impl IntoElement {
    let size = ui.tour.sea_line * 1.6;
    let dot = |along: f32| {
        div()
            .absolute()
            .left(relative(along))
            .ml(-size / 2.)
            .top(-size / 2. + ui.tour.sea_line / 4.)
            .size(size)
            .rounded_full()
            .bg(ui.text_muted)
    };
    div()
        .relative()
        .flex_1()
        .min_w(px(48.))
        .max_w(px(120.))
        .h(ui.tour.sea_line / 2.)
        .mx(ui.space_md)
        .bg(ui.fill_strong)
        .child(dot(flow))
        .child(dot(1. - flow))
}
