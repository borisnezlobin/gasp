//! Drawing that's still on its way from a background thread: equations,
//! code colours, web images and hover previews. A snapshot waits for none
//! to be left before it captures a frame.

use std::sync::atomic::{AtomicUsize, Ordering};

static PENDING: AtomicUsize = AtomicUsize::new(0);

/// One render on its way; it counts as done when this is dropped, which
/// should be after its result is in the view.
pub struct PendingRender(());

impl PendingRender {
    pub fn start() -> Self {
        PENDING.fetch_add(1, Ordering::Relaxed);
        Self(())
    }
}

impl Drop for PendingRender {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::Relaxed);
    }
}

/// How many renders are on their way.
pub fn pending() -> usize {
    PENDING.load(Ordering::Relaxed)
}
