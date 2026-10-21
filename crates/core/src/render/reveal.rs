//! Deciding whether a node's symbols are revealed.

use std::ops::Range;

use crate::syntax::{Markup, MarkupKind, Node, NodeId, NodeKind, SyntaxKind, SyntaxTree};

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

/// Markers drawn as something else (a bullet, number, checkbox or callout
/// header). In element scope they show their source only while a selection
/// is inside them, so the drawing appears as soon as the marker is typed.
pub(crate) fn is_drawn_marker(kind: MarkupKind) -> bool {
    matches!(
        kind,
        MarkupKind::ListMarker | MarkupKind::TaskMarker | MarkupKind::CalloutHeader
    )
}

/// The drawn marker `token` shows and hides with: itself, or for the `>`
/// just before a callout's header, the header.
pub(crate) fn drawn_marker_of<'a>(
    text: &str,
    node: &'a Node,
    token: &'a Markup,
) -> Option<&'a Markup> {
    if is_drawn_marker(token.kind) {
        return Some(token);
    }
    if token.kind != MarkupKind::QuoteMarker {
        return None;
    }
    node.markup.iter().find(|markup| {
        markup.kind == MarkupKind::CalloutHeader
            && token.range.end <= markup.range.start
            && text[token.range.end..markup.range.start]
                .bytes()
                .all(|byte| matches!(byte, b' ' | b'\t'))
    })
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

    /// Whether drawn marker `token` of node `id` shows its source. In
    /// element scope that's while a selection is inside the node's drawn
    /// markers (a task's `- [ ] ` counts as one), or while the caret sits
    /// right after a list marker that has no space after it yet, such as
    /// a `-` that may still become `--`.
    pub fn drawn_marker_revealed(&self, id: NodeId, token: &Markup) -> bool {
        let kind = token.kind.syntax_kind();
        let element = RevealMode::AroundCursor {
            scope: RevealScope::Element,
        };
        if self.settings.mode_for(kind) != element {
            return self.revealed(id, kind, Some(&token.range));
        }
        let span = self.drawn_marker_span(id, token);
        let being_typed = span == token.range
            && token.kind == MarkupKind::ListMarker
            && self.is_bare_marker(&span)
            && self.caret_at(span.end);
        being_typed || self.enters(&span)
    }

    /// The node's drawn markers and the `>` before a callout header, as
    /// one range.
    fn drawn_marker_span(&self, id: NodeId, token: &Markup) -> Range<usize> {
        let node = self.tree.node(id);
        node.markup
            .iter()
            .filter(|markup| drawn_marker_of(self.text, node, markup).is_some())
            .fold(token.range.clone(), |span, markup| {
                span.start.min(markup.range.start)..span.end.max(markup.range.end)
            })
    }

    /// Whether `marker` ends its line without a space after it.
    fn is_bare_marker(&self, marker: &Range<usize>) -> bool {
        let ends_in_space = self.text[..marker.end].ends_with([' ', '\t']);
        let ends_line = self.text[marker.end..]
            .chars()
            .next()
            .is_none_or(|next| matches!(next, '\n' | '\r'));
        !ends_in_space && ends_line
    }

    fn caret_at(&self, at: usize) -> bool {
        self.selections
            .iter()
            .any(|selection| selection.is_empty() && selection.start == at)
    }

    /// Whether a selection is inside `range`: a caret before one of its
    /// characters, or a selection overlapping them. A caret at its end is
    /// past it.
    pub fn enters(&self, range: &Range<usize>) -> bool {
        self.selections.iter().any(|selection| {
            let caret_inside = selection.is_empty() && range.contains(&selection.start);
            caret_inside || (selection.start < range.end && selection.end > range.start)
        })
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
