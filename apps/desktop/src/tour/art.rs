//! The tour's whale: the icon's humpback turned to breach, and a strip of
//! frames of it swimming. Both come in ink for the light theme and chalk
//! for the dark one (`assets/tour/build_whales.py` makes them), and are
//! decoded off the main thread when the tour opens and handed back to the
//! window's atlas when it closes.

use std::sync::Arc;
use std::time::Duration;

use gpui::{Corners, IntoElement, Pixels, RenderImage, Styled, Window, canvas, px};
use image::{Frame, RgbaImage, imageops};

const SWIM_LIGHT: &[u8] = include_bytes!("../../assets/tour/swim-light.png");
const SWIM_DARK: &[u8] = include_bytes!("../../assets/tour/swim-dark.png");
const BREACH_LIGHT: &[u8] = include_bytes!("../../assets/tour/breach-light.png");
const BREACH_DARK: &[u8] = include_bytes!("../../assets/tour/breach-dark.png");

/// Frames in the swimming strip, laid side by side.
const SWIM_FRAMES: u32 = 30;
/// How long each swimming frame shows: the kit's cruise loop at 30 frames
/// a second, with every other frame kept.
pub const SWIM_FRAME_TIME: Duration = Duration::from_millis(66);

/// The decoded whales for one theme.
pub struct WhaleArt {
    pub swim: Arc<RenderImage>,
    pub breach: Arc<RenderImage>,
}

impl WhaleArt {
    /// Decodes the whales for `dark` or light. Slow enough to keep off the
    /// main thread.
    pub fn decode(dark: bool) -> Option<WhaleArt> {
        let (swim, breach) = if dark {
            (SWIM_DARK, BREACH_DARK)
        } else {
            (SWIM_LIGHT, BREACH_LIGHT)
        };
        let strip = image::load_from_memory(swim).ok()?.to_rgba8();
        let breach = image::load_from_memory(breach).ok()?.to_rgba8();
        Some(WhaleArt {
            swim: Arc::new(RenderImage::new(split_strip(strip))),
            breach: Arc::new(RenderImage::new(vec![bgra_frame(breach)])),
        })
    }

    /// Gives the window's atlas back the room the whales took.
    pub fn release(&self, window: &mut Window) {
        window.drop_image(self.swim.clone()).ok();
        window.drop_image(self.breach.clone()).ok();
    }

    /// The swimming frame to show `elapsed` after the tour opened.
    pub fn swim_frame(&self, elapsed: Duration) -> usize {
        let frames = self.swim.frame_count().max(1) as u128;
        (elapsed.as_millis() / SWIM_FRAME_TIME.as_millis() % frames) as usize
    }
}

fn split_strip(strip: RgbaImage) -> Vec<Frame> {
    let width = strip.width() / SWIM_FRAMES;
    (0..SWIM_FRAMES)
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
pub fn drawn(image: &Arc<RenderImage>, frame: usize, width: Pixels) -> impl IntoElement {
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
            assert!(height_at(&art.breach, px(100.)) > px(50.));
        }
    }

    #[test]
    fn the_swim_loops() {
        let art = WhaleArt::decode(false).unwrap();
        assert_eq!(art.swim_frame(Duration::ZERO), 0);
        assert_eq!(art.swim_frame(SWIM_FRAME_TIME * 31), 1);
    }
}
