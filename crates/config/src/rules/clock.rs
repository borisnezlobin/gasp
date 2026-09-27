//! Time sources for delayed rules. Times are durations since an arbitrary start.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// Supplies the current time to the rules engine.
pub trait Clock {
    fn now(&self) -> Duration;
}

/// A clock that only moves when told to. Use it in tests.
#[derive(Debug, Default)]
pub struct ManualClock(Cell<Duration>);

impl ManualClock {
    pub fn new() -> ManualClock {
        ManualClock::default()
    }

    pub fn advance(&self, by: Duration) {
        self.0.set(self.0.get() + by);
    }

    pub fn advance_ms(&self, millis: u64) {
        self.advance(Duration::from_millis(millis));
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

/// Wall-clock time since the clock was created.
#[derive(Debug)]
pub struct SystemClock(Instant);

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock(Instant::now())
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}
