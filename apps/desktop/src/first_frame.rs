//! Work that can wait until the app's first frame is on screen.
//!
//! Rendering math starts Typst, which parses its fonts and then compiles
//! every visible equation on all cores. Started while the first frame is
//! still being drawn, it competes with that frame for the CPU and the
//! window takes longer to appear. Views [`defer`] such work while
//! [`is_waiting`]; equations show their placeholder in the first frame.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{App, Window};

type Work = Box<dyn FnOnce(&mut App)>;

/// The longest work waits for a first frame.
const MAX_WAIT: Duration = Duration::from_secs(2);

/// Set from launch until the first window's first frame is presented.
static WAITING: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Work deferred while waiting; it all runs on the main thread.
    static DEFERRED: RefCell<Vec<Work>> = const { RefCell::new(Vec::new()) };
}

/// Starts holding work back. The app's launch calls this before opening
/// its first window, so windows in tests never wait.
pub fn hold() {
    WAITING.store(true, Ordering::Relaxed);
}

/// Runs the deferred work once `window` has presented a frame, or after
/// [`MAX_WAIT`] should no frame come (a window that never maps).
pub fn release_when_presented(window: &Window, cx: &mut App) {
    window.on_next_frame(|_, cx| {
        // Tasks run once the frame callback returns, which is after the
        // frame is drawn and presented.
        cx.spawn(async move |cx| cx.update(release).ok()).detach();
    });
    cx.spawn(async move |cx| {
        cx.background_executor().timer(MAX_WAIT).await;
        cx.update(release).ok();
    })
    .detach();
}

/// Stops holding work back and runs what was deferred.
pub fn release(cx: &mut App) {
    WAITING.store(false, Ordering::Relaxed);
    for work in DEFERRED.with_borrow_mut(std::mem::take) {
        work(cx);
    }
}

/// Whether work is being held back.
pub fn is_waiting() -> bool {
    WAITING.load(Ordering::Relaxed)
}

/// Runs `work` when the first frame is on screen. Call it only while
/// [`is_waiting`]; otherwise do the work now.
pub fn defer(work: impl FnOnce(&mut App) + 'static) {
    DEFERRED.with_borrow_mut(|deferred| deferred.push(Box::new(work)));
}
