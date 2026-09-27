//! The editor view: an `editor-core` state plus everything needed to draw
//! it and take input.

use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use editor_core::document::{Document, Selection, SelectionRange};
use editor_core::history::EditorState;
use editor_core::pipeline::{EditRequest, Pipeline};
use editor_core::syntax;
use editor_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::{App, Bounds, Context, FocusHandle, Focusable, Pixels, Point, Window, px};

use crate::actions::ClickUnit;
use crate::bench::Bench;
use crate::frame::{FrameLayout, PlacedLine};
use crate::images::ImageStore;
use crate::line_layout::{LineInput, VisualLine, layout_line};
use crate::metrics::LineMetrics;
use crate::stats::Timings;
use crate::theme::Theme;

/// Edits between timing reports when logging is on.
const TIMING_LOG_INTERVAL: usize = 100;

/// A live-preview Markdown editor view.
pub struct EditorView {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) state: EditorState,
    pub(crate) marked: Option<Range<usize>>,
    pub(crate) metrics: LineMetrics,
    pub(crate) theme: Theme,
    pub(crate) images: ImageStore,
    pub(crate) scroll_y: Pixels,
    pub(crate) goal_x: Option<Pixels>,
    pub(crate) is_selecting: bool,
    pub(crate) click_unit: ClickUnit,
    pub(crate) click_origin: Range<usize>,
    pub(crate) autoscroll: bool,
    pub(crate) frame: Option<FrameLayout>,
    pub(crate) timings: Timings,
    pub(crate) bench: Option<Bench>,
    pub(crate) log_timings: bool,
    pipeline: Pipeline,
    clock: Instant,
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EditorView {
    /// A view of `text`. Images are looked up in `image_dirs`.
    pub fn new(text: &str, image_dirs: Vec<PathBuf>, cx: &mut Context<Self>) -> Self {
        let theme = Theme::default();
        let doc = Document::from(text);
        Self {
            focus_handle: cx.focus_handle(),
            metrics: LineMetrics::build(&doc, &theme),
            state: EditorState::new(doc),
            marked: None,
            theme,
            images: ImageStore::new(image_dirs),
            scroll_y: px(0.),
            goal_x: None,
            is_selecting: false,
            click_unit: ClickUnit::Character,
            click_origin: 0..0,
            autoscroll: false,
            frame: None,
            timings: Timings::default(),
            bench: None,
            log_timings: false,
            pipeline: Pipeline::builtin(),
            clock: Instant::now(),
        }
    }

    pub fn text(&self) -> String {
        self.state.doc().to_string()
    }

    pub fn doc(&self) -> &Document {
        self.state.doc()
    }

    /// The primary selection as an ordered byte range.
    pub fn selected_range(&self) -> Range<usize> {
        self.state.selection().primary().range()
    }

    pub fn cursor(&self) -> usize {
        self.state.selection().primary().head
    }

    pub fn anchor(&self) -> usize {
        self.state.selection().primary().anchor
    }

    /// The IME composition, if one is in progress.
    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn timings(&self) -> &Timings {
        &self.timings
    }

    pub fn frame(&self) -> Option<&FrameLayout> {
        self.frame.as_ref()
    }

    pub fn set_log_timings(&mut self, enabled: bool) {
        self.log_timings = enabled;
    }

    pub(crate) fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }

    /// Sets the selection and keeps the head on screen.
    pub fn select(&mut self, anchor: usize, head: usize, cx: &mut Context<Self>) {
        let doc = self.state.doc();
        let range = SelectionRange::new(
            doc.floor_char_boundary(anchor),
            doc.floor_char_boundary(head),
        );
        let transaction =
            Transaction::select(Selection::single(range), Origin::Input, self.now_ms());
        self.state
            .apply(transaction)
            .expect("a selection-only transaction always applies");
        self.autoscroll = true;
        cx.notify();
    }

    /// Moves the head, keeping the anchor when `extend` is set.
    pub fn move_to(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        let anchor = if extend { self.anchor() } else { offset };
        self.select(anchor, offset, cx);
    }

    /// Replaces `range` with `text`, leaves the cursor after it and
    /// returns the inserted range. Any composition ends.
    pub fn replace(
        &mut self,
        range: Range<usize>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Range<usize> {
        let doc = self.state.doc();
        let range = doc.floor_char_boundary(range.start)
            ..doc.floor_char_boundary(range.end.max(range.start));
        let old_lines = doc.line_of_offset(range.start)..doc.line_of_offset(range.end) + 1;
        let inserted = range.start..range.start + text.len();
        let transaction = Transaction::new(
            ChangeSet::replace(range, text),
            Origin::Input,
            self.now_ms(),
        )
        .with_selection(Selection::cursor(inserted.end));
        self.state
            .apply(transaction)
            .expect("ranges are clamped to character boundaries");
        let doc = self.state.doc();
        let new_lines = doc.line_of_offset(inserted.start)..doc.line_of_offset(inserted.end) + 1;
        self.metrics.splice(old_lines, new_lines, doc, &self.theme);
        self.marked = None;
        self.goal_x = None;
        self.autoscroll = true;
        self.timings.input_started.get_or_insert_with(Instant::now);
        cx.notify();
        inserted
    }

    /// Replaces the selection, or the composition while one is active.
    pub fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        let range = self.marked.clone().unwrap_or_else(|| self.selected_range());
        self.replace(range, text, cx);
    }

    /// Applies a transaction from a command or the input pipeline and
    /// re-measures the lines. Any composition ends.
    pub fn apply_transaction(&mut self, transaction: Transaction, cx: &mut Context<Self>) {
        if self.state.apply(transaction).is_err() {
            return;
        }
        self.goal_x = None;
        self.timings.input_started.get_or_insert_with(Instant::now);
        self.after_history_step(cx);
    }

    /// Runs a core editing command on the current document and selection.
    pub fn run_edit(
        &mut self,
        command: impl FnOnce(&Document, &Selection, u64) -> Transaction,
        cx: &mut Context<Self>,
    ) {
        let transaction = command(self.state.doc(), self.state.selection(), self.now_ms());
        self.apply_transaction(transaction, cx);
    }

    /// Deletes the selection, or the range `around` the cursor when nothing
    /// is selected.
    pub fn delete_or(
        &mut self,
        around: impl FnOnce(&Document, usize) -> Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let mut range = self.selected_range();
        if range.is_empty() {
            range = around(self.state.doc(), range.start);
        }
        if !range.is_empty() {
            self.replace(range, "", cx);
        }
    }

    /// Enter runs through the input pipeline so lists continue.
    pub fn newline(&mut self, cx: &mut Context<Self>) {
        let tree = syntax::parse(&self.state.doc().to_string());
        let transaction = self.pipeline.run(
            EditRequest::Newline,
            self.state.doc(),
            self.state.selection(),
            &tree,
            self.now_ms(),
        );
        match transaction {
            Some(transaction) => self.apply_transaction(transaction, cx),
            None => self.insert("\n", cx),
        }
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        if self.state.undo(self.now_ms()) {
            self.after_history_step(cx);
        }
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        if self.state.redo(self.now_ms()) {
            self.after_history_step(cx);
        }
    }

    fn after_history_step(&mut self, cx: &mut Context<Self>) {
        self.metrics = LineMetrics::build(self.state.doc(), &self.theme);
        self.marked = None;
        self.autoscroll = true;
        cx.notify();
    }

    /// Stores the frame just painted and records its timings.
    pub(crate) fn finish_frame(&mut self, frame: FrameLayout, paint_started: Instant) {
        self.timings.paint.push(paint_started.elapsed());
        self.frame = Some(frame);
        let Some(input_started) = self.timings.input_started.take() else {
            return;
        };
        self.timings.input_to_paint.push(input_started.elapsed());
        if self.log_timings
            && self
                .timings
                .input_to_paint
                .len()
                .is_multiple_of(TIMING_LOG_INTERVAL)
        {
            eprintln!("{}", self.timings.report());
        }
    }

    /// Scrolls by `delta` and clamps to the document.
    pub fn scroll_by(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        let viewport = self
            .frame
            .as_ref()
            .map_or(px(0.), |frame| frame.bounds.size.height);
        let max_scroll = (self.metrics.total_height() - viewport).max(px(0.));
        self.scroll_y = (self.scroll_y + delta).clamp(px(0.), max_scroll);
        cx.notify();
    }

    /// Scrolls just enough to show the cursor's row.
    pub(crate) fn apply_autoscroll(&mut self, viewport_height: Pixels) {
        if !std::mem::take(&mut self.autoscroll) {
            return;
        }
        let line = self.state.doc().line_of_offset(self.cursor());
        let top = self.metrics.top_of(line);
        let bottom = top + self.metrics.height(line);
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if bottom > self.scroll_y + viewport_height {
            self.scroll_y = bottom - viewport_height;
        }
    }

    /// Lays out the lines visible in `bounds`.
    pub(crate) fn layout_frame(&mut self, bounds: Bounds<Pixels>, window: &Window) -> FrameLayout {
        let padding = self.theme.text_padding;
        let viewport_height = (bounds.size.height - padding * 2.).max(px(0.));
        self.apply_autoscroll(viewport_height);
        let visible = self.metrics.visible(self.scroll_y, viewport_height);
        let mut top = bounds.top() + padding + visible.first_top;
        let mut lines = Vec::with_capacity(visible.end - visible.first);
        for line in visible.first..visible.end {
            let visual = self.layout_doc_line(line, window);
            let height = visual.height;
            lines.push(PlacedLine { top, visual });
            top += height;
        }
        FrameLayout {
            bounds,
            text_left: bounds.left() + padding,
            lines,
        }
    }

    /// Shapes one document line.
    pub(crate) fn layout_doc_line(&mut self, line: usize, window: &Window) -> VisualLine {
        let doc = self.state.doc();
        let range = doc.line_range(line);
        let text = doc.slice(range.clone());
        let cursor = self.cursor();
        let input = LineInput {
            text: &text,
            line,
            start: range.start,
            row_height: self.metrics.height(line),
            cursor: (range.start <= cursor && cursor <= range.end).then(|| cursor - range.start),
            marked: clip_to_line(self.marked.as_ref(), &range),
        };
        layout_line(&input, &self.theme, &mut self.images, window.text_system())
    }

    /// A line laid out in the last frame, or freshly shaped when it was
    /// off screen.
    pub(crate) fn visual_line(&mut self, line: usize, window: &Window) -> VisualLine {
        let cached = self.frame.as_ref().and_then(|frame| frame.line(line));
        match cached {
            Some(placed) => placed.visual.clone(),
            None => self.layout_doc_line(line, window),
        }
    }

    /// The document offset under a window position.
    pub(crate) fn offset_for_point(&mut self, position: Point<Pixels>, window: &Window) -> usize {
        let Some(frame) = self.frame.as_ref() else {
            return 0;
        };
        let content_top = frame.bounds.top() + self.theme.text_padding;
        let line = self
            .metrics
            .line_at_y(position.y - content_top + self.scroll_y);
        let x = position.x - frame.text_left;
        let visual = self.visual_line(line, window);
        visual.start + visual.offset_for_x(x)
    }
}

fn clip_to_line(marked: Option<&Range<usize>>, line: &Range<usize>) -> Option<Range<usize>> {
    let marked = marked?;
    let start = marked.start.max(line.start);
    let end = marked.end.min(line.end);
    (start < end).then(|| start - line.start..end - line.start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_is_clipped_to_each_line() {
        assert_eq!(clip_to_line(Some(&(3..8)), &(0..5)), Some(3..5));
        assert_eq!(clip_to_line(Some(&(3..8)), &(6..10)), Some(0..2));
        assert_eq!(clip_to_line(Some(&(3..8)), &(9..10)), None);
        assert_eq!(clip_to_line(None, &(0..10)), None);
    }
}
