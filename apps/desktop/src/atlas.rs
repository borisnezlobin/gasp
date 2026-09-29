//! Letting go of painted images.
//!
//! An image painted into a window is copied into GPUI's texture atlas and
//! stays there, in video memory, until the app drops it: GPUI never does.
//! Equations, note images and link card pictures are dropped from their
//! caches when they're no longer needed (a note closes, an equation isn't
//! drawn for a while), so every image painted is held here too, and once
//! this is the only hold left it's dropped from every window's atlas.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, Global, ImageId, RenderImage, Window};

/// Paints between looks for images nothing else holds.
const PAINTS_BETWEEN_SWEEPS: usize = 30;
/// Looks an image must go unheld for before it's dropped, so one a cache
/// let go of this frame isn't taken from under a frame still being drawn.
const SWEEPS_BEFORE_DROPPING: u8 = 2;

#[derive(Default)]
struct PaintedImages {
    images: HashMap<ImageId, Painted>,
    paints_since_sweep: usize,
}

struct Painted {
    image: Arc<RenderImage>,
    unheld_sweeps: u8,
}

impl Global for PaintedImages {}

/// Notes that `image` was painted.
pub fn painted(image: &Arc<RenderImage>, cx: &mut App) {
    let painted = cx.default_global::<PaintedImages>();
    painted
        .images
        .entry(image.id)
        .or_insert_with(|| Painted {
            image: image.clone(),
            unheld_sweeps: 0,
        })
        .unheld_sweeps = 0;
}

/// Drops the images nothing else holds from every window's atlas, looking
/// once every [`PAINTS_BETWEEN_SWEEPS`] calls. Call after painting.
pub fn sweep(window: &mut Window, cx: &mut App) {
    let painted = cx.default_global::<PaintedImages>();
    painted.paints_since_sweep += 1;
    if painted.paints_since_sweep < PAINTS_BETWEEN_SWEEPS {
        return;
    }
    painted.paints_since_sweep = 0;
    for image in unheld_images(&mut painted.images) {
        cx.drop_image(image, Some(window));
    }
}

/// Counts another look for each image only this module holds, and takes
/// out the ones unheld long enough.
fn unheld_images(images: &mut HashMap<ImageId, Painted>) -> Vec<Arc<RenderImage>> {
    let mut unheld = Vec::new();
    images.retain(|_, painted| {
        if Arc::strong_count(&painted.image) > 1 {
            painted.unheld_sweeps = 0;
            return true;
        }
        painted.unheld_sweeps += 1;
        if painted.unheld_sweeps < SWEEPS_BEFORE_DROPPING {
            return true;
        }
        unheld.push(painted.image.clone());
        false
    });
    unheld
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Arc<RenderImage> {
        Arc::new(crate::images::render_image(image::RgbaImage::new(2, 2)))
    }

    #[test]
    fn only_images_nothing_holds_are_dropped_after_a_grace_look() {
        let held = image();
        let let_go = image();
        let mut images: HashMap<ImageId, Painted> = [&held, &let_go]
            .into_iter()
            .map(|image| {
                let painted = Painted {
                    image: image.clone(),
                    unheld_sweeps: 0,
                };
                (image.id, painted)
            })
            .collect();
        let let_go_id = let_go.id;
        drop(let_go);
        assert!(unheld_images(&mut images).is_empty(), "one look of grace");
        let dropped = unheld_images(&mut images);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].id, let_go_id);
        assert!(images.contains_key(&held.id));
        assert!(!images.contains_key(&let_go_id));
    }
}
