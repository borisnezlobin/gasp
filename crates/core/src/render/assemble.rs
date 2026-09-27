//! Sorting node effects into per-line plans.

use std::ops::Range;

use crate::syntax::SyntaxTree;

use super::effects::Effects;
use super::output::{LinePlan, LineStyle, Placement, StyleKey, StyledRun, Widget};
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
            })
            .collect(),
    };
    let spans = sorter.sort_spans(effects.spans);
    sorter.sort_hidden(effects.hidden);
    sorter.sort_widgets(effects.widgets);
    sorter.sort_line_styles(effects.line_styles);
    sorter.sort_collapsed(effects.collapsed);
    let mut plans = sorter.plans;
    for (plan, line_spans) in plans.iter_mut().zip(spans) {
        plan.runs = build_runs(&plan.range, &line_spans);
        plan.hidden = merge(&plan.range, std::mem::take(&mut plan.hidden));
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

    fn slots_of(&self, range: &Range<usize>) -> Vec<usize> {
        line_span(self.tree, range)
            .filter_map(|line| self.slot(line))
            .collect()
    }

    fn sort_spans(
        &self,
        spans: Vec<(Range<usize>, StyleKey)>,
    ) -> Vec<Vec<(Range<usize>, StyleKey)>> {
        let mut per_line = vec![Vec::new(); self.plans.len()];
        for (range, style) in spans {
            for slot in self.slots_of(&range) {
                per_line[slot].push((range.clone(), style));
            }
        }
        per_line
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
            let slots: Vec<usize> = lines.filter_map(|line| self.slot(line)).collect();
            for slot in slots {
                self.plans[slot].line_styles.push(style);
            }
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

fn line_span(tree: &SyntaxTree, range: &Range<usize>) -> Range<usize> {
    let lines = tree.lines();
    lines.line_of(range.start)..lines.line_of(range.end) + 1
}

/// Splits `line` at every span boundary and gives each piece the styles of
/// the spans covering it.
fn build_runs(line: &Range<usize>, spans: &[(Range<usize>, StyleKey)]) -> Vec<StyledRun> {
    if line.is_empty() {
        return Vec::new();
    }
    let mut cuts: Vec<usize> = vec![line.start, line.end];
    for (range, _) in spans {
        cuts.extend(
            [range.start, range.end]
                .into_iter()
                .filter(|at| line.contains(at)),
        );
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut runs: Vec<StyledRun> = Vec::with_capacity(cuts.len());
    for piece in cuts.windows(2) {
        let (start, end) = (piece[0], piece[1]);
        let mut styles: Vec<StyleKey> = spans
            .iter()
            .filter(|(range, _)| range.start <= start && end <= range.end)
            .map(|(_, style)| *style)
            .collect();
        styles.sort_unstable();
        styles.dedup();
        match runs.last_mut() {
            Some(last) if last.styles == styles => last.range.end = end,
            _ => runs.push(StyledRun {
                range: start..end,
                styles,
            }),
        }
    }
    runs
}

/// Clips ranges to the line, then sorts and merges them.
fn merge(line: &Range<usize>, mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.iter_mut().for_each(|range| {
        *range = range.start.max(line.start)..range.end.min(line.end);
    });
    ranges.retain(|range| !range.is_empty());
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// A line whose text is all hidden, with no widget and no cursor on it,
/// such as a closing fence or a setext underline, takes no space.
fn is_blank_after_hiding(revealer: &Revealer<'_>, plan: &LinePlan) -> bool {
    !plan.range.is_empty()
        && plan.hidden.first() == Some(&plan.range)
        && plan.widgets.is_empty()
        && !revealer.touches(&plan.range)
}
