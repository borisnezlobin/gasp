//! Rendered math for the math widgets. LaTeX goes through `gasp-math`
//! (mitex and Typst) to SVG, which is rasterised with resvg and tinted with
//! the text colour, all on a background thread.
//!
//! GPUI 0.2.2 can turn SVG bytes into an image (`Image::to_image_data`), but
//! only at 1x and without converting to the BGRA order its images use, and
//! the size type of its `SvgRenderer` isn't public. Calling resvg (the
//! version GPUI already builds) gives crisp, correctly coloured equations.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gasp_math::{MathError, RenderedMath, evict_layout_memory, render_latex};
use gpui::{Hsla, Pixels, RenderImage, Rgba, SharedString, px};
use image::{Frame, RgbaImage};
use resvg::{tiny_skia, usvg};

/// Renders LaTeX (`tex`, display mode, font size) to SVG.
pub type RenderFn =
    Arc<dyn Fn(&str, bool, f64) -> Result<Arc<RenderedMath>, MathError> + Send + Sync>;

/// Rasterise at twice the device resolution so edges stay smooth.
const OVERSAMPLE: f32 = 2.;

/// Rasterised equations one store keeps, in bytes, before it lets go of
/// those drawn least recently: a long, math-heavy note's worth. One that's
/// scrolled back to renders again.
const KEPT_BYTES: usize = 32 << 20;

/// Typst memoises layouts; every this many renders, the ones not used
/// lately are let go so memory doesn't grow with every equation seen.
const RENDERS_BETWEEN_EVICTIONS: usize = 200;
const EVICTION_AGE: usize = 2;

static RENDERS: AtomicUsize = AtomicUsize::new(0);

/// One equation at one size, scale and colour.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MathKey {
    pub tex: String,
    pub display: bool,
    font_size_bits: u32,
    scale_bits: u32,
    color: u32,
}

impl MathKey {
    pub fn new(tex: &str, display: bool, font_size: Pixels, scale: f32, color: Hsla) -> Self {
        Self {
            tex: tex.to_owned(),
            display,
            font_size_bits: f32::from(font_size).to_bits(),
            scale_bits: scale.to_bits(),
            color: pack(color),
        }
    }

    pub fn font_size(&self) -> f32 {
        f32::from_bits(self.font_size_bits)
    }

    fn scale(&self) -> f32 {
        f32::from_bits(self.scale_bits)
    }
}

fn pack(color: Hsla) -> u32 {
    let rgba = Rgba::from(color);
    [rgba.r, rgba.g, rgba.b, rgba.a]
        .iter()
        .fold(0, |packed, channel| {
            (packed << 8) | (channel * 255.).round() as u32
        })
}

/// A rasterised equation and where its baseline is. Sizes are in pixels.
#[derive(Clone, Debug)]
pub struct MathImage {
    pub image: Arc<RenderImage>,
    pub width: Pixels,
    pub height: Pixels,
    /// Distance from the top edge to the baseline.
    pub baseline: Pixels,
}

/// Where an equation's render is.
#[derive(Clone, Debug)]
pub enum MathState {
    Pending,
    Ready(Arc<MathImage>),
    Failed(SharedString),
}

/// A render to run off the main thread.
pub struct MathRequest {
    pub key: MathKey,
    render: RenderFn,
}

impl MathRequest {
    /// Renders and rasterises the equation. Safe to call on any thread.
    pub fn run(&self) -> MathState {
        let key = &self.key;
        let rendered = (self.render)(&key.tex, key.display, f64::from(key.font_size()));
        if RENDERS
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(RENDERS_BETWEEN_EVICTIONS)
        {
            evict_layout_memory(EVICTION_AGE);
        }
        let image = rendered
            .map_err(|error| error.message().to_owned())
            .and_then(|rendered| rasterize(&rendered, key));
        match image {
            Ok(image) => MathState::Ready(Arc::new(image)),
            Err(message) => MathState::Failed(message.into()),
        }
    }
}

/// Rendered equations by key, plus the renders still to start. Only
/// equations looked up since the last [`MathStore::begin_frame`] start, so
/// laying out lines off screen (to move the cursor, say) renders nothing.
/// Past [`KEPT_BYTES`] of rendered equations, those not drawn lately are
/// let go.
pub struct MathStore {
    render: RenderFn,
    entries: HashMap<MathKey, Entry>,
    started: HashSet<MathKey>,
    queued: Vec<MathKey>,
    /// Counts [`MathStore::begin_frame`]s.
    frame: u64,
    /// The pixels of every ready equation, in bytes.
    ready_bytes: usize,
    kept_bytes: usize,
}

struct Entry {
    state: MathState,
    /// The frame it was last looked up in.
    last_used: u64,
}

/// The bytes a render's pixels take.
fn pixel_bytes(state: &MathState) -> usize {
    match state {
        MathState::Ready(image) => {
            let size = image.image.size(0);
            size.width.0.max(0) as usize * size.height.0.max(0) as usize * 4
        }
        MathState::Pending | MathState::Failed(_) => 0,
    }
}

impl Default for MathStore {
    /// Renders with Typst. The store caches by source, size, scale and
    /// colour itself, so renders call `render_latex` directly and run in
    /// parallel rather than queueing on a shared `MathCache`.
    fn default() -> Self {
        Self::with_renderer(Arc::new(|tex, display, font_size| {
            render_latex(tex, display, font_size).map(Arc::new)
        }))
    }
}

impl MathStore {
    /// A store that renders with `render` instead of Typst, as tests do.
    pub fn with_renderer(render: RenderFn) -> Self {
        Self {
            render,
            entries: HashMap::new(),
            started: HashSet::new(),
            queued: Vec::new(),
            frame: 0,
            ready_bytes: 0,
            kept_bytes: KEPT_BYTES,
        }
    }

    /// Keeps `bytes` of rendered equations instead of [`KEPT_BYTES`].
    pub fn keeping(mut self, bytes: usize) -> Self {
        self.kept_bytes = bytes;
        self
    }

    /// The equation's state, queueing its render if it hasn't started.
    pub fn lookup(&mut self, key: MathKey) -> MathState {
        let frame = self.frame;
        let entry = self.entries.entry(key.clone()).or_insert(Entry {
            state: MathState::Pending,
            last_used: frame,
        });
        entry.last_used = frame;
        let state = entry.state.clone();
        let waiting = matches!(state, MathState::Pending) && !self.started.contains(&key);
        if waiting && !self.queued.contains(&key) {
            self.queued.push(key);
        }
        state
    }

    /// The state of some equation with this source, at any size or colour.
    pub fn find(&self, tex: &str) -> Option<&MathState> {
        self.entries
            .iter()
            .find(|(key, _)| key.tex == tex)
            .map(|(_, entry)| &entry.state)
    }

    /// Forgets renders queued by earlier layouts; the frame about to be laid
    /// out queues the ones it shows.
    pub fn begin_frame(&mut self) {
        self.queued.clear();
        self.frame += 1;
    }

    /// Renders to start now.
    pub fn take_requests(&mut self) -> Vec<MathRequest> {
        let keys: Vec<MathKey> = self.queued.drain(..).collect();
        self.started.extend(keys.iter().cloned());
        keys.into_iter()
            .map(|key| MathRequest {
                key,
                render: self.render.clone(),
            })
            .collect()
    }

    /// Stores a finished render.
    pub fn finish(&mut self, key: MathKey, state: MathState) {
        self.started.remove(&key);
        self.ready_bytes += pixel_bytes(&state);
        let last_used = self
            .entries
            .get(&key)
            .map_or(self.frame, |old| old.last_used);
        let replaced = self.entries.insert(key, Entry { state, last_used });
        if let Some(old) = replaced {
            self.ready_bytes -= pixel_bytes(&old.state);
        }
        if self.ready_bytes > self.kept_bytes {
            self.let_go_of_unused();
        }
    }

    /// Lets go of every rendered equation; the next draw renders them again.
    pub fn release_renders(&mut self) {
        self.entries
            .retain(|_, entry| !matches!(entry.state, MathState::Ready(_)));
        self.ready_bytes = 0;
    }

    /// Lets go of the equations drawn least recently, but none the last
    /// frame drew, until a quarter of what it keeps is free.
    fn let_go_of_unused(&mut self) {
        let mut unused: Vec<(u64, MathKey, usize)> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.last_used < self.frame)
            .map(|(key, entry)| (entry.last_used, key.clone(), pixel_bytes(&entry.state)))
            .filter(|(_, _, bytes)| *bytes > 0)
            .collect();
        unused.sort_unstable_by_key(|(last_used, _, _)| *last_used);
        for (_, key, bytes) in unused {
            if self.ready_bytes <= self.kept_bytes / 4 * 3 {
                break;
            }
            self.entries.remove(&key);
            self.ready_bytes -= bytes;
        }
    }

    /// Whether no render is queued or running.
    pub fn is_idle(&self) -> bool {
        self.queued.is_empty() && self.started.is_empty()
    }
}

/// The SVG's coverage, filled with the key's colour, in GPUI's BGRA order.
fn rasterize(rendered: &RenderedMath, key: &MathKey) -> Result<MathImage, String> {
    let tree = usvg::Tree::from_str(&rendered.svg, &usvg::Options::default())
        .map_err(|error| error.to_string())?;
    let pixels_per_point = key.scale() * OVERSAMPLE;
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
    let [red, green, blue, alpha] = key.color.to_be_bytes();
    let mut pixels = Vec::with_capacity(pixmap.pixels().len() * 4);
    for pixel in pixmap.pixels() {
        let coverage = u16::from(pixel.alpha()) * u16::from(alpha) / 255;
        pixels.extend([blue, green, red, coverage as u8]);
    }
    let buffer = RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixels)
        .ok_or("the rasterised equation has the wrong size")?;
    Ok(MathImage {
        image: Arc::new(RenderImage::new(vec![Frame::new(buffer)])),
        width: px(rendered.width as f32),
        height: px(rendered.height as f32),
        baseline: px(rendered.baseline as f32),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A renderer that draws a box `4 * tex.len()` wide and 10 high, with
    /// its baseline 8 from the top. `\bad` fails.
    pub fn stub_renderer() -> RenderFn {
        Arc::new(|tex, _display, _size| {
            if tex.contains("\\bad") {
                return Err(MathError::Convert("unknown command".into()));
            }
            let width = 4. * tex.len() as f64;
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"10\" \
                 viewBox=\"0 0 {width} 10\"><rect width=\"{width}\" height=\"10\"/></svg>"
            );
            Ok(Arc::new(RenderedMath {
                svg,
                width,
                height: 10.,
                baseline: 8.,
            }))
        })
    }

    fn key(tex: &str) -> MathKey {
        MathKey::new(tex, false, px(16.), 1., gpui::black())
    }

    fn run_all(store: &mut MathStore) {
        for request in store.take_requests() {
            let state = request.run();
            store.finish(request.key, state);
        }
    }

    #[test]
    fn lookups_queue_one_render_each() {
        let mut store = MathStore::with_renderer(stub_renderer());
        assert!(matches!(store.lookup(key("x")), MathState::Pending));
        assert!(matches!(store.lookup(key("x")), MathState::Pending));
        assert!(!store.is_idle());
        let requests = store.take_requests();
        assert_eq!(requests.len(), 1);
        let state = requests[0].run();
        store.finish(requests[0].key.clone(), state);
        let MathState::Ready(image) = store.lookup(key("x")) else {
            panic!("the render finished");
        };
        assert_eq!(image.width, px(4.));
        assert_eq!(image.baseline, px(8.));
        assert_eq!(image.image.size(0).width.0, 8);
        assert!(store.is_idle());
    }

    #[test]
    fn renders_start_only_for_the_frame_being_drawn() {
        let mut store = MathStore::with_renderer(stub_renderer());
        store.lookup(key("off screen"));
        store.begin_frame();
        store.lookup(key("x"));
        let requests = store.take_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].key.tex, "x");
        assert!(
            store.take_requests().is_empty(),
            "started renders aren't queued again"
        );
        store.lookup(key("off screen"));
        assert_eq!(store.take_requests().len(), 1, "a later frame starts it");
    }

    #[test]
    fn rasterised_pixels_take_the_text_colour() {
        let mut store = MathStore::with_renderer(stub_renderer());
        let red = gpui::hsla(0., 1., 0.5, 1.);
        let key = MathKey::new("xy", false, px(16.), 1., red);
        store.lookup(key.clone());
        run_all(&mut store);
        let MathState::Ready(image) = store.lookup(key) else {
            panic!("the render finished");
        };
        let bytes = image.image.as_bytes(0).unwrap();
        assert_eq!(&bytes[..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn failures_keep_their_message() {
        let mut store = MathStore::with_renderer(stub_renderer());
        store.lookup(key("\\bad"));
        run_all(&mut store);
        let MathState::Failed(message) = store.lookup(key("\\bad")) else {
            panic!("the render failed");
        };
        assert_eq!(message.as_ref(), "unknown command");
    }

    #[test]
    fn equations_not_drawn_lately_are_let_go_past_the_limit() {
        // Each stub render of a 25-letter source is 200 by 20 pixels.
        let bytes = 200 * 20 * 4;
        let mut store = MathStore::with_renderer(stub_renderer()).keeping(bytes * 4);
        let source = |n: usize| format!("{n:0>25}");
        for n in 0..4 {
            store.begin_frame();
            store.lookup(key(&source(n)));
            run_all(&mut store);
        }
        assert!(
            store
                .entries
                .values()
                .all(|entry| pixel_bytes(&entry.state) == bytes)
        );
        store.begin_frame();
        store.lookup(key(&source(0)));
        store.lookup(key(&source(4)));
        run_all(&mut store);
        assert!(store.ready_bytes <= bytes * 3, "back under three quarters");
        let kept = |n: usize| store.entries.contains_key(&key(&source(n)));
        assert!(kept(0) && kept(4), "the frame's own equations stay");
        assert!(!kept(1) && !kept(2), "the oldest go first");
        assert!(kept(3));
        assert!(matches!(store.lookup(key(&source(1))), MathState::Pending));
    }

    #[test]
    fn keys_differ_by_size_and_colour() {
        let base = key("x");
        assert_ne!(base, MathKey::new("x", false, px(20.), 1., gpui::black()));
        assert_ne!(base, MathKey::new("x", false, px(16.), 1., gpui::white()));
        assert_eq!(base.font_size(), 16.);
    }

    #[test]
    fn real_equations_render() {
        let mut store = MathStore::default();
        let key = key("x^2");
        store.lookup(key.clone());
        run_all(&mut store);
        let MathState::Ready(image) = store.lookup(key) else {
            panic!("x^2 renders");
        };
        assert!(image.width > px(0.) && image.baseline <= image.height);
    }

    #[test]
    fn equations_with_descenders_keep_their_depth() {
        let mut store = MathStore::default();
        let key = key("\\det(P) \\neq 0");
        store.lookup(key.clone());
        run_all(&mut store);
        let MathState::Ready(image) = store.lookup(key) else {
            panic!("the equation renders");
        };
        assert!(
            image.baseline < image.height - px(1.),
            "baseline {:?} of {:?}",
            image.baseline,
            image.height
        );
    }
}
