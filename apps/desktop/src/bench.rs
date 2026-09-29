//! The layout benchmark: types into a long note, then scrolls through it,
//! one step per frame, and reports frame timings.

use std::time::{Duration, Instant};

use gpui::{App, Context, Entity, Window};

use crate::editor::EditorView;
use crate::stats::Timings;

const TYPING_TEXT: &str = "the quick brown fox ";

/// What `--in-math` types: letters, digits and operators that snippets
/// and the math helpers look at on every key.
const MATH_TEXT: &str = "x2 + ab - cd ";

/// How much to type and scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchConfig {
    /// Keystrokes typed before measuring starts.
    pub warmup: usize,
    pub keystrokes: usize,
    pub scroll_pages: usize,
    /// Type at the end of a code block's first line instead of in the
    /// middle of the note, to measure highlighting.
    pub in_code: bool,
    /// Type math at the end of a math block's first line, to measure
    /// snippets and the math helpers.
    pub in_math: bool,
    /// Type in the first body cell of the first table after the middle,
    /// to measure the table grid.
    pub in_table: bool,
    /// Whether sentence tints and grammar flags are on, to measure what
    /// they cost.
    pub prose: bool,
    /// Draw into a window that's never shown, asking for each frame
    /// instead of waiting for the display (macOS).
    pub hidden: bool,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            warmup: 20,
            keystrokes: 300,
            scroll_pages: 100,
            in_code: false,
            in_math: false,
            in_table: false,
            prose: true,
            hidden: false,
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
            let text = if self.config.in_math {
                MATH_TEXT
            } else {
                TYPING_TEXT
            };
            Some(BenchAction::Type(typing_char(text, step)))
        } else {
            (step < scrolling_end).then_some(BenchAction::ScrollPage)
        };
        BenchStep { checkpoint, action }
    }
}

fn typing_char(text: &str, step: usize) -> char {
    let bytes = text.as_bytes();
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
        let fence = match (config.in_code, config.in_math) {
            (true, _) => Some("\n```"),
            (_, true) => Some("\n$$"),
            _ => None,
        };
        let start = fence
            .and_then(|fence| self.block_line_after(middle, fence))
            .or_else(|| {
                config
                    .in_table
                    .then(|| self.table_cell_after(middle))
                    .flatten()
            })
            .unwrap_or(middle);
        self.move_to(start, false, cx);
        self.bench = Some(bench);
        cx.notify();
    }

    /// The end of the first line inside the next block after `offset`
    /// that opens with `fence` at the start of a line, such as a code
    /// block or a `$$` math block.
    fn block_line_after(&self, offset: usize, fence: &str) -> Option<usize> {
        let text = self.source.text();
        let fence = offset + text[offset..].find(fence)? + 1;
        let first = self.doc().line_of_offset(fence) + 1;
        Some(self.doc().line_end(first))
    }

    /// The end of the first body cell's text in the first table after
    /// `offset`.
    fn table_cell_after(&self, offset: usize) -> Option<usize> {
        let text = self.source.text();
        let row = offset + text[offset..].find("\n|")? + 1;
        let table = gasp_core::table::Table::at(text, self.source.tree(), row)?;
        let cell = gasp_core::table::CellPos::new(1, 0);
        table.content(text, cell).map(|content| content.end)
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
            // Through the input pipeline, as a keystroke goes.
            Some(BenchAction::Type(character)) => {
                self.type_text(character.encode_utf8(&mut [0; 4]), cx)
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
            Checkpoint::StartTyping => crate::keytrace::clear(),
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
            if let Some(phases) = crate::keytrace::report() {
                println!("{phases}");
            }
            if let Some(memory) = crate::memory::report() {
                println!("{memory}");
            }
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
            ..BenchConfig::default()
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
