//! Math drawn as math on the phone: LaTeX goes through `gasp-math`
//! (mitex and Typst) to SVG, which is rasterised with resvg into coverage
//! alone, one byte a pixel. The phone fills that with the text colour, so
//! one render serves light and dark mode, and keeps its own cache.
//!
//! Renders are safe to run on several threads at once, as the desktop runs
//! them.

use std::sync::atomic::{AtomicUsize, Ordering};

use gasp_math::{RenderedMath, evict_layout_memory, render_latex, warm_up};
use resvg::{tiny_skia, usvg};

/// Typst memoises layouts; every this many renders, the ones not used
/// lately are let go so memory doesn't grow with every equation seen.
const RENDERS_BETWEEN_EVICTIONS: usize = 200;
const EVICTION_AGE: usize = 2;

static RENDERS: AtomicUsize = AtomicUsize::new(0);

/// An equation rasterised as coverage: how much ink each pixel holds.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MathImage {
    /// The equation's size in points.
    pub width: f64,
    pub height: f64,
    /// From the top edge down to the baseline, in points, for lining
    /// inline math up with the text around it.
    pub baseline: f64,
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// `pixel_width * pixel_height` bytes, row by row from the top.
    pub coverage: Vec<u8>,
}

/// What rendering one equation came to.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum MathRender {
    Drawn {
        image: MathImage,
    },
    /// The TeX couldn't be converted or laid out; `message` says why.
    Failed {
        message: String,
    },
}

/// Does the one-time setup (fonts, Typst's library, the mitex scope) so
/// the first equation on screen doesn't wait for it.
#[uniffi::export]
pub fn warm_up_math() {
    warm_up();
}

/// Renders `tex` at `font_size` points, `display` for a `$$` block, with
/// `pixels_per_point` pixels to each point of the result.
#[uniffi::export]
pub fn render_math(
    tex: String,
    display: bool,
    font_size: f64,
    pixels_per_point: f64,
) -> MathRender {
    let rendered = render_latex(&tex, display, font_size);
    if RENDERS
        .fetch_add(1, Ordering::Relaxed)
        .is_multiple_of(RENDERS_BETWEEN_EVICTIONS)
    {
        evict_layout_memory(EVICTION_AGE);
    }
    let image = rendered
        .map_err(|error| error.message().to_owned())
        .and_then(|rendered| coverage(&rendered, pixels_per_point as f32));
    match image {
        Ok(image) => MathRender::Drawn { image },
        Err(message) => MathRender::Failed { message },
    }
}

fn coverage(rendered: &RenderedMath, pixels_per_point: f32) -> Result<MathImage, String> {
    let tree = usvg::Tree::from_str(&rendered.svg, &usvg::Options::default())
        .map_err(|error| error.to_string())?;
    let width = (rendered.width as f32 * pixels_per_point).ceil().max(1.);
    let height = (rendered.height as f32 * pixels_per_point).ceil().max(1.);
    let mut pixmap =
        tiny_skia::Pixmap::new(width as u32, height as u32).ok_or("the equation has no area")?;
    let svg_size = tree.size();
    let transform = tiny_skia::Transform::from_scale(
        width / svg_size.width().max(f32::EPSILON),
        height / svg_size.height().max(f32::EPSILON),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(MathImage {
        width: rendered.width,
        height: rendered.height,
        baseline: rendered.baseline,
        pixel_width: pixmap.width(),
        pixel_height: pixmap.height(),
        coverage: pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_equation_comes_back_as_coverage_at_the_asked_scale() {
        let MathRender::Drawn { image } = render_math("x^2".into(), false, 16., 3.) else {
            panic!("x^2 renders");
        };
        assert_eq!(
            image.coverage.len(),
            (image.pixel_width * image.pixel_height) as usize
        );
        assert_eq!(image.pixel_width, (image.width * 3.).ceil() as u32);
        assert!(image.baseline > 0. && image.baseline < image.height);
        assert!(image.coverage.contains(&255));
    }

    #[test]
    fn broken_tex_says_why() {
        assert!(matches!(
            render_math("\\left( x".into(), false, 16., 2.),
            MathRender::Failed { .. }
        ));
    }
}
