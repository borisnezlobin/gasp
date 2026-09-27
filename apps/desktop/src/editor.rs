//! The editor view: an `editor-core` state plus everything needed to draw
//! it and take input.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use editor_config::Config;
use editor_config::settings::SymbolSettings;
use editor_core::document::{Document, Selection, SelectionRange};
use editor_core::history::EditorState;
use editor_core::pipeline::{EditRequest, Pipeline};
use editor_core::render::{LinePlan, RenderInput, RevealSettings, plan_lines};
use editor_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::{App, Bounds, Context, EventEmitter, FocusHandle, Focusable, Pixels, Point, Window, px};

use crate::actions::ClickUnit;
use crate::bench::Bench;
use crate::frame::{FrameLayout, PlacedLine};
use crate::images::ImageStore;
use crate::line_layout::{LayoutContext, LayoutResources, VisualLine, layout_line};
use crate::metrics::{Estimator, LineMetrics};
use crate::preview::code_highlight::CodeHighlighter;
use crate::preview::folds::Folds;
use crate::preview::math::{MathStore, RenderFn};
use crate::preview::reveal::reveal_settings;
use crate::preview::source::{Source, SourceChange};
use crate::stats::Timings;
use crate::suggest::SuggestState;
use crate::theme::Theme;

/// Edits between timing reports when logging is on.
const TIMING_LOG_INTERVAL: usize = 100;

/// Lines planned together while laying out a frame.
const PLAN_CHUNK: usize = 48;

/// The column width assumed before the first frame.
const INITIAL_COLUMN_WIDTH: f32 = 700.;

/// What the editor tells its container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorEvent {
    /// The text changed, by typing, a command, undo or a reload.
    Edited,
    /// The selection moved without the text changing.
    SelectionChanged,
    /// Mod-click on a link, or `link.follow` with the cursor in one. The
    /// target is a URL, a path, or `note#heading` for a wikilink.
    OpenLink(String),
}

/// Ranges drawn with a background, such as find matches. Each kind is
/// replaced as a whole by [`EditorView::set_highlights`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HighlightKind {
    SearchMatch,
    ActiveSearchMatch,
}

/// A live-preview Markdown editor view.
pub struct EditorView {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) state: EditorState,
    pub(crate) marked: Option<Range<usize>>,
    pub(crate) metrics: LineMetrics,
    /// The theme at zoom 1.
    pub(crate) base_theme: Theme,
    /// The theme at the current zoom; everything draws from this.
    pub(crate) theme: Theme,
    pub(crate) zoom: f32,
    pub(crate) readable_width: bool,
    pub(crate) symbols: SymbolSettings,
    pub(crate) reveal: RevealSettings,
    pub(crate) source: Source,
    pub(crate) math: MathStore,
    pub(crate) folds: Folds,
    pub(crate) images: ImageStore,
    pub(crate) code: CodeHighlighter,
    /// Width of the text column in the last frame.
    pub(crate) column_width: Pixels,
    /// How far the view is scrolled down, counting the header.
    pub(crate) scroll_y: Pixels,
    /// Room above the first line for something that scrolls with the
    /// text, such as the note's inline title.
    pub(crate) header_height: Pixels,
    pub(crate) goal_x: Option<Pixels>,
    pub(crate) is_selecting: bool,
    pub(crate) click_unit: ClickUnit,
    pub(crate) click_origin: Range<usize>,
    pub(crate) autoscroll: bool,
    pub(crate) frame: Option<FrameLayout>,
    pub(crate) timings: Timings,
    pub(crate) bench: Option<Bench>,
    pub(crate) log_timings: bool,
    pub(crate) highlights: BTreeMap<HighlightKind, Vec<Range<usize>>>,
    pub(crate) suggest: SuggestState,
    pub(crate) pipeline: Pipeline,
    /// Whether pasted text gets curly quotes, from the settings.
    pub(crate) curl_pasted_quotes: bool,
    clock: Instant,
}

impl EventEmitter<EditorEvent> for EditorView {}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EditorView {
    /// A view of `text` with the built-in settings and theme. Images are
    /// looked up in `image_dirs`.
    pub fn new(text: &str, image_dirs: Vec<PathBuf>, cx: &mut Context<Self>) -> Self {
        let config = Config::defaults();
        let mut base_theme = Theme::from_config(&config);
        base_theme.resolve_fonts(&cx.text_system().all_font_names());
        let source = Source::new(text);
        let column_width = px(INITIAL_COLUMN_WIDTH);
        let estimator = Estimator {
            theme: &base_theme,
            column_width,
        };
        let symbols = config.settings.markdown.symbols.clone();
        Self {
            focus_handle: cx.focus_handle(),
            metrics: LineMetrics::build(&source, &estimator),
            state: EditorState::new(Document::from(text)),
            marked: None,
            theme: base_theme.clone(),
            base_theme,
            zoom: 1.,
            readable_width: true,
            reveal: reveal_settings(&symbols),
            symbols,
            source,
            math: MathStore::default(),
            folds: Folds::default(),
            images: ImageStore::new(image_dirs),
            code: CodeHighlighter::default(),
            column_width,
            scroll_y: px(0.),
            header_height: px(0.),
            goal_x: None,
            is_selecting: false,
            click_unit: ClickUnit::Character,
            click_origin: 0..0,
            autoscroll: false,
            frame: None,
            timings: Timings::default(),
            bench: None,
            log_timings: false,
            highlights: Default::default(),
            suggest: SuggestState::default(),
            pipeline: Pipeline::builtin(),
            curl_pasted_quotes: config.settings.editor.curl_pasted_quotes,
            clock: Instant::now(),
        }
    }

    /// Takes the theme and Markdown symbol settings from a loaded config,
    /// as when the config folder changes.
    pub fn apply_config(&mut self, config: &Config, cx: &mut Context<Self>) {
        let mut theme = Theme::from_config(config);
        theme.resolve_fonts(&cx.text_system().all_font_names());
        self.base_theme = theme;
        self.symbols = config.settings.markdown.symbols.clone();
        self.reveal = reveal_settings(&self.symbols);
        self.apply_typing_settings(&config.settings.editor);
        self.set_zoom(self.zoom, cx);
    }

    /// Renders math with `render` instead of Typst, for tests.
    pub fn set_math_renderer(&mut self, render: RenderFn, cx: &mut Context<Self>) {
        self.math = MathStore::with_renderer(render);
        cx.notify();
    }

    pub fn text(&self) -> String {
        self.state.doc().to_string()
    }

    pub fn doc(&self) -> &Document {
        self.state.doc()
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn is_readable_width(&self) -> bool {
        self.readable_width
    }

    pub fn reveal_settings(&self) -> &RevealSettings {
        &self.reveal
    }

    /// The primary selection as an ordered byte range.
    pub fn selected_range(&self) -> Range<usize> {
        self.state.selection().primary().range()
    }

    /// Every selected range, in order, for the render planner.
    pub fn selected_ranges(&self) -> Vec<Range<usize>> {
        self.state
            .selection()
            .ranges()
            .iter()
            .map(SelectionRange::range)
            .collect()
    }

    /// Whether there is an edit to undo.
    pub fn can_undo(&self) -> bool {
        self.state.history().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.state.history().can_redo()
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

    /// How far the view is scrolled down, in pixels.
    pub fn scroll_offset(&self) -> Pixels {
        self.scroll_y
    }

    pub fn header_height(&self) -> Pixels {
        self.header_height
    }

    /// Leaves `height` above the first line, scrolled with the text.
    pub fn set_header_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        if height != self.header_height {
            self.header_height = height;
            cx.notify();
        }
    }

    /// The furthest the view can scroll with `viewport` of room.
    fn max_scroll(&self, viewport: Pixels) -> Pixels {
        (self.header_height + self.metrics.total_height() - viewport).max(px(0.))
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
        self.refresh_suggestions(cx);
        cx.emit(EditorEvent::SelectionChanged);
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
        let inserted = range.start..range.start + text.len();
        let transaction = Transaction::new(
            ChangeSet::replace(range.clone(), text),
            Origin::Input,
            self.now_ms(),
        )
        .with_selection(Selection::cursor(inserted.end));
        self.state
            .apply(transaction)
            .expect("ranges are clamped to character boundaries");
        let change = self.source.replace(range, text);
        self.source_changed(change);
        self.marked = None;
        self.goal_x = None;
        self.autoscroll = true;
        self.timings.input_started.get_or_insert_with(Instant::now);
        self.refresh_suggestions(cx);
        cx.emit(EditorEvent::Edited);
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
        let single_edit = match transaction.changes.edits() {
            [edit] => Some(edit.clone()),
            _ => None,
        };
        if self.state.apply(transaction).is_err() {
            return;
        }
        // One edit, as typing makes, updates the source in place; anything
        // else finds what changed by comparing the whole text.
        self.goal_x = None;
        self.timings.input_started.get_or_insert_with(Instant::now);
        match single_edit {
            Some(edit) => {
                let change = self.source.replace(edit.range, &edit.insert);
                self.source_changed(change);
                self.after_edit(cx);
            }
            None => self.after_history_step(cx),
        }
    }

    /// Applies transactions in order, each its own undo step, as the input
    /// pipeline returns them.
    pub fn apply_transactions(&mut self, transactions: Vec<Transaction>, cx: &mut Context<Self>) {
        for transaction in transactions {
            self.apply_transaction(transaction, cx);
        }
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
        self.run_pipeline(EditRequest::Newline, cx);
    }

    /// Runs a request through the input pipeline and applies what it
    /// makes of it.
    pub(crate) fn run_pipeline(&mut self, request: EditRequest, cx: &mut Context<Self>) {
        let transactions = self.pipeline.run_steps(
            request,
            self.state.doc(),
            self.state.selection(),
            self.source.tree(),
            self.now_ms(),
        );
        self.apply_transactions(transactions, cx);
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
        if let Some(change) = self.source.sync(self.state.doc()) {
            self.source_changed(change);
        }
        self.after_edit(cx);
    }

    /// Everything an edit settles once the source matches the document.
    fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.marked = None;
        self.autoscroll = true;
        self.refresh_suggestions(cx);
        cx.emit(EditorEvent::Edited);
        cx.notify();
    }

    /// Re-estimates the changed lines and moves fold overrides.
    fn source_changed(&mut self, change: SourceChange) {
        let estimator = Estimator {
            theme: &self.theme,
            column_width: self.column_width,
        };
        self.metrics
            .splice(change.old_lines, change.new_lines, &self.source, &estimator);
        self.folds.map(&change.edit);
        self.code
            .text_changed(change.edit.old.clone(), change.edit.new_len);
    }

    /// Replaces the whole text, as when the file changed on disk. The
    /// cursor keeps its offset where it still fits, and the replacement is
    /// one undo step.
    pub fn replace_all_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.state.doc().to_string() == text {
            return;
        }
        let cursor = self.cursor().min(text.len());
        let whole = 0..self.state.doc().len();
        let transaction = Transaction::new(
            ChangeSet::replace(whole, text),
            Origin::Other("reload".into()),
            self.now_ms(),
        );
        if self.state.apply(transaction).is_err() {
            return;
        }
        let cursor = self.state.doc().floor_char_boundary(cursor);
        self.after_history_step(cx);
        self.select(cursor, cursor, cx);
    }

    /// Replaces the ranges drawn for `kind`.
    pub fn set_highlights(
        &mut self,
        kind: HighlightKind,
        ranges: Vec<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        if ranges.is_empty() {
            self.highlights.remove(&kind);
        } else {
            self.highlights.insert(kind, ranges);
        }
        cx.notify();
    }

    /// The ranges drawn for `kind`.
    pub fn highlights(&self, kind: HighlightKind) -> &[Range<usize>] {
        self.highlights.get(&kind).map_or(&[], Vec::as_slice)
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

    fn viewport_height(&self) -> Pixels {
        self.frame.as_ref().map_or(px(0.), |frame| {
            (frame.bounds.size.height - self.theme.text_padding * 2.).max(px(0.))
        })
    }

    /// Scrolls by `delta` and clamps to the document.
    pub fn scroll_by(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        let max_scroll = self.max_scroll(self.viewport_height());
        self.scroll_y = (self.scroll_y + delta).clamp(px(0.), max_scroll);
        cx.notify();
    }

    /// Re-estimates every line, as after a zoom or a new column width.
    pub(crate) fn remeasure(&mut self) {
        let estimator = Estimator {
            theme: &self.theme,
            column_width: self.column_width,
        };
        self.metrics = LineMetrics::build(&self.source, &estimator);
        self.autoscroll = true;
    }

    /// Where the text column goes in `bounds`: centred at the readable
    /// width when that is on, else the full width less padding.
    pub(crate) fn column_for(&self, bounds: Bounds<Pixels>) -> (Pixels, Pixels) {
        let available = (bounds.size.width - self.theme.text_padding * 2.).max(px(1.));
        let width = if self.readable_width {
            available.min(self.theme.editor_max_width)
        } else {
            available
        };
        (bounds.left() + (bounds.size.width - width) / 2., width)
    }

    /// Plans `lines` for the current selection and settings.
    pub(crate) fn plan(&self, lines: Range<usize>) -> Vec<LinePlan> {
        let selections = self.selected_ranges();
        let input = RenderInput {
            text: self.source.text(),
            tree: self.source.tree(),
            selections: &selections,
            settings: &self.reveal,
        };
        let mut plans = plan_lines(&input, lines).lines;
        self.folds
            .apply(&mut plans, self.source.tree(), &selections);
        plans
    }

    /// Lays out a planned line against the current column.
    pub(crate) fn layout_plan(&mut self, plan: &LinePlan, window: &Window) -> VisualLine {
        let context = LayoutContext {
            source: &self.source,
            theme: &self.theme,
            column_width: self.column_width,
            zoom: self.zoom,
            scale_factor: window.scale_factor(),
            marked: self.marked.clone(),
        };
        let mut resources = LayoutResources {
            text_system: window.text_system(),
            images: &mut self.images,
            math: &mut self.math,
            code: &mut self.code,
        };
        layout_line(plan, &context, &mut resources)
    }

    /// Lays out one document line and records its height.
    pub(crate) fn layout_doc_line(&mut self, line: usize, window: &Window) -> VisualLine {
        let plan = self
            .plan(line..line + 1)
            .pop()
            .expect("every line in the document has a plan");
        let visual = self.layout_plan(&plan, window);
        self.metrics.set(line, visual.height);
        visual
    }

    /// A line laid out in the last frame, or freshly laid out when it was
    /// off screen.
    pub(crate) fn visual_line(&mut self, line: usize, window: &Window) -> VisualLine {
        let cached = self.frame.as_ref().and_then(|frame| frame.line(line));
        match cached {
            Some(placed) => placed.visual.clone(),
            None => self.layout_doc_line(line, window),
        }
    }

    /// Scrolls just enough to show the cursor's row. When jumping down,
    /// the lines above the cursor are measured first so the row lands
    /// exactly at the bottom.
    pub(crate) fn apply_autoscroll(&mut self, viewport: Pixels, window: &Window) {
        if !std::mem::take(&mut self.autoscroll) {
            return;
        }
        let line = self.source.line_of(self.cursor());
        let visual = self.layout_doc_line(line, window);
        let relative = self.cursor() - visual.start;
        let (row_top, row_bottom) = visual
            .row_for_offset(relative)
            .map_or((px(0.), visual.height), |row| {
                (visual.rows[row].top, visual.rows[row].bottom())
            });
        let line_top = self.header_height + self.metrics.top_of(line);
        if line_top + row_top < self.scroll_y {
            // The first line brings the header back into view with it.
            self.scroll_y = if line == 0 {
                px(0.)
            } else {
                line_top + row_top
            };
        } else if line_top + row_bottom > self.scroll_y + viewport {
            self.measure_above(line, viewport - row_bottom, window);
            self.scroll_y = self.header_height + self.metrics.top_of(line) + row_bottom - viewport;
        }
    }

    fn measure_above(&mut self, line: usize, mut room: Pixels, window: &Window) {
        let mut above = line;
        while room > px(0.) && above > 0 {
            above -= 1;
            room -= self.layout_doc_line(above, window).height;
        }
    }

    /// Lays out the lines visible in `bounds`.
    pub(crate) fn layout_frame(&mut self, bounds: Bounds<Pixels>, window: &Window) -> FrameLayout {
        let (text_left, column_width) = self.column_for(bounds);
        if column_width != self.column_width {
            self.column_width = column_width;
            self.remeasure();
        }
        let padding = self.theme.text_padding;
        let viewport = (bounds.size.height - padding * 2.).max(px(0.));
        self.math.begin_frame();
        self.code.begin_frame();
        self.apply_autoscroll(viewport, window);
        self.scroll_y = self.scroll_y.clamp(px(0.), self.max_scroll(viewport));
        let text_scroll = self.scroll_y - self.header_height;
        let (first, first_top) = self.metrics.line_at_y(text_scroll.max(px(0.)));
        let mut top = bounds.top() + padding + first_top - text_scroll;
        let mut lines = Vec::new();
        let mut plans = Vec::new().into_iter();
        let mut line = first;
        while line < self.source.line_count() && top < bounds.bottom() {
            let plan = match plans.next() {
                Some(plan) => plan,
                None => {
                    plans = self.plan(line..line + PLAN_CHUNK).into_iter();
                    plans.next().expect("the line exists")
                }
            };
            let visual = self.layout_plan(&plan, window);
            self.metrics.set(line, visual.height);
            let height = visual.height;
            lines.push(PlacedLine { top, visual });
            top += height;
            line += 1;
        }
        FrameLayout {
            bounds,
            text_left,
            column_width,
            lines,
            highlights: Vec::new(),
        }
    }

    /// The document offset under a window position. Positions above or
    /// below the frame map to lines off screen, so drags keep selecting.
    pub(crate) fn offset_for_point(&mut self, position: Point<Pixels>, window: &Window) -> usize {
        let Some(frame) = self.frame.as_ref() else {
            return 0;
        };
        let content_top = frame.bounds.top() + self.theme.text_padding + self.header_height;
        let inside = frame
            .lines
            .first()
            .zip(frame.lines.last())
            .is_some_and(|(first, last)| first.top <= position.y && position.y < last.bottom());
        if inside {
            return frame.offset_at(position).unwrap_or(0);
        }
        let x = position.x - frame.text_left;
        let (line, line_top) = self
            .metrics
            .line_at_y(position.y - content_top + self.scroll_y);
        let y = position.y - content_top + self.scroll_y - line_top;
        let visual = self.visual_line(line, window);
        visual.start + visual.offset_for_point(x, y)
    }
}
