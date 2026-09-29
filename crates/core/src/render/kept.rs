//! A whole note's plan kept between cursor moves and edits.
//!
//! What a line shows depends on the selection only through the nodes on
//! it, and a node's markup reveals when a selection touches a region
//! inside its top-level block (or the whole note, which any selection
//! touches). So when only the selection moves, the lines of the top-level
//! blocks that neither the old nor the new selection is in plan exactly
//! as before, and only the others are planned again. An edit that the
//! tree reparsed block by block leaves the other blocks' lines as they
//! were, only moved.

use std::ops::Range;

use super::{LinePlan, RenderInput, RevealSettings, plan, plan_lines};
use crate::syntax::{Edit, Reparsed, SyntaxTree};

/// The last whole-note plan, for re-planning only what a cursor move or
/// an edit can change. The owner tells it about every edit with
/// [`KeptPlan::edited`], or calls [`KeptPlan::forget`] when the text
/// changes some other way.
#[derive(Clone, Debug, Default)]
pub struct KeptPlan {
    kept: Option<Kept>,
    /// How the edit since the last plan moved the kept lines.
    splice: Option<LineSplice>,
}

/// How an edit moved a plan's lines: `removed` lines from `at` gave way to
/// `inserted` new ones, and every line after them moved `bytes` on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineSplice {
    pub at: usize,
    pub removed: usize,
    pub inserted: usize,
    pub bytes: isize,
}

impl LineSplice {
    /// Where line `line`, before the edit, is after it. Lines the edit
    /// took out go to where the new ones start.
    pub fn moved_line(&self, line: usize) -> usize {
        if line < self.at {
            line
        } else if line >= self.at + self.removed {
            line + self.inserted - self.removed
        } else {
            self.at
        }
    }

    /// Where the lines `lines`, before the edit, are after it, taking in
    /// the new lines when they reach into the ones taken out.
    pub fn moved_lines(&self, lines: &Range<usize>) -> Range<usize> {
        let end = if lines.end <= self.at {
            lines.end
        } else if lines.end >= self.at + self.removed {
            lines.end + self.inserted - self.removed
        } else {
            self.at + self.inserted
        };
        self.moved_line(lines.start)..end
    }
}

/// Which lines a plan may have changed since the one before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanChanges {
    /// It was planned afresh, so any line may differ.
    Whole,
    /// The edit's splice, if there was one, and the lines planned again
    /// (numbered after the splice), sorted and apart. Every other line
    /// is the one before, moved by the splice.
    Lines {
        splice: Option<LineSplice>,
        replanned: Vec<Range<usize>>,
    },
}

#[derive(Clone, Debug)]
struct Kept {
    lines: Vec<LinePlan>,
    selections: Vec<Range<usize>>,
    settings: RevealSettings,
    /// Lines an edit reparsed, still to plan.
    unplanned: Option<Range<usize>>,
}

impl KeptPlan {
    /// Drops the kept plan.
    pub fn forget(&mut self) {
        self.kept = None;
        self.splice = None;
    }

    /// Follows `edit`, after which `tree` reparsed what `reparsed` says:
    /// the lines of reparsed blocks wait to be planned, and the lines
    /// after them move along.
    pub fn edited(&mut self, edit: &Edit, reparsed: &Reparsed, tree: &SyntaxTree) {
        let followed = match (self.kept.as_mut(), reparsed) {
            (Some(kept), Reparsed::Blocks { old, new }) => kept.follow(edit, old, new, tree),
            _ => None,
        };
        match followed {
            Some(splice) => self.splice = Some(splice),
            None => self.forget(),
        }
    }

    /// Every line's plan for `input`, the same as [`plan`] gives.
    pub fn plan(&mut self, input: &RenderInput<'_>) -> &[LinePlan] {
        self.plan_with_changes(input).0
    }

    /// Every line's plan for `input`, and which lines may differ from the
    /// last plan's.
    pub fn plan_with_changes(&mut self, input: &RenderInput<'_>) -> (&[LinePlan], PlanChanges) {
        let selections = ordered(input.selections);
        let splice = self.splice.take();
        let (kept, changes) = match self.kept.take() {
            Some(mut kept) if kept.can_follow(input.settings, &selections) => {
                let replanned = kept.replan_moved(input, &selections);
                kept.selections = selections;
                (kept, PlanChanges::Lines { splice, replanned })
            }
            _ => {
                let kept = Kept {
                    lines: plan(input).lines,
                    selections,
                    settings: input.settings.clone(),
                    unplanned: None,
                };
                (kept, PlanChanges::Whole)
            }
        };
        (&self.kept.insert(kept).lines, changes)
    }
}

impl Kept {
    /// Makes room for the reparsed lines and moves the ones after them,
    /// answering how. Answers `None` when the kept lines don't line up
    /// with the edit, which plans afresh.
    fn follow(
        &mut self,
        edit: &Edit,
        old: &Range<usize>,
        new: &Range<usize>,
        tree: &SyntaxTree,
    ) -> Option<LineSplice> {
        if self.unplanned.is_some() {
            return None;
        }
        let index = tree.lines();
        let reparsed = index.line_of(new.start)..index.line_of(new.end) + 1;
        let before = self
            .lines
            .partition_point(|line| line.range.end < old.start);
        let after = self
            .lines
            .partition_point(|line| line.range.start <= old.end);
        if before != reparsed.start || after < before {
            return None;
        }
        let bytes = edit.new_len as isize - edit.old.len() as isize;
        let lines = reparsed.len() as isize - (after - before) as isize;
        for line in &mut self.lines[after..] {
            line.shift(bytes, lines);
        }
        self.lines
            .splice(before..after, reparsed.clone().map(LinePlan::unplanned));
        for selection in &mut self.selections {
            *selection = moved_through(selection.start, edit)..moved_through(selection.end, edit);
        }
        let splice = LineSplice {
            at: before,
            removed: after - before,
            inserted: reparsed.len(),
            bytes,
        };
        self.unplanned = Some(reparsed);
        (self.lines.len() == index.line_count()).then_some(splice)
    }
    /// Whether moving from the kept selections to `selections` changes
    /// only the blocks they're in. Going from no selection to one reveals
    /// what any selection reveals, all over the note, so that plans afresh.
    fn can_follow(&self, settings: &RevealSettings, selections: &[Range<usize>]) -> bool {
        self.settings == *settings && !self.selections.is_empty() && !selections.is_empty()
    }

    /// Plans again the lines the selections left or entered and the
    /// lines an edit reparsed, answering which they were.
    fn replan_moved(
        &mut self,
        input: &RenderInput<'_>,
        selections: &[Range<usize>],
    ) -> Vec<Range<usize>> {
        let touched = self.selections.iter().chain(selections);
        let mut stale: Vec<Range<usize>> = touched
            .flat_map(|selection| stale_lines(input.tree, input.text, selection))
            .chain(self.unplanned.take())
            .collect();
        stale.sort_by_key(|lines| lines.start);
        let stale = merged(stale);
        for lines in &stale {
            let fresh = plan_lines(input, lines.clone()).lines;
            let lines = lines.start..lines.end.min(self.lines.len());
            self.lines.splice(lines, fresh);
        }
        let count = self.lines.len();
        stale
            .into_iter()
            .map(|lines| lines.start.min(count)..lines.end.min(count))
            .filter(|lines| !lines.is_empty())
            .collect()
    }
}

/// The lines whose plan can change when `selection` arrives or leaves:
/// its own lines and those of every top-level block that shares one.
fn stale_lines(tree: &SyntaxTree, text: &str, selection: &Range<usize>) -> Vec<Range<usize>> {
    let index = tree.lines();
    let selected = index.line_of(selection.start.min(text.len()))
        ..index.line_of(selection.end.min(text.len())) + 1;
    let from = index.line_start(selected.start);
    let to = index.line_range(text, selected.end - 1).end;
    let blocks = tree.blocks();
    let first = blocks.partition_point(|&id| tree.node(id).range.end < from);
    let block_lines = blocks[first..]
        .iter()
        .map(|&id| &tree.node(id).range)
        .take_while(|range| range.start <= to)
        .map(|range| index.line_of(range.start)..index.line_of(range.end) + 1);
    std::iter::once(selected).chain(block_lines).collect()
}

/// Sorted line ranges joined where they overlap or touch.
fn merged(ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut joined: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match joined.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => joined.push(range),
        }
    }
    joined
}

/// Where `at`, before `edit`, is after it: offsets inside the replaced
/// text go to its start.
fn moved_through(at: usize, edit: &Edit) -> usize {
    if at <= edit.old.start {
        at
    } else if at >= edit.old.end {
        at + edit.new_len - edit.old.len()
    } else {
        edit.old.start
    }
}

fn ordered(selections: &[Range<usize>]) -> Vec<Range<usize>> {
    selections
        .iter()
        .map(|s| s.start.min(s.end)..s.start.max(s.end))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{RevealMode, RevealScope};
    use crate::syntax::parse;

    const NOTE: &str = "---\ntags: [a, b]\n---\n# Title *em*\n\nText with **bold**, $x^2$ \
        and [a link](https://a.org).\n\n- one\n- two `code`\n  - nested ==mark==\n\n> [!note]- \
        Folded\n> body $$y$$\n\n| a | b |\n| - | - |\n| *1* | 2 |\n\n```rust\nlet x = 1;\n```\n\n\
        $$\n\\frac{a}{b}\n$$\n\nLast line[^1].\n\n[^1]: A note.\n";

    fn check_moves(settings: &RevealSettings) {
        let tree = parse(NOTE);
        let mut kept = KeptPlan::default();
        let offsets: Vec<usize> = (0..=NOTE.len())
            .filter(|&at| NOTE.is_char_boundary(at))
            .collect();
        let selections = offsets
            .iter()
            .map(|&at| at..at)
            .chain(offsets.windows(7).map(|pair| pair[0]..pair[6]))
            .chain([NOTE.len()..0, 3..NOTE.len() - 5]);
        for selection in selections {
            let selections = [selection.clone()];
            let input = RenderInput {
                text: NOTE,
                tree: &tree,
                selections: &selections,
                settings,
            };
            let fresh = plan(&input).lines;
            assert_eq!(kept.plan(&input), &fresh[..], "selection {selection:?}");
        }
    }

    #[test]
    fn moving_the_cursor_plans_as_a_fresh_plan_would() {
        check_moves(&RevealSettings::default());
        for scope in [RevealScope::Line, RevealScope::Block] {
            check_moves(&RevealSettings::new(RevealMode::AroundCursor { scope }));
        }
    }

    /// The lines an app would show if it took only the changed lines of
    /// each plan and moved the rest by the splice.
    fn follow_changes(shown: &mut Vec<LinePlan>, lines: &[LinePlan], changes: PlanChanges) {
        let PlanChanges::Lines { splice, replanned } = changes else {
            *shown = lines.to_vec();
            return;
        };
        if let Some(splice) = splice {
            let moved_by = splice.inserted as isize - splice.removed as isize;
            for line in &mut shown[splice.at + splice.removed..] {
                line.shift(splice.bytes, moved_by);
            }
            let fresh = (0..splice.inserted).map(|at| LinePlan::unplanned(splice.at + at));
            shown.splice(splice.at..splice.at + splice.removed, fresh);
        }
        for range in replanned {
            shown[range.clone()].clone_from_slice(&lines[range]);
        }
    }

    #[test]
    fn the_changes_name_every_line_that_differs() {
        let settings = RevealSettings::default();
        let mut text = NOTE.to_owned();
        let mut tree = parse(&text);
        let mut kept = KeptPlan::default();
        let mut shown = Vec::new();
        let mut spliced = 0;
        for step in 0..NOTE.len() {
            if step % 5 == 2 {
                let mut at = step * 7 % text.len();
                while !text.is_char_boundary(at) {
                    at -= 1;
                }
                let typed = ["x", "\n", "*", "$"][step % 4];
                text.insert_str(at, typed);
                let edit = Edit {
                    old: at..at,
                    new_len: typed.len(),
                };
                let reparsed = tree.edit(&text, &edit);
                kept.edited(&edit, &reparsed, &tree);
            }
            let mut cursor = step * 3 % text.len();
            while !text.is_char_boundary(cursor) {
                cursor -= 1;
            }
            let caret = cursor..cursor;
            let input = RenderInput {
                text: &text,
                tree: &tree,
                selections: std::slice::from_ref(&caret),
                settings: &settings,
            };
            let (lines, changes) = kept.plan_with_changes(&input);
            if matches!(
                changes,
                PlanChanges::Lines {
                    splice: Some(_),
                    ..
                }
            ) {
                spliced += 1;
            }
            follow_changes(&mut shown, lines, changes);
            assert_eq!(shown, plan(&input).lines, "step {step}");
        }
        assert!(spliced > NOTE.len() / 10, "{spliced} edits followed");
    }

    #[test]
    fn new_settings_plan_afresh() {
        let tree = parse(NOTE);
        let mut kept = KeptPlan::default();
        let cursor = 5..5;
        let selections = [cursor];
        for settings in [
            RevealSettings::default(),
            RevealSettings::new(RevealMode::AlwaysShown),
        ] {
            let input = RenderInput {
                text: NOTE,
                tree: &tree,
                selections: &selections,
                settings: &settings,
            };
            let fresh = plan(&input).lines;
            assert_eq!(kept.plan(&input), &fresh[..]);
        }
    }
}
