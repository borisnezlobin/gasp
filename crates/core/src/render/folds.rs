//! Folding: a heading's section or a callout's body collapsed to its first
//! line, without editing the note.
//!
//! A callout's `-` and `+` set whether it starts folded, and a click on its
//! header overrides that for the view. A heading folds everything up to the
//! next heading of the same or a higher level. Moving the cursor into a
//! folded body shows it again, which is the keyboard twin of the click; a
//! cursor on the heading's own line leaves it folded, so it can be toggled
//! from there.

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

use crate::syntax::{Edit, Fold, MarkupKind, NodeKind, SyntaxTree};

use super::output::{LinePlan, Placement, Widget, WidgetKind};

/// A heading and the lines its section covers after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingSection {
    /// Where the heading's line starts, which names the fold.
    pub line_start: usize,
    /// The heading's line, zero-based.
    pub line: usize,
    pub level: u8,
    /// The lines below the heading up to the next heading of the same or a
    /// higher level, end exclusive. Empty when nothing is there to fold.
    pub body: Range<usize>,
    /// Where the body's text is, from the start of its first line to the
    /// start of the line after it (or `usize::MAX` at the note's end).
    pub body_text: Range<usize>,
}

impl HeadingSection {
    /// Whether `offset` is in the folded-away part: from the body's start
    /// up to the next section's first line, or to the note's end for the
    /// last section.
    pub fn hides(&self, offset: usize) -> bool {
        !self.body.is_empty() && self.body_text.start <= offset && offset < self.body_text.end
    }

    /// Whether a selection opens the section: an end of it is in the
    /// body. One that takes in the whole body, such as select all, leaves
    /// it folded, as does a cursor at the start of the next section.
    fn opened_by(&self, selection: &Range<usize>, note_len: usize) -> bool {
        let (start, end) = (
            selection.start.min(selection.end),
            selection.start.max(selection.end),
        );
        let body_end = self.body_text.end.min(note_len);
        let covers = start <= self.body_text.start && end >= body_end;
        let inside = |offset: usize| self.body_text.start <= offset && offset < self.body_text.end;
        !self.body.is_empty() && !covers && (inside(start) || inside(end))
    }
}

/// What a fold command acts on at the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FoldTarget {
    Heading(HeadingSection),
    /// A callout marked foldable (`[!note]-` or `[!note]+`), named by where
    /// its `[!type]` token starts.
    Callout {
        header: usize,
        folded: bool,
        /// The whole callout, header line included.
        range: Range<usize>,
    },
}

/// Folded headings, and callouts folded or unfolded by a click.
#[derive(Clone, Debug, Default)]
pub struct Folds {
    /// Callout overrides by the offset of the callout's `[!type]` token.
    folded: HashMap<usize, bool>,
    /// Folded headings by where their line starts.
    headings: BTreeSet<usize>,
}

impl Folds {
    pub fn is_empty(&self) -> bool {
        self.folded.is_empty() && self.headings.is_empty()
    }

    /// Flips the callout whose header token starts at `header`.
    pub fn toggle(&mut self, header: usize, folded_now: bool) {
        self.folded.insert(header, !folded_now);
    }

    /// Folds or unfolds the heading whose line starts at `line_start`;
    /// answers whether it's folded now.
    pub fn toggle_heading(&mut self, line_start: usize) -> bool {
        if self.headings.remove(&line_start) {
            return false;
        }
        self.headings.insert(line_start);
        true
    }

    pub fn is_heading_folded(&self, line_start: usize) -> bool {
        self.headings.contains(&line_start)
    }

    /// How many headings are folded.
    pub fn folded_heading_count(&self) -> usize {
        self.headings.len()
    }

    /// Whether the callout with its `[!type]` token at `header` is folded:
    /// a click's choice, else what its `-` or `+` says. `None` when it
    /// doesn't fold.
    pub fn callout_folded(&self, header: usize, fold: Option<Fold>) -> Option<bool> {
        let fold = fold?;
        Some(
            self.folded
                .get(&header)
                .copied()
                .unwrap_or(fold == Fold::Closed),
        )
    }

    /// Folds or unfolds `target`; answers whether it's folded now.
    pub fn toggle_target(&mut self, target: &FoldTarget) -> bool {
        match target {
            FoldTarget::Heading(section) => self.toggle_heading(section.line_start),
            FoldTarget::Callout { header, folded, .. } => {
                self.toggle(*header, *folded);
                !folded
            }
        }
    }

    /// The folded headings' sections, in order.
    pub fn folded_sections(&self, tree: &SyntaxTree) -> Vec<HeadingSection> {
        if self.headings.is_empty() {
            return Vec::new();
        }
        heading_sections(tree)
            .into_iter()
            .filter(|section| self.headings.contains(&section.line_start))
            .filter(|section| !section.body.is_empty())
            .collect()
    }

    /// Unfolds every heading whose section a selection reaches into, so
    /// the cursor never sits in hidden text. Answers whether any opened.
    pub fn unfold_where_selected(
        &mut self,
        tree: &SyntaxTree,
        selections: &[Range<usize>],
    ) -> bool {
        let note_len = tree.root().range.end;
        let opened: Vec<usize> = self
            .folded_sections(tree)
            .into_iter()
            .filter(|section| {
                selections
                    .iter()
                    .any(|selection| section.opened_by(selection, note_len))
            })
            .map(|section| section.line_start)
            .collect();
        for line_start in &opened {
            self.headings.remove(line_start);
        }
        !opened.is_empty()
    }

    /// What a fold command at `at` acts on: the heading on its line, else
    /// the foldable callout it's in, else the innermost section it's in.
    pub fn target_at(&self, tree: &SyntaxTree, at: usize) -> Option<FoldTarget> {
        let line = tree.lines().line_of(at);
        let sections: Vec<HeadingSection> = heading_sections(tree)
            .into_iter()
            .filter(|section| !section.body.is_empty())
            .collect();
        if let Some(section) = sections.iter().find(|section| section.line == line) {
            return Some(FoldTarget::Heading(section.clone()));
        }
        if let Some(callout) = self.callout_at(tree, at) {
            return Some(callout);
        }
        sections
            .into_iter()
            .rev()
            .find(|section| section.line < line && section.body.contains(&line))
            .map(FoldTarget::Heading)
    }

    /// The innermost foldable callout holding `at`.
    fn callout_at(&self, tree: &SyntaxTree, at: usize) -> Option<FoldTarget> {
        tree.path_at(at).into_iter().rev().find_map(|id| {
            let node = tree.node(id);
            let NodeKind::Callout(info) = &node.kind else {
                return None;
            };
            let header = node
                .markup
                .iter()
                .find(|markup| markup.kind == MarkupKind::CalloutHeader)?
                .range
                .start;
            let folded = self.callout_folded(header, info.fold)?;
            Some(FoldTarget::Callout {
                header,
                folded,
                range: node.range.clone(),
            })
        })
    }

    /// Adds a count of the hidden lines after each folded heading that
    /// stays folded, for the apps that draw one: its body's lines up to
    /// the last one with text. Headings a selection opens get none.
    pub fn mark_folded_headings(
        &self,
        plans: &mut [LinePlan],
        text: &str,
        tree: &SyntaxTree,
        selections: &[Range<usize>],
    ) {
        if self.headings.is_empty() {
            return;
        }
        let note_len = tree.root().range.end;
        for section in self.folded_sections(tree) {
            let opened = selections
                .iter()
                .any(|selection| section.opened_by(selection, note_len));
            let Some(plan) = plans.iter_mut().find(|plan| plan.line == section.line) else {
                continue;
            };
            if opened || plan.collapsed {
                continue;
            }
            let end = plan.range.end;
            plan.widgets.push(Widget {
                kind: WidgetKind::FoldedLines {
                    count: written_lines(text, tree, &section.body),
                },
                range: end..end,
                placement: Placement::Replace,
            });
        }
    }

    /// Folds every heading that has something under it.
    pub fn fold_all_headings(&mut self, tree: &SyntaxTree) {
        self.headings = heading_sections(tree)
            .into_iter()
            .filter(|section| !section.body.is_empty())
            .map(|section| section.line_start)
            .collect();
    }

    pub fn unfold_all_headings(&mut self) {
        self.headings.clear();
    }

    /// The lines of the folded headings, for keeping between launches.
    pub fn folded_heading_lines(&self, tree: &SyntaxTree) -> Vec<usize> {
        heading_sections(tree)
            .into_iter()
            .filter(|section| self.headings.contains(&section.line_start))
            .map(|section| section.line)
            .collect()
    }

    /// Folds the headings on `lines` again, skipping lines that aren't
    /// headings any more.
    pub fn restore_heading_lines(&mut self, lines: &[usize], tree: &SyntaxTree) {
        self.headings = heading_sections(tree)
            .into_iter()
            .filter(|section| lines.contains(&section.line) && !section.body.is_empty())
            .map(|section| section.line_start)
            .collect();
    }

    /// Moves folds after an edit, dropping ones inside it.
    pub fn map(&mut self, edit: &Edit) {
        if self.is_empty() {
            return;
        }
        let delta = edit.new_len as isize - edit.old.len() as isize;
        let moved = |at: usize| {
            if at >= edit.old.end {
                at.saturating_add_signed(delta)
            } else {
                at
            }
        };
        self.folded = self
            .folded
            .drain()
            .filter(|(header, _)| !edit.old.contains(header))
            .map(|(header, folded)| (moved(header), folded))
            .collect();
        self.headings = std::mem::take(&mut self.headings)
            .into_iter()
            .filter(|start| !edit.old.contains(start))
            .map(moved)
            .collect();
    }

    /// The lines whose plan the folds can change: the section of every
    /// folded heading, and every callout folded or unfolded in this view.
    pub fn reached_lines(&self, tree: &SyntaxTree) -> Vec<Range<usize>> {
        let index = tree.lines();
        let callouts = self.folded.keys().filter_map(|&header| {
            let callout = tree
                .path_at(header)
                .into_iter()
                .map(|id| tree.node(id))
                .find(|node| matches!(node.kind, NodeKind::Callout(_)))?;
            Some(index.line_of(callout.range.start)..index.line_of(callout.range.end) + 1)
        });
        let sections: Vec<Range<usize>> = if self.headings.is_empty() {
            Vec::new()
        } else {
            heading_sections(tree)
                .into_iter()
                .filter(|section| self.headings.contains(&section.line_start))
                .map(|section| section.body)
                .collect()
        };
        callouts.chain(sections).collect()
    }

    /// Applies the folds to planned lines. Bodies a selection touches stay
    /// open.
    pub fn apply(&self, plans: &mut [LinePlan], tree: &SyntaxTree, selections: &[Range<usize>]) {
        if !self.folded.is_empty() {
            for plan in plans.iter_mut() {
                self.apply_to_header(plan);
                self.apply_to_body(plan, tree, selections);
            }
        }
        if !self.headings.is_empty() {
            self.fold_sections(plans, tree, selections);
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
            if !on_header_line && !touches(selections, &node.range) {
                let all_hidden = !plan.range.is_empty()
                    && plan.hidden.first() == Some(&plan.range)
                    && plan.widgets.is_empty();
                plan.collapsed = folded || all_hidden;
            }
        }
    }

    fn fold_sections(
        &self,
        plans: &mut [LinePlan],
        tree: &SyntaxTree,
        selections: &[Range<usize>],
    ) {
        let note_len = tree.root().range.end;
        let folded: Vec<HeadingSection> = self
            .folded_sections(tree)
            .into_iter()
            .filter(|section| {
                !selections
                    .iter()
                    .any(|selection| section.opened_by(selection, note_len))
            })
            .collect();
        for plan in plans.iter_mut() {
            if folded
                .iter()
                .any(|section| section.body.contains(&plan.line))
            {
                plan.collapsed = true;
            }
        }
    }
}

/// How many of `lines` there are up to the last one with text on it, at
/// least one.
fn written_lines(text: &str, tree: &SyntaxTree, lines: &Range<usize>) -> usize {
    let index = tree.lines();
    let last_written = lines
        .clone()
        .rev()
        .find(|&line| !text[index.line_range(text, line)].trim().is_empty());
    last_written.map_or(1, |line| line + 1 - lines.start)
}

/// Whether any selection reaches into `range`, its ends included.
fn touches(selections: &[Range<usize>], range: &Range<usize>) -> bool {
    selections
        .iter()
        .any(|selection| selection.start <= range.end && selection.end >= range.start)
}

/// Every heading with the section below it, in order.
pub fn heading_sections(tree: &SyntaxTree) -> Vec<HeadingSection> {
    let lines = tree.lines();
    let headings: Vec<(usize, u8)> = tree
        .block_preorder()
        .into_iter()
        .filter_map(|id| {
            let node = tree.node(id);
            match node.kind {
                NodeKind::Heading { level, .. } => Some((lines.line_of(node.range.start), level)),
                _ => None,
            }
        })
        .collect();
    let line_count = lines.line_count();
    headings
        .iter()
        .enumerate()
        .map(|(index, &(line, level))| {
            let end = headings[index + 1..]
                .iter()
                .find(|(_, next_level)| *next_level <= level)
                .map_or(line_count, |(next_line, _)| *next_line);
            let body = (line + 1).min(end)..end;
            let body_text = match (body.is_empty(), body.end < line_count) {
                (true, _) => 0..0,
                (false, true) => lines.line_start(body.start)..lines.line_start(body.end),
                (false, false) => lines.line_start(body.start)..usize::MAX,
            };
            HeadingSection {
                line_start: lines.line_start(line),
                line,
                level,
                body,
                body_text,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{RenderInput, RevealSettings, plan};
    use crate::syntax::parse;

    const NOTE: &str = "> [!note]- Title\n> hidden body\n\nafter";

    #[test]
    fn headings_are_found_in_every_kind_of_block() {
        let text = "# One\n\npara *em*\n\n- item\n  ## In a list\n\n> ### In a quote\n\n\
                    > [!note] Callout\n> #### In a callout\n\n[^1]: note\n\n    ##### In a \
                    footnote\n\n<<<<<<< this device\n## Mine\n=======\n## Theirs\n>>>>>>> other\n\n\
                    Setext\n======\n\n| # not | a heading |\n| - | - |\n| x | y |\n";
        let tree = parse(text);
        let every_heading: Vec<(usize, u8)> = tree
            .preorder()
            .into_iter()
            .filter_map(|id| match tree.node(id).kind {
                NodeKind::Heading { level, .. } => {
                    Some((tree.lines().line_of(tree.node(id).range.start), level))
                }
                _ => None,
            })
            .collect();
        let found: Vec<(usize, u8)> = heading_sections(&tree)
            .iter()
            .map(|section| (section.line, section.level))
            .collect();
        assert_eq!(found, every_heading);
        assert!(found.len() >= 7, "{found:?}");
    }

    fn planned(text: &str, folds: &Folds, cursor: usize) -> Vec<LinePlan> {
        let tree = parse(text);
        let settings = RevealSettings::default();
        let caret = cursor..cursor;
        let selections = std::slice::from_ref(&caret);
        let mut plan = plan(&RenderInput {
            text,
            tree: &tree,
            selections,
            settings: &settings,
        });
        folds.apply(&mut plan.lines, &tree, selections);
        plan.lines
    }

    fn collapsed(lines: &[LinePlan]) -> Vec<usize> {
        lines
            .iter()
            .filter(|line| line.collapsed)
            .map(|line| line.line)
            .collect()
    }

    #[test]
    fn a_click_unfolds_a_closed_callout() {
        let mut folds = Folds::default();
        let end = NOTE.len();
        assert!(planned(NOTE, &folds, end)[1].collapsed);
        folds.toggle(2, true);
        let lines = planned(NOTE, &folds, end);
        assert!(!lines[1].collapsed);
        let folded = lines[0]
            .widgets
            .iter()
            .any(|widget| matches!(widget.kind, WidgetKind::CalloutHeader { folded: true, .. }));
        assert!(!folded);
        folds.toggle(2, false);
        assert!(planned(NOTE, &folds, end)[1].collapsed);
    }

    #[test]
    fn edits_move_overrides() {
        let mut folds = Folds::default();
        folds.toggle(10, true);
        folds.toggle_heading(20);
        folds.map(&Edit {
            old: 0..0,
            new_len: 3,
        });
        assert_eq!(folds.folded.get(&13), Some(&false));
        assert!(folds.is_heading_folded(23));
        folds.map(&Edit {
            old: 12..14,
            new_len: 0,
        });
        assert!(folds.folded.is_empty());
        assert!(folds.is_heading_folded(21));
        folds.map(&Edit {
            old: 20..30,
            new_len: 0,
        });
        assert!(folds.is_empty());
    }

    const SECTIONS: &str = "# One\ntext\n## Two\nmore\n# Three\nlast";

    #[test]
    fn a_heading_folds_up_to_the_next_of_its_level() {
        let tree = parse(SECTIONS);
        let sections = heading_sections(&tree);
        let bodies: Vec<Range<usize>> = sections.iter().map(|s| s.body.clone()).collect();
        assert_eq!(bodies, [1..4, 3..4, 5..6]);
        let mut folds = Folds::default();
        assert!(folds.toggle_heading(0));
        let end = SECTIONS.len();
        assert_eq!(collapsed(&planned(SECTIONS, &folds, end)), [1, 2, 3]);
        assert!(!folds.toggle_heading(0));
        assert!(collapsed(&planned(SECTIONS, &folds, end)).is_empty());
    }

    #[test]
    fn a_cursor_in_a_folded_section_shows_it() {
        let mut folds = Folds::default();
        folds.toggle_heading(0);
        assert!(collapsed(&planned(SECTIONS, &folds, 7)).is_empty());
        assert_eq!(collapsed(&planned(SECTIONS, &folds, 2)), [1, 2, 3]);
    }

    #[test]
    fn folds_are_kept_as_heading_lines() {
        let tree = parse(SECTIONS);
        let mut folds = Folds::default();
        folds.fold_all_headings(&tree);
        assert_eq!(folds.folded_heading_lines(&tree), [0, 2, 4]);
        let mut restored = Folds::default();
        restored.restore_heading_lines(&[2, 3], &tree);
        assert_eq!(restored.folded_heading_lines(&tree), [2]);
        folds.unfold_all_headings();
        assert!(folds.is_empty());
    }

    #[test]
    fn only_a_selection_end_in_the_body_opens_a_section() {
        let mut folds = Folds::default();
        folds.toggle_heading(0);
        let next_heading = SECTIONS.find("# Three").unwrap();
        assert_eq!(
            collapsed(&planned(SECTIONS, &folds, next_heading)),
            [1, 2, 3]
        );
        let tree = parse(SECTIONS);
        let everything = 0..SECTIONS.len();
        assert!(!folds.unfold_where_selected(&tree, std::slice::from_ref(&everything)));
        let caret = next_heading..next_heading;
        assert!(!folds.unfold_where_selected(&tree, std::slice::from_ref(&caret)));
        let in_body = 8..8;
        assert!(folds.unfold_where_selected(&tree, std::slice::from_ref(&in_body)));
        assert!(!folds.is_heading_folded(0));
    }

    #[test]
    fn the_notes_end_is_inside_the_last_section() {
        let tree = parse(SECTIONS);
        let mut folds = Folds::default();
        let last = SECTIONS.find("# Three").unwrap();
        folds.toggle_heading(last);
        let end = SECTIONS.len();
        assert!(heading_sections(&tree)[2].hides(end));
        let caret = end..end;
        assert!(folds.unfold_where_selected(&tree, std::slice::from_ref(&caret)));
    }

    #[test]
    fn a_fold_command_finds_the_heading_callout_or_section() {
        let text = "# One\ntext\n\n> [!tip]- Tip\n> body\n\n## Two\nmore\n";
        let tree = parse(text);
        let folds = Folds::default();
        let heading_line = |at: usize| match folds.target_at(&tree, at) {
            Some(FoldTarget::Heading(section)) => Some(section.line),
            _ => None,
        };
        assert_eq!(heading_line(2), Some(0));
        assert_eq!(heading_line(text.find("text").unwrap()), Some(0));
        assert_eq!(heading_line(text.find("more").unwrap()), Some(6));
        let in_callout = folds.target_at(&tree, text.find("body").unwrap());
        assert!(matches!(
            in_callout,
            Some(FoldTarget::Callout { folded: true, .. })
        ));
    }

    #[test]
    fn a_folded_heading_counts_what_it_hides() {
        let mut folds = Folds::default();
        folds.toggle_heading(0);
        let tree = parse(SECTIONS);
        let caret = 2..2;
        let mut lines = planned(SECTIONS, &folds, 2);
        folds.mark_folded_headings(&mut lines, SECTIONS, &tree, std::slice::from_ref(&caret));
        let counts: Vec<usize> = lines[0]
            .widgets
            .iter()
            .filter_map(|widget| match widget.kind {
                WidgetKind::FoldedLines { count } => Some(count),
                _ => None,
            })
            .collect();
        assert_eq!(counts, [3]);
        let mut target = folds.target_at(&tree, 2).unwrap();
        assert!(!folds.toggle_target(&target));
        target = folds.target_at(&tree, 2).unwrap();
        assert!(folds.toggle_target(&target));
    }
}
