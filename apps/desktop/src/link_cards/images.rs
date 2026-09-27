//! Link card images: downloaded once, kept in the app's cache folder so
//! reopening a note doesn't fetch them again, and cropped to the card's
//! thumbnail shape when decoded.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use gpui::RenderImage;
use image::{DynamicImage, imageops::FilterType};

use crate::images::{RemoteImage, render_image};

/// Images bigger than this aren't previews.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// Thumbnails are stored at most this many pixels tall, which covers a
/// card at twice the default zoom on a Retina screen.
const THUMBNAIL_HEIGHT: u32 = 400;

/// The decoded image `request` asks for, from the cache or the web.
/// `None` when it can't be had, and the card goes without.
pub fn load(request: &RemoteImage) -> Option<RenderImage> {
    let cached = cache_path(&request.url);
    let bytes = cached
        .as_ref()
        .and_then(|path| std::fs::read(path).ok())
        .or_else(|| {
            let bytes = super::net::get(&request.url, MAX_IMAGE_BYTES).ok()?;
            if let Some(path) = &cached {
                store(path, &bytes);
            }
            Some(bytes)
        })?;
    let image = image::load_from_memory(&bytes).ok()?;
    let image = match request.aspect {
        Some(aspect) => cover(image, aspect),
        None => image,
    };
    Some(render_image(image.to_rgba8()))
}

/// The middle of `image` at `aspect` (width over height), scaled down to
/// the thumbnail height, as CSS `object-fit: cover` would show it.
pub fn cover(image: DynamicImage, aspect: f32) -> DynamicImage {
    let (width, height) = (image.width().max(1), image.height().max(1));
    let wanted_width = ((height as f32) * aspect).round() as u32;
    let (crop_width, crop_height) = if wanted_width <= width {
        (wanted_width.max(1), height)
    } else {
        (width, ((width as f32) / aspect).round().max(1.) as u32)
    };
    let x = (width - crop_width) / 2;
    let y = (height - crop_height) / 2;
    let cropped = image.crop_imm(x, y, crop_width, crop_height);
    if crop_height <= THUMBNAIL_HEIGHT {
        return cropped;
    }
    let scaled_width = ((THUMBNAIL_HEIGHT as f32) * aspect).round() as u32;
    cropped.resize_exact(scaled_width, THUMBNAIL_HEIGHT, FilterType::Triangle)
}

fn cache_path(url: &str) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let dir = dirs::cache_dir()?.join("editor").join("link-cards");
    Some(dir.join(format!("{:016x}", hasher.finish())))
}

/// Writes the download for next time; a cache that can't be written
/// only means fetching again.
fn store(path: &PathBuf, bytes: &[u8]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn wide_images_lose_their_sides_and_tall_ones_their_ends() {
        let wide = cover(DynamicImage::ImageRgba8(RgbaImage::new(300, 100)), 1.5);
        assert_eq!((wide.width(), wide.height()), (150, 100));
        let tall = cover(DynamicImage::ImageRgba8(RgbaImage::new(90, 300)), 1.5);
        assert_eq!((tall.width(), tall.height()), (90, 60));
        let big = cover(DynamicImage::ImageRgba8(RgbaImage::new(2400, 1600)), 1.5);
        assert_eq!((big.width(), big.height()), (600, 400));
    }
}
