//! The tour's whale: the website's humpback gliding through moving water
//! on the first step, and a strip of frames of the kit's whale swimming
//! along the bottom of the others. Both come in ink for the light theme
//! and chalk for the dark one (`assets/tour/build_whales.py` makes them),
//! and are decoded off the main thread when the tour opens and handed back
//! to the window's atlas when it closes.

use std::sync::Arc;
use std::time::Duration;

use gpui::{Corners, IntoElement, Pixels, RenderImage, Styled, Window, canvas, px};
use image::{Frame, RgbaImage, imageops};

const SWIM_LIGHT: &[u8] = include_bytes!("../../assets/tour/swim-light.png");
const SWIM_DARK: &[u8] = include_bytes!("../../assets/tour/swim-dark.png");
const GLIDE_LIGHT: &[u8] = include_bytes!("../../assets/tour/glide-light.png");
const GLIDE_DARK: &[u8] = include_bytes!("../../assets/tour/glide-dark.png");
const STILL_LIGHT: &[u8] = include_bytes!("../../assets/tour/still-light.png");
const STILL_DARK: &[u8] = include_bytes!("../../assets/tour/still-dark.png");

/// Frames in the swimming strip, laid side by side.
const SWIM_FRAMES: u32 = 30;
/// How long each swimming frame shows: the kit's cruise loop at 30 frames
/// a second, with every other frame kept.
pub const SWIM_FRAME_TIME: Duration = Duration::from_millis(66);
/// Frames in the gliding strip, one loop of the water.
const GLIDE_FRAMES: u32 = 32;
/// How long each gliding frame leads into the next: six seconds a loop.
const GLIDE_FRAME_TIME: Duration = Duration::from_millis(187);
/// The room the build left round the gliding whale for the water to
/// push it into, as a share of the whale's width.
pub const GLIDE_PAD: f32 = 10. / 760.;

/// Two neighbouring frames of a loop and how far from the first to the
/// second the clock is, to draw the second over the first at that
/// opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blend {
    pub from: usize,
    pub to: usize,
    pub toward: f32,
}

/// The decoded whales for one theme.
pub struct WhaleArt {
    pub swim: Arc<RenderImage>,
    pub glide: Arc<RenderImage>,
}

impl WhaleArt {
    /// Decodes the whales for `dark` or light. Slow enough to keep off the
    /// main thread.
    pub fn decode(dark: bool) -> Option<WhaleArt> {
        let (swim, glide) = if dark {
            (SWIM_DARK, GLIDE_DARK)
        } else {
            (SWIM_LIGHT, GLIDE_LIGHT)
        };
        let swim = image::load_from_memory(swim).ok()?.to_rgba8();
        let glide = image::load_from_memory(glide).ok()?.to_rgba8();
        Some(WhaleArt {
            swim: Arc::new(RenderImage::new(split_strip(swim, SWIM_FRAMES))),
            glide: Arc::new(RenderImage::new(split_strip(glide, GLIDE_FRAMES))),
        })
    }

    /// The swimming whale's first frame on its own, for places that show
    /// it still, such as an empty vault's new tab.
    pub fn still(dark: bool) -> Option<Arc<RenderImage>> {
        let bytes = if dark { STILL_DARK } else { STILL_LIGHT };
        let still = image::load_from_memory(bytes).ok()?.to_rgba8();
        Some(Arc::new(RenderImage::new(vec![bgra_frame(still)])))
    }

    /// Gives the window's atlas back the room the whales took.
    pub fn release(&self, window: &mut Window) {
        window.drop_image(self.swim.clone()).ok();
        window.drop_image(self.glide.clone()).ok();
    }

    /// The swimming frame to show `elapsed` after the tour opened.
    pub fn swim_frame(&self, elapsed: Duration) -> usize {
        let frames = self.swim.frame_count().max(1) as u128;
        (elapsed.as_millis() / SWIM_FRAME_TIME.as_millis() % frames) as usize
    }

    /// The gliding frames to show `elapsed` into the water's loop.
    pub fn glide_blend(&self, elapsed: Duration) -> Blend {
        let frames = self.glide.frame_count().max(1);
        let beats = elapsed.as_secs_f32() / GLIDE_FRAME_TIME.as_secs_f32();
        let from = beats.floor() as usize % frames;
        Blend {
            from,
            to: (from + 1) % frames,
            toward: beats.fract(),
        }
    }
}

fn split_strip(strip: RgbaImage, frames: u32) -> Vec<Frame> {
    let width = strip.width() / frames;
    (0..frames)
        .map(|index| {
            let frame = imageops::crop_imm(&strip, index * width, 0, width, strip.height());
            bgra_frame(frame.to_image())
        })
        .collect()
}

/// GPUI's images are BGRA.
fn bgra_frame(mut pixels: RgbaImage) -> Frame {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Frame::new(pixels)
}

/// How tall `image` stands at `width`.
pub fn height_at(image: &RenderImage, width: Pixels) -> Pixels {
    let size = image.size(0);
    if size.width.0 == 0 {
        return px(0.);
    }
    width * (size.height.0 as f32 / size.width.0 as f32)
}

/// One frame of `image`, `width` wide at its own proportions.
pub fn drawn(image: &Arc<RenderImage>, frame: usize, width: Pixels) -> impl IntoElement + use<> {
    let height = height_at(image, width);
    let image = image.clone();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window
                .paint_image(bounds, Corners::default(), image.clone(), frame, false)
                .ok();
        },
    )
    .w(width)
    .h(height)
    .flex_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_themes_decode_into_whole_strips() {
        for dark in [false, true] {
            let art = WhaleArt::decode(dark).unwrap();
            assert_eq!(art.swim.frame_count(), SWIM_FRAMES as usize);
            assert_eq!(art.glide.frame_count(), GLIDE_FRAMES as usize);
            assert!(height_at(&art.glide, px(100.)) > px(30.));
        }
    }

    #[test]
    fn the_swim_loops() {
        let art = WhaleArt::decode(false).unwrap();
        assert_eq!(art.swim_frame(Duration::ZERO), 0);
        assert_eq!(art.swim_frame(SWIM_FRAME_TIME * 31), 1);
    }

    #[test]
    fn the_glide_blends_round_its_loop() {
        let art = WhaleArt::decode(false).unwrap();
        let start = art.glide_blend(Duration::ZERO);
        assert_eq!((start.from, start.to, start.toward), (0, 1, 0.));
        let last = art.glide_blend(GLIDE_FRAME_TIME * (GLIDE_FRAMES - 1) + GLIDE_FRAME_TIME / 2);
        assert_eq!((last.from, last.to), (GLIDE_FRAMES as usize - 1, 0));
        assert!((last.toward - 0.5).abs() < 0.01);
    }
}
