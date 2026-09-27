//! The layout benchmark: types into a long note, then scrolls through it,
//! one step per frame, and reports frame timings.

use std::time::{Duration, Instant};

use gpui::{App, Context, Entity, Window};

use crate::editor::EditorView;
use crate::stats::Timings;

const TYPING_TEXT: &str = "the quick brown fox ";

/// How much to type and scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchConfig {
    /// Keystrokes typed before measuring starts.
    pub warmup: usize,
    pub keystrokes: usize,
    pub scroll_pages: usize,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            warmup: 20,
            keystrokes: 300,
            scroll_pages: 100,
        }
    }
}

/// What one bench step does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchAction {
    Type(char),
    ScrollPage,
}

/// A phase boundary reached at a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checkpoint {
    StartTyping,
    TypingDone,
    ScrollingDone,
}

/// One step: an optional checkpoint, then an action, or `None` when done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchStep {
    pub checkpoint: Option<Checkpoint>,
    pub action: Option<BenchAction>,
}

/// Measurements from a finished run.
#[derive(Clone, Debug, Default)]
pub struct BenchResults {
    pub lines: usize,
    pub bytes: usize,
    pub full_layout: Duration,
    pub typing: Timings,
    pub scrolling: Timings,
}

impl BenchResults {
    pub fn report(&self) -> String {
        format!(
            "note: {} lines, {} bytes\nfull layout of every line: {:.1}ms\n\
             -- typing (one character per frame) --\n{}\n\
             -- scrolling (one page per frame) --\n{}",
            self.lines,
            self.bytes,
            self.full_layout.as_secs_f64() * 1000.,
            self.typing.report(),
            self.scrolling.report()
        )
    }
}

/// A running benchmark.
#[derive(Clone, Debug)]
pub struct Bench {
    config: BenchConfig,
    step: usize,
    pub results: BenchResults,
}

impl Bench {
    pub fn new(config: BenchConfig) -> Self {
        Self {
            config,
            step: 0,
            results: BenchResults::default(),
        }
    }

    pub fn next_step(&mut self) -> BenchStep {
        let step = self.step;
        self.step += 1;
        let typing_end = self.config.warmup + self.config.keystrokes;
        let scrolling_end = typing_end + self.config.scroll_pages;
        let checkpoint = [
            (self.config.warmup, Checkpoint::StartTyping),
            (typing_end, Checkpoint::TypingDone),
            (scrolling_end, Checkpoint::ScrollingDone),
        ]
        .iter()
        .find(|(at, _)| *at == step)
        .map(|(_, checkpoint)| *checkpoint);
        let action = if step < typing_end {
            Some(BenchAction::Type(typing_char(step)))
        } else {
            (step < scrolling_end).then_some(BenchAction::ScrollPage)
        };
        BenchStep { checkpoint, action }
    }
}

fn typing_char(step: usize) -> char {
    let bytes = TYPING_TEXT.as_bytes();
    char::from(bytes[step % bytes.len()])
}

impl EditorView {
    /// Starts a benchmark: lays out every line once, puts the cursor in
    /// the middle of the note, then steps once per frame.
    pub fn start_bench(
        &mut self,
        config: BenchConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut bench = Bench::new(config);
        bench.results.lines = self.doc().line_count();
        bench.results.bytes = self.doc().len();
        let started = Instant::now();
        for line in 0..self.doc().line_count() {
            self.layout_doc_line(line, window);
        }
        bench.results.full_layout = started.elapsed();
        let middle = self.doc().line_end(self.doc().line_count() / 2);
        self.move_to(middle, false, cx);
        self.bench = Some(bench);
        cx.notify();
    }

    pub(crate) fn schedule_bench_step(view: &Entity<Self>, window: &mut Window, cx: &mut App) {
        if view.read(cx).bench.is_none() {
            return;
        }
        let view = view.clone();
        window.on_next_frame(move |window, cx| {
            view.update(cx, |view, cx| view.bench_step(window, cx));
        });
    }

    fn bench_step(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(bench) = self.bench.as_mut() else {
            return;
        };
        let step = bench.next_step();
        if let Some(checkpoint) = step.checkpoint {
            self.bench_checkpoint(checkpoint);
        }
        match step.action {
            Some(BenchAction::Type(character)) => {
                self.insert(character.encode_utf8(&mut [0; 4]), cx)
            }
            Some(BenchAction::ScrollPage) => self.scroll_page(cx),
            None => self.finish_bench(cx),
        }
    }

    fn bench_checkpoint(&mut self, checkpoint: Checkpoint) {
        let timings = std::mem::take(&mut self.timings);
        let Some(bench) = self.bench.as_mut() else {
            return;
        };
        match checkpoint {
            Checkpoint::StartTyping => {}
            Checkpoint::TypingDone => bench.results.typing = timings,
            Checkpoint::ScrollingDone => bench.results.scrolling = timings,
        }
    }

    fn scroll_page(&mut self, cx: &mut Context<Self>) {
        let page = self
            .frame
            .as_ref()
            .map_or(gpui::px(600.), |frame| frame.bounds.size.height);
        self.scroll_by(page, cx);
    }

    fn finish_bench(&mut self, cx: &mut Context<Self>) {
        if let Some(bench) = self.bench.take() {
            println!("{}", bench.results.report());
        }
        cx.quit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_through_warmup_typing_and_scrolling() {
        let mut bench = Bench::new(BenchConfig {
            warmup: 1,
            keystrokes: 2,
            scroll_pages: 1,
        });
        let steps: Vec<BenchStep> = (0..5).map(|_| bench.next_step()).collect();
        let checkpoints: Vec<Option<Checkpoint>> =
            steps.iter().map(|step| step.checkpoint).collect();
        assert_eq!(
            checkpoints,
            vec![
                None,
                Some(Checkpoint::StartTyping),
                None,
                Some(Checkpoint::TypingDone),
                Some(Checkpoint::ScrollingDone)
            ]
        );
        assert_eq!(steps[0].action, Some(BenchAction::Type('t')));
        assert_eq!(steps[2].action, Some(BenchAction::Type('e')));
        assert_eq!(steps[3].action, Some(BenchAction::ScrollPage));
        assert_eq!(steps[4].action, None);
    }

    #[test]
    fn report_lists_each_phase() {
        let report = BenchResults::default().report();
        assert!(report.contains("typing"));
        assert!(report.contains("scrolling"));
    }
}
