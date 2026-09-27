//! Page images for the print preview: a laid-out page drawn to pixels at
//! the width the preview shows it.

use typst::utils::Scalar;
use typst_layout::{Page, PagedDocument};

/// One page drawn to pixels: opaque RGBA rows, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A page's size in points, width then height.
pub fn page_size_pt(page: &Page) -> (f64, f64) {
    let size = page.frame.size();
    (size.x.to_pt(), size.y.to_pt())
}

/// Pixels per point that make a page `page_width_pt` wide come out
/// `width_px` pixels wide.
pub fn pixel_per_pt(page_width_pt: f64, width_px: u32) -> f64 {
    if page_width_pt <= 0.0 {
        return 1.0;
    }
    f64::from(width_px.max(1)) / page_width_pt
}

/// Draws `page` `width_px` pixels wide, on white.
pub fn render_page(page: &Page, width_px: u32) -> PageImage {
    let (width_pt, _) = page_size_pt(page);
    let options = typst_render::RenderOptions {
        pixel_per_pt: Scalar::new(pixel_per_pt(width_pt, width_px)),
        render_bleed: false,
    };
    let pixmap = typst_render::render(page, &options);
    let (width, height) = (pixmap.width(), pixmap.height());
    let mut rgba = pixmap.take();
    over_white(&mut rgba);
    PageImage {
        width,
        height,
        rgba,
    }
}

/// Draws every page of `document` `width_px` pixels wide.
pub fn render_pages(document: &PagedDocument, width_px: u32) -> Vec<PageImage> {
    document
        .pages()
        .iter()
        .map(|page| render_page(page, width_px))
        .collect()
}

/// Lays premultiplied pixels over white, so the page is opaque whatever
/// its fill.
fn over_white(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let clear = 255 - pixel[3];
        for channel in &mut pixel[..3] {
            *channel = channel.saturating_add(clear);
        }
        pixel[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_fits_the_width() {
        assert!((pixel_per_pt(595.0, 1190) - 2.0).abs() < 1e-9);
        assert_eq!(pixel_per_pt(0.0, 100), 1.0);
    }

    #[test]
    fn clear_pixels_become_white() {
        let mut pixels = vec![0, 0, 0, 0, 10, 20, 30, 255, 50, 0, 0, 128];
        over_white(&mut pixels);
        assert_eq!(
            pixels,
            [255, 255, 255, 255, 10, 20, 30, 255, 177, 127, 127, 255]
        );
    }
}
