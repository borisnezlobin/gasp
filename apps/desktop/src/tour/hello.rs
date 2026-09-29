//! The first step: the app icon made large. The name sits on a paragraph
//! whose first line is the one sentence about the app, and the whale
//! breaches out of the paragraph just past the end of that sentence, rising
//! in front of the end of the name. It rises as the window opens, leans
//! towards the pointer, and leaps when clicked.

use std::time::Instant;

use gpui::{
    AnyElement, Context, Font, FontWeight, Pixels, SharedString, TextRun, Window, div, prelude::*,
    px,
};

use super::art::{self, height_at};
use super::{Tour, lean, motion, sea};
use crate::theme::UiTheme;
use crate::ui::{Button, Selectable, ui_theme};

const NAME: &str = "Gasp";
const TAGLINE: &str = "Plain Markdown notes on your Mac and iPhone.";
/// How much of the whale stays under the surface once it's up.
const UNDER_SURFACE: f32 = 0.34;
/// Where the whale's body crosses the surface, as a share of its width
/// from its left edge: the drawing leans, tail low and head high.
const CROSSING: f32 = 0.24;
/// How far the whale leans towards the pointer.
const LEAN: f32 = 14.;
/// How high a click makes the whale leap.
const LEAP: f32 = 36.;
/// How much of the paragraph's width each line under the tagline takes.
const LOWER_LINES: [f32; 3] = [0.96, 0.84, 0.58];

/// Where everything on the step goes, worked out from the window's size
/// and the name's and tagline's widths.
struct Layout {
    left: Pixels,
    surface: Pixels,
    paragraph: Pixels,
    whale_left: Pixels,
}

fn layout(window: &mut Window, ui: &UiTheme) -> Layout {
    let viewport = window.viewport_size();
    let name = text_width(NAME, ui.tour.title_size, FontWeight::BOLD, window, ui);
    let tagline = text_width(TAGLINE, tagline_size(ui), FontWeight::NORMAL, window, ui);
    let whale = ui.tour.breach_width;
    let crossing = tagline + ui.space_xl * 2.;
    let whale_left = crossing - whale * CROSSING;
    let width = (whale_left + whale).max(name);
    let left = ((viewport.width - width) / 2.).max(sea::margin(ui));
    Layout {
        left,
        surface: viewport.height * 0.6,
        paragraph: width,
        whale_left: left + whale_left,
    }
}

fn tagline_size(ui: &UiTheme) -> Pixels {
    ui.font_size * 1.2
}

fn text_width(
    text: &'static str,
    size: Pixels,
    weight: FontWeight,
    window: &mut Window,
    ui: &UiTheme,
) -> Pixels {
    let run = TextRun {
        len: text.len(),
        font: Font {
            weight,
            ..gpui::font(ui.font_family.clone())
        },
        color: ui.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(SharedString::new_static(text), size, &[run], None)
        .width
}

pub fn render(
    tour: &mut Tour,
    now: Instant,
    window: &mut Window,
    cx: &mut Context<Tour>,
) -> AnyElement {
    let ui = ui_theme(cx);
    let at = layout(window, &ui);
    let spacing = sea::line_spacing(&ui);
    let title_size = ui.tour.title_size;
    let whale = tour.art.clone().map(|art| {
        let height = height_at(&art.breach, ui.tour.breach_width);
        let risen = motion::ease_out(motion::progress(tour.opened, now, ui.tour.breach_rise));
        let leap = tour.leapt.map_or(0., |at| {
            motion::pulse(motion::progress(at, now, ui.tour.press * 2))
        });
        let leaning = lean(tour.pointer, px(LEAN));
        let top = at.surface - height * (1. - UNDER_SURFACE) + height * (1. - risen) * 0.8
            - px(LEAP) * leap
            + leaning.y;
        div()
            .id("tour-whale")
            .selector(|| "tour-whale".to_owned())
            .absolute()
            .left(at.whale_left + leaning.x)
            .top(top)
            .cursor_pointer()
            .on_click(cx.listener(|tour, _, _, cx| tour.leap(cx)))
            .child(art::drawn(&art.breach, 0, ui.tour.breach_width))
    });
    let name = div()
        .absolute()
        .left(at.left)
        .top(at.surface - title_size * 1.15 - spacing)
        .flex()
        .flex_row()
        .items_center()
        .gap(title_size * 0.04)
        .text_size(title_size)
        .line_height(title_size * 1.15)
        .font_weight(FontWeight::BOLD)
        .text_color(ui.text_strong)
        .child(NAME)
        .child(
            div()
                .w(ui.tour.caret_width * 2.)
                .h(title_size * 0.78)
                .rounded_full()
                .when(motion::caret_on(tour.opened, now), |caret| {
                    caret.bg(ui.caret_mark)
                }),
        );
    let tagline_height = tagline_size(&ui) * 1.5;
    let tagline = div()
        .absolute()
        .left(at.left)
        .top(at.surface - tagline_height / 2.)
        .h(tagline_height)
        .flex()
        .items_center()
        .text_size(tagline_size(&ui))
        .text_color(ui.text_muted)
        .child(TAGLINE);
    let lower = LOWER_LINES.iter().enumerate().map(|(index, share)| {
        let top = at.surface + spacing * (index + 1) as f32;
        sea::line(at.left, top, at.paragraph * *share, ui.fill_strong, &ui)
    });
    let start = div()
        .absolute()
        .left(at.left)
        .top(at.surface + spacing * (LOWER_LINES.len() + 2) as f32)
        .child(
            Button::new("tour-start", "Show me around")
                .primary()
                .on_click(cx.listener(|tour, _, window, cx| tour.advance(window, cx))),
        );
    div()
        .size_full()
        .relative()
        .child(name)
        .children(whale)
        .child(sea::underwater(at.surface + tagline_height / 2., &ui))
        .child(tagline)
        .children(lower)
        .child(start)
        .into_any_element()
}
