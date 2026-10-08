//! How far along a motion is, and the curves the tour moves on. Motions
//! are worked out from the clock at each frame rather than kept in state,
//! so a frame drawn late lands where it should.

use std::time::{Duration, Instant};

/// How far through `duration` the clock is since `start`, from 0 to 1.
pub fn progress(start: Instant, now: Instant, duration: Duration) -> f32 {
    if duration.is_zero() {
        return 1.;
    }
    let elapsed = now.saturating_duration_since(start).as_secs_f32();
    (elapsed / duration.as_secs_f32()).clamp(0., 1.)
}

/// Fast out of the gate, settling gently: for things arriving.
pub fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

/// Slow to start and slow to stop: for the whale swimming between steps.
pub fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4. * t * t * t
    } else {
        1. - (-2. * t + 2.).powi(3) / 2.
    }
}

pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Whether a caret blinking since `since` shows at `now`: on for half a
/// second, off for half a second.
pub fn caret_on(since: Instant, now: Instant) -> bool {
    (now.saturating_duration_since(since).as_millis() / 530).is_multiple_of(2)
}

/// How long until the blinking caret next turns on or off.
pub fn until_caret_flips(since: Instant, now: Instant) -> Duration {
    let into = now.saturating_duration_since(since).as_millis() % 530;
    Duration::from_millis((530 - into) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_runs_from_zero_to_one() {
        let start = Instant::now();
        let second = Duration::from_secs(1);
        assert_eq!(progress(start, start, second), 0.);
        assert_eq!(progress(start, start + second / 2, second), 0.5);
        assert_eq!(progress(start, start + second * 3, second), 1.);
        assert_eq!(progress(start, start, Duration::ZERO), 1.);
    }

    #[test]
    fn curves_start_and_end_in_place() {
        for curve in [ease_out, ease_in_out] {
            assert_eq!(curve(0.), 0.);
            assert!((curve(1.) - 1.).abs() < 1e-6);
        }
    }
}
