//! Folding on the desktop: a chevron in the margin beside a heading
//! (shown while the pointer is on the heading's line, and always once
//! it's folded), the count of hidden lines after a folded heading, the
//! `fold.*` commands, and keeping the caret out of folded text. Moving
//! the caret across a fold steps over it; anything else that lands in one
//! (a click on a match, search, a jump to a line) unfolds it.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gasp_core::render::folds::{FoldTarget, Folds, HeadingSection, heading_sections};
use gasp_core::syntax::SyntaxTree;
use gpui::{Bounds, Context, Pixels, Point, point, size};

use crate::editor::EditorView;
use crate::frame::{FrameLayout, PlacedLine};
use crate::line_layout::Hit;
use crate::metrics::Estimator;
use crate::theme::Theme;

/// A heading's fold chevron, as drawn in the last frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FoldChevron {
    pub line: usize,
    pub line_start: usize,
    pub folded: bool,
    pub bounds: Bounds<Pixels>,
    /// The pointer is on it.
    pub hot: bool,
}

/// A control that folds or unfolds a heading, by the heading's line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FoldControl {
    Chevron(usize),
    Count(usize),
}

/// What the editor keeps for folding besides the folds themselves.
#[derive(Default)]
pub struct FoldUi {
    /// Every heading's section, worked out once per version of the text.
    sections: RefCell<Option<Rc<Vec<HeadingSection>>>>,
    /// The heading line under the pointer, whose chevron shows.
    hovered_line: Option<usize>,
    /// The fold control under the pointer.
    hot: Option<FoldControl>,
    /// Folded heading lines kept from an earlier launch, waiting for the
    /// note's parse.
    pending: Option<Vec<usize>>,
    /// The chevrons drawn in the last frame.
    pub chevrons: Vec<FoldChevron>,
}

impl FoldUi {
    /// The text changed: its sections are worked out again when asked.
    pub fn forget_sections(&self) {
        self.sections.borrow_mut().take();
    }
}

/// The square a heading's chevron sits in, left of the text column and
/// centred on the heading's first row.
pub fn chevron_bounds(placed: &PlacedLine, text_left: Pixels, theme: &Theme) -> Bounds<Pixels> {
    let side = theme.icon_size + theme.space_sm;
    let center_y = placed
        .visual
        .caret_rows()
        .next()
        .map_or(placed.top + placed.visual.height / 2., |(_, row)| {
            placed.top + row.top + row.caret_top + row.caret_height / 2.
        });
    Bounds::new(
        point(text_left - theme.space_sm - side, center_y - side / 2.),
        size(side, side),
    )
}

impl EditorView {
    /// Every heading's section in the note, from a cache kept until the
    /// text changes.
    pub(crate) fn heading_sections(&self) -> Rc<Vec<HeadingSection>> {
        let mut cached = self.fold_ui.sections.borrow_mut();
        cached
            .get_or_insert_with(|| Rc::new(heading_sections(self.source.tree())))
            .clone()
    }

    /// The section of the heading on `line`, when there's something
    /// under it to fold.
    fn foldable_heading(&self, line: usize) -> Option<HeadingSection> {
        let sections = self.heading_sections();
        let index = sections
            .binary_search_by_key(&line, |section| section.line)
            .ok()?;
        Some(sections[index].clone()).filter(|section| !section.body.is_empty())
    }

    /// Whether the heading on `line` is folded.
    pub fn is_line_folded(&self, line: usize) -> bool {
        self.foldable_heading(line)
            .is_some_and(|section| self.folds.is_heading_folded(section.line_start))
    }

    /// The chevrons drawn in the last frame.
    pub fn fold_chevrons(&self) -> &[FoldChevron] {
        &self.fold_ui.chevrons
    }

    /// `fold.toggle`: folds or unfolds the heading the caret is on or in,
    /// or the foldable callout it's in.
    pub fn toggle_fold_at_cursor(&mut self, cx: &mut Context<Self>) {
        let at = self.cursor();
        let Some(target) = self.folds.target_at(self.source.tree(), at) else {
            return;
        };
        let folded = self.change_folds(|folds, _| folds.toggle_target(&target));
        if folded {
            self.step_caret_out_of(&target, cx);
        }
        cx.notify();
    }

    /// Folds or unfolds the heading whose line starts at `line_start`, as
    /// its chevron or count does.
    pub fn toggle_heading_fold(&mut self, line_start: usize, cx: &mut Context<Self>) {
        let line = self.source.line_of(line_start);
        let Some(section) = self.foldable_heading(line) else {
            return;
        };
        let target = FoldTarget::Heading(section);
        if self.change_folds(|folds, _| folds.toggle_target(&target)) {
            self.step_caret_out_of(&target, cx);
        }
        cx.notify();
    }

    /// `fold.all`: every heading with something under it.
    pub fn fold_all(&mut self, cx: &mut Context<Self>) {
        self.change_folds(|folds, tree| folds.fold_all_headings(tree));
        let caret = self.cursor();
        let outermost = self
            .folds
            .folded_sections(self.source.tree())
            .into_iter()
            .find(|section| section.hides(caret));
        if let Some(section) = outermost {
            self.step_caret_out_of(&FoldTarget::Heading(section), cx);
        }
        cx.notify();
    }

    /// `fold.unfold-all`.
    pub fn unfold_all(&mut self, cx: &mut Context<Self>) {
        self.change_folds(|folds, _| folds.unfold_all_headings());
        cx.notify();
    }

    /// The lines of the folded headings, for keeping between launches.
    pub fn folded_heading_lines(&self) -> Vec<usize> {
        if let Some(pending) = &self.fold_ui.pending {
            return pending.clone();
        }
        self.folds.folded_heading_lines(self.source.tree())
    }

    /// Folds the headings on `lines` again, once the note is parsed.
    pub fn restore_folded_headings(&mut self, lines: Vec<usize>, cx: &mut Context<Self>) {
        if self.source.is_plain() {
            self.fold_ui.pending = Some(lines);
            return;
        }
        self.change_folds(|folds, tree| folds.restore_heading_lines(&lines, tree));
        cx.notify();
    }

    /// Folds kept for a note still being parsed, once its parse arrives.
    pub(crate) fn restore_pending_folds(&mut self) {
        let Some(lines) = self.fold_ui.pending.take() else {
            return;
        };
        self.change_folds(|folds, tree| folds.restore_heading_lines(&lines, tree));
        let selections = self.selected_ranges();
        self.change_folds(|folds, tree| folds.unfold_where_selected(tree, &selections));
    }

    /// Unfolds any heading the selection reaches into, so the caret never
    /// sits in hidden text. Answers whether one opened.
    pub(crate) fn unfold_at_selection(&mut self) -> bool {
        if self.folds.folded_heading_count() == 0 {
            return false;
        }
        let selections = self.selected_ranges();
        self.change_folds(|folds, tree| folds.unfold_where_selected(tree, &selections))
    }

    /// Where a caret motion from `from` to `target` ends when folds are in
    /// the way: past a fold going forward, at its heading's end going
    /// back, and at the heading's end past the last one.
    pub(crate) fn step_over_folds(&self, from: usize, target: usize) -> usize {
        if self.folds.folded_heading_count() == 0 {
            return target;
        }
        let folded = self.folds.folded_sections(self.source.tree());
        let mut target = target;
        for _ in 0..folded.len() {
            let Some(section) = folded.iter().find(|section| section.hides(target)) else {
                break;
            };
            let heading_end = self.source.line_range(section.line).end;
            let forward_to = Some(section.body_text.end).filter(|end| *end != usize::MAX);
            target = match target >= from {
                true => forward_to.unwrap_or(heading_end),
                false => heading_end,
            };
        }
        target
    }

    /// After folding `target` with the caret in it, puts the caret where
    /// it stays in view: the heading's line end, or beside a callout.
    fn step_caret_out_of(&mut self, target: &FoldTarget, cx: &mut Context<Self>) {
        let caret = self.cursor();
        let landing = match target {
            FoldTarget::Heading(section) if section.hides(caret) => {
                Some(self.source.line_range(section.line).end)
            }
            FoldTarget::Callout { range, .. } if range.start <= caret && caret <= range.end => {
                self.beside_callout(range)
            }
            _ => None,
        };
        if let Some(at) = landing {
            self.select(at, at, cx);
        }
    }

    /// The start of the line after a callout, or the end of the line
    /// before it at the note's end.
    fn beside_callout(&self, range: &Range<usize>) -> Option<usize> {
        let last = self.source.line_of(range.end.saturating_sub(1));
        if last + 1 < self.source.line_count() {
            return Some(self.source.line_range(last + 1).start);
        }
        let first = self.source.line_of(range.start);
        first
            .checked_sub(1)
            .map(|line| self.source.line_range(line).end)
    }

    /// Changes the folds and gives the lines whose heading folded or
    /// unfolded their new heights at once, so the scroll and the note's
    /// height follow before those lines are drawn.
    pub(crate) fn change_folds<R>(
        &mut self,
        change: impl FnOnce(&mut Folds, &SyntaxTree) -> R,
    ) -> R {
        let before = self.folds.folded_sections(self.source.tree());
        let result = change(&mut self.folds, self.source.tree());
        let after = self.folds.folded_sections(self.source.tree());
        if before != after {
            let changed: Vec<Range<usize>> = before
                .iter()
                .chain(&after)
                .filter(|section| before.contains(section) != after.contains(section))
                .map(|section| section.body.clone())
                .collect();
            self.remeasure_fold_lines(&changed, &after);
        }
        result
    }

    /// Collapsed lines take no height; the rest get their estimates back
    /// until they're laid out.
    fn remeasure_fold_lines(&mut self, changed: &[Range<usize>], folded: &[HeadingSection]) {
        let estimator = Estimator {
            theme: &self.theme,
            column_width: self.column_width,
        };
        for line in changed.iter().flat_map(Clone::clone) {
            let hidden = folded.iter().any(|section| section.body.contains(&line));
            let height = match hidden {
                true => gpui::px(0.),
                false => estimator.estimate(self.source.line_text(line)),
            };
            self.metrics.set(line, height);
        }
        self.scroll_anchor = None;
    }

    /// Gives every folded line no height, after the lines were estimated
    /// afresh.
    pub(crate) fn collapse_folded_metrics(&mut self) {
        if self.folds.folded_heading_count() == 0 {
            return;
        }
        for section in self.folds.folded_sections(self.source.tree()) {
            for line in section.body {
                self.metrics.set(line, gpui::px(0.));
            }
        }
    }

    /// The chevrons to draw over `frame`: beside the heading under the
    /// pointer and every folded one.
    pub(crate) fn fold_chevrons_for(&mut self, frame: &FrameLayout) -> Vec<FoldChevron> {
        if self.read_only {
            return Vec::new();
        }
        let pointer = self.pointer_at;
        let hovered = pointer.and_then(|at| self.foldable_line_at(frame, at));
        let chevrons: Vec<FoldChevron> = frame
            .lines
            .iter()
            .filter(|placed| !placed.visual.is_collapsed())
            .filter_map(|placed| {
                let section = self.foldable_heading(placed.visual.line)?;
                let folded = self.folds.is_heading_folded(section.line_start);
                if !folded && hovered != Some(section.line) {
                    return None;
                }
                let bounds = chevron_bounds(placed, frame.text_left, &self.theme);
                Some(FoldChevron {
                    line: section.line,
                    line_start: section.line_start,
                    folded,
                    hot: pointer.is_some_and(|at| bounds.contains(&at)),
                    bounds,
                })
            })
            .collect();
        self.fold_ui.chevrons.clone_from(&chevrons);
        chevrons
    }

    /// The foldable heading line under `position`, anywhere across the
    /// editor's width.
    fn foldable_line_at(&self, frame: &FrameLayout, position: Point<Pixels>) -> Option<usize> {
        let placed = frame
            .lines
            .iter()
            .find(|placed| placed.top <= position.y && position.y < placed.bottom())?;
        let line = placed.visual.line;
        self.foldable_heading(line).map(|_| line)
    }

    /// Follows the pointer over headings, redrawing when the chevron
    /// that shows or the one under the pointer changes. Answers whether
    /// the pointer is on a chevron or a folded heading's count.
    pub(crate) fn hover_folds(
        &mut self,
        position: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.read_only {
            return false;
        }
        let frame = self.frame.as_ref();
        let hovered = frame
            .zip(position)
            .and_then(|(frame, at)| self.foldable_line_at(frame, at));
        let previous = std::mem::replace(&mut self.fold_ui.hovered_line, hovered);
        let hot = position.and_then(|at| self.fold_control_at(at));
        if (hovered, hot) != (previous, self.fold_ui.hot) {
            self.fold_ui.hot = hot;
            cx.notify();
        }
        hot.is_some()
    }

    /// The fold control at `position`: a chevron that shows, or a folded
    /// heading's count.
    fn fold_control_at(&self, position: Point<Pixels>) -> Option<FoldControl> {
        let frame = self.frame.as_ref()?;
        if let Some((line, _)) = self.chevron_at(frame, position) {
            return Some(FoldControl::Chevron(line));
        }
        let (placed, piece) = frame.piece_at(position)?;
        (piece.hit == Hit::Unfold).then_some(FoldControl::Count(placed.visual.line))
    }

    /// The heading line, and where it starts, whose chevron shows at
    /// `position`.
    fn chevron_at(&self, frame: &FrameLayout, position: Point<Pixels>) -> Option<(usize, usize)> {
        frame
            .lines
            .iter()
            .filter(|placed| !placed.visual.is_collapsed())
            .find(|placed| chevron_bounds(placed, frame.text_left, &self.theme).contains(&position))
            .map(|placed| (placed.visual.line, placed.visual.start))
            .filter(|(line, _)| {
                let shows = self.fold_ui.hovered_line == Some(*line) || self.is_line_folded(*line);
                shows && self.foldable_heading(*line).is_some()
            })
    }

    /// The line whose hidden-line count is under the pointer, drawn
    /// darker.
    pub(crate) fn hot_count_line(&self) -> Option<usize> {
        match self.fold_ui.hot {
            Some(FoldControl::Count(line)) => Some(line),
            _ => None,
        }
    }

    /// A press on a chevron folds or unfolds its heading. Answers whether
    /// the press was on one.
    pub(crate) fn click_fold_chevron(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.read_only {
            return false;
        }
        let pressed = self
            .frame
            .as_ref()
            .and_then(|frame| self.chevron_at(frame, position));
        let Some((_, line_start)) = pressed else {
            return false;
        };
        self.toggle_heading_fold(line_start, cx);
        true
    }
}
