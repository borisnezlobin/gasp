//! Where a keystroke's time goes. With `EDITOR_TRACE_KEYS=1` the editor
//! times named phases of every frame that follows an edit — the input
//! pipeline, the source update, planning, laying out lines, prose and so
//! on — and the layout bench prints their percentiles next to
//! input-to-paint. Off, a span costs one load of a flag.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::stats::Samples;

const ENV_VAR: &str = "EDITOR_TRACE_KEYS";

static ENABLED: OnceLock<bool> = OnceLock::new();

#[derive(Default)]
struct Phases {
    /// Time spent in each phase since the last frame ended.
    current: BTreeMap<&'static str, Duration>,
    /// One sample per keystroke for every phase seen so far.
    samples: BTreeMap<&'static str, Samples>,
    keystrokes: usize,
}

thread_local! {
    static PHASES: RefCell<Phases> = RefCell::new(Phases::default());
}

/// Whether phases are being timed.
pub fn enabled() -> bool {
    *ENABLED
        .get_or_init(|| std::env::var(ENV_VAR).is_ok_and(|value| !value.is_empty() && value != "0"))
}

/// Times a phase until dropped: `let _phase = keytrace::span("plan");`.
#[must_use = "a span measures until it is dropped"]
pub struct Span {
    name: &'static str,
    started: Instant,
}

/// Starts timing `name`, or does nothing when tracing is off.
pub fn span(name: &'static str) -> Option<Span> {
    enabled().then(|| Span {
        name,
        started: Instant::now(),
    })
}

impl Drop for Span {
    fn drop(&mut self) {
        let took = self.started.elapsed();
        PHASES.with_borrow_mut(|phases| {
            *phases.current.entry(self.name).or_default() += took;
        });
    }
}

/// Ends a frame. When it followed an edit, its phases become one sample
/// each; otherwise they're dropped, so scrolling doesn't count.
pub fn end_frame(after_input: bool) {
    if !enabled() {
        return;
    }
    PHASES.with_borrow_mut(|phases| {
        let current = std::mem::take(&mut phases.current);
        if after_input {
            phases.record(&current);
        }
    });
}

impl Phases {
    fn record(&mut self, current: &BTreeMap<&'static str, Duration>) {
        for name in current.keys() {
            let keystrokes = self.keystrokes;
            self.samples.entry(name).or_insert_with(|| {
                let mut samples = Samples::default();
                (0..keystrokes).for_each(|_| samples.push(Duration::ZERO));
                samples
            });
        }
        for (name, samples) in &mut self.samples {
            samples.push(current.get(name).copied().unwrap_or_default());
        }
        self.keystrokes += 1;
    }
}

/// Forgets every sample, as when the bench starts measuring.
pub fn clear() {
    PHASES.with_borrow_mut(|phases| *phases = Phases::default());
}

/// Each phase's percentiles, slowest first, or `None` when off.
pub fn report() -> Option<String> {
    if !enabled() {
        return None;
    }
    PHASES.with_borrow(|phases| {
        let mut rows: Vec<_> = phases
            .samples
            .iter()
            .filter_map(|(name, samples)| Some((*name, samples.summary()?)))
            .collect();
        rows.sort_by_key(|(_, summary)| std::cmp::Reverse((summary.p50, summary.p95)));
        let lines: Vec<String> = rows
            .iter()
            .map(|(name, summary)| format!("  {name}: {summary}"))
            .collect();
        Some(format!("-- keystroke phases --\n{}", lines.join("\n")))
    })
}
