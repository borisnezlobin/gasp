//! Images for inline widgets: decoded from the note's folder when found,
//! otherwise a generated placeholder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Pixels, RenderImage, Size, px, size};
use image::{Frame, RgbaImage};

const PLACEHOLDER_SIZE: (u32, u32) = (96, 64);
const MAX_ASPECT_RATIO: f32 = 4.;

/// Decoded images by their link target.
pub struct ImageStore {
    search_dirs: Vec<PathBuf>,
    by_target: HashMap<String, Arc<RenderImage>>,
    placeholder: Arc<RenderImage>,
}

impl ImageStore {
    /// Looks for images in each directory and its `images` folder.
    pub fn new(search_dirs: Vec<PathBuf>) -> Self {
        Self {
            search_dirs,
            by_target: HashMap::new(),
            placeholder: Arc::new(render_image(placeholder_pixels())),
        }
    }

    pub fn image(&mut self, target: &str) -> Arc<RenderImage> {
        if let Some(image) = self.by_target.get(target) {
            return image.clone();
        }
        let image = self
            .find(target)
            .and_then(|path| decode(&path))
            .map_or_else(|| self.placeholder.clone(), Arc::new);
        self.by_target.insert(target.to_owned(), image.clone());
        image
    }

    fn find(&self, target: &str) -> Option<PathBuf> {
        let file_name = Path::new(target).file_name()?;
        self.search_dirs
            .iter()
            .flat_map(|dir| [dir.join(target), dir.join("images").join(file_name)])
            .find(|path| path.is_file())
    }
}

/// The size an image is drawn at: the width written in the note (`|300`)
/// or its natural size, scaled by the view's zoom, never wider than
/// `max_width`, keeping its aspect ratio unless a height is written too.
pub fn display_size(
    image: &RenderImage,
    requested: (Option<u32>, Option<u32>),
    zoom: f32,
    max_width: Pixels,
) -> Size<Pixels> {
    let pixels = image.size(0);
    let natural = (pixels.width.0.max(1) as f32, pixels.height.0.max(1) as f32);
    let aspect = (natural.0 / natural.1).clamp(1. / MAX_ASPECT_RATIO, MAX_ASPECT_RATIO);
    let wanted_width = requested.0.map_or(natural.0, |width| width as f32) * zoom;
    let wanted_height = match requested {
        (Some(_), Some(height)) => height as f32 * zoom,
        _ => wanted_width / aspect,
    };
    let shrink = (f32::from(max_width.max(px(1.))) / wanted_width).min(1.);
    size(px(wanted_width * shrink), px(wanted_height * shrink))
}

fn decode(path: &Path) -> Option<RenderImage> {
    let pixels = image::open(path).ok()?.to_rgba8();
    Some(render_image(pixels))
}

/// GPUI wants BGRA.
fn render_image(mut pixels: RgbaImage) -> RenderImage {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    RenderImage::new(vec![Frame::new(pixels)])
}

fn placeholder_pixels() -> RgbaImage {
    let (width, height) = PLACEHOLDER_SIZE;
    RgbaImage::from_fn(width, height, |x, y| {
        let shade = ((x * 255) / width) as u8;
        let band = if (x / 16 + y / 16) % 2 == 0 { 40 } else { 0 };
        image::Rgba([shade, 120 + band, 255 - shade, 255])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_images_use_the_placeholder() {
        let mut store = ImageStore::new(vec![]);
        let image = store.image("missing.png");
        assert_eq!(image.size(0).width.0, PLACEHOLDER_SIZE.0 as i32);
        assert!(Arc::ptr_eq(&image, &store.image("missing.png")));
    }

    #[test]
    fn display_size_keeps_the_aspect_ratio() {
        let image = render_image(RgbaImage::new(40, 20));
        let natural = display_size(&image, (None, None), 1., px(1000.));
        assert_eq!((natural.width, natural.height), (px(40.), px(20.)));
        let wide = display_size(&image, (Some(300), None), 1., px(1000.));
        assert_eq!((wide.width, wide.height), (px(300.), px(150.)));
        let capped = display_size(&image, (Some(300), None), 1., px(100.));
        assert_eq!((capped.width, capped.height), (px(100.), px(50.)));
        let zoomed = display_size(&image, (Some(300), None), 1.5, px(1000.));
        assert_eq!(zoomed.width, px(450.));
        let explicit = display_size(&image, (Some(100), Some(100)), 1., px(1000.));
        assert_eq!(explicit.height, px(100.));
    }

    #[test]
    fn decodes_corpus_pngs() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus");
        let Some(dir) = first_images_dir(&corpus) else {
            return;
        };
        let note_dir = dir.parent().unwrap().to_path_buf();
        let name = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.ends_with(".png"))
            .unwrap();
        let mut store = ImageStore::new(vec![note_dir]);
        let image = store.image(&name);
        assert!(!Arc::ptr_eq(&image, &store.placeholder));
    }

    fn first_images_dir(root: &Path) -> Option<PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).ok()?.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() && path.file_name().is_some_and(|name| name == "images") {
                    return Some(path);
                }
                if path.is_dir() {
                    pending.push(path);
                }
            }
        }
        None
    }
}
