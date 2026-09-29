use std::time::Duration;

use crate::clock::Stopwatch;

/// Durations of repeated runs of one measured operation.
#[derive(Default, Clone)]
pub struct Samples {
    durations: Vec<Duration>,
}

impl Samples {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `operation` `runs` times and records each run.
    pub fn collect<T>(runs: usize, mut operation: impl FnMut() -> T) -> Self {
        let mut samples = Self::new();
        for _ in 0..runs {
            samples.time(&mut operation);
        }
        samples
    }

    /// Runs `operation` once, records how long it took and returns its result.
    pub fn time<T>(&mut self, operation: impl FnOnce() -> T) -> T {
        let started = Stopwatch::start();
        let result = std::hint::black_box(operation());
        self.durations.push(started.elapsed());
        result
    }

    pub fn push(&mut self, duration: Duration) {
        self.durations.push(duration);
    }

    pub fn len(&self) -> usize {
        self.durations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.durations.is_empty()
    }

    pub fn median(&self) -> Duration {
        self.quantile(0.5)
    }

    pub fn p95(&self) -> Duration {
        self.quantile(0.95)
    }

    pub fn max(&self) -> Duration {
        self.quantile(1.)
    }

    pub fn min(&self) -> Duration {
        self.quantile(0.)
    }

    pub fn total(&self) -> Duration {
        self.durations.iter().sum()
    }

    /// The duration at `fraction` of the way through the sorted runs, or zero
    /// with no runs.
    pub fn quantile(&self, fraction: f64) -> Duration {
        let mut sorted = self.durations.clone();
        sorted.sort();
        let Some(last) = sorted.len().checked_sub(1) else {
            return Duration::ZERO;
        };
        sorted[(last as f64 * fraction.clamp(0., 1.)).round() as usize]
    }
}

/// A duration as milliseconds or microseconds, whichever reads better.
pub fn format_duration(duration: Duration) -> String {
    let micros = duration.as_secs_f64() * 1e6;
    if micros >= 1000. {
        format!("{:.2} ms", micros / 1000.)
    } else {
        format!("{micros:.1} µs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_come_from_the_sorted_runs() {
        let mut samples = Samples::new();
        for millis in [5, 1, 3, 2, 4] {
            samples.push(Duration::from_millis(millis));
        }
        assert_eq!(samples.median(), Duration::from_millis(3));
        assert_eq!(samples.min(), Duration::from_millis(1));
        assert_eq!(samples.max(), Duration::from_millis(5));
        assert_eq!(samples.total(), Duration::from_millis(15));
    }

    #[test]
    fn no_runs_read_as_zero() {
        assert_eq!(Samples::new().p95(), Duration::ZERO);
    }

    #[test]
    fn durations_format_in_the_readable_unit() {
        assert_eq!(format_duration(Duration::from_micros(55)), "55.0 µs");
        assert_eq!(format_duration(Duration::from_micros(20_500)), "20.50 ms");
    }
}
