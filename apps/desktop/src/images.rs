//! Images for inline widgets: read from the note's folder when found,
//! else from wherever the vault index finds the file name (Obsidian
//! resolves `![[name.png]]` anywhere in the vault), otherwise a generated
//! placeholder. Images on the web, such as a link
//! card's preview, are downloaded in the background and show once they
//! arrive.
//!
//! A file's size is read from its header straight away, so layout gives it
//! its room at once; the pixels are decoded on a background thread and
//! shrunk by a whole factor to no smaller than they're drawn on screen (a
//! screenshot of a Retina display shown in the text column needs a quarter
//! of its pixels), and decoded again, sharper, if the image is later drawn
//! larger.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Pixels, RenderImage, Size, px, size};
use image::{Frame, RgbaImage};

const PLACEHOLDER_SIZE: (u32, u32) = (96, 64);
const MAX_ASPECT_RATIO: f32 = 4.;
const CHANNELS: usize = 4;

/// An image as layout needs it.
#[derive(Clone)]
pub struct NoteImage {
    /// What to draw: nothing yet while the file decodes.
    pub image: Arc<RenderImage>,
    /// The file's own size in pixels, which layout sizes the image by.
    pub natural: (u32, u32),
}

/// One image the store knows.
struct Entry {
    shown: NoteImage,
    /// The file to decode, or `None` for the placeholder.
    path: Option<PathBuf>,
    /// How wide `shown` was decoded, zero before the first decode.
    decoded_width: u32,
    /// The width a decode on its way was asked for.
    decoding_width: Option<u32>,
}

/// A local file to decode in the background at `width` pixels (or its
/// own width, if that's smaller).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decode {
    pub target: String,
    pub path: PathBuf,
    pub width: u32,
}

/// Decoded images by their link target.
pub struct ImageStore {
    search_dirs: Vec<PathBuf>,
    by_target: HashMap<String, Entry>,
    /// Targets not found next to the note, drawn as the placeholder until
    /// the vault index finds them.
    missing: HashSet<String>,
    /// Missing targets still to look up in the vault index.
    vault_lookups: Vec<String>,
    placeholder: Arc<RenderImage>,
    /// Drawn in an image's place while it decodes.
    blank: Arc<RenderImage>,
    decodes: Vec<Decode>,
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
            blank: Arc::new(render_image(RgbaImage::new(1, 1))),
            decodes: Vec::new(),
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

    /// The image `target` links to, as far as it has decoded, sharp
    /// enough to draw `width` pixels wide: a decode at that width is
    /// queued for [`ImageStore::take_decodes`] when it isn't.
    pub fn image(&mut self, target: &str, width: u32) -> NoteImage {
        self.know(target);
        self.decode_if_blurry(target, width);
        self.by_target[target].shown.clone()
    }

    /// The image's size in pixels, which is known before it decodes.
    pub fn natural_size(&mut self, target: &str) -> (u32, u32) {
        self.know(target);
        self.by_target[target].shown.natural
    }

    /// Makes an entry for `target` on first sight.
    fn know(&mut self, target: &str) {
        if self.by_target.contains_key(target) {
            return;
        }
        let entry = match self.find(target) {
            Some(path) => self.entry_for_file(path),
            None => {
                self.missing.insert(target.to_owned());
                self.vault_lookups.push(target.to_owned());
                None
            }
        };
        let entry = entry.unwrap_or_else(|| self.placeholder_entry());
        self.by_target.insert(target.to_owned(), entry);
    }

    fn decode_if_blurry(&mut self, target: &str, width: u32) {
        let Some(entry) = self.by_target.get_mut(target) else {
            return;
        };
        let Some(path) = entry.path.clone() else {
            return;
        };
        let wanted = width.clamp(1, entry.shown.natural.0.max(1));
        let enough = entry.decoded_width.max(entry.decoding_width.unwrap_or(0));
        if wanted <= enough {
            return;
        }
        entry.decoding_width = Some(wanted);
        self.decodes.push(Decode {
            target: target.to_owned(),
            path,
            width: wanted,
        });
    }

    /// An entry for the file at `path`, sized from its header; `None`
    /// when it isn't an image.
    fn entry_for_file(&self, path: PathBuf) -> Option<Entry> {
        let natural = image::image_dimensions(&path).ok()?;
        Some(Entry {
            shown: NoteImage {
                image: self.blank.clone(),
                natural,
            },
            path: Some(path),
            decoded_width: 0,
            decoding_width: None,
        })
    }

    fn placeholder_entry(&self) -> Entry {
        Entry {
            shown: NoteImage {
                image: self.placeholder.clone(),
                natural: PLACEHOLDER_SIZE,
            },
            path: None,
            decoded_width: 0,
            decoding_width: None,
        }
    }

    /// Files to decode in the background, then hand to
    /// [`ImageStore::finish_decode`].
    pub fn take_decodes(&mut self) -> Vec<Decode> {
        std::mem::take(&mut self.decodes)
    }

    /// Shows a finished decode, unless a sharper one was asked for since.
    /// `None` (the file wouldn't decode) shows the placeholder.
    pub fn finish_decode(&mut self, decode: &Decode, image: Option<RenderImage>) {
        let placeholder = self.placeholder_entry();
        let Some(entry) = self.by_target.get_mut(&decode.target) else {
            return;
        };
        if entry.decoding_width != Some(decode.width) {
            return;
        }
        entry.decoding_width = None;
        match image {
            Some(image) => {
                entry.decoded_width = image.size(0).width.0.max(0) as u32;
                entry.shown.image = Arc::new(image);
            }
            None => *entry = placeholder,
        }
    }

    /// The empty image drawn in place of one that's decoding.
    pub fn blank(&self) -> Arc<RenderImage> {
        self.blank.clone()
    }

    /// Lets go of every decoded file's pixels, keeping their sizes; the
    /// next draw decodes them again.
    pub fn release_pixels(&mut self) {
        for entry in self.by_target.values_mut() {
            if entry.path.is_some() {
                entry.shown.image = self.blank.clone();
                entry.decoded_width = 0;
                entry.decoding_width = None;
            }
        }
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
    /// isn't an image, which leaves the placeholder.
    pub fn found_in_vault(&mut self, target: &str, path: &Path) -> bool {
        let Some(entry) = self.entry_for_file(path.to_path_buf()) else {
            return false;
        };
        self.missing.remove(target);
        self.by_target.insert(target.to_owned(), entry);
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

/// Decodes `decode`'s file, shrunk by the largest whole factor that keeps
/// it at least as wide as asked. Slow for a large file, so call it off the
/// main thread.
pub fn decode_file(decode: &Decode) -> Option<RenderImage> {
    let pixels = image::open(&decode.path).ok()?.to_rgba8();
    let factor = (pixels.width() / decode.width.max(1)).max(1);
    Some(render_image(shrink_by(pixels, factor)))
}

/// Averages each `factor` by `factor` block of pixels into one, weighting
/// colour by opacity so transparent pixels don't darken the edges around
/// them. Blocks at the right and bottom edges may be smaller.
fn shrink_by(pixels: RgbaImage, factor: u32) -> RgbaImage {
    if factor <= 1 {
        return pixels;
    }
    let (width, height) = pixels.dimensions();
    let (out_width, out_height) = (width.div_ceil(factor), height.div_ceil(factor));
    let mut out = Vec::with_capacity(out_width as usize * out_height as usize * CHANNELS);
    let mut sums = vec![0u64; out_width as usize * CHANNELS];
    let rows = pixels.as_raw().chunks_exact(width as usize * CHANNELS);
    for (y, row) in (0u32..).zip(rows) {
        for (x, pixel) in row.chunks_exact(CHANNELS).enumerate() {
            let at = x / factor as usize * CHANNELS;
            add_weighted(&mut sums[at..at + CHANNELS], pixel);
        }
        if (y + 1) % factor == 0 || y + 1 == height {
            let block_rows = y % factor + 1;
            average_into(&mut out, &mut sums, width, factor, block_rows);
        }
    }
    RgbaImage::from_raw(out_width, out_height, out).unwrap_or(pixels)
}

/// Adds one pixel to a block's sums: colour times opacity, then opacity.
fn add_weighted(sums: &mut [u64], pixel: &[u8]) {
    let alpha = u64::from(pixel[3]);
    for channel in 0..3 {
        sums[channel] += u64::from(pixel[channel]) * alpha;
    }
    sums[3] += alpha;
}

/// Turns a row of blocks' sums into pixels, and clears the sums.
fn average_into(out: &mut Vec<u8>, sums: &mut [u64], width: u32, factor: u32, rows: u32) {
    for (column, sum) in (0u32..).zip(sums.chunks_exact_mut(CHANNELS)) {
        let columns = (width - column * factor).min(factor);
        let count = u64::from(columns * rows);
        let alpha = sum[3];
        for weighted in &sum[..3] {
            let colour = (weighted + alpha / 2).checked_div(alpha).unwrap_or(0);
            out.push(colour as u8);
        }
        out.push(((alpha + count / 2) / count) as u8);
        sum.fill(0);
    }
}

/// The size an image is drawn at: the width written in the note (`|300`)
/// or its natural size, scaled by the view's zoom, never wider than
/// `max_width`, keeping its aspect ratio unless a height is written too.
pub fn display_size(
    natural: (u32, u32),
    requested: (Option<u32>, Option<u32>),
    zoom: f32,
    max_width: Pixels,
) -> Size<Pixels> {
    let natural = (natural.0.max(1) as f32, natural.1.max(1) as f32);
    let aspect = (natural.0 / natural.1).clamp(1. / MAX_ASPECT_RATIO, MAX_ASPECT_RATIO);
    let wanted_width = requested.0.map_or(natural.0, |width| width as f32) * zoom;
    let wanted_height = match requested {
        (Some(_), Some(height)) => height as f32 * zoom,
        _ => wanted_width / aspect,
    };
    let shrink = (f32::from(max_width.max(px(1.))) / wanted_width).min(1.);
    size(px(wanted_width * shrink), px(wanted_height * shrink))
}

/// GPUI wants BGRA.
pub(crate) fn render_image(mut pixels: RgbaImage) -> RenderImage {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    RenderImage::new(vec![Frame::new(pixels)])
}

/// The stand-in for an image that isn't there (yet): a faint see-through
/// grey with a diagonal hatch, which reads as empty on a light or a dark
/// page, where the old colour gradient read as a picture.
fn placeholder_pixels() -> RgbaImage {
    const HATCH_EVERY: u32 = 12;
    const HATCH_WIDTH: u32 = 2;
    let (width, height) = PLACEHOLDER_SIZE;
    RgbaImage::from_fn(width, height, |x, y| {
        let on_hatch = (x + y) % HATCH_EVERY < HATCH_WIDTH;
        let alpha = if on_hatch { 72 } else { 36 };
        image::Rgba([128, 128, 128, alpha])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("editor-images-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Decodes what the store queued, as the view does in the background.
    fn run_decodes(store: &mut ImageStore) {
        for decode in store.take_decodes() {
            let image = decode_file(&decode);
            store.finish_decode(&decode, image);
        }
    }

    #[test]
    fn the_placeholder_is_a_see_through_grey() {
        let pixels = placeholder_pixels();
        for pixel in pixels.pixels() {
            let [red, green, blue, alpha] = pixel.0;
            assert!(red == green && green == blue, "no colour");
            assert!(alpha < 128, "see-through");
        }
    }

    #[test]
    fn missing_images_use_the_placeholder() {
        let mut store = ImageStore::new(vec![]);
        let image = store.image("missing.png", 100);
        assert_eq!(image.natural, PLACEHOLDER_SIZE);
        assert_eq!(image.image.size(0).width.0, PLACEHOLDER_SIZE.0 as i32);
        assert!(Arc::ptr_eq(
            &image.image,
            &store.image("missing.png", 100).image
        ));
        assert!(store.take_decodes().is_empty());
    }

    #[test]
    fn missing_images_wait_for_the_vault_index() {
        let dir = temp_dir("vault");
        std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
        let found = dir.join("elsewhere/pic.png");
        RgbaImage::new(3, 2).save(&found).unwrap();
        let mut store = ImageStore::new(vec![dir.join("notes")]);
        assert_eq!(store.note_dir(), Some(dir.join("notes").as_path()));

        assert!(Arc::ptr_eq(
            &store.image("pic.png", 3).image,
            &store.placeholder
        ));
        assert_eq!(store.take_vault_lookups(), ["pic.png"]);
        store.image("pic.png", 3);
        assert!(store.take_vault_lookups().is_empty(), "asked once");

        assert!(!store.found_in_vault("pic.png", &dir.join("nothing.png")));
        assert!(store.found_in_vault("pic.png", &found));
        assert_eq!(store.image("pic.png", 3).natural, (3, 2));
        run_decodes(&mut store);
        assert_eq!(store.image("pic.png", 3).image.size(0).width.0, 3);
        assert!(!store.retry_missing(), "nothing is missing now");

        store.image("gone.png", 3);
        assert!(store.retry_missing());
        assert!(store.take_vault_lookups().is_empty());
        store.image("gone.png", 3);
        assert_eq!(
            store.take_vault_lookups(),
            ["gone.png"],
            "a retry looks it up again"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn images_decode_no_wider_than_drawn_and_sharpen_when_drawn_wider() {
        let dir = temp_dir("sizes");
        RgbaImage::new(2000, 1000)
            .save(dir.join("wide.png"))
            .unwrap();
        let mut store = ImageStore::new(vec![dir.clone()]);
        let pending = store.image("wide.png", 600);
        assert_eq!(pending.natural, (2000, 1000), "sized before it decodes");
        assert!(Arc::ptr_eq(&pending.image, &store.blank));
        run_decodes(&mut store);
        // A third of the width is the smallest whole shrink that's 600 wide.
        let drawn = store.image("wide.png", 600).image.size(0);
        assert_eq!((drawn.width.0, drawn.height.0), (667, 334));
        store.image("wide.png", 650);
        assert!(store.take_decodes().is_empty(), "sharp enough already");
        store.image("wide.png", 1500);
        run_decodes(&mut store);
        assert_eq!(store.image("wide.png", 1500).image.size(0).width.0, 2000);
        store.image("wide.png", 4000);
        assert!(store.take_decodes().is_empty(), "never wider than the file");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn shrinking_averages_blocks_by_opacity() {
        let mut pixels = RgbaImage::new(3, 2);
        pixels.put_pixel(0, 0, image::Rgba([200, 100, 0, 255]));
        pixels.put_pixel(1, 0, image::Rgba([0, 0, 0, 0]));
        pixels.put_pixel(0, 1, image::Rgba([100, 100, 100, 255]));
        pixels.put_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
        pixels.put_pixel(2, 0, image::Rgba([10, 20, 30, 255]));
        pixels.put_pixel(2, 1, image::Rgba([30, 40, 50, 255]));
        let shrunk = shrink_by(pixels, 2);
        assert_eq!(shrunk.dimensions(), (2, 1));
        // Transparent pixels add no colour, only lower the opacity.
        assert_eq!(shrunk.get_pixel(0, 0).0, [150, 100, 50, 128]);
        // The right edge's block is one column wide.
        assert_eq!(shrunk.get_pixel(1, 0).0, [20, 30, 40, 255]);
    }

    #[test]
    fn released_pixels_decode_again_when_drawn() {
        let dir = temp_dir("release");
        RgbaImage::new(40, 20).save(dir.join("pic.png")).unwrap();
        let mut store = ImageStore::new(vec![dir.clone()]);
        store.image("pic.png", 40);
        run_decodes(&mut store);
        store.release_pixels();
        let released = store.image("pic.png", 40);
        assert!(Arc::ptr_eq(&released.image, &store.blank));
        assert_eq!(released.natural, (40, 20));
        assert_eq!(store.take_decodes().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stale_decode_never_replaces_a_sharper_one() {
        let dir = temp_dir("stale");
        RgbaImage::new(2000, 1000)
            .save(dir.join("wide.png"))
            .unwrap();
        let mut store = ImageStore::new(vec![dir.clone()]);
        store.image("wide.png", 300);
        let small = store.take_decodes();
        store.image("wide.png", 1500);
        let large = store.take_decodes();
        for decode in large.iter().chain(&small) {
            let image = decode_file(decode);
            store.finish_decode(decode, image);
        }
        assert_eq!(store.image("wide.png", 1500).image.size(0).width.0, 2000);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_that_will_not_decode_shows_the_placeholder() {
        let dir = temp_dir("broken");
        RgbaImage::new(40, 20).save(dir.join("pic.png")).unwrap();
        let mut store = ImageStore::new(vec![dir.clone()]);
        store.image("pic.png", 40);
        std::fs::write(dir.join("pic.png"), b"not a png any more").unwrap();
        run_decodes(&mut store);
        assert!(Arc::ptr_eq(
            &store.image("pic.png", 40).image,
            &store.placeholder
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn display_size_keeps_the_aspect_ratio() {
        let image = (40, 20);
        let natural = display_size(image, (None, None), 1., px(1000.));
        assert_eq!((natural.width, natural.height), (px(40.), px(20.)));
        let wide = display_size(image, (Some(300), None), 1., px(1000.));
        assert_eq!((wide.width, wide.height), (px(300.), px(150.)));
        let capped = display_size(image, (Some(300), None), 1., px(100.));
        assert_eq!((capped.width, capped.height), (px(100.), px(50.)));
        let zoomed = display_size(image, (Some(300), None), 1.5, px(1000.));
        assert_eq!(zoomed.width, px(450.));
        let explicit = display_size(image, (Some(100), Some(100)), 1., px(1000.));
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
        store.image(&name, 800);
        run_decodes(&mut store);
        let image = store.image(&name, 800).image;
        assert!(!Arc::ptr_eq(&image, &store.placeholder));
        assert!(!Arc::ptr_eq(&image, &store.blank));
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
