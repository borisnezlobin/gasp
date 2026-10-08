//! Glyph outlines from Core Text, read from the font installed with
//! macOS, so the app ships none of its outlines.

#![allow(unsafe_code)]

use std::cell::RefCell;

use core_graphics::geometry::CG_AFFINE_TRANSFORM_IDENTITY;
use core_graphics::path::{CGPath, CGPathElementType};
use core_text::font::{self as ct_font, CTFont};
use gpui::{Point, point};

use super::{GlyphOutline, Segment};

/// The outline of each character of `text` in the font named `font`, in
/// ems, or `None` when the font is missing or lacks a character.
pub fn outlines(font: &str, text: &str) -> Option<Vec<GlyphOutline>> {
    let font = ct_font::new_from_name(font, 1.).ok()?;
    let characters: Vec<u16> = text.encode_utf16().collect();
    let mut glyphs = vec![0u16; characters.len()];
    // SAFETY: both buffers hold `characters.len()` entries.
    let found = unsafe {
        font.get_glyphs_for_characters(
            characters.as_ptr(),
            glyphs.as_mut_ptr(),
            characters.len() as isize,
        )
    };
    if !found {
        return None;
    }
    glyphs
        .into_iter()
        .map(|glyph| outline(&font, glyph))
        .collect()
}

fn outline(font: &CTFont, glyph: u16) -> Option<GlyphOutline> {
    let path = font
        .create_path_for_glyph(glyph, &CG_AFFINE_TRANSFORM_IDENTITY)
        .ok()?;
    Some(GlyphOutline(segments(&path)))
}

fn segments(path: &CGPath) -> Vec<Segment> {
    let segments = RefCell::new(Vec::new());
    let collect = |element: core_graphics::path::CGPathElementRef| {
        let points: Vec<Point<f32>> = element
            .points()
            .iter()
            .map(|at| point(at.x as f32, -(at.y as f32)))
            .collect();
        let segment = match (element.element_type, points.as_slice()) {
            (CGPathElementType::MoveToPoint, [to]) => Segment::Move(*to),
            (CGPathElementType::AddLineToPoint, [to]) => Segment::Line(*to),
            (CGPathElementType::AddQuadCurveToPoint, [control, to]) => Segment::Quad(*control, *to),
            (CGPathElementType::AddCurveToPoint, [first, second, to]) => {
                Segment::Cubic(*first, *second, *to)
            }
            _ => Segment::Close,
        };
        segments.borrow_mut().push(segment);
    };
    path.apply(&collect);
    segments.into_inner()
}
