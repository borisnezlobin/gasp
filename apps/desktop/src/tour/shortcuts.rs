//! The keyboard step: four shortcuts, each a picture of what it opens
//! over its keys and name. Pressing the keys, or clicking the tile, plays
//! its picture, which stays showing what the shortcut did until another
//! is pressed.

use std::time::Instant;

use gpui::{AnyElement, Context, Div, Pixels, Window, div, prelude::*, px};

use super::sketch::{self, list_row, note, paper, text_bar};
use super::{Step, Tour, explanation, heading, motion, sea, stage};
use crate::icons::{IconName, icon};
use crate::theme::UiTheme;
use crate::ui::{Selectable, keycap, ui_theme};

/// The commands the step shows, in order.
pub const SHORTCUT_COMMANDS: [&str; 4] =
    ["note.new", "switcher.open", "palette.open", "search.open"];

/// The widest the four tiles get together.
const ROW_WIDTH: f32 = 960.;
/// The picture's height over its tile's width.
const PICTURE_ASPECT: f32 = 0.72;

pub fn render(tour: &Tour, now: Instant, window: &Window, cx: &mut Context<Tour>) -> AnyElement {
    let ui = ui_theme(cx);
    let room = (window.viewport_size().width - sea::margin(&ui) * 2.).min(px(ROW_WIDTH));
    let gap = ui.space_xl;
    let tile_width =
        (room - gap * (SHORTCUT_COMMANDS.len() - 1) as f32) / SHORTCUT_COMMANDS.len() as f32;
    let tiles: Vec<AnyElement> = SHORTCUT_COMMANDS
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let shown = shown(tour, index, now, &ui);
            tile(index, id, shown, tile_width, cx).into_any_element()
        })
        .collect();
    let content = div()
        .flex()
        .flex_col()
        .gap(ui.space_md)
        .child(heading(Step::Shortcuts, &ui))
        .child(explanation(
            "Press one to see what it does. They work anywhere in Gasp, and ⌘P finds every other command.",
            &ui,
        ))
        .child(div().h(ui.space_xl))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(gap)
                .children(tiles),
        );
    stage(px(ROW_WIDTH), &ui, content)
}

/// How far tile `index`'s picture has played: 0 at rest, 1 once its
/// shortcut has been pressed.
fn shown(tour: &Tour, index: usize, now: Instant, ui: &UiTheme) -> f32 {
    match tour.pressed {
        Some((pressed, at)) if pressed == index => {
            motion::ease_out(motion::progress(at, now, ui.tour.press))
        }
        _ => 0.,
    }
}

fn tile(
    index: usize,
    id: &'static str,
    shown: f32,
    width: Pixels,
    cx: &mut Context<Tour>,
) -> impl IntoElement {
    let ui = ui_theme(cx);
    let picture = picture(index, width - ui.space_lg * 2., shown, &ui);
    let keys = crate::ui::hints::shortcut(id, cx).map(|shortcut| keycap(shortcut, &ui.keycap));
    div()
        .id(("tour-shortcut", index))
        .selector(move || format!("tour-shortcut-{index}"))
        .flex()
        .flex_col()
        .flex_none()
        .w(width)
        .gap(ui.space_md)
        .p(ui.space_lg)
        .rounded(ui.dialog_radius)
        .bg(ui.note_background)
        .shadow(ui.surface_shadows())
        .cursor_pointer()
        .when(shown > 0., |tile| {
            tile.shadow(vec![ui.ring(ui.caret_mark.opacity(0.5 * shown))])
        })
        .on_click(cx.listener(move |tour, _, _, cx| tour.press_shortcut(index, cx)))
        .child(picture)
        .child(crate::ui::hints::command_title(id))
        .child(div().flex().flex_row().children(keys))
}

/// The picture of what shortcut `index` does, `shown` of the way from
/// before to after.
fn picture(index: usize, width: Pixels, shown: f32, ui: &UiTheme) -> Div {
    let height = width * PICTURE_ASPECT;
    let stage = div()
        .relative()
        .w(width)
        .h(height)
        .overflow_hidden()
        .rounded(ui.menu_radius)
        .bg(ui.app_background);
    match index {
        0 => new_note(stage, width, height, shown, ui),
        1 => switcher(stage, width, shown, ui),
        2 => palette(stage, width, shown, ui),
        _ => search(stage, width, shown, ui),
    }
}

/// A note, and a fresh sheet sliding over it with the caret at its top.
fn new_note(stage: Div, width: Pixels, height: Pixels, shown: f32, ui: &UiTheme) -> Div {
    let sheet = width * 0.56;
    let slide = motion::lerp(1., 0., shown);
    let fresh = paper(ui)
        .absolute()
        .left(width * 0.38 + width * 0.6 * slide)
        .top(height * 0.16)
        .w(sheet)
        .h(height * 0.9)
        .p(ui.space_lg)
        .opacity(shown)
        .child(sea::caret(ui.space_lg, ui.space_lg + ui.space_xs, true, ui));
    stage
        .child(
            div()
                .absolute()
                .left(width * 0.08)
                .top(height * 0.1)
                .child(note(sheet, &[0.9, 0.75, 0.85, 0.5], ui)),
        )
        .child(fresh)
}

/// The quick switcher: a search field over notes, the match lit.
fn switcher(stage: Div, width: Pixels, shown: f32, ui: &UiTheme) -> Div {
    let inner = width * 0.8;
    let typed = inner * motion::lerp(0.08, 0.36, shown);
    stage.flex().justify_center().pt(ui.space_lg).child(
        paper(ui)
            .w(inner)
            .p(ui.space_sm)
            .gap(ui.space_xs)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(ui.space_sm)
                    .h(ui.tour.sea_line * 4.)
                    .px(ui.space_sm)
                    .child(
                        icon(IconName::MagnifyingGlass)
                            .size(ui.small_icon_size)
                            .text_color(ui.icon),
                    )
                    .child(sketch::bar(typed, ui.tour.sea_line * 0.8, ui.text_faint))
                    .child(
                        div()
                            .w(ui.tour.caret_width)
                            .h(ui.tour.sea_line * 2.5)
                            .rounded_full()
                            .bg(ui.caret_mark),
                    ),
            )
            .children([0.7, 0.5, 0.8].iter().enumerate().map(|(row, share)| {
                list_row(
                    inner - ui.space_sm * 2.,
                    *share,
                    row == 0 && shown > 0.5,
                    ui,
                )
                .opacity(if row == 0 { 1. } else { 1. - shown * 0.6 })
            })),
    )
}

/// The command palette: commands dropping in, each with its keys.
fn palette(stage: Div, width: Pixels, shown: f32, ui: &UiTheme) -> Div {
    let inner = width * 0.8;
    let rows = [0.6, 0.45, 0.7, 0.5]
        .iter()
        .enumerate()
        .map(|(row, share)| {
            let arrived = (shown * 4. - row as f32).clamp(0., 1.);
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .h(ui.tour.sea_line * 3.6)
                .px(ui.space_sm)
                .opacity(0.25 + 0.75 * arrived)
                .child(text_bar(inner * *share * 0.7, ui))
                .child(sketch::bar(px(18.), ui.tour.sea_line * 1.6, ui.fill_strong))
        });
    stage.flex().justify_center().pt(ui.space_lg).child(
        paper(ui)
            .w(inner)
            .p(ui.space_sm)
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(ui.tour.sea_line * 4.)
                    .px(ui.space_sm)
                    .child(
                        icon(IconName::Command)
                            .size(ui.small_icon_size)
                            .text_color(ui.icon),
                    ),
            )
            .children(rows),
    )
}

/// Searching every note: three notes, with the matches lighting up.
fn search(stage: Div, width: Pixels, shown: f32, ui: &UiTheme) -> Div {
    let sheet = width * 0.29;
    let inner = sheet - ui.space_md * 2.;
    let sheets = (0..3).map(|column| {
        let lines = (0..4).map(move |line| {
            let matched = (column + line) % 3 == 1;
            let bar = text_bar(inner * [0.9, 0.6, 0.8, 0.7][line], ui);
            if matched {
                bar.bg(ui.highlight.opacity(0.35 + 0.65 * shown))
            } else {
                bar
            }
        });
        paper(ui)
            .w(sheet)
            .p(ui.space_md)
            .gap(ui.space_sm)
            .children(lines)
    });
    stage
        .flex()
        .flex_row()
        .justify_center()
        .items_start()
        .gap(width * 0.03)
        .pt(ui.space_lg)
        .children(sheets)
}
