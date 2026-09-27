//! Callouts folded or unfolded by clicking their header. The source's `-`
//! and `+` set the initial state; a click overrides it for this view
//! without editing the note. Moving the cursor into a callout always shows
//! its body, which is the keyboard twin of the click.

use std::collections::HashMap;
use std::ops::Range;

use editor_core::render::{LinePlan, WidgetKind};
use editor_core::syntax::{Edit, MarkupKind, NodeKind, SyntaxTree};

/// Fold overrides by the offset of the callout's `[!type]` token.
#[derive(Clone, Debug, Default)]
pub struct Folds {
    folded: HashMap<usize, bool>,
}

impl Folds {
    pub fn is_empty(&self) -> bool {
        self.folded.is_empty()
    }

    /// Flips the callout whose header token starts at `header`.
    pub fn toggle(&mut self, header: usize, folded_now: bool) {
        self.folded.insert(header, !folded_now);
    }

    /// Moves overrides after an edit, dropping ones inside it.
    pub fn map(&mut self, edit: &Edit) {
        if self.folded.is_empty() {
            return;
        }
        let delta = edit.new_len as isize - edit.old.len() as isize;
        self.folded = self
            .folded
            .drain()
            .filter(|(header, _)| !edit.old.contains(header))
            .map(|(header, folded)| {
                let moved = if header >= edit.old.end {
                    header.saturating_add_signed(delta)
                } else {
                    header
                };
                (moved, folded)
            })
            .collect();
    }

    /// Applies the overrides to planned lines. Callouts a selection touches
    /// stay open.
    pub fn apply(&self, plans: &mut [LinePlan], tree: &SyntaxTree, selections: &[Range<usize>]) {
        if self.folded.is_empty() {
            return;
        }
        for plan in plans {
            self.apply_to_header(plan);
            self.apply_to_body(plan, tree, selections);
        }
    }

    fn apply_to_header(&self, plan: &mut LinePlan) {
        for widget in &mut plan.widgets {
            if let WidgetKind::CalloutHeader {
                folded,
                fold: Some(_),
                ..
            } = &mut widget.kind
                && let Some(&wanted) = self.folded.get(&widget.range.start)
            {
                *folded = wanted;
            }
        }
    }

    fn apply_to_body(&self, plan: &mut LinePlan, tree: &SyntaxTree, selections: &[Range<usize>]) {
        let start = plan.range.start;
        for id in tree.path_at(start) {
            let node = tree.node(id);
            if !matches!(node.kind, NodeKind::Callout(_)) {
                continue;
            }
            let Some(header) = node
                .markup
                .iter()
                .find(|markup| markup.kind == MarkupKind::CalloutHeader)
            else {
                continue;
            };
            let Some(&folded) = self.folded.get(&header.range.start) else {
                continue;
            };
            let on_header_line = tree.lines().line_of(node.range.start) == plan.line;
            let touched = selections.iter().any(|selection| {
                selection.start <= node.range.end && selection.end >= node.range.start
            });
            if !on_header_line && !touched {
                let all_hidden = !plan.range.is_empty()
                    && plan.hidden.first() == Some(&plan.range)
                    && plan.widgets.is_empty();
                plan.collapsed = folded || all_hidden;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use editor_core::render::{RenderInput, RevealSettings, plan};
    use editor_core::syntax::parse;

    use super::*;

    const NOTE: &str = "> [!note]- Title\n> hidden body\n\nafter";

    fn planned(folds: &Folds, cursor: usize) -> Vec<LinePlan> {
        let tree = parse(NOTE);
        let settings = RevealSettings::default();
        let caret = cursor..cursor;
        let selections = std::slice::from_ref(&caret);
        let mut plan = plan(&RenderInput {
            text: NOTE,
            tree: &tree,
            selections,
            settings: &settings,
        });
        folds.apply(&mut plan.lines, &tree, selections);
        plan.lines
    }

    #[test]
    fn a_click_unfolds_a_closed_callout() {
        let mut folds = Folds::default();
        let end = NOTE.len();
        assert!(planned(&folds, end)[1].collapsed);
        folds.toggle(2, true);
        let lines = planned(&folds, end);
        assert!(!lines[1].collapsed);
        let folded = lines[0]
            .widgets
            .iter()
            .any(|widget| matches!(widget.kind, WidgetKind::CalloutHeader { folded: true, .. }));
        assert!(!folded);
        folds.toggle(2, false);
        assert!(planned(&folds, end)[1].collapsed);
    }

    #[test]
    fn edits_move_overrides() {
        let mut folds = Folds::default();
        folds.toggle(10, true);
        folds.map(&Edit {
            old: 0..0,
            new_len: 3,
        });
        assert_eq!(folds.folded.get(&13), Some(&false));
        folds.map(&Edit {
            old: 12..14,
            new_len: 0,
        });
        assert!(folds.is_empty());
    }
}
