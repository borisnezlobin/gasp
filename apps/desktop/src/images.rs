//! Images for inline widgets: decoded from the note's folder when found,
//! else from wherever the vault index finds the file name (Obsidian
//! resolves `![[name.png]]` anywhere in the vault), otherwise a generated
//! placeholder. Images on the web, such as a link
//! card's preview, are downloaded in the background and show once they
//! arrive.

use std::collections::{HashMap, HashSet};
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
    /// Targets not found next to the note, drawn as the placeholder until
    /// the vault index finds them.
    missing: HashSet<String>,
    /// Missing targets still to look up in the vault index.
    vault_lookups: Vec<String>,
    placeholder: Arc<RenderImage>,
    /// Web images by URL: `None` while downloading or after failing.
    remote: HashMap<String, Option<Arc<RenderImage>>>,
    remote_requests: Vec<RemoteImage>,
}

/// A web image to download, cropped to `aspect` (width over height) when
/// given, as a card's thumbnail is.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteImage {
    pub url: String,
    pub aspect: Option<f32>,
}

impl ImageStore {
    /// Looks for images in each directory and its `images` folder.
    pub fn new(search_dirs: Vec<PathBuf>) -> Self {
        Self {
            search_dirs,
            by_target: HashMap::new(),
            missing: HashSet::new(),
            vault_lookups: Vec::new(),
            placeholder: Arc::new(render_image(placeholder_pixels())),
            remote: HashMap::new(),
            remote_requests: Vec::new(),
        }
    }

    /// The web image at `url` once it has downloaded. The first ask
    /// queues the download for [`ImageStore::take_remote_requests`].
    pub fn remote_image(&mut self, url: &str, aspect: Option<f32>) -> Option<Arc<RenderImage>> {
        if let Some(image) = self.remote.get(url) {
            return image.clone();
        }
        self.remote.insert(url.to_owned(), None);
        self.remote_requests.push(RemoteImage {
            url: url.to_owned(),
            aspect,
        });
        None
    }

    pub fn take_remote_requests(&mut self) -> Vec<RemoteImage> {
        std::mem::take(&mut self.remote_requests)
    }

    /// Keeps a downloaded image; `None` leaves the card without it.
    pub fn finish_remote(&mut self, url: String, image: Option<RenderImage>) {
        self.remote.insert(url, image.map(Arc::new));
    }

    pub fn image(&mut self, target: &str) -> Arc<RenderImage> {
        if let Some(image) = self.by_target.get(target) {
            return image.clone();
        }
        let image = match self.find(target) {
            Some(path) => decode(&path).map(Arc::new),
            None => {
                self.missing.insert(target.to_owned());
                self.vault_lookups.push(target.to_owned());
                None
            }
        };
        let image = image.unwrap_or_else(|| self.placeholder.clone());
        self.by_target.insert(target.to_owned(), image.clone());
        image
    }

    /// The images layout couldn't find near the note, for the vault index
    /// to look for.
    pub fn take_vault_lookups(&mut self) -> Vec<String> {
        std::mem::take(&mut self.vault_lookups)
    }

    /// The note's folder, which a vault lookup prefers.
    pub fn note_dir(&self) -> Option<&Path> {
        self.search_dirs.first().map(PathBuf::as_path)
    }

    /// Shows the file the vault index found for `target`. False when it
    /// can't be decoded, which leaves the placeholder.
    pub fn found_in_vault(&mut self, target: &str, path: &Path) -> bool {
        let Some(image) = decode(path) else {
            return false;
        };
        self.missing.remove(target);
        self.by_target.insert(target.to_owned(), Arc::new(image));
        true
    }

    /// Forgets which images were missing so the next layout looks for them
    /// again, as after the vault changed. False when none were.
    pub fn retry_missing(&mut self) -> bool {
        self.vault_lookups.clear();
        let any = !self.missing.is_empty();
        for target in self.missing.drain() {
            self.by_target.remove(&target);
        }
        any
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
pub(crate) fn render_image(mut pixels: RgbaImage) -> RenderImage {
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
    fn missing_images_wait_for_the_vault_index() {
        let dir = std::env::temp_dir().join(format!("editor-images-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
        let found = dir.join("elsewhere/pic.png");
        RgbaImage::new(3, 2).save(&found).unwrap();
        let mut store = ImageStore::new(vec![dir.join("notes")]);
        assert_eq!(store.note_dir(), Some(dir.join("notes").as_path()));

        assert!(Arc::ptr_eq(&store.image("pic.png"), &store.placeholder));
        assert_eq!(store.take_vault_lookups(), ["pic.png"]);
        store.image("pic.png");
        assert!(store.take_vault_lookups().is_empty(), "asked once");

        assert!(!store.found_in_vault("pic.png", &dir.join("nothing.png")));
        assert!(store.found_in_vault("pic.png", &found));
        assert_eq!(store.image("pic.png").size(0).width.0, 3);
        assert!(!store.retry_missing(), "nothing is missing now");

        store.image("gone.png");
        assert!(store.retry_missing());
        assert!(store.take_vault_lookups().is_empty());
        store.image("gone.png");
        assert_eq!(
            store.take_vault_lookups(),
            ["gone.png"],
            "a retry looks it up again"
        );
        std::fs::remove_dir_all(dir).unwrap();
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
