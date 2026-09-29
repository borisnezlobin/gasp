//! Asking GPUI for frames and waiting for them to settle.

use std::path::Path;
use std::time::{Duration, Instant};

use gpui::AsyncApp;

use super::metal_capture::LayerCapture;

/// How often a frame is asked for while the view settles.
pub(super) const FRAME_INTERVAL: Duration = Duration::from_millis(30);
/// How long the view has to go without drawing to count as settled.
const SETTLE_TIME: Duration = Duration::from_millis(400);
/// The longest a snapshot waits for the view to settle before it takes
/// whatever was drawn last.
pub(super) const SETTLE_LIMIT: Duration = Duration::from_secs(30);

/// Asks for frames until none has been needed for [`SETTLE_TIME`] and no
/// equation, code block or image is still being drawn in the background.
/// Answers false when the view was still changing at [`SETTLE_LIMIT`].
pub(super) async fn settle(capture: &LayerCapture, cx: &mut AsyncApp) -> bool {
    let started = Instant::now();
    let mut last_change = Instant::now();
    let mut drawn = capture.frames_drawn();
    while started.elapsed() < SETTLE_LIMIT {
        capture.request_frame();
        let busy = crate::pending_renders::pending() > 0;
        if capture.frames_drawn() != drawn || busy {
            drawn = capture.frames_drawn();
            last_change = Instant::now();
        } else if drawn > 0 && last_change.elapsed() >= SETTLE_TIME {
            return true;
        }
        cx.background_executor().timer(FRAME_INTERVAL).await;
    }
    false
}

/// Settles, warning when the view never stopped changing.
pub(super) async fn settle_or_warn(capture: &LayerCapture, cx: &mut AsyncApp) {
    if !settle(capture, cx).await {
        eprintln!(
            "{} --snapshot: the view was still changing after {}s, so this is its last frame",
            gasp_config::COMMAND_NAME,
            SETTLE_LIMIT.as_secs()
        );
    }
}

/// Keeps drawing frames as they're needed for `duration`, as the screen
/// would while someone waits.
pub(super) async fn keep_drawing(capture: &LayerCapture, duration: Duration, cx: &mut AsyncApp) {
    let started = Instant::now();
    loop {
        capture.request_frame();
        let left = duration.saturating_sub(started.elapsed());
        if left.is_zero() {
            return;
        }
        cx.background_executor()
            .timer(left.min(FRAME_INTERVAL))
            .await;
    }
}

/// Writes the last frame drawn to `path` as a PNG.
pub(super) fn save_last_frame(capture: &LayerCapture, path: &Path) -> Result<(), String> {
    let image = capture.last_frame()?;
    image
        .save(path)
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}
