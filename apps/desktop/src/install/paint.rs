//! Draws a [`Frame`] of the install window's page: the whale as two
//! single-colour SVGs turned on the GPU, the water laid over what's under
//! the surface, the text lines as filled ribbons, the caret and its glow,
//! and the flying letters as text.

use std::f32::consts::PI;

use gpui::{
    App, Bounds, BoxShadow, ContentMask, Corners, Font, Hsla, Negate, PathBuilder, Pixels, Point,
    SharedString, TextRun, TransformationMatrix, Window, canvas, fill, font, linear_color_stop,
    linear_gradient, point, prelude::*, px, radians, size,
};

use super::scene::{self, Caret, Frame, Letter, Moment, Pt, WhalePose};
use crate::theme::InstallTheme;

/// The whale's two layers, served by [`crate::icons::Assets`].
pub const WHALE_BODY: &str = "install/whale-body.svg";
pub const WHALE_LIGHT: &str = "install/whale-light.svg";

/// Points around each round end of a text line.
const CAP_SEGMENTS: usize = 8;

/// The page at `moment`, as an element the stage's size.
pub fn stage(theme: &InstallTheme, moment: Moment, font_family: SharedString) -> impl IntoElement {
    let frame = scene::frame(&theme.stage, moment);
    let theme = theme.clone();
    let (width, height) = (theme.stage.width, theme.stage.height);
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            paint_frame(
                &frame,
                &theme,
                &font(font_family.clone()),
                bounds,
                window,
                cx,
            );
        },
    )
    .w(px(width))
    .h(px(height))
    .flex_none()
}

fn paint_frame(
    frame: &Frame,
    theme: &InstallTheme,
    letter_font: &Font,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let page = Page {
        origin: bounds.origin,
        theme,
    };
    window.paint_quad(fill(bounds, theme.paper));
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        // Spray flies behind the whale: over its dark body the letters
        // would read as smudges.
        for letter in &frame.letters {
            page.paint_letter(letter, letter_font, window, cx);
        }
        if let Some(whale) = frame.whale {
            page.paint_whale(whale, window, cx);
        }
        page.paint_water(frame.surface, bounds, window);
        page.paint_depths(bounds, window);
        for piece in &frame.lines {
            page.paint_line(piece, theme.line, window);
        }
        if let Some(written_to) = frame.written_to {
            page.paint_written(written_to, window);
        }
        page.paint_caret(frame.caret, window);
    });
}

/// Where the page is in the window, and how it's drawn.
struct Page<'a> {
    origin: Point<Pixels>,
    theme: &'a InstallTheme,
}

impl Page<'_> {
    fn at(&self, spot: Pt) -> Point<Pixels> {
        point(self.origin.x + px(spot.x), self.origin.y + px(spot.y))
    }

    fn paint_whale(&self, whale: WhalePose, window: &mut Window, cx: &mut App) {
        let stage = &self.theme.stage;
        let length = stage.whale_length;
        let center = self.at(whale.center);
        let whale_size = size(px(length), px(length * stage.whale_aspect));
        let bounds = Bounds::centered_at(center, whale_size);
        let pivot = center.scale(window.scale_factor());
        // GPUI turns clockwise; the pose's angle is nose up.
        let turn = TransformationMatrix::unit()
            .translate(pivot)
            .rotate(radians(-whale.angle.to_radians()))
            .translate(pivot.negate());
        for (layer, color) in [
            (WHALE_BODY, self.theme.whale_body),
            (WHALE_LIGHT, self.theme.whale_light),
        ] {
            if let Err(error) = window.paint_svg(bounds, layer.into(), turn, color, cx) {
                eprintln!("could not draw the whale: {error}");
            }
        }
    }

    fn paint_water(&self, surface: f32, bounds: Bounds<Pixels>, window: &mut Window) {
        let top = self.origin.y + px(surface);
        let water = Bounds::from_corners(point(bounds.left(), top), bounds.bottom_right());
        window.paint_quad(fill(water, self.theme.water));
    }

    /// Fades the water to paper under the lines, so the whale rises out
    /// of the depths rather than from the page's edge.
    fn paint_depths(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let stage = &self.theme.stage;
        let top = self.origin.y + px(stage.depths_top);
        let depths = Bounds::from_corners(point(bounds.left(), top), bounds.bottom_right());
        let clear = self.theme.paper.opacity(0.);
        let shade = linear_gradient(
            180.,
            linear_color_stop(clear, 0.),
            linear_color_stop(self.theme.paper, 1.),
        );
        window.paint_quad(fill(depths, shade));
    }

    fn paint_line(&self, piece: &[Pt], color: Hsla, window: &mut Window) {
        let outline = ribbon(piece, self.theme.stage.line_thickness / 2.);
        if outline.len() < 3 {
            return;
        }
        let points: Vec<Point<Pixels>> = outline.into_iter().map(|spot| self.at(spot)).collect();
        let mut path = PathBuilder::fill();
        path.add_polygon(&points, true);
        match path.build() {
            Ok(path) => window.paint_path(path, color),
            Err(error) => eprintln!("could not draw a text line: {error}"),
        }
    }

    /// The first line, written again as far as `to`.
    fn paint_written(&self, to: f32, window: &mut Window) {
        let stage = &self.theme.stage;
        let from = stage.line_left;
        if to - from < stage.line_thickness {
            return;
        }
        let piece = [
            Pt {
                x: from,
                y: stage.line_top,
            },
            Pt {
                x: to,
                y: stage.line_top,
            },
        ];
        self.paint_line(&piece, self.theme.written_line, window);
    }

    fn paint_caret(&self, caret: Caret, window: &mut Window) {
        let stage = &self.theme.stage;
        let bounds = Bounds::new(
            self.at(caret.at),
            size(px(stage.caret_width), px(stage.caret_height)),
        );
        let radius = Corners::all(px(stage.caret_width / 2.));
        if caret.alpha <= 0. {
            return;
        }
        if caret.glow > 0. {
            let glow = BoxShadow {
                color: self.theme.caret_glow.opacity(caret.glow),
                offset: point(px(0.), px(0.)),
                blur_radius: self.theme.caret_glow_blur,
                spread_radius: px(stage.caret_width * caret.glow),
            };
            window.paint_shadows(bounds, radius, &[glow]);
        }
        let color = self.theme.caret.opacity(caret.alpha);
        window.paint_quad(fill(bounds, color).corner_radii(radius));
    }

    fn paint_letter(&self, letter: &Letter, letter_font: &Font, window: &mut Window, cx: &mut App) {
        let text: SharedString = letter.ch.to_string().into();
        let run = TextRun {
            len: text.len(),
            font: letter_font.clone(),
            color: self.theme.letter.opacity(letter.alpha),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = px(self.theme.stage.letter_size);
        let shaped = window
            .text_system()
            .shape_line(text, font_size, &[run], None);
        if let Err(error) = shaped.paint(self.at(letter.at), font_size, window, cx) {
            eprintln!("could not draw a letter: {error}");
        }
    }
}

/// The outline of a line `half` thick either side of `centre`, with
/// round ends: along the top, round the end, back along the bottom.
fn ribbon(centre: &[Pt], half: f32) -> Vec<Pt> {
    if centre.len() < 2 {
        return Vec::new();
    }
    let normals: Vec<Pt> = (0..centre.len())
        .map(|index| {
            let before = centre[index.saturating_sub(1)];
            let after = centre[(index + 1).min(centre.len() - 1)];
            normal(before, after)
        })
        .collect();
    let side = |sign: f32| {
        centre.iter().zip(&normals).map(move |(spot, n)| Pt {
            x: spot.x + n.x * half * sign,
            y: spot.y + n.y * half * sign,
        })
    };
    let last = centre.len() - 1;
    let mut outline: Vec<Pt> = side(-1.).collect();
    outline.extend(cap(centre[last], normals[last], half, false));
    outline.extend(side(1.).rev());
    outline.extend(cap(centre[0], normals[0], half, true));
    outline
}

/// The unit normal to the direction from `a` to `b`, pointing down the page.
fn normal(a: Pt, b: Pt) -> Pt {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx.hypot(dy).max(f32::EPSILON);
    Pt {
        x: -dy / length,
        y: dx / length,
    }
}

/// The points round a line's end at `centre`: from its top edge to its
/// bottom at the far end, or back up at the start.
fn cap(centre: Pt, n: Pt, half: f32, start: bool) -> impl Iterator<Item = Pt> {
    let top = (-n.y).atan2(-n.x);
    let from = if start { top + PI } else { top };
    (1..CAP_SEGMENTS).map(move |step| {
        let angle = from + PI * step as f32 / CAP_SEGMENTS as f32;
        Pt {
            x: centre.x + angle.cos() * half,
            y: centre.y + angle.sin() * half,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ribbon_wraps_its_line_with_round_ends() {
        let line = [
            Pt { x: 0., y: 10. },
            Pt { x: 50., y: 10. },
            Pt { x: 100., y: 10. },
        ];
        let outline = ribbon(&line, 4.);
        let xs = outline.iter().map(|spot| spot.x);
        let ys = outline.iter().map(|spot| spot.y);
        let (left, right) = xs.fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
        let (top, bottom) = ys.fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
        assert!(
            (left + 4.).abs() < 0.5 && (right - 104.).abs() < 0.5,
            "{left} {right}"
        );
        assert!((top - 6.).abs() < 0.01 && (bottom - 14.).abs() < 0.01);
    }

    #[test]
    fn a_point_is_not_a_line() {
        assert!(ribbon(&[Pt::default()], 4.).is_empty());
    }
}
