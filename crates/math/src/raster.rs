//! Equations rasterised straight from Typst's layout into coverage, one
//! byte a pixel, for apps that fill it with the text colour. Going through
//! SVG instead means writing every glyph outline out as text and parsing it
//! back, which costs more than laying the equation out; drawing the layout
//! directly also reuses glyphs Typst has already rasterised at that size.

use typst::layout::{Abs, Size};
use typst::utils::Scalar;
use typst_layout::Page;

use crate::MathError;
use crate::render::{Metrics, layout_latex};

/// An equation rasterised as coverage: how much ink each pixel holds.
#[derive(Debug, Clone, PartialEq)]
pub struct MathCoverage {
    /// Width of the pixels in points: the equation's width rounded up to
    /// whole pixels. Drawn this wide, the equation is exactly its size.
    pub width: f64,
    /// Height of the pixels in points, rounded up like `width`.
    pub height: f64,
    /// Distance from the top edge to the baseline, in points.
    pub baseline: f64,
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// `pixel_width * pixel_height` bytes, row by row from the top.
    pub coverage: Vec<u8>,
}

/// Converts LaTeX math with mitex, lays it out and rasterises it at
/// `pixels_per_point`.
pub fn rasterize_latex(
    src: &str,
    display: bool,
    font_size: f64,
    pixels_per_point: f32,
) -> Result<MathCoverage, MathError> {
    let page = layout_latex(src, display, font_size)?;
    let metrics = Metrics::of(&page, display);
    if metrics.width <= 0. || metrics.height <= 0. {
        return Err(MathError::Render("the equation has no area".to_owned()));
    }
    Ok(rasterize_page(page, metrics, pixels_per_point))
}

/// Points rounded up to whole pixels, and at least one.
fn whole_pixels(points: f64, pixels_per_point: f32) -> f32 {
    (points as f32 * pixels_per_point).ceil().max(1.)
}

fn rasterize_page(mut page: Page, metrics: Metrics, pixels_per_point: f32) -> MathCoverage {
    let pixel_width = whole_pixels(metrics.width, pixels_per_point);
    let pixel_height = whole_pixels(metrics.height, pixels_per_point);
    let width = f64::from(pixel_width / pixels_per_point);
    let height = f64::from(pixel_height / pixels_per_point);
    page.frame
        .set_size(Size::new(Abs::pt(width), Abs::pt(height)));
    let options = typst_render::RenderOptions {
        pixel_per_pt: Scalar::new(f64::from(pixels_per_point)),
        render_bleed: false,
    };
    let pixmap = typst_render::render(&page, &options);
    MathCoverage {
        width,
        height,
        baseline: metrics.baseline,
        pixel_width: pixmap.width(),
        pixel_height: pixmap.height(),
        coverage: pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_latex;

    #[test]
    fn pixels_cover_the_equation_at_the_asked_scale() {
        let exact = render_latex(r"\frac{a}{b} + x^2", false, 16.).unwrap();
        let raster = rasterize_latex(r"\frac{a}{b} + x^2", false, 16., 3.).unwrap();
        assert_eq!(raster.pixel_width, (exact.width * 3.).ceil() as u32);
        assert_eq!(raster.pixel_height, (exact.height * 3.).ceil() as u32);
        assert_eq!(
            raster.coverage.len(),
            (raster.pixel_width * raster.pixel_height) as usize
        );
        assert!((raster.width * 3. - f64::from(raster.pixel_width)).abs() < 1e-3);
        assert!(raster.width >= exact.width && raster.width - exact.width < 1. / 3.);
        assert_eq!(raster.baseline, exact.baseline);
        assert!(raster.coverage.contains(&255));
    }

    #[test]
    fn ink_lands_where_the_svg_draws_it() {
        let raster = rasterize_latex("x", false, 16., 2.).unwrap();
        let columns = raster.pixel_width as usize;
        let inked_columns = (0..columns)
            .filter(|&x| {
                raster
                    .coverage
                    .iter()
                    .skip(x)
                    .step_by(columns)
                    .any(|&alpha| alpha > 0)
            })
            .count();
        assert!(inked_columns + 2 >= columns, "{inked_columns} of {columns}");
    }

    #[test]
    fn broken_and_empty_equations_are_errors() {
        assert!(rasterize_latex(r"\left( x", false, 16., 2.).is_err());
        assert!(rasterize_latex(r"\mathbb{", false, 16., 2.).is_err());
    }
}
