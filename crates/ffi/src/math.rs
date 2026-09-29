//! Math drawn as math on the phone: LaTeX goes through `gasp-math`
//! (mitex and Typst) and is rasterised straight from the layout into
//! coverage alone, one byte a pixel. The phone fills that with the text
//! colour, so one render serves light and dark mode, and keeps its own
//! cache. `gasp-math` lets go of old layouts as it renders.
//!
//! Renders are safe to run on several threads at once, as the desktop runs
//! them.

use gasp_math::{MathCoverage, rasterize_latex, warm_up};

/// An equation rasterised as coverage: how much ink each pixel holds.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MathImage {
    /// The size of the pixels in points: the equation's size rounded up
    /// to whole pixels, so drawn this big the pixels land one to one.
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
    match rasterize_latex(&tex, display, font_size, pixels_per_point as f32) {
        Ok(image) => MathRender::Drawn {
            image: math_image(image),
        },
        Err(error) => MathRender::Failed {
            message: error.message().to_owned(),
        },
    }
}

fn math_image(image: MathCoverage) -> MathImage {
    MathImage {
        width: image.width,
        height: image.height,
        baseline: image.baseline,
        pixel_width: image.pixel_width,
        pixel_height: image.pixel_height,
        coverage: image.coverage,
    }
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
        assert_eq!(image.pixel_width, (image.width * 3.).round() as u32);
        assert!(image.baseline > 0. && image.baseline < image.height);
        assert!(image.coverage.iter().any(|&ink| ink > 250));
    }

    #[test]
    fn broken_tex_says_why() {
        assert!(matches!(
            render_math("\\left( x".into(), false, 16., 2.),
            MathRender::Failed { .. }
        ));
    }
}
