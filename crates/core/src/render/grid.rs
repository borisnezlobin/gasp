//! Tables drawn as grids. Each row's line gets a [`TableRowPlan`] naming
//! its cells, the delimiter row's line takes no space, and the pipes hide.
//! The cells' text is planned as any text is, so its markup reveals
//! around the cursor; the backslash of an escaped pipe hides with it.

use std::ops::Range;

use crate::syntax::{NodeId, NodeKind};
use crate::table::cell_content;

use super::effects::Planner;
use super::output::TableRowPlan;

impl Planner<'_> {
    /// A table: its source while it shows it, its grid otherwise.
    pub(super) fn visit_table(&mut self, id: NodeId) {
        self.add_line_styles(id);
        let markup = self.revealer.tree.node(id).markup.clone();
        for token in &markup {
            self.markup_effect(id, token);
        }
        if self.revealer.table_shows_source(id) {
            return;
        }
        for delimiter in &markup {
            let line = self.revealer.tree.lines().line_of(delimiter.range.start);
            self.effects.collapsed.push(line..line + 1);
        }
        let tree = self.revealer.tree;
        let node = tree.node(id);
        let NodeKind::Table { alignments } = &node.kind else {
            return;
        };
        let count = node.children.len();
        for (index, &row) in node.children.iter().enumerate() {
            let cells: Vec<Range<usize>> = tree
                .node(row)
                .children
                .iter()
                .map(|&cell| self.cell_text(&tree.node(cell).range))
                .collect();
            let line = tree.lines().line_of(tree.node(row).range.start);
            let plan = TableRowPlan {
                index,
                count,
                alignments: alignments.clone(),
                cells,
                table_start: node.range.start,
            };
            self.effects.table_rows.push((line, plan));
        }
    }

    /// What a cell shows: its text without the padding around it, but
    /// reaching to a cursor in the padding. Escaped pipes' backslashes
    /// hide unless the cursor is at them.
    fn cell_text(&mut self, cell: &Range<usize>) -> Range<usize> {
        let text = self.revealer.text;
        let mut shown = cell_content(text, cell);
        for selection in &self.revealer.selections {
            for end in [selection.start, selection.end] {
                if cell.start <= end && end <= cell.end {
                    shown = shown.start.min(end)..shown.end.max(end);
                }
            }
        }
        let source = &text[cell.clone()];
        let escapes = source.match_indices("\\|").map(|(at, _)| cell.start + at);
        let hidden: Vec<Range<usize>> = escapes
            .filter(|&at| !self.revealer.touches(&(at..at + 2)))
            .map(|at| at..at + 1)
            .collect();
        self.effects.hidden.extend(hidden);
        shown
    }
}
