//! Sync, in one step: the Mac and the iPhone share the vault through
//! iCloud, drawn as a cloud between them with each note travelling from
//! a device into it and on to the other. "Sync with iCloud" is the one
//! big button; "Use GitHub instead" and "Not now" sit beside it, quieter.
//! Either way of syncing opens the vault with "Set up sync" on that path,
//! where GitHub's one question from macOS, the Keychain's, is explained.

use std::time::Instant;

use gpui::{AnyElement, Context, Div, Pixels, Window, div, prelude::*, px, relative};

use super::history::{self, Line, Moment, Origin};
use super::sketch::{laptop, phone};
use super::{AfterOpening, Step, Tour, explanation, heading, stage};
use crate::sync::StartAt;
use crate::theme::UiTheme;
use crate::ui::{Button, ui_theme};

/// The step's choices, in the order the keyboard walks them.
const CHOICES: [SyncChoice; 3] = [SyncChoice::ICloud, SyncChoice::GitHub, SyncChoice::NotNow];

/// How many choices the step has.
pub const CHOICE_COUNT: usize = CHOICES.len();

/// The gap between notes in the cloud.
const NOTE_GAP: f32 = 30.;
/// How wide the cloud is drawn.
const CLOUD_WIDTH: f32 = 230.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncChoice {
    ICloud,
    GitHub,
    NotNow,
}

impl SyncChoice {
    fn after(self) -> AfterOpening {
        match self {
            SyncChoice::ICloud => AfterOpening::SetUpSync(StartAt::ICloud),
            SyncChoice::GitHub => AfterOpening::SetUpSync(StartAt::GitHub),
            SyncChoice::NotNow => AfterOpening::Nothing,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SyncChoice::ICloud => "Sync with iCloud",
            SyncChoice::GitHub => "Use GitHub instead",
            SyncChoice::NotNow => "Not now",
        }
    }

    fn id(self) -> &'static str {
        match self {
            SyncChoice::ICloud => "tour-sync-icloud",
            SyncChoice::GitHub => "tour-sync-github",
            SyncChoice::NotNow => "tour-sync-later",
        }
    }
}

impl Tour {
    pub fn take_sync_choice(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let after = CHOICES
            .get(index)
            .map_or(AfterOpening::Nothing, |choice| choice.after());
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

/// The buttons where Continue is on other steps: Not now and GitHub
/// quiet, then iCloud as the main one.
pub fn walk_buttons(tour: &Tour, window: &Window, cx: &mut Context<Tour>) -> AnyElement {
    let ui = ui_theme(cx);
    let focused = tour.focus_handle.is_focused(window);
    let mut buttons: Vec<AnyElement> =
        CHOICES
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let ringed = crate::ui::focus_visible::ring(focused && tour.selected == index, cx);
                let button = Button::new(choice.id(), choice.label()).focused(ringed);
                let button = if *choice == SyncChoice::ICloud {
                    button.primary()
                } else {
                    button.quiet()
                };
                button
                    .on_click(cx.listener(move |tour, _, window, cx| {
                        tour.take_sync_choice(index, window, cx)
                    }))
                    .into_any_element()
            })
            .collect();
    buttons.reverse();
    div()
        .flex()
        .flex_row()
        .gap(ui.space_md)
        .children(buttons)
        .into_any_element()
}

/// How sync works: the devices, iCloud and a note going round.
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
            "iCloud keeps your notes the same on your Mac and iPhone, with nothing to sign up for.",
            &ui,
        ))
        .child(div().h(ui.space_xl * 3.))
        .child(diagram(&moment, &ui));
    stage(px(900.), &ui, content)
}

/// The Mac, iCloud and the iPhone, joined by the lines notes travel along.
fn diagram(moment: &Moment, ui: &UiTheme) -> impl IntoElement {
    let on = |line: Line| moment.travelling.filter(|note| note.line == line);
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .child(labelled(laptop(px(190.), ui), "This Mac", ui))
        .child(link(on(Line::Mac), ui))
        .child(labelled(cloud(moment, ui), "iCloud Drive", ui))
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

/// A cloud on the note's paper, holding the notes that reached it, the
/// newest at the right. Its outline is three shapes drawn twice: once
/// with the paper's shadow, then again without, over the seams.
fn cloud(moment: &Moment, ui: &UiTheme) -> Div {
    let width = px(CLOUD_WIDTH);
    let height = width * 0.62;
    let lobes = |shadowed: bool| {
        cloud_lobes(width, height)
            .into_iter()
            .map(move |(left, top, w, h)| {
                div()
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(w)
                    .h(h)
                    .rounded_full()
                    .bg(ui.note_background)
                    .when(shadowed, |lobe| lobe.shadow(ui.menu_shadows()))
            })
    };
    div()
        .relative()
        .w(width)
        .h(height)
        .children(lobes(true))
        .children(lobes(false))
        .child(held_notes(moment, width, height, ui))
}

/// The cloud's shapes as (left, top, width, height): a long base and two
/// round tops, the right one bigger.
fn cloud_lobes(width: Pixels, height: Pixels) -> [(Pixels, Pixels, Pixels, Pixels); 3] {
    [
        (width * 0.04, height * 0.44, width * 0.92, height * 0.5),
        (width * 0.14, height * 0.24, width * 0.4, width * 0.4),
        (width * 0.38, height * 0.02, width * 0.5, width * 0.5),
    ]
}

/// The notes in the cloud, as dots that slide left as a new one lands.
fn held_notes(moment: &Moment, width: Pixels, height: Pixels, ui: &UiTheme) -> impl IntoElement {
    let gap = px(NOTE_GAP);
    let row = gap * history::SHOWN_COMMITS as f32;
    let last = moment.commits.len().saturating_sub(1);
    let notes = moment.commits.iter().enumerate().map(|(slot, origin)| {
        let shown = if slot == last {
            moment.newest_shown
        } else {
            1.
        };
        let left = gap * (slot as f32 + 0.5 + moment.slide);
        note_dot(*origin, ui)
            .absolute()
            .left(left - dot_size(ui) / 2.)
            .top(-dot_size(ui) / 2.)
            .opacity(shown)
    });
    div()
        .absolute()
        .left((width - row) / 2.)
        .top(height * 0.66)
        .w(row)
        .h(px(0.))
        .children(notes)
}

fn dot_size(ui: &UiTheme) -> Pixels {
    ui.tour.sea_line * 2.2
}

/// A note, in the cloud or on its way: filled from the Mac, a ring from
/// the phone.
fn note_dot(origin: Origin, ui: &UiTheme) -> Div {
    let dot = div().size(dot_size(ui)).rounded_full();
    match origin {
        Origin::Mac => dot.bg(ui.text_muted),
        Origin::Phone => dot
            .bg(ui.note_background)
            .shadow(vec![ui.ring(ui.text_muted)]),
    }
}

/// A line between a device and iCloud, with the note on it.
fn link(note: Option<history::Travelling>, ui: &UiTheme) -> impl IntoElement {
    let size = dot_size(ui);
    let dot = note.map(|note| {
        note_dot(note.origin, ui)
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
