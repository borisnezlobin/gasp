//! The sea: lines like the lines of text in the app icon, which the whale
//! swims under. Along the bottom of every step after the first, the top
//! line is the tour's progress. It's darker as far as the red caret, and
//! the whale's nose is at the caret. The whale is under the surface, so
//! the window's colour washes over it and the lower lines cross it.

use std::rc::Rc;
use std::time::Instant;

use gpui::{AnyElement, Div, Hsla, Pixels, Window, div, prelude::*, px};

use super::art::{self, WhaleArt};
use super::motion;
use crate::theme::UiTheme;

/// How much of the lines' width each lower line takes, as a paragraph's
/// ragged edge would.
const LOWER_LINES: [f32; 2] = [0.93, 0.71];
/// Where the caret starts and stops along the top line, leaving the
/// whale room behind it at the start.
const CARET_RANGE: (f32, f32) = (0.12, 1.);
/// How much of the window's colour lies over what's under the surface.
const UNDERWATER: f32 = 0.62;

/// The room the sea takes along the bottom of the window.
pub fn band_height(ui: &UiTheme) -> Pixels {
    line_spacing(ui) * 4.
}

/// The space from one line of the sea to the next.
pub fn line_spacing(ui: &UiTheme) -> Pixels {
    ui.tour.sea_line * 4.
}

/// The space the tour keeps from the window's sides.
pub fn margin(ui: &UiTheme) -> Pixels {
    ui.space_xl * 4.
}

/// One line of the sea: a rounded bar, `width` long, at `left`, `top`.
pub fn line(left: Pixels, top: Pixels, width: Pixels, color: Hsla, ui: &UiTheme) -> Div {
    div()
        .absolute()
        .left(left)
        .top(top)
        .w(width.max(px(0.)))
        .h(ui.tour.sea_line)
        .rounded_full()
        .bg(color)
}

/// The wash over everything under the surface at `top`.
pub fn underwater(top: Pixels, ui: &UiTheme) -> Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(top)
        .bottom_0()
        .bg(ui.app_background.opacity(UNDERWATER))
}

/// The blinking red caret, centred on a line at `center`.
pub fn caret(left: Pixels, center: Pixels, on: bool, ui: &UiTheme) -> Div {
    let height = ui.tour.sea_line * 3.;
    div()
        .absolute()
        .left(left)
        .top(center - height / 2.)
        .w(ui.tour.caret_width)
        .h(height)
        .rounded_full()
        .when(on, |caret| caret.bg(ui.caret_mark))
}

/// The sea along the bottom of a step, with the caret and the whale
/// `tide` of the way along, 0 at the first step and 1 at the last.
pub fn band(
    art: Option<Rc<WhaleArt>>,
    tide: f32,
    opened: Instant,
    now: Instant,
    window: &Window,
    ui: &UiTheme,
) -> AnyElement {
    let width = window.viewport_size().width;
    let left = margin(ui);
    let span = (width - left * 2.).max(px(0.));
    let spacing = line_spacing(ui);
    let surface = spacing;
    let along = motion::lerp(CARET_RANGE.0, CARET_RANGE.1, tide);
    let caret_at = left + span * along;
    let gap = ui.space_md;
    let whale = art.map(|art| swimmer(&art, caret_at, surface, opened, now, ui));
    let lower = LOWER_LINES.iter().enumerate().map(|(index, share)| {
        let top = surface + spacing * (index + 1) as f32;
        line(left, top, span * *share, ui.fill_strong, ui)
    });
    div()
        .id("tour-sea")
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(band_height(ui))
        .children(whale)
        .child(underwater(surface + ui.tour.sea_line, ui))
        .child(line(
            left,
            surface,
            caret_at - gap - left,
            ui.text_faint,
            ui,
        ))
        .child(line(
            caret_at + gap,
            surface,
            left + span - caret_at - gap,
            ui.fill_strong,
            ui,
        ))
        .children(lower)
        .child(caret(
            caret_at,
            surface + ui.tour.sea_line / 2.,
            motion::caret_on(opened, now),
            ui,
        ))
        .into_any_element()
}

/// The whale swimming just under the surface with its nose at the caret.
fn swimmer(
    art: &WhaleArt,
    nose: Pixels,
    surface: Pixels,
    opened: Instant,
    now: Instant,
    ui: &UiTheme,
) -> AnyElement {
    let width = ui.tour.swimmer_width;
    let frame = art.swim_frame(now.saturating_duration_since(opened));
    div()
        .absolute()
        .left(nose - width)
        .top(surface + ui.tour.sea_line * 2.)
        .child(art::drawn(&art.swim, frame, width))
        .into_any_element()
}
