//! Names that let tests and snapshot scripts find an element on screen.
//!
//! [`Selectable::selector`] names an element. GPUI's own
//! `debug_selector` only records bounds in its test builds, and keeps them
//! where only its test context can read them, so the name is given to it
//! for tests and, in a snapshot run, also to a probe: an empty canvas laid
//! over the element that records the element's bounds as each frame is
//! laid out. Outside a snapshot run no probe is added and nothing is
//! recorded, so the app draws exactly the elements it would without
//! names.
//!
//! A frame's names are read once it has been presented, so a lookup
//! always sees one whole frame rather than the half of one being drawn.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use gpui::{Bounds, InteractiveElement, ParentElement, Pixels, Styled, canvas};

thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static FRAMES: RefCell<RecordedFrames> = RefCell::new(RecordedFrames::default());
}

/// Every named element's bounds in window coordinates, by name.
pub type SelectorBounds = BTreeMap<String, Bounds<Pixels>>;

#[derive(Default)]
struct RecordedFrames {
    /// Frames presented so far.
    presented: u64,
    /// The frame whose names are being recorded, counted as `presented`
    /// was when it started.
    recording_frame: u64,
    recording: SelectorBounds,
    /// The whole frame before the one being recorded.
    previous: SelectorBounds,
}

impl RecordedFrames {
    fn record(&mut self, name: String, bounds: Bounds<Pixels>) {
        if self.recording_frame != self.presented {
            self.previous = std::mem::take(&mut self.recording);
            self.recording_frame = self.presented;
        }
        self.recording.insert(name, bounds);
    }

    fn last_presented(&self) -> &SelectorBounds {
        if self.presented > self.recording_frame {
            &self.recording
        } else {
            &self.previous
        }
    }
}

/// Names elements for tests and snapshot scripts.
pub trait Selectable: InteractiveElement + ParentElement + Sized {
    /// Names this element. Tests find it with `debug_bounds`, and snapshot
    /// scripts with `bounds`, `hover-element` and `click-element`.
    fn selector(self, name: impl FnOnce() -> String) -> Self {
        if !RECORDING.get() {
            return self.debug_selector(name);
        }
        let name = name();
        let probe = bounds_probe(name.clone());
        self.debug_selector(move || name).child(probe)
    }
}

impl<E: InteractiveElement + ParentElement> Selectable for E {}

fn bounds_probe(name: String) -> impl gpui::IntoElement {
    canvas(
        move |bounds, _, _| FRAMES.with_borrow_mut(|frames| frames.record(name, bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Records named elements' bounds from now on, on this thread.
pub fn record_selectors() {
    RECORDING.set(true);
}

/// Tells the recorder a frame reached the screen.
pub fn frame_presented() {
    FRAMES.with_borrow_mut(|frames| frames.presented += 1);
}

/// The named elements in the last frame presented.
pub fn selectors_in_last_frame() -> SelectorBounds {
    FRAMES.with_borrow(|frames| frames.last_presented().clone())
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, size};

    use super::*;

    fn at(x: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(0.)), size(px(10.), px(10.)))
    }

    #[test]
    fn lookups_see_the_last_whole_frame() {
        let mut frames = RecordedFrames::default();
        frames.record("a".into(), at(1.));
        frames.record("b".into(), at(2.));
        assert!(frames.last_presented().is_empty());
        frames.presented += 1;
        assert_eq!(frames.last_presented().len(), 2);
        frames.record("a".into(), at(3.));
        assert_eq!(frames.last_presented().get("b"), Some(&at(2.)));
        frames.presented += 1;
        assert_eq!(frames.last_presented().get("a"), Some(&at(3.)));
        assert_eq!(frames.last_presented().get("b"), None);
    }
}
