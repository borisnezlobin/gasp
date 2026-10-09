//! The first step's name drawn from its glyphs' outlines, so each glyph
//! can be stretched and turned on its own as the soft glyphs move. The
//! outlines come from the system's Charter Bold on macOS; without them
//! the name is set as plain letters that only move.

use gpui::{FillOptions, Hsla, PathBuilder, PathStyle, Pixels, Point, Window, point, px};
use lyon::math::{Transform, vector};

#[cfg(target_os = "macos")]
mod macos;

/// The font the outlines come from: the app's own, in bold.
#[cfg(target_os = "macos")]
const OUTLINE_FONT: &str = "Charter-Bold";

/// One piece of a glyph's outline, in ems from the pen on the baseline,
/// down being positive. Only macOS reads outlines so far.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Move(Point<f32>),
    Line(Point<f32>),
    Quad(Point<f32>, Point<f32>),
    Cubic(Point<f32>, Point<f32>, Point<f32>),
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlyphOutline(pub Vec<Segment>);

/// Where one glyph is drawn in a frame.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    /// The pen on the baseline, where the glyph sits at rest.
    pub origin: Point<Pixels>,
    /// The point it stretches and turns about.
    pub pivot: Point<Pixels>,
    pub offset: Point<Pixels>,
    /// How much longer it is along `angle`, and shorter across it.
    pub stretch: f32,
    pub angle: f32,
}

/// The outlines of each character of `text` in the app's bold, when this
/// platform can give them.
pub fn outlines(text: &str) -> Option<Vec<GlyphOutline>> {
    #[cfg(target_os = "macos")]
    {
        macos::outlines(OUTLINE_FONT, text)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        None
    }
}

impl GlyphOutline {
    /// Fills the glyph `size` tall in its `pose`.
    pub fn paint(&self, size: Pixels, pose: &Pose, color: Hsla, window: &mut Window) {
        let mut builder = PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::non_zero()));
        for segment in &self.0 {
            add_segment(&mut builder, *segment);
        }
        builder.transform(placement(size, pose));
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

fn add_segment(builder: &mut PathBuilder, segment: Segment) {
    let at = |p: Point<f32>| point(px(p.x), px(p.y));
    match segment {
        Segment::Move(to) => builder.move_to(at(to)),
        Segment::Line(to) => builder.line_to(at(to)),
        Segment::Quad(control, to) => builder.curve_to(at(to), at(control)),
        Segment::Cubic(first, second, to) => builder.cubic_bezier_to(at(to), at(first), at(second)),
        Segment::Close => builder.close(),
    }
}

/// From ems to the window: scaled to `size`, moved to the pen, then
/// stretched along the pose's angle about its pivot and moved by its
/// offset.
fn placement(size: Pixels, pose: &Pose) -> Transform {
    let (sin, cos) = pose.angle.sin_cos();
    let along = pose.stretch.max(0.01);
    let across = 1. / along;
    let stretch = Transform::new(
        along * cos * cos + across * sin * sin,
        (along - across) * cos * sin,
        (along - across) * cos * sin,
        along * sin * sin + across * cos * cos,
        0.,
        0.,
    );
    let from_pivot = pose.origin - pose.pivot;
    let to_place = pose.pivot + pose.offset;
    Transform::scale(f32::from(size), f32::from(size))
        .then_translate(vector(f32::from(from_pivot.x), f32::from(from_pivot.y)))
        .then(&stretch)
        .then_translate(vector(f32::from(to_place.x), f32::from(to_place.y)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyon::math::point as lyon_point;

    #[test]
    fn a_still_glyph_sits_at_its_pen() {
        let pose = Pose {
            origin: point(px(10.), px(100.)),
            pivot: point(px(30.), px(70.)),
            offset: point(px(0.), px(0.)),
            stretch: 1.,
            angle: 0.3,
        };
        let placed = placement(px(50.), &pose).transform_point(lyon_point(0.5, -1.));
        assert!(
            (placed.x - 35.).abs() < 1e-3 && (placed.y - 50.).abs() < 1e-3,
            "{placed:?}"
        );
    }

    #[test]
    fn a_stretched_glyph_grows_along_its_way_and_thins_across() {
        let pose = Pose {
            origin: point(px(0.), px(0.)),
            pivot: point(px(0.), px(0.)),
            offset: point(px(5.), px(0.)),
            stretch: 2.,
            angle: 0.,
        };
        let placement = placement(px(1.), &pose);
        let along = placement.transform_point(lyon_point(1., 0.));
        let across = placement.transform_point(lyon_point(0., 1.));
        assert!((along.x - 7.).abs() < 1e-4);
        assert!((across.y - 0.5).abs() < 1e-4 && (across.x - 5.).abs() < 1e-4);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_name_has_an_outline_for_each_letter() {
        let Some(glyphs) = outlines("Gasp") else {
            return;
        };
        assert_eq!(glyphs.len(), 4);
        assert!(glyphs.iter().all(|glyph| glyph.0.len() > 4));
        let p_reaches_below = glyphs[3].0.iter().any(|segment| match segment {
            Segment::Move(at) | Segment::Line(at) => at.y > 0.1,
            _ => false,
        });
        assert!(p_reaches_below, "down is positive");
    }
}
