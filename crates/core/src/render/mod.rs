//! The render planner: turns source text, its syntax tree, the selections
//! and the reveal settings into per-line styled runs, hidden ranges and
//! widgets. The apps only draw what the plan says.

mod assemble;
mod effects;
mod output;
mod reveal;
mod settings;
mod widgets;

#[cfg(test)]
mod tests;

use std::ops::Range;

use crate::syntax::SyntaxTree;

pub use output::{
    LinePlan, LineStyle, Placement, RenderPlan, StyleKey, StyledRun, Widget, WidgetKind,
};
pub use settings::{RevealMode, RevealScope, RevealSettings};

use effects::{Effects, Planner};
use reveal::Revealer;

/// Everything the planner needs.
#[derive(Clone, Copy, Debug)]
pub struct RenderInput<'a> {
    pub text: &'a str,
    pub tree: &'a SyntaxTree,
    /// Selections as byte ranges; an empty range is a cursor. Either end
    /// may come first.
    pub selections: &'a [Range<usize>],
    pub settings: &'a RevealSettings,
}

/// Plans every line of the document.
pub fn plan(input: &RenderInput<'_>) -> RenderPlan {
    plan_lines(input, 0..input.tree.lines().line_count())
}

/// Plans only `lines` (zero-based, end exclusive), such as the viewport.
pub fn plan_lines(input: &RenderInput<'_>, lines: Range<usize>) -> RenderPlan {
    let line_index = input.tree.lines();
    let lines = lines.start.min(line_index.line_count())..lines.end.min(line_index.line_count());
    if lines.is_empty() {
        return RenderPlan::default();
    }
    let span =
        line_index.line_start(lines.start)..line_index.line_range(input.text, lines.end - 1).end;
    let mut planner = Planner {
        revealer: Revealer {
            text: input.text,
            tree: input.tree,
            selections: input
                .selections
                .iter()
                .map(|s| s.start.min(s.end)..s.start.max(s.end))
                .collect(),
            settings: input.settings,
        },
        effects: Effects::default(),
    };
    for id in input.tree.nodes_overlapping(span) {
        planner.visit(id);
    }
    RenderPlan {
        lines: assemble::assemble(&planner.revealer, planner.effects, lines),
    }
}
