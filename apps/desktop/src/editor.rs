//! The editor view: an `gasp-core` state plus everything needed to draw
//! it and take input.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use gasp_config::Config;
use gasp_config::settings::SymbolSettings;
use gasp_config::typing::TypingTables;
use gasp_core::document::{Document, Selection, SelectionRange};
use gasp_core::history::EditorState;
use gasp_core::pipeline::{EditRequest, Pipeline, TabStops};
use gasp_core::render::{
    LinePlan, Placement, RenderInput, RevealSettings, Widget, WidgetKind, plan_lines,
};
use gasp_core::transaction::{ChangeSet, Origin, Transaction};
use gpui::{App, Bounds, Context, EventEmitter, FocusHandle, Focusable, Pixels, Point, Window, px};

use crate::actions::ClickUnit;
use crate::bench::Bench;
use crate::footnotes::FootnoteChecks;
use crate::frame::{FrameLayout, PlacedLine};
use crate::hover::HoverState;
use crate::images::ImageStore;
use crate::line_cache::{LayoutEpoch, LineCache};
use crate::line_layout::{LayoutContext, LayoutResources, VisualLine, layout_line};
use crate::link_cards::LinkCards;
use crate::metrics::{Estimator, LineMetrics};
use crate::preview::code_highlight::{CodeHighlighter, spans_for_line};
use crate::preview::folds::Folds;
use crate::preview::layout::{frame_for, layout_framed};
use crate::preview::math::{MathStore, RenderFn};
use crate::preview::reveal::reveal_settings;
use crate::preview::source::{Source, SourceChange};
use crate::preview::table::{TableStore, table_columns};
use crate::stats::Timings;
use crate::suggest::SuggestState;
use crate::theme::Theme;

/// Edits between timing reports when logging is on.
const TIMING_LOG_INTERVAL: usize = 100;

/// The fewest lines planned together while laying out a frame; a frame
/// plans about as many lines as its estimates say fit, so it doesn't
/// plan lines it won't show.
const PLAN_CHUNK: usize = 8;

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
    /// Missing, duplicate, unused or empty footnotes, drawn underlined.
    FootnoteProblem,
    /// A snippet's tab stop that Tab still goes to. The editor keeps these
    /// itself; an empty stop is drawn as a small block.
    TabStop,
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
    /// Where the marker of the task under the pointer starts, so its box
    /// can show it's clickable.
    pub(crate) hovered_task: Option<usize>,
    /// The line of the link card under the pointer, and whether the
    /// pointer is on its Open button.
    pub(crate) hovered_card: Option<(usize, bool)>,
    /// What the pointer shows over the text: a hand over controls and
    /// over links while Mod is held, an arrow over other widgets.
    pub(crate) pointer_cursor: gpui::CursorStyle,
    /// Where the pointer last moved over the editor, so pressing Mod can
    /// change what it shows without it moving.
    pub(crate) pointer_at: Option<gpui::Point<Pixels>>,
    /// Scrolls the note while a drag selection is held past its top or
    /// bottom, so holding still there keeps selecting.
    pub(crate) drag_scroll: Option<gpui::Task<()>>,
    pub(crate) click_unit: ClickUnit,
    pub(crate) click_origin: Range<usize>,
    pub(crate) autoscroll: bool,
    /// The first line that started inside the view last frame. Lines
    /// above it that change height as they're laid out (an estimate
    /// replaced by the real height, an image or equation arriving) move
    /// the scroll with them, so what the reader sees stays put.
    pub(crate) scroll_anchor: Option<usize>,
    pub(crate) frame: Option<FrameLayout>,
    pub(crate) timings: Timings,
    pub(crate) bench: Option<Bench>,
    pub(crate) log_timings: bool,
    pub(crate) highlights: BTreeMap<HighlightKind, Vec<Range<usize>>>,
    pub(crate) suggest: SuggestState,
    pub(crate) pipeline: Pipeline,
    /// The snippets and replacements in the pipeline, and whether the
    /// snippet step enlarges brackets, to notice when they change.
    pub(crate) typing: Option<(TypingTables, bool)>,
    /// The tab stops of the snippet being filled in.
    pub(crate) tab_stops: Option<TabStops>,
    /// Whether pasted text gets curly quotes, from the settings.
    pub(crate) curl_pasted_quotes: bool,
    /// A view that only shows its note, such as a hover preview: edits,
    /// suggestions and the caret are off.
    pub(crate) read_only: bool,
    /// Whether markup around the cursor shows as source. Only the editor
    /// with the keyboard reveals it; each frame sets this from its focus,
    /// so a split showing the same note elsewhere reads as a preview.
    pub(crate) reveals_at_cursor: bool,
    /// The cursor goes after the frontmatter once the note is parsed.
    pub(crate) cursor_after_frontmatter: bool,
    pub(crate) footnotes: FootnoteChecks,
    pub(crate) hover: HoverState,
    pub(crate) cards: LinkCards,
    /// A document offset kept at the top of the view until the reader
    /// scrolls, such as the heading a preview opened at. Line heights are
    /// estimates until laid out, so it's re-applied each frame.
    pub(crate) pinned_top: Option<usize>,
    /// Rounds a read-only view's background, for one shown in a card:
    /// children clip to a rectangle, so the card can't round it.
    pub(crate) corner_radius: Pixels,
    /// A read-only view's content height when last drawn, to notice when
    /// laying out lines changed it.
    drawn_height: std::cell::Cell<Pixels>,
    /// Whether code blocks number their lines unless a block says.
    pub(crate) code_line_numbers: bool,
    /// The copy button on the code block under the pointer.
    pub(crate) code_copy: crate::code_copy::CodeCopy,
    /// Sentence-length tints and grammar flags.
    pub(crate) prose: crate::prose::ProseState,
    /// Lines laid out in earlier frames, reused while they're unchanged.
    pub(crate) line_cache: LineCache,
    /// Tables' measured rows and columns.
    pub(crate) tables: TableStore,
    /// The table editor: the cell being edited, the table edited as
    /// Markdown, handles, and row and column drags.
    pub(crate) table_edit: crate::table_edit::TableEditing,
    /// The toolbars that float by the selection or the cursor's line.
    pub(crate) floating: crate::toolbar::floating::FloatingToolbars,
    clock: Instant,
    /// Lets go of the note's decoded images and equations once its tab
    /// has been hidden a while.
    pub(crate) release_when_hidden: Option<gpui::Task<()>>,
    /// Waits for the installed fonts, while they're still being listed.
    _fonts: Option<gpui::Subscription>,
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
        Self::with_config(text, image_dirs, &Config::defaults(), cx)
    }

    /// A view of `text` styled by `config`, as [`EditorView::apply_config`]
    /// would, without measuring the text twice.
    pub fn with_config(
        text: &str,
        image_dirs: Vec<PathBuf>,
        config: &Config,
        cx: &mut Context<Self>,
    ) -> Self {
        let span = crate::trace::span("editor-parse");
        let source = Source::new(text);
        drop(span);
        Self::with_source(source, image_dirs, config, cx)
    }

    /// A view of text already parsed, as on a background thread.
    pub fn with_source(
        source: Source,
        image_dirs: Vec<PathBuf>,
        config: &Config,
        cx: &mut Context<Self>,
    ) -> Self {
        let span = crate::trace::span("editor-theme");
        let mut base_theme = Theme::from_config(config, crate::ui::is_dark(cx));
        base_theme.resolve_fonts(&crate::ui::installed_fonts(cx).unwrap_or_default());
        drop(span);
        let span = crate::trace::span("editor-measure");
        let column_width = px(INITIAL_COLUMN_WIDTH);
        let estimator = Estimator {
            theme: &base_theme,
            column_width,
        };
        let symbols = config.settings.markdown.symbols.clone();
        let metrics = LineMetrics::build(&source, &estimator);
        drop(span);
        let mut view = Self {
            focus_handle: cx.focus_handle(),
            metrics,
            state: EditorState::new(Document::from(source.text())),
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
            hovered_task: None,
            hovered_card: None,
            pointer_cursor: gpui::CursorStyle::IBeam,
            pointer_at: None,
            drag_scroll: None,
            click_unit: ClickUnit::Character,
            click_origin: 0..0,
            autoscroll: false,
            scroll_anchor: None,
            frame: None,
            timings: Timings::default(),
            bench: None,
            log_timings: false,
            highlights: Default::default(),
            suggest: SuggestState::default(),
            pipeline: Pipeline::builtin(),
            typing: None,
            tab_stops: None,
            curl_pasted_quotes: config.settings.editor.curl_pasted_quotes,
            read_only: false,
            reveals_at_cursor: true,
            cursor_after_frontmatter: false,
            footnotes: FootnoteChecks::default(),
            hover: HoverState::default(),
            cards: LinkCards::default(),
            pinned_top: None,
            corner_radius: px(0.),
            drawn_height: std::cell::Cell::new(px(0.)),
            code_line_numbers: config.settings.editor.code_line_numbers,
            code_copy: crate::code_copy::CodeCopy::default(),
            prose: crate::prose::ProseState::from_settings(&config.settings.prose),
            line_cache: LineCache::default(),
            tables: TableStore::default(),
            table_edit: Default::default(),
            floating: crate::toolbar::floating::FloatingToolbars::new(&config.toolbars),
            clock: Instant::now(),
            release_when_hidden: None,
            _fonts: None,
        };
        view._fonts = crate::ui::installed_fonts(cx)
            .is_none()
            .then(|| crate::ui::observe_installed_fonts(cx, Self::fonts_arrived));
        view.apply_typing_settings(config);
        view.check_footnotes_soon(cx);
        view
    }

    /// Swaps a fallback in for any theme font the installed fonts lack,
    /// once they've been listed. The fonts the theme names are drawn by
    /// name until then, so nothing changes when they're all installed.
    fn fonts_arrived(&mut self, cx: &mut Context<Self>) {
        let Some(names) = crate::ui::installed_fonts(cx) else {
            return;
        };
        let before = self.base_theme.font_families();
        self.base_theme.resolve_fonts(&names);
        if self.base_theme.font_families() != before {
            self.set_zoom(self.zoom, cx);
        }
    }

    /// Takes the theme and Markdown symbol settings from a loaded config,
    /// as when the config folder changes.
    pub fn apply_config(&mut self, config: &Config, cx: &mut Context<Self>) {
        let mut theme = Theme::from_config(config, crate::ui::is_dark(cx));
        theme.resolve_fonts(&crate::ui::installed_fonts(cx).unwrap_or_default());
        self.base_theme = theme;
        self.symbols = config.settings.markdown.symbols.clone();
        self.reveal = reveal_settings(&self.symbols);
        self.apply_typing_settings(config);
        self.clear_preview_cache();
        self.code_line_numbers = config.settings.editor.code_line_numbers;
        self.apply_prose_settings(&config.settings.prose, cx);
        self.floating.configure(&config.toolbars);
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

    /// Where the marker of the task under the pointer starts.
    pub fn hovered_task(&self) -> Option<usize> {
        self.hovered_task
    }

    /// The pointer's look over the editor right now.
    pub fn pointer_cursor(&self) -> gpui::CursorStyle {
        self.pointer_cursor
    }

    /// The link card under the pointer: its line, and whether the pointer
    /// is on its Open button.
    pub fn hovered_card(&self) -> Option<(usize, bool)> {
        self.hovered_card
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
        self.drop_stale_tab_stops();
        self.tables.forget_columns();
        self.autoscroll = true;
        // A place kept at the top gives way once the reader moves.
        if !self.read_only {
            self.pinned_top = None;
        }
        self.refresh_suggestions(cx);
        self.keep_card_offer(cx);
        self.caret_moved_in_tables(cx);
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }

    /// Where the note's body starts: the line after its frontmatter, or
    /// the start when it has none.
    pub fn body_start(&self) -> usize {
        let tree = self.source.tree();
        let frontmatter = tree
            .path_at(0)
            .into_iter()
            .map(|id| tree.node(id))
            .find(|node| node.kind == gasp_core::syntax::NodeKind::Frontmatter);
        let Some(node) = frontmatter else {
            return 0;
        };
        let line = self.source.line_of(node.range.end.saturating_sub(1));
        let next = (line + 1).min(self.source.line_count().saturating_sub(1));
        match next > line {
            true => self.source.line_range(next).start,
            false => self.source.line_range(line).end,
        }
    }

    /// Puts the cursor where the body starts, so a note opens with its
    /// frontmatter shown as properties rather than as source.
    pub fn place_cursor_after_frontmatter(&mut self, cx: &mut Context<Self>) {
        if self.source.is_plain() {
            // Where the frontmatter ends is known once the parse arrives.
            self.cursor_after_frontmatter = true;
            return;
        }
        let start = self.body_start();
        if start > 0 && self.selected_range() == (0..0) {
            self.select(start, start, cx);
        }
    }

    /// Where the reader is: the cursor, and the start of the line at the
    /// top of the view (zero at the very top).
    pub fn position(&self) -> (usize, usize) {
        // Until the reader moves, the line kept at the top is still there.
        if let Some(top) = self.pinned_top {
            return (self.cursor(), top);
        }
        let text_scroll = self.scroll_y - self.header_height;
        let top = if text_scroll > px(0.) {
            // A view scrolled to a line's top can land a hair above it.
            let (line, _) = self.metrics.line_at_y(text_scroll + px(0.5));
            self.state.doc().line_start(line)
        } else {
            0
        };
        (self.cursor(), top)
    }

    /// Puts the reader back where [`EditorView::position`] said they
    /// were, keeping that line at the top until they move or scroll.
    pub fn restore_position(&mut self, cursor: usize, top: usize, cx: &mut Context<Self>) {
        let len = self.state.doc().len();
        self.cursor_after_frontmatter = false;
        self.select(cursor.min(len), cursor.min(len), cx);
        self.autoscroll = false;
        self.pinned_top = (top > 0).then_some(top.min(len));
        cx.notify();
    }

    /// Moves the head, keeping the anchor when `extend` is set.
    pub fn move_to(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        let anchor = if extend { self.anchor() } else { offset };
        self.select(anchor, offset, cx);
    }

    /// Ends the current typing group, so the next edit is an undo step
    /// of its own rather than part of what was just typed.
    pub fn break_undo_group(&mut self) {
        self.state.history_mut().break_group();
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
        let changes = ChangeSet::replace(range.clone(), text);
        if let Some(stops) = self.tab_stops.as_mut() {
            stops.map(&changes);
        }
        let transaction = Transaction::new(changes, Origin::Input, self.now_ms())
            .with_selection(Selection::cursor(inserted.end));
        self.state
            .apply(transaction)
            .expect("ranges are clamped to character boundaries");
        let change = self.source.replace(range, text);
        self.source_changed(change);
        self.typed_in_table();
        self.note_typing(cx);
        self.marked = None;
        self.goal_x = None;
        self.autoscroll = true;
        self.timings.input_started.get_or_insert_with(Instant::now);
        self.close_preview(cx);
        self.refresh_suggestions(cx);
        self.keep_card_offer(cx);
        self.schedule_footnote_checks(cx);
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
        let typed = transaction.meta.origin == Origin::Input && !transaction.changes.is_empty();
        let phase = crate::keytrace::span("state-apply");
        if self.state.apply(transaction).is_err() {
            return;
        }
        if typed {
            self.typed_in_table();
        }
        drop(phase);
        self.goal_x = None;
        self.timings.input_started.get_or_insert_with(Instant::now);
        // One edit, as typing makes, updates the source in place; anything
        // else finds what changed by comparing the whole text.
        match single_edit {
            Some(edit) => {
                let phase = crate::keytrace::span("source-update");
                let change = self.source.replace(edit.range, &edit.insert);
                self.source_changed(change);
                drop(phase);
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
        if self.cell_block().is_some() {
            return self.clear_cell_block(cx);
        }
        let mut range = self.selected_range();
        if range.is_empty() {
            // In a table's cell, deleting stops at the cell's edge.
            range = self.clamp_to_cell(around(self.state.doc(), range.start));
        }
        if !range.is_empty() {
            self.replace(range, "", cx);
        }
    }

    /// Enter runs through the input pipeline so lists continue.
    pub fn newline(&mut self, cx: &mut Context<Self>) {
        self.run_pipeline(EditRequest::Newline, cx);
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
        // Undo can take a snippet's text away from under its stops.
        self.tab_stops = None;
        self.forget_table_typing();
        if let Some(change) = self.source.sync(self.state.doc()) {
            self.source_changed(change);
        }
        self.after_edit(cx);
    }

    /// Everything an edit settles once the source matches the document.
    fn after_edit(&mut self, cx: &mut Context<Self>) {
        let _phase = crate::keytrace::span("after-edit");
        self.close_preview(cx);
        self.marked = None;
        self.autoscroll = true;
        self.refresh_suggestions(cx);
        self.keep_card_offer(cx);
        self.schedule_footnote_checks(cx);
        self.caret_moved_in_tables(cx);
        cx.emit(EditorEvent::Edited);
        cx.notify();
    }

    /// Applies a tidy-up as part of the last undo step, such as padding a
    /// table's columns once typing in it is done.
    pub(crate) fn apply_tidy(&mut self, transaction: Transaction, cx: &mut Context<Self>) {
        let edit = match transaction.changes.edits() {
            [edit] => edit.clone(),
            _ => return,
        };
        if self.state.apply_into_last(transaction).is_err() {
            return;
        }
        let change = self.source.replace(edit.range, &edit.insert);
        self.source_changed(change);
        self.after_edit(cx);
    }

    /// Re-estimates the changed lines and moves fold overrides.
    fn source_changed(&mut self, change: SourceChange) {
        self.scroll_anchor = None;
        self.tables.forget_columns();
        let estimator = Estimator {
            theme: &self.theme,
            column_width: self.column_width,
        };
        self.metrics
            .splice(change.old_lines, change.new_lines, &self.source, &estimator);
        self.folds.map(&change.edit);
        self.code
            .text_changed(change.edit.old.clone(), change.edit.new_len);
        self.prose_edited(&change.edit);
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

    /// Whether a read-only view's content height changed since it was
    /// last asked, as when laying out lines replaced their estimates.
    pub(crate) fn take_height_change(&self) -> bool {
        if !self.read_only {
            return false;
        }
        let height = self.metrics.total_height();
        (height - self.drawn_height.replace(height)).abs() > px(0.5)
    }

    /// Stores the frame just painted and records its timings.
    pub(crate) fn finish_frame(&mut self, frame: FrameLayout, paint_started: Instant) {
        self.timings.paint.push(paint_started.elapsed());
        self.frame = Some(frame);
        let Some(input_started) = self.timings.input_started.take() else {
            crate::keytrace::end_frame(false);
            return;
        };
        self.timings.input_to_paint.push(input_started.elapsed());
        crate::keytrace::end_frame(true);
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
        self.pinned_top = None;
        let max_scroll = self.max_scroll(self.viewport_height());
        self.scroll_y = (self.scroll_y + delta).clamp(px(0.), max_scroll);
        cx.notify();
    }

    /// Whether the note shows as plain lines while it's parsed elsewhere.
    pub fn is_parsing(&self) -> bool {
        self.source.is_plain()
    }

    /// Takes the parse of its text made in the background. An edit made
    /// meanwhile already parsed the note, so a parse that arrives after
    /// one is dropped.
    pub fn take_parsed(&mut self, parsed: Source, cx: &mut Context<Self>) {
        if !self.source.is_plain() || parsed.text() != self.source.text() {
            return;
        }
        self.source = parsed;
        self.remeasure();
        if std::mem::take(&mut self.cursor_after_frontmatter) {
            self.place_cursor_after_frontmatter(cx);
        }
        self.check_footnotes_soon(cx);
        cx.notify();
    }

    /// Re-estimates every line, as after a zoom or a new column width.
    pub(crate) fn remeasure(&mut self) {
        let estimator = Estimator {
            theme: &self.theme,
            column_width: self.column_width,
        };
        self.metrics = LineMetrics::build(&self.source, &estimator);
        self.line_cache.clear();
        self.tables.clear();
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

    /// The selections lines are planned with: none in an editor without
    /// the keyboard, so it reads as a preview, or while several cells of
    /// a table are selected, so they read as cells.
    pub(crate) fn planned_selections(&self) -> Vec<Range<usize>> {
        match self.reveals_at_cursor && !self.table_edit.block_selected {
            true => self.selected_ranges(),
            false => Vec::new(),
        }
    }

    /// The reveal settings lines are planned with: the table being edited
    /// as Markdown shows its source.
    pub(crate) fn planned_reveal(&self) -> std::borrow::Cow<'_, RevealSettings> {
        match self.table_edit.source_table(self.cursor()) {
            Some(at) => {
                let mut reveal = self.reveal.clone();
                reveal.source_table = Some(at);
                std::borrow::Cow::Owned(reveal)
            }
            None => std::borrow::Cow::Borrowed(&self.reveal),
        }
    }

    /// Plans `lines` for the current selection and settings. An editor
    /// without the keyboard plans as if nothing were selected.
    pub(crate) fn plan(&self, lines: Range<usize>) -> Vec<LinePlan> {
        let selections = self.planned_selections();
        let reveal = self.planned_reveal();
        let input = RenderInput {
            text: self.source.text(),
            tree: self.source.tree(),
            selections: &selections,
            settings: &reveal,
        };
        let mut plans = plan_lines(&input, lines).lines;
        self.folds
            .apply(&mut plans, self.source.tree(), &selections);
        self.place_empty_tab_stops(&mut plans);
        plans
    }

    /// Gives each empty tab stop of the snippet being filled in room of
    /// its own on its line, unless its text is hidden there.
    fn place_empty_tab_stops(&self, plans: &mut [LinePlan]) {
        let Some(stops) = self.tab_stops.as_ref() else {
            return;
        };
        for at in stops.pending().filter(|range| range.is_empty()) {
            let at = at.start;
            let plan = plans
                .iter_mut()
                .find(|plan| plan.range.start <= at && at <= plan.range.end);
            let Some(plan) = plan.filter(|plan| !plan.collapsed) else {
                continue;
            };
            if plan
                .hidden
                .iter()
                .any(|hidden| hidden.start < at && at < hidden.end)
            {
                continue;
            }
            plan.widgets.push(Widget {
                kind: WidgetKind::EmptyTabStop,
                range: at..at,
                placement: Placement::Replace,
            });
        }
    }

    /// Lays out a planned line against the current column, or takes it
    /// from the line cache when nothing it depends on changed.
    pub(crate) fn layout_plan(&mut self, plan: &LinePlan, window: &Window) -> VisualLine {
        let selections = self.planned_selections();
        let reveal = match self.planned_reveal() {
            std::borrow::Cow::Borrowed(_) => None,
            std::borrow::Cow::Owned(reveal) => Some(reveal),
        };
        let context = LayoutContext {
            source: &self.source,
            theme: &self.theme,
            column_width: self.column_width,
            zoom: self.zoom,
            scale_factor: window.scale_factor(),
            marked: self.marked.clone(),
            code_line_numbers: self.code_line_numbers,
            reveal: reveal.as_ref().unwrap_or(&self.reveal),
            selections: &selections,
        };
        let mut resources = LayoutResources {
            text_system: window.text_system(),
            images: &mut self.images,
            math: &mut self.math,
            code: &mut self.code,
            tables: &mut self.tables,
        };
        let cache = &mut self.line_cache;
        cache.begin_frame(LayoutEpoch {
            column_width: context.column_width,
            zoom: context.zoom,
            scale_factor: context.scale_factor,
            code_line_numbers: context.code_line_numbers,
        });
        let composing = context
            .marked
            .as_ref()
            .is_some_and(|marked| marked.start <= plan.range.end && plan.range.start <= marked.end);
        if plan.collapsed || composing {
            return layout_line(plan, &context, &mut resources);
        }
        let frame = frame_for(plan, &context);
        let spans = spans_for_line(plan, context.source, resources.code);
        let text = &context.source.text()[plan.range.clone()];
        let columns = table_columns(plan, &frame, &context, &mut resources);
        let Some(key) = LineCache::key(plan, text, &frame, spans.as_ref(), columns.as_deref())
        else {
            return layout_framed(plan, frame, spans, &context, &mut resources);
        };
        if let Some(visual) = cache.get(&key, plan, text, &frame) {
            return visual;
        }
        let visual = layout_framed(plan, frame, spans, &context, &mut resources);
        cache.insert(key, plan, text, &visual);
        visual
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
        self.tables.forget_columns();
        let scrolled_from = self.scroll_y;
        let anchored = self.pinned_top.is_none() && !self.autoscroll;
        match self.pinned_top {
            Some(offset) => {
                self.autoscroll = false;
                let line = self.source.line_of(offset.min(self.source.text().len()));
                self.scroll_y = self.header_height + self.metrics.top_of(line);
            }
            None => {
                let _phase = crate::keytrace::span("autoscroll");
                self.apply_autoscroll(viewport, window)
            }
        }
        self.scroll_y = self.scroll_y.clamp(px(0.), self.max_scroll(viewport));
        let (mut lines, drift) = self.place_lines(bounds, window);
        if anchored && drift != px(0.) {
            self.scroll_y = (self.scroll_y + drift).clamp(px(0.), self.max_scroll(viewport));
            lines = self.place_lines(bounds, window).0;
        }
        // The pane placed the inline title for the old scroll before this
        // paint moved it; draw again so the title moves with the text.
        if self.scroll_y != scrolled_from && self.header_height > px(0.) {
            window.request_animation_frame();
        }
        self.scroll_anchor = scroll_anchor(&lines, bounds.top() + padding);
        FrameLayout {
            bounds,
            text_left,
            column_width,
            lines,
            highlights: Vec::new(),
        }
    }

    /// Lays out and places the lines the scroll shows in `bounds`, and
    /// answers how much the lines above the scroll anchor grew as they
    /// were laid out.
    fn place_lines(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &Window,
    ) -> (Vec<PlacedLine>, Pixels) {
        let padding = self.theme.text_padding;
        let text_scroll = self.scroll_y - self.header_height;
        let (first, first_top) = self.metrics.line_at_y(text_scroll.max(px(0.)));
        let mut top = bounds.top() + padding + first_top - text_scroll;
        let mut lines = Vec::new();
        let mut drift = px(0.);
        let mut plans = Vec::new().into_iter();
        let mut line = first;
        while line < self.source.line_count() && top < bounds.bottom() {
            let plan = match plans.next() {
                Some(plan) => plan,
                None => {
                    let _phase = crate::keytrace::span("plan");
                    let room = bounds.bottom() - top;
                    let count = self.metrics.lines_within(line, room).max(PLAN_CHUNK);
                    plans = self.plan(line..line + count).into_iter();
                    plans.next().expect("the line exists")
                }
            };
            let phase = crate::keytrace::span("layout-lines");
            let visual = self.layout_plan(&plan, window);
            drop(phase);
            if self.anchors_growth_of(line) {
                drift += visual.height - self.metrics.height(line);
            }
            self.metrics.set(line, visual.height);
            let height = visual.height;
            lines.push(PlacedLine { top, visual });
            top += height;
            line += 1;
        }
        (lines, drift)
    }

    /// Whether a change in `line`'s height moves the scroll with it: it's
    /// above the anchor, and it isn't the line being edited, which grows
    /// downward as it's typed in.
    fn anchors_growth_of(&self, line: usize) -> bool {
        self.scroll_anchor.is_some_and(|anchor| line < anchor)
            && line != self.source.line_of(self.cursor())
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

/// The first line that starts inside the view, or the one the view
/// starts in when none does.
fn scroll_anchor(lines: &[PlacedLine], view_top: Pixels) -> Option<usize> {
    lines
        .iter()
        .find(|placed| placed.top >= view_top && !placed.visual.is_collapsed())
        .or(lines.first())
        .map(|placed| placed.visual.line)
}
