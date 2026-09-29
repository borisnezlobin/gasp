//! The plan as the phone holds it: after the first whole plan, each call
//! hands over only the lines that may have changed and how an edit moved
//! the rest, so a keystroke costs what it changed rather than the note's
//! length. The phone keeps its own copy and moves the lines it wasn't
//! sent.

use std::ops::Range;

use gasp_core::render::folds::Folds;
use gasp_core::render::{self, LineSplice, PlanChanges};
use gasp_core::syntax::SyntaxTree;

use crate::offsets::Utf16Offsets;
use crate::plan::{FoldableHeadings, LinePlan, line_plan};

/// What changed in the plan since the phone last asked.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PlanUpdate {
    /// Every line is in `lines`, and the phone's copy is replaced.
    pub whole: bool,
    /// How many lines the note has now.
    pub line_count: u32,
    /// How the edit since the last update moved the lines, applied before
    /// `lines` are put in.
    pub splice: Option<PlanSplice>,
    /// The lines that may have changed, in order, with their offsets now.
    pub lines: Vec<LinePlan>,
}

/// `removed` lines from `at` gave way to `inserted` new ones, which arrive
/// in the update's lines, and every line after them moved `shift` UTF-16
/// units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlanSplice {
    pub at: u32,
    pub removed: u32,
    pub inserted: u32,
    pub shift: i32,
}

/// What the phone was last sent, to work out what to send next.
#[derive(Debug, Default)]
pub(crate) struct ShownPlan {
    /// Whether the phone holds a plan from an update.
    holds_plan: bool,
    /// How much longer the text grew, in UTF-16 units, since the last
    /// update.
    utf16_growth: i64,
    /// The foldable headings as the phone has them, or `None` when the
    /// text or the folds changed since.
    headings: Option<FoldableHeadings>,
    sent_headings: FoldableHeadings,
    /// The lines the folds reached as the phone has them.
    fold_lines: Vec<Range<usize>>,
}

/// Everything an update is made from.
pub(crate) struct UpdateInput<'a> {
    pub lines: &'a [render::LinePlan],
    pub changes: PlanChanges,
    pub tree: &'a SyntaxTree,
    pub folds: &'a Folds,
    pub selections: &'a [Range<usize>],
    pub offsets: &'a Utf16Offsets,
}

impl ShownPlan {
    /// The text changed by `utf16_growth` units.
    pub(crate) fn text_edited(&mut self, utf16_growth: i64) {
        self.utf16_growth += utf16_growth;
        self.headings = None;
    }

    /// Headings or callouts were folded or unfolded.
    pub(crate) fn folds_changed(&mut self) {
        self.headings = None;
    }

    pub(crate) fn update(&mut self, mut input: UpdateInput<'_>, whole: bool) -> PlanUpdate {
        let headings = self
            .headings
            .take()
            .unwrap_or_else(|| FoldableHeadings::of(input.tree, input.folds));
        let fold_lines = input.folds.reached_lines(input.tree);
        let changes = match std::mem::replace(&mut input.changes, PlanChanges::Whole) {
            PlanChanges::Lines { splice, replanned } if self.holds_plan && !whole => {
                Some((splice, replanned))
            }
            _ => None,
        };
        let update = match changes {
            Some((splice, replanned)) => {
                let wanted = self.changed_lines(splice, replanned, &headings, &fold_lines);
                self.partial_update(&input, splice, &wanted, &headings)
            }
            None => whole_update(&input, &headings),
        };
        self.holds_plan = true;
        self.utf16_growth = 0;
        self.sent_headings = headings.clone();
        self.headings = Some(headings);
        self.fold_lines = fold_lines;
        update
    }

    /// The lines to send: the replanned ones, the ones the edit put in,
    /// those the folds reach now or reached before, and headings whose
    /// fold changed.
    fn changed_lines(
        &self,
        splice: Option<LineSplice>,
        replanned: Vec<Range<usize>>,
        headings: &FoldableHeadings,
        fold_lines: &[Range<usize>],
    ) -> Vec<Range<usize>> {
        let moved =
            |lines: &Range<usize>| splice.map_or(lines.clone(), |edit| edit.moved_lines(lines));
        let mut wanted = replanned;
        wanted.extend(splice.map(|edit| edit.at..edit.at + edit.inserted));
        wanted.extend(self.fold_lines.iter().map(moved));
        wanted.extend(fold_lines.iter().cloned());
        wanted.extend(refolded_headings(&self.sent_headings, headings, splice));
        wanted.sort_by_key(|lines| lines.start);
        joined(wanted)
    }

    fn partial_update(
        &self,
        input: &UpdateInput<'_>,
        splice: Option<LineSplice>,
        wanted: &[Range<usize>],
        headings: &FoldableHeadings,
    ) -> PlanUpdate {
        let count = input.lines.len();
        let mut lines = Vec::new();
        for range in wanted {
            let range = range.start.min(count)..range.end.min(count);
            let mut planned = input.lines[range].to_vec();
            if !input.folds.is_empty() {
                input
                    .folds
                    .apply(&mut planned, input.tree, input.selections);
            }
            lines.extend(
                planned
                    .iter()
                    .map(|line| line_plan(line, input.offsets, headings)),
            );
        }
        PlanUpdate {
            whole: false,
            line_count: count as u32,
            splice: splice.map(|edit| PlanSplice {
                at: edit.at as u32,
                removed: edit.removed as u32,
                inserted: edit.inserted as u32,
                shift: self.utf16_growth as i32,
            }),
            lines,
        }
    }
}

fn whole_update(input: &UpdateInput<'_>, headings: &FoldableHeadings) -> PlanUpdate {
    let mut planned = input.lines.to_vec();
    if !input.folds.is_empty() {
        input
            .folds
            .apply(&mut planned, input.tree, input.selections);
    }
    PlanUpdate {
        whole: true,
        line_count: planned.len() as u32,
        splice: None,
        lines: planned
            .iter()
            .map(|line| line_plan(line, input.offsets, headings))
            .collect(),
    }
}

/// The lines of headings that became foldable, stopped being foldable, or
/// were folded or unfolded, between what was sent (moved by `splice`) and
/// `now`.
fn refolded_headings(
    sent: &FoldableHeadings,
    now: &FoldableHeadings,
    splice: Option<LineSplice>,
) -> Vec<Range<usize>> {
    let moved = sent
        .0
        .iter()
        .map(|&(line, folded)| (splice.map_or(line, |edit| edit.moved_line(line)), folded));
    let mut before = moved.peekable();
    let mut after = now.0.iter().copied().peekable();
    let mut changed = Vec::new();
    loop {
        let differing = match (before.peek().copied(), after.peek().copied()) {
            (None, None) => break,
            (Some(old), Some(new)) if old == new => {
                before.next();
                after.next();
                continue;
            }
            (Some(old), Some(new)) if old.0 <= new.0 => before.next().map(|_| old.0),
            (Some(old), None) => before.next().map(|_| old.0),
            (_, Some(new)) => after.next().map(|_| new.0),
        };
        changed.extend(differing.map(|line| line..line + 1));
    }
    changed
}

/// Sorted line ranges joined where they overlap or touch.
fn joined(ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut joined: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges.into_iter().filter(|range| !range.is_empty()) {
        match joined.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => joined.push(range),
        }
    }
    joined
}

#[cfg(test)]
mod tests {
    use crate::document::NoteDocument;
    use crate::offsets::TextRange;
    use crate::plan::{LinePlan, WidgetKind};

    use super::PlanUpdate;

    fn moved(range: &mut TextRange, by: i32) {
        range.start = (range.start as i32 + by) as u32;
        range.end = (range.end as i32 + by) as u32;
    }

    /// Moves a line the way the phone does for lines it wasn't sent.
    fn shift(line: &mut LinePlan, by: i32, lines: i32) {
        line.line = (line.line as i32 + lines) as u32;
        moved(&mut line.range, by);
        line.runs
            .iter_mut()
            .for_each(|run| moved(&mut run.range, by));
        line.hidden.iter_mut().for_each(|hidden| moved(hidden, by));
        for widget in &mut line.widgets {
            moved(&mut widget.range, by);
            match &mut widget.kind {
                WidgetKind::CalloutHeader {
                    title: Some(title), ..
                } => moved(title, by),
                WidgetKind::CodeBlock { content, .. } => moved(content, by),
                _ => {}
            }
        }
        if let Some(row) = &mut line.table_row {
            row.cells.iter_mut().for_each(|cell| moved(cell, by));
            row.table_start = (row.table_start as i32 + by) as u32;
        }
    }

    /// The phone's copy of the plan after `update`.
    fn follow(shown: &mut Vec<LinePlan>, update: PlanUpdate) {
        if update.whole {
            *shown = update.lines;
            return;
        }
        if let Some(splice) = update.splice {
            let (at, removed, inserted) = (
                splice.at as usize,
                splice.removed as usize,
                splice.inserted as usize,
            );
            let lines = inserted as i32 - removed as i32;
            for line in &mut shown[at + removed..] {
                shift(line, splice.shift, lines);
            }
            let placeholder = shown[0].clone();
            shown.splice(at..at + removed, (0..inserted).map(|_| placeholder.clone()));
        }
        assert_eq!(shown.len(), update.line_count as usize);
        for line in update.lines {
            let index = line.line as usize;
            shown[index] = line;
        }
    }

    const NOTE: &str = "# Title *em*\n\nText **bold** $x^2$ [link](https://a.org) é𝜋\n\n\
        - one\n- two\n\n## Folded section\n\nunder it\n\n> [!note]- Folded\n> body\n\n\
        | a | b |\n| - | - |\n| 1 | 2 |\n\n```rust\nlet x = 1;\n```\n\nlast";

    #[test]
    fn updates_keep_the_phone_copy_as_a_fresh_plan() {
        let document = NoteDocument::new(NOTE.into());
        let mut shown = Vec::new();
        let mut partial = 0;
        let length = |document: &NoteDocument| document.text().encode_utf16().count() as u32;
        for step in 0..NOTE.len() as u32 * 2 {
            let utf16_len = length(&document);
            match step % 11 {
                3 => {
                    let at = step * 5 % utf16_len;
                    let typed = ["x", "\n", "*", "é", "## "][step as usize % 5];
                    document.replace(TextRange { start: at, end: at }, typed.into());
                }
                7 if utf16_len > 4 => {
                    let at = step * 3 % (utf16_len - 2);
                    document.replace(
                        TextRange {
                            start: at,
                            end: at + 2,
                        },
                        String::new(),
                    );
                }
                9 => {
                    let at = step * 13 % utf16_len;
                    document.toggle_fold(at);
                }
                _ => {}
            }
            let utf16_len = length(&document);
            let start = step * 7 % (utf16_len + 1);
            let selection = TextRange {
                start,
                end: (start + step % 3 * 4).min(utf16_len),
            };
            let update = document.plan_update(selection, false);
            partial += usize::from(!update.whole);
            follow(&mut shown, update);
            assert_eq!(shown, document.plan(selection).lines, "step {step}");
        }
        assert!(partial > NOTE.len(), "only {partial} partial updates");
    }

    #[test]
    fn asking_for_the_whole_plan_sends_every_line() {
        let document = NoteDocument::new(NOTE.into());
        let cursor = TextRange { start: 3, end: 3 };
        assert!(document.plan_update(cursor, false).whole);
        let again = document.plan_update(cursor, false);
        assert!(!again.whole && again.lines.len() <= 2, "{again:?}");
        let whole = document.plan_update(cursor, true);
        assert_eq!(whole.lines, document.plan(cursor).lines);
    }

    #[test]
    fn a_replacement_splitting_a_character_changes_nothing() {
        let document = NoteDocument::new("a𝜋b".into());
        let length = document.replace(TextRange { start: 2, end: 2 }, "x".into());
        assert_eq!(length, 4);
        assert_eq!(document.text(), "a𝜋b");
        document.replace(TextRange { start: 1, end: 3 }, "pi".into());
        assert_eq!(document.text(), "apib");
    }
}
