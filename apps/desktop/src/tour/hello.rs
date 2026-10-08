//! The first step, made like the website's homepage: the name set huge in
//! soft glyphs, the one sentence about the app ending in the icon's red
//! caret, and the lines of a note with a humpback gliding beneath them,
//! seen through moving water. The glyphs lean toward the pointer and part
//! around it up close, and the pointer rings the lines as it crosses
//! them. With Reduce Motion on, everything holds still.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Bounds, Context, Font, FontWeight, Hsla, MouseButton, MouseDownEvent,
    PathBuilder, PathStyle, Pixels, Point, ShapedLine, SharedString, StrokeOptions, TextRun,
    Window, canvas, div, point, prelude::*, px, size,
};
use lyon::tessellation::{LineCap, LineJoin};

use super::art::{self, Blend, GLIDE_PAD, WhaleArt, height_at};
use super::liquid::{LiquidMotion, lift_at};
use super::wordmark::{GlyphOutline, Pose};
use super::{Tour, motion, sea, water};
use crate::theme::UiTheme;
use crate::ui::{Button, Selectable, ui_theme};

pub const NAME: &str = "Gasp";
const LEDE: &str = "A Markdown editor optimized for speed and efficiency.";
/// Each note line's length, as a share of the content's width.
const SEA_LINES: [f32; 6] = [1., 0.96, 0.88, 0.64, 0.92, 0.4];
/// How much of the window's width and height the name may take.
const NAME_SHARE: (f32, f32) = (0.21, 0.26);
/// How much of the window's width the whale may take, and where its
/// middle sits across the lines.
const WHALE_SHARE: f32 = 0.44;
const WHALE_CROSS: f32 = 0.66;
/// How far below the first line the whale's back is, in whale widths.
const WHALE_DIVE: f32 = 0.07;
/// How opaque the whale's frames are where its body is solid: the page's
/// wash over it, drawn into the frames, leaves this much.
const WHALE_OPACITY: f32 = 0.35;
/// How far either way the whale glides and bobs, in whale widths.
const GLIDE_REACH: f32 = 0.05;
const BOB_REACH: f32 = 0.012;
/// Room above the first line and below the last for a ring to lift
/// them into.
const LIFT_ROOM: f32 = 9.;
/// The least room the lines take, in whale widths, so the whale's fins
/// have the water to themselves.
const SEA_FOR_WHALE: f32 = 0.42;
/// How strongly the water bends the lines, in points, and how stretched
/// its waves are: long across, short down.
const LINE_WATER: f32 = 1.5;
const WATER_FREQUENCY: (f32, f32) = (0.008, 0.05);
/// The distance between the points each line is drawn through.
const LINE_STEP: f32 = 5.;
/// Where the name's ink sits around its baseline, in its size.
const INK_ABOVE: f32 = 0.7;
const INK_BELOW: f32 = 0.24;
/// The height of the point each glyph stretches about, above the
/// baseline, in the name's size.
const GLYPH_MIDDLE: f32 = 0.35;
/// The line the sentence about the app sits on, in its size.
const LEDE_LINE: f32 = 1.4;

/// Where everything on the step goes, for the window's size.
struct Layout {
    left: Pixels,
    content: Pixels,
    word_size: Pixels,
    baseline: Pixels,
    /// Each glyph's pen across the window, and its advance.
    pens: Vec<(Pixels, Pixels)>,
    lede_top: Pixels,
    sea_top: Pixels,
    sea_height: Pixels,
    whale_width: Pixels,
    button_top: Pixels,
}

impl Layout {
    fn new(window: &mut Window, ui: &UiTheme) -> Layout {
        let viewport = window.viewport_size();
        let left = sea::margin(ui);
        let content = (viewport.width - left * 2.)
            .min(ui.tour.hero_width)
            .max(px(1.));
        let word_size = ui
            .tour
            .title_size
            .min(viewport.width * NAME_SHARE.0)
            .min(viewport.height * NAME_SHARE.1);
        let whale_width = ui.tour.glide_width.min(viewport.width * WHALE_SHARE);
        let lede_height = ui.tour.lede_size * LEDE_LINE;
        let lines_height = px(LIFT_ROOM * 2.) + ui.tour.sea_pitch * SEA_LINES.len() as f32;
        let sea_height = lines_height.max(whale_width * SEA_FOR_WHALE);
        let gap = ui.space_xl * 2.5;
        let name_to_lede = ui.space_xl * 1.5;
        let block = word_size * (INK_ABOVE + INK_BELOW)
            + name_to_lede
            + lede_height
            + gap * 2.
            + sea_height
            + ui.button_height;
        let room_top = ui.tab_height + ui.space_lg;
        let ink_top = room_top + ((viewport.height - room_top - block) / 2.).max(px(0.));
        let baseline = ink_top + word_size * INK_ABOVE;
        let lede_top = baseline + word_size * INK_BELOW + name_to_lede;
        let sea_top = lede_top + lede_height + gap;
        Layout {
            left,
            content,
            word_size,
            baseline,
            pens: pens(&shape_name(word_size, window, ui), left),
            lede_top,
            sea_top,
            sea_height,
            whale_width,
            button_top: sea_top + sea_height + gap,
        }
    }

    /// Where each glyph rests: the point its spring pulls it back to.
    fn homes(&self) -> Vec<Point<Pixels>> {
        let middle = self.baseline - self.word_size * GLYPH_MIDDLE;
        self.pens
            .iter()
            .map(|(pen, advance)| point(*pen + *advance / 2., middle))
            .collect()
    }

    fn sea_bounds(&self) -> Bounds<Pixels> {
        Bounds::new(
            point(self.left, self.sea_top),
            size(self.content, self.sea_height),
        )
    }

    fn whale_left(&self) -> Pixels {
        self.left + self.content * WHALE_CROSS - self.whale_width / 2.
    }

    fn whale_top(&self) -> Pixels {
        self.sea_top + px(LIFT_ROOM) + self.whale_width * WHALE_DIVE
    }

    /// Where the whale's head is on the water, for the ring that greets it.
    fn whale_head(&self) -> Point<Pixels> {
        point(
            self.whale_left() + self.whale_width * 0.9,
            self.whale_top() + self.whale_width * 0.1,
        )
    }

    fn line_centre(&self, index: usize, ui: &UiTheme) -> Pixels {
        self.sea_top + px(LIFT_ROOM) + ui.tour.sea_thickness / 2. + ui.tour.sea_pitch * index as f32
    }
}

fn bold_run(len: usize, family: SharedString, color: Hsla) -> TextRun {
    TextRun {
        len,
        font: Font {
            weight: FontWeight::BOLD,
            ..gpui::font(family)
        },
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn shape_name(word_size: Pixels, window: &mut Window, ui: &UiTheme) -> ShapedLine {
    let run = bold_run(NAME.len(), ui.font_family.clone(), ui.text_strong);
    window
        .text_system()
        .shape_line(SharedString::new_static(NAME), word_size, &[run], None)
}

/// Each glyph's pen and advance, from the name shaped as one line so its
/// kerning holds.
fn pens(line: &ShapedLine, left: Pixels) -> Vec<(Pixels, Pixels)> {
    let mut starts: Vec<Pixels> = line
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.position.x))
        .collect();
    starts.push(line.width);
    starts
        .windows(2)
        .map(|pair| (left + pair[0], pair[1] - pair[0]))
        .collect()
}

pub fn render(
    tour: &mut Tour,
    now: Instant,
    window: &mut Window,
    cx: &mut Context<Tour>,
) -> AnyElement {
    let ui = ui_theme(cx);
    let at = Layout::new(window, &ui);
    let still = tour.still;
    tour.liquid.set_sea(at.sea_bounds());
    if !still {
        tour.liquid.advance(now, &at.homes(), at.word_size);
        tour.liquid.greet(at.whale_head(), ui.tour.greet, now);
    }
    let time = if still { 0. } else { tour.liquid.time(now) };
    let liquid = &tour.liquid;
    let whale = tour.art.as_deref().map(|art| glide(art, &at, time, &ui));
    let lines = sea_lines(&at, time, liquid, now, &ui);
    let name = wordmark(tour.wordmark.clone(), &at, time, still, liquid, &ui);
    let caret_on = still || motion::caret_on(tour.opened, now);
    div()
        .id("tour-hello")
        .size_full()
        .relative()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|tour, event: &MouseDownEvent, _, _| tour.press_water(event.position)),
        )
        .children(whale)
        .child(lines)
        .child(name)
        .child(lede(&at, caret_on, &ui))
        .child(
            div().absolute().left(at.left).top(at.button_top).child(
                Button::new("tour-start", "Show me around")
                    .primary()
                    .on_click(cx.listener(|tour, _, window, cx| tour.advance(window, cx))),
            ),
        )
        .into_any_element()
}

/// The frames of the water's loop to show `time` seconds in: one frame
/// with the next laid over it as the clock moves toward it.
fn water_blend(art: &WhaleArt, time: f32) -> Blend {
    art.glide_blend(Duration::from_secs_f32(time.max(0.)))
}

/// How opaque the frame under the next one is drawn as the next fades in
/// at `toward`, so the whale's body keeps the opacity the frames give it
/// rather than darkening between frames.
fn under_opacity(toward: f32) -> f32 {
    (1. - toward) / (1. - WHALE_OPACITY * toward)
}

/// The whale under the lines, seen through the water and gliding slowly
/// from side to side.
fn glide(art: &WhaleArt, at: &Layout, time: f32, ui: &UiTheme) -> AnyElement {
    let blend = water_blend(art, time);
    let wave =
        |period: Duration| (std::f32::consts::TAU * time / period.as_secs_f32().max(0.1)).sin();
    let pad = at.whale_width * GLIDE_PAD;
    let width = at.whale_width + pad * 2.;
    let left = at.whale_left() - pad + at.whale_width * GLIDE_REACH * wave(ui.tour.glide_period);
    let top = at.whale_top() - pad + at.whale_width * BOB_REACH * wave(ui.tour.bob_period);
    let frame = |index: usize| art::drawn(&art.glide, index, width);
    div()
        .absolute()
        .left(left)
        .top(top)
        .w(width)
        .h(height_at(&art.glide, width))
        .child(
            div()
                .absolute()
                .opacity(under_opacity(blend.toward))
                .child(frame(blend.from)),
        )
        .child(
            div()
                .absolute()
                .opacity(blend.toward)
                .child(frame(blend.to)),
        )
        .into_any_element()
}

/// The note's lines, bent a little by the water and lifted by the rings
/// crossing them.
fn sea_lines(
    at: &Layout,
    time: f32,
    liquid: &LiquidMotion,
    now: Instant,
    ui: &UiTheme,
) -> AnyElement {
    let thickness = ui.tour.sea_thickness;
    let lines: Vec<Vec<Point<Pixels>>> = SEA_LINES
        .iter()
        .enumerate()
        .map(|(index, share)| {
            let line = SeaLine {
                left: at.left,
                length: at.content * *share,
                centre: at.line_centre(index, ui),
                sea_top: at.sea_top,
            };
            line.points(time, liquid, now)
        })
        .collect();
    let color = ui.sea;
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for line in &lines {
                paint_line(line, bounds.origin, thickness, color, window);
            }
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

/// One of the note's lines at rest.
struct SeaLine {
    left: Pixels,
    length: Pixels,
    centre: Pixels,
    sea_top: Pixels,
}

impl SeaLine {
    /// The points the line runs through this frame.
    fn points(&self, time: f32, liquid: &LiquidMotion, now: Instant) -> Vec<Point<Pixels>> {
        let steps = (f32::from(self.length) / LINE_STEP).ceil().max(1.) as usize;
        let down = f32::from(self.centre - self.sea_top);
        (0..=steps)
            .map(|step| {
                let x = self.left + self.length * (step as f32 / steps as f32);
                let bend = water::drift(f32::from(x - self.left), down, time, WATER_FREQUENCY).y
                    * LINE_WATER;
                let lift = lift_at(liquid.rings(), point(x, self.centre), now);
                point(x, self.centre + px(bend) + lift)
            })
            .collect()
    }
}

fn paint_line(
    line: &[Point<Pixels>],
    origin: Point<Pixels>,
    thickness: Pixels,
    color: Hsla,
    window: &mut Window,
) {
    let Some((first, rest)) = line.split_first() else {
        return;
    };
    let options = StrokeOptions::default()
        .with_line_width(f32::from(thickness))
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round);
    let mut builder = PathBuilder::stroke(thickness).with_style(PathStyle::Stroke(options));
    builder.move_to(*first + origin);
    for point in rest {
        builder.line_to(*point + origin);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Where each glyph is this frame: where its spring has it, stretched
/// along the way it moves and bobbing a little.
fn poses(at: &Layout, time: f32, still: bool, liquid: &LiquidMotion) -> Vec<Pose> {
    let word_size = f32::from(at.word_size);
    at.pens
        .iter()
        .zip(at.homes())
        .zip(liquid.glyphs())
        .map(|(((pen, _), home), glyph)| {
            let (offset, stretch, bob) = if still {
                (point(0., 0.), 1., 0.)
            } else {
                (
                    glyph.offset(),
                    glyph.stretch(time),
                    glyph.bob(time, word_size),
                )
            };
            Pose {
                origin: point(*pen, at.baseline),
                pivot: home,
                offset: point(px(offset.x), px(offset.y + bob)),
                stretch,
                angle: glyph.angle(),
            }
        })
        .collect()
}

/// The name, from its glyphs' outlines when there are any.
fn wordmark(
    outlines: Option<Rc<[GlyphOutline]>>,
    at: &Layout,
    time: f32,
    still: bool,
    liquid: &LiquidMotion,
    ui: &UiTheme,
) -> AnyElement {
    let word_size = at.word_size;
    let poses = poses(at, time, still, liquid);
    let ink = ui.text_strong;
    let family = ui.font_family.clone();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let placed: Vec<Pose> = poses
                .iter()
                .map(|pose| Pose {
                    origin: pose.origin + bounds.origin,
                    pivot: pose.pivot + bounds.origin,
                    ..*pose
                })
                .collect();
            let Some(outlines) = outlines else {
                paint_letters(&placed, word_size, family, ink, window, cx);
                return;
            };
            for (outline, pose) in outlines.iter().zip(&placed) {
                outline.paint(word_size, pose, ink, window);
            }
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

/// The name as plain letters that move but don't stretch, where the
/// glyphs' outlines aren't to be had.
fn paint_letters(
    poses: &[Pose],
    word_size: Pixels,
    family: SharedString,
    ink: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    for ((index, letter), pose) in NAME.char_indices().zip(poses) {
        let len = letter.len_utf8();
        let run = bold_run(len, family.clone(), ink);
        let text = SharedString::from(NAME[index..index + len].to_owned());
        let line = window
            .text_system()
            .shape_line(text, word_size, &[run], None);
        let top_left = pose.origin + pose.offset - point(px(0.), line.ascent);
        line.paint(top_left, line.ascent + line.descent, window, cx)
            .ok();
    }
}

/// The one sentence about the app, ending in the icon's red caret where
/// the next word would go.
fn lede(at: &Layout, caret_on: bool, ui: &UiTheme) -> impl IntoElement {
    let size = ui.tour.lede_size;
    div()
        .absolute()
        .left(at.left)
        .top(at.lede_top)
        .max_w(at.content)
        .h(size * LEDE_LINE)
        .flex()
        .flex_row()
        .items_center()
        .gap(size * 0.2)
        .text_size(size)
        .line_height(size * LEDE_LINE)
        .text_color(ui.text_strong)
        .child(LEDE)
        .child(
            div()
                .id("tour-lede-caret")
                .selector(|| "tour-lede-caret".to_owned())
                .flex_none()
                .w(ui.tour.lede_caret_width)
                .h(size * 1.05)
                .rounded_full()
                .when(caret_on, |caret| caret.bg(ui.caret_mark)),
        )
}
