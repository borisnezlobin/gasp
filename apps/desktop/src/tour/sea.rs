//! The sea along the bottom of every step after the first: the whale
//! swimming under it, as far across as the tour has come, and the red
//! caret that marks lines like the lines of text in the app icon.

use std::rc::Rc;
use std::time::Instant;

use gpui::{AnyElement, Div, Pixels, Window, div, prelude::*, px};

use super::art::{self, WhaleArt};
use super::motion;
use crate::theme::UiTheme;

/// Where the whale's nose starts and stops across the window, leaving it
/// room behind it at the start.
const NOSE_RANGE: (f32, f32) = (0.12, 1.);
/// How strongly the swimming whale shows, so it marks progress without
/// pulling the eye from the step.
const SWIMMER_OPACITY: f32 = 0.55;

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

/// The whale along the bottom of a step, `tide` of the way across: 0 at
/// the first step and 1 at the last, so where it has swum to is how far
/// along the tour is.
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
    let nose = left + span * motion::lerp(NOSE_RANGE.0, NOSE_RANGE.1, tide);
    let whale = art.map(|art| swimmer(&art, nose, line_spacing(ui), opened, now, ui));
    div()
        .id("tour-sea")
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(band_height(ui))
        .children(whale)
        .into_any_element()
}

/// The whale swimming along the bottom with its nose at `nose`.
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
        .opacity(SWIMMER_OPACITY)
        .child(art::drawn(&art.swim, frame, width))
        .into_any_element()
}
