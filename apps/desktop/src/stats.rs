//! Timing samples and their percentiles.

use std::fmt;
use std::time::Duration;

/// A list of timing samples for one measurement.
#[derive(Clone, Debug, Default)]
pub struct Samples {
    durations: Vec<Duration>,
}

/// The summary of a list of samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub count: usize,
    pub p50: Duration,
    pub p95: Duration,
    pub max: Duration,
}

impl Samples {
    pub fn push(&mut self, duration: Duration) {
        self.durations.push(duration);
    }

    pub fn len(&self) -> usize {
        self.durations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.durations.is_empty()
    }

    pub fn clear(&mut self) {
        self.durations.clear();
    }

    /// Nearest-rank percentiles, or `None` without samples.
    pub fn summary(&self) -> Option<Summary> {
        let mut sorted = self.durations.clone();
        sorted.sort_unstable();
        let max = *sorted.last()?;
        Some(Summary {
            count: sorted.len(),
            p50: percentile(&sorted, 50),
            p95: percentile(&sorted, 95),
            max,
        })
    }
}

fn percentile(sorted: &[Duration], percent: usize) -> Duration {
    let rank = (sorted.len() * percent).div_ceil(100).max(1);
    sorted[rank - 1]
}

impl fmt::Display for Summary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "n={} p50={:.3}ms p95={:.3}ms max={:.3}ms",
            self.count,
            millis(self.p50),
            millis(self.p95),
            millis(self.max)
        )
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn empty_samples_have_no_summary() {
        assert_eq!(Samples::default().summary(), None);
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut samples = Samples::default();
        for value in (1..=100).rev() {
            samples.push(ms(value));
        }
        let summary = samples.summary().unwrap();
        assert_eq!(summary.p50, ms(50));
        assert_eq!(summary.p95, ms(95));
        assert_eq!(summary.max, ms(100));
        assert_eq!(summary.count, 100);
    }

    #[test]
    fn a_single_sample_is_every_percentile() {
        let mut samples = Samples::default();
        samples.push(ms(3));
        let summary = samples.summary().unwrap();
        assert_eq!(
            (summary.p50, summary.p95, summary.max),
            (ms(3), ms(3), ms(3))
        );
        assert_eq!(
            summary.to_string(),
            "n=1 p50=3.000ms p95=3.000ms max=3.000ms"
        );
    }
}

/// Per-frame timings the editor view records.
#[derive(Clone, Debug, Default)]
pub struct Timings {
    /// Laying out the visible lines (styling, shaping, geometry).
    pub layout: Samples,
    /// The part of layout spent on sentence tints and grammar flags.
    pub prose: Samples,
    /// Painting the laid-out lines.
    pub paint: Samples,
    /// From an edit to the end of the next frame's paint.
    pub input_to_paint: Samples,
    pub(crate) input_started: Option<std::time::Instant>,
}

impl Timings {
    pub fn clear(&mut self) {
        self.layout.clear();
        self.prose.clear();
        self.paint.clear();
        self.input_to_paint.clear();
    }

    pub fn report(&self) -> String {
        [
            ("layout", &self.layout),
            ("  of which prose", &self.prose),
            ("paint", &self.paint),
            ("input-to-paint", &self.input_to_paint),
        ]
        .iter()
        .filter_map(|(name, samples)| Some(format!("{name}: {}", samples.summary()?)))
        .collect::<Vec<_>>()
        .join("\n")
    }
}
