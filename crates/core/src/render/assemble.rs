//! Sorting node effects into per-line plans.

use std::ops::Range;

use crate::syntax::SyntaxTree;

use super::effects::Effects;
use super::output::{LinePlan, LineStyle, Placement, StyleKey, StyledRun, TableRowPlan, Widget};
use super::reveal::Revealer;

/// Builds the plans for `lines` from the collected effects.
pub(crate) fn assemble(
    revealer: &Revealer<'_>,
    effects: Effects,
    lines: Range<usize>,
) -> Vec<LinePlan> {
    let (text, tree) = (revealer.text, revealer.tree);
    let mut sorter = LineSorter {
        tree,
        first: lines.start,
        plans: lines
            .map(|line| LinePlan {
                line,
                range: tree.lines().line_range(text, line),
                line_styles: Vec::new(),
                runs: Vec::new(),
                hidden: Vec::new(),
                widgets: Vec::new(),
                collapsed: false,
                table_row: None,
                shown_marker: None,
            })
            .collect(),
    };
    let spans = sorter.sort_spans(&effects.spans);
    sorter.sort_hidden(effects.hidden);
    sorter.sort_widgets(effects.widgets);
    sorter.sort_line_styles(effects.line_styles);
    sorter.sort_collapsed(effects.collapsed);
    sorter.sort_table_rows(effects.table_rows);
    sorter.sort_shown_markers(effects.shown_markers);
    let mut plans = sorter.plans;
    let mut cuts = Vec::new();
    for (slot, plan) in plans.iter_mut().enumerate() {
        plan.runs = build_runs(&plan.range, spans.line(slot), &mut cuts);
        merge(&plan.range, &mut plan.hidden);
        plan.collapsed |= is_blank_after_hiding(revealer, plan);
    }
    plans
}

/// Hands each effect to the plans of the lines it touches.
struct LineSorter<'a> {
    tree: &'a SyntaxTree,
    first: usize,
    plans: Vec<LinePlan>,
}

impl LineSorter<'_> {
    fn slot(&self, line: usize) -> Option<usize> {
        line.checked_sub(self.first)
            .filter(|&i| i < self.plans.len())
    }

    /// The slots of the planned lines that `range` touches.
    fn slots_of(&self, range: &Range<usize>) -> Range<usize> {
        self.slots_of_lines(line_span(self.tree, range))
    }

    fn slots_of_lines(&self, lines: Range<usize>) -> Range<usize> {
        let planned = self.first..self.first + self.plans.len();
        let start = lines.start.clamp(planned.start, planned.end);
        let end = lines.end.clamp(start, planned.end);
        start - self.first..end - self.first
    }

    /// Each line's spans, in the order they came, gathered by a counting
    /// sort into one list.
    fn sort_spans<'s>(&self, spans: &'s [(Range<usize>, StyleKey)]) -> SpansByLine<'s> {
        let slots: Vec<Range<usize>> = spans
            .iter()
            .map(|(range, _)| self.slots_of(range))
            .collect();
        let mut starts = vec![0; self.plans.len() + 1];
        for slot in slots.iter().flat_map(Range::clone) {
            starts[slot + 1] += 1;
        }
        for slot in 1..starts.len() {
            starts[slot] += starts[slot - 1];
        }
        let mut next = starts.clone();
        let mut entries = Vec::new();
        if let Some(first) = spans.first() {
            entries = vec![first; starts[starts.len() - 1]];
        }
        for (span, slots) in spans.iter().zip(slots) {
            for slot in slots {
                entries[next[slot]] = span;
                next[slot] += 1;
            }
        }
        SpansByLine { entries, starts }
    }

    fn sort_hidden(&mut self, hidden: Vec<Range<usize>>) {
        for range in hidden {
            for slot in self.slots_of(&range) {
                self.plans[slot].hidden.push(range.clone());
            }
        }
    }

    fn sort_widgets(&mut self, widgets: Vec<Widget>) {
        for widget in widgets {
            let anchor = match widget.placement {
                Placement::Below => widget.range.end,
                Placement::Replace | Placement::Above => widget.range.start,
            };
            if let Some(slot) = self.slot(self.tree.lines().line_of(anchor)) {
                self.plans[slot].widgets.push(widget);
            }
        }
    }

    fn sort_line_styles(&mut self, styles: Vec<(Range<usize>, LineStyle)>) {
        for (lines, style) in styles {
            for slot in self.slots_of_lines(lines) {
                self.plans[slot].line_styles.push(style);
            }
        }
    }

    fn sort_table_rows(&mut self, rows: Vec<(usize, TableRowPlan)>) {
        for (line, row) in rows {
            if let Some(slot) = self.slot(line) {
                self.plans[slot].table_row = Some(row);
            }
        }
    }

    /// A line's shown markers, such as a task's `- ` and `[ ] `, join
    /// into one range.
    fn sort_shown_markers(&mut self, markers: Vec<Range<usize>>) {
        for marker in markers {
            let line = self.tree.lines().line_of(marker.start);
            let Some(slot) = self.slot(line) else {
                continue;
            };
            let joined = match self.plans[slot].shown_marker.take() {
                Some(shown) => shown.start.min(marker.start)..shown.end.max(marker.end),
                None => marker,
            };
            self.plans[slot].shown_marker = Some(joined);
        }
    }

    fn sort_collapsed(&mut self, collapsed: Vec<Range<usize>>) {
        for line in collapsed.into_iter().flatten() {
            if let Some(slot) = self.slot(line) {
                self.plans[slot].collapsed = true;
            }
        }
    }
}

/// The styled spans touching each planned line.
struct SpansByLine<'s> {
    entries: Vec<&'s (Range<usize>, StyleKey)>,
    /// Where each line's entries start, with the end after the last.
    starts: Vec<usize>,
}

impl<'s> SpansByLine<'s> {
    fn line(&self, slot: usize) -> &[&'s (Range<usize>, StyleKey)] {
        &self.entries[self.starts[slot]..self.starts[slot + 1]]
    }
}

fn line_span(tree: &SyntaxTree, range: &Range<usize>) -> Range<usize> {
    let lines = tree.lines();
    lines.line_of(range.start)..lines.line_of(range.end) + 1
}

/// Splits `line` at every span boundary and gives each piece the styles of
/// the spans covering it.
fn build_runs(
    line: &Range<usize>,
    spans: &[&(Range<usize>, StyleKey)],
    cuts: &mut Vec<usize>,
) -> Vec<StyledRun> {
    if line.is_empty() {
        return Vec::new();
    }
    cuts.clear();
    cuts.extend([line.start, line.end]);
    for (range, _) in spans {
        cuts.extend(
            [range.start, range.end]
                .into_iter()
                .filter(|at| line.contains(at)),
        );
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut runs: Vec<StyledRun> = Vec::with_capacity(cuts.len() - 1);
    let mut styles: Vec<StyleKey> = Vec::new();
    for piece in cuts.windows(2) {
        let (start, end) = (piece[0], piece[1]);
        styles.clear();
        styles.extend(
            spans
                .iter()
                .filter(|(range, _)| range.start <= start && end <= range.end)
                .map(|(_, style)| *style),
        );
        styles.sort_unstable();
        styles.dedup();
        match runs.last_mut() {
            Some(last) if last.styles == styles => last.range.end = end,
            _ => runs.push(StyledRun {
                range: start..end,
                styles: styles.clone(),
            }),
        }
    }
    runs
}

/// Clips ranges to the line, then sorts and merges them, in place.
fn merge(line: &Range<usize>, ranges: &mut Vec<Range<usize>>) {
    ranges.iter_mut().for_each(|range| {
        *range = range.start.max(line.start)..range.end.min(line.end);
    });
    ranges.retain(|range| !range.is_empty());
    ranges.sort_by_key(|range| range.start);
    let mut kept: usize = 0;
    for index in 0..ranges.len() {
        let range = ranges[index].clone();
        match kept.checked_sub(1).map(|last| &mut ranges[last]) {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => {
                ranges[kept] = range;
                kept += 1;
            }
        }
    }
    ranges.truncate(kept);
}

/// A line whose text is all hidden, with no widget and no cursor on it,
/// such as a closing fence or a setext underline, takes no space.
fn is_blank_after_hiding(revealer: &Revealer<'_>, plan: &LinePlan) -> bool {
    !plan.range.is_empty()
        && plan.hidden.first() == Some(&plan.range)
        && plan.widgets.is_empty()
        && !revealer.touches(&plan.range)
}
