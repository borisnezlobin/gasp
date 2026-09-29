//! Folding headings and callouts on the phone. The note keeps its folds
//! and moves them along with every edit; the plan leaves folded lines out.

use editor_core::render::folds::{HeadingSection, heading_sections};
use editor_core::render::{RenderInput, RevealSettings, WidgetKind, plan_lines};
use editor_core::syntax::{MarkupKind, NodeKind};

use crate::document::{NoteDocument, ParsedText};
use crate::offsets::TextRange;

/// A heading with lines under it to fold.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HeadingFold {
    /// The heading's line, zero-based.
    pub line: u32,
    /// The heading's whole line.
    pub range: TextRange,
    pub level: u8,
    pub folded: bool,
}

#[uniffi::export]
impl NoteDocument {
    /// Every heading with something under it, and whether it's folded.
    pub fn heading_folds(&self) -> Vec<HeadingFold> {
        let parsed = self.lock();
        heading_sections(&parsed.tree)
            .into_iter()
            .filter(|section| !section.body.is_empty())
            .map(|section| heading_fold(&parsed, &section))
            .collect()
    }

    /// Folds or unfolds the heading on the line holding `offset`, or the
    /// foldable callout whose header is there. Answers whether anything
    /// changed.
    pub fn toggle_fold(&self, offset: u32) -> bool {
        let mut parsed = self.lock();
        let at = parsed.offsets.byte(offset);
        let line = parsed.tree.lines().line_of(at);
        if let Some(heading) = foldable_heading(&parsed, line) {
            parsed.folds.toggle_heading(heading.line_start);
            return true;
        }
        match callout_header(&parsed, at, line) {
            Some((header, folded_now)) => {
                parsed.folds.toggle(header, folded_now);
                true
            }
            None => false,
        }
    }

    /// Whether [`NoteDocument::toggle_fold`] would fold something at
    /// `offset`.
    pub fn can_fold(&self, offset: u32) -> bool {
        let parsed = self.lock();
        let at = parsed.offsets.byte(offset);
        let line = parsed.tree.lines().line_of(at);
        foldable_heading(&parsed, line).is_some() || callout_header(&parsed, at, line).is_some()
    }

    pub fn fold_all_headings(&self) {
        let mut parsed = self.lock();
        let ParsedText { tree, folds, .. } = &mut *parsed;
        folds.fold_all_headings(tree);
    }

    pub fn unfold_all_headings(&self) {
        self.lock().folds.unfold_all_headings();
    }

    /// The lines of the folded headings, to keep on this device.
    pub fn folded_heading_lines(&self) -> Vec<u32> {
        let parsed = self.lock();
        parsed
            .folds
            .folded_heading_lines(&parsed.tree)
            .into_iter()
            .map(|line| line as u32)
            .collect()
    }

    /// Folds the headings on `lines` again, as kept from an earlier launch.
    pub fn restore_folded_headings(&self, lines: Vec<u32>) {
        let lines: Vec<usize> = lines.into_iter().map(|line| line as usize).collect();
        let mut parsed = self.lock();
        let ParsedText { tree, folds, .. } = &mut *parsed;
        folds.restore_heading_lines(&lines, tree);
    }
}

fn heading_fold(parsed: &ParsedText, section: &HeadingSection) -> HeadingFold {
    let range = parsed.tree.lines().line_range(&parsed.text, section.line);
    HeadingFold {
        line: section.line as u32,
        range: parsed.offsets.range(&range),
        level: section.level,
        folded: parsed.folds.is_heading_folded(section.line_start),
    }
}

fn foldable_heading(parsed: &ParsedText, line: usize) -> Option<HeadingSection> {
    heading_sections(&parsed.tree)
        .into_iter()
        .find(|section| section.line == line && !section.body.is_empty())
}

/// Where the header token of the callout whose header is on `line` starts,
/// and whether it's folded now, when its type says it folds (`[!note]-`
/// or `[!note]+`).
fn callout_header(parsed: &ParsedText, at: usize, line: usize) -> Option<(usize, bool)> {
    let header = parsed.tree.path_at(at).into_iter().find_map(|id| {
        let node = parsed.tree.node(id);
        let is_callout = matches!(node.kind, NodeKind::Callout(_));
        let on_header = parsed.tree.lines().line_of(node.range.start) == line;
        node.markup
            .iter()
            .find(|markup| markup.kind == MarkupKind::CalloutHeader)
            .filter(|_| is_callout && on_header)
            .map(|markup| markup.range.start)
    });
    let header = header?;
    Some((header, callout_folded(parsed, line, header)?))
}

/// Whether the foldable callout with its header token at `header` is
/// folded now; `None` when it doesn't fold.
fn callout_folded(parsed: &ParsedText, line: usize, header: usize) -> Option<bool> {
    let settings = RevealSettings::default();
    let mut plan = plan_lines(
        &RenderInput {
            text: &parsed.text,
            tree: &parsed.tree,
            selections: &[],
            settings: &settings,
        },
        line..line + 1,
    );
    parsed.folds.apply(&mut plan.lines, &parsed.tree, &[]);
    plan.lines
        .iter()
        .flat_map(|planned| &planned.widgets)
        .find_map(|widget| match &widget.kind {
            WidgetKind::CalloutHeader {
                folded,
                fold: Some(_),
                ..
            } if widget.range.start == header => Some(*folded),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "# One\ntext\n## Two\nmore\n\n> [!note]- Tip\n> hidden\n";

    fn collapsed(document: &NoteDocument) -> Vec<u32> {
        let end = document.text().len() as u32;
        document
            .plan(TextRange { start: end, end })
            .lines
            .into_iter()
            .filter(|line| line.collapsed)
            .map(|line| line.line)
            .collect()
    }

    #[test]
    fn a_tap_on_a_heading_folds_its_section() {
        let document = NoteDocument::new(NOTE.into());
        assert_eq!(collapsed(&document), [6]);
        assert!(document.toggle_fold(2));
        assert!(document.heading_folds()[0].folded);
        assert!(collapsed(&document).contains(&1));
        assert!(!document.toggle_fold(8), "plain text doesn't fold");
    }

    #[test]
    fn a_tap_on_a_callout_header_opens_it() {
        let document = NoteDocument::new(NOTE.into());
        let header = NOTE.find("[!note]").unwrap() as u32;
        assert!(document.can_fold(header));
        assert!(document.toggle_fold(header));
        assert!(!collapsed(&document).contains(&6));
        assert!(!document.can_fold(8));
    }

    #[test]
    fn folds_follow_edits_and_survive_a_relaunch() {
        let document = NoteDocument::new(NOTE.into());
        document.toggle_fold(13);
        document.update(format!("intro\n{NOTE}"));
        assert_eq!(document.folded_heading_lines(), [3]);
        let reopened = NoteDocument::new(document.text());
        reopened.restore_folded_headings(vec![3]);
        assert!(reopened.heading_folds()[1].folded);
        reopened.unfold_all_headings();
        reopened.fold_all_headings();
        assert_eq!(reopened.folded_heading_lines(), [1, 3]);
    }
}
