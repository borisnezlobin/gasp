//! Deciding whether a node's symbols are revealed.

use std::ops::Range;

use crate::syntax::{MarkupKind, NodeId, NodeKind, SyntaxKind, SyntaxTree};

use super::settings::{RevealMode, RevealScope, RevealSettings};

pub(crate) struct Revealer<'a> {
    pub text: &'a str,
    pub tree: &'a SyntaxTree,
    pub selections: Vec<Range<usize>>,
    pub settings: &'a RevealSettings,
}

/// Markers that repeat per line, so they reveal per line: the `>` of a quote
/// line shows when the cursor is on that line, not anywhere in the quote.
pub(crate) fn is_line_marker(node: &NodeKind, kind: MarkupKind) -> bool {
    match kind {
        MarkupKind::QuoteMarker
        | MarkupKind::ListMarker
        | MarkupKind::TaskMarker
        | MarkupKind::CalloutHeader
        | MarkupKind::ConflictMarker => true,
        MarkupKind::FootnoteMarker => matches!(node, NodeKind::FootnoteDefinition { .. }),
        _ => false,
    }
}

fn is_leaf_block(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Paragraph
            | NodeKind::Heading { .. }
            | NodeKind::CodeBlock(_)
            | NodeKind::MathBlock
            | NodeKind::HtmlBlock(_)
            | NodeKind::Table { .. }
            | NodeKind::TableCell
            | NodeKind::ThematicBreak
            | NodeKind::Frontmatter
            | NodeKind::CommentBlock
            | NodeKind::LinkDefinition { .. }
            | NodeKind::ListItem { .. }
            | NodeKind::CalloutTitle
            | NodeKind::Document
    )
}

impl Revealer<'_> {
    /// Whether `kind` symbols of node `id` are shown. `marker` is the
    /// token's range for per-line markers.
    pub fn revealed(&self, id: NodeId, kind: SyntaxKind, marker: Option<&Range<usize>>) -> bool {
        match self.settings.mode_for(kind) {
            RevealMode::AlwaysShown => true,
            RevealMode::AlwaysHidden => false,
            RevealMode::AroundCursor { scope } => self.touches(&self.region(id, scope, marker)),
        }
    }

    /// Whether table `id` shows its Markdown source rather than its grid:
    /// when tables' symbols are always shown, or while it's edited as
    /// text.
    pub fn table_shows_source(&self, id: NodeId) -> bool {
        let range = &self.tree.node(id).range;
        self.settings.mode_for(SyntaxKind::Table) == RevealMode::AlwaysShown
            || self
                .settings
                .source_table
                .is_some_and(|at| range.start <= at && at <= range.end)
    }

    /// Whether any selection touches `range`, ends included.
    pub fn touches(&self, range: &Range<usize>) -> bool {
        self.selections
            .iter()
            .any(|selection| selection.start <= range.end && selection.end >= range.start)
    }

    fn region(
        &self,
        id: NodeId,
        scope: RevealScope,
        marker: Option<&Range<usize>>,
    ) -> Range<usize> {
        let node = self.tree.node(id);
        match (scope, marker) {
            (RevealScope::Element | RevealScope::Line, Some(marker)) => self.full_lines(marker),
            (RevealScope::Element, None) => node.range.clone(),
            (RevealScope::Line, None) => self.full_lines(&node.range),
            (RevealScope::Block, Some(_)) => self.full_lines(&node.range),
            (RevealScope::Block, None) => self.full_lines(&self.tree.node(self.block_of(id)).range),
        }
    }

    fn block_of(&self, id: NodeId) -> NodeId {
        std::iter::once(id)
            .chain(self.tree.ancestors(id))
            .find(|&candidate| is_leaf_block(&self.tree.node(candidate).kind))
            .unwrap_or(SyntaxTree::ROOT)
    }

    /// `range` widened to whole lines, without the final line terminator.
    pub fn full_lines(&self, range: &Range<usize>) -> Range<usize> {
        let lines = self.tree.lines();
        let first = lines.line_of(range.start);
        let last = lines.line_of(range.end);
        lines.line_start(first)..lines.line_range(self.text, last).end
    }
}
