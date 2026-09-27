//! Container markers that repeat on every line: `>` quote markers, list item
//! markers and the indentation that continues list items and footnotes.

use super::kinds::{MarkupKind, NodeKind};
use super::markup::list_marker;
use super::tree::{LineIndex, Node, NodeId};

/// Where the container prefixes found so far end on each line.
pub(crate) struct Prefixes<'a> {
    text: &'a str,
    lines: &'a LineIndex,
    ends: Vec<usize>,
}

impl<'a> Prefixes<'a> {
    pub fn new(text: &'a str, lines: &'a LineIndex) -> Self {
        let ends = (0..lines.line_count())
            .map(|l| lines.line_start(l))
            .collect();
        Self { text, lines, ends }
    }

    /// Adds container markup to `node`. Must be called parents first.
    pub fn visit(&mut self, node: &mut Node) {
        match node.kind {
            NodeKind::BlockQuote => self.quote_markers(node),
            NodeKind::ListItem { .. } => {
                let marker = list_marker(self.text, node.range.start, node.range.end);
                self.consume_first_line(node.range.start, marker.end);
                let width = marker.end - marker.start;
                node.add_markup(MarkupKind::ListMarker, marker);
                self.consume_indentation(node, width);
            }
            NodeKind::FootnoteDefinition { .. } => {
                if let Some(label_end) = node.markup.first().map(|m| m.range.end) {
                    self.consume_first_line(node.range.start, label_end);
                }
                self.consume_indentation(node, 4);
            }
            _ => {}
        }
    }

    fn line_span(&self, node: &Node) -> std::ops::Range<usize> {
        self.lines.line_of(node.range.start)..self.lines.line_of(node.range.end) + 1
    }

    fn consume_first_line(&mut self, start: usize, end: usize) {
        let line = self.lines.line_of(start);
        self.ends[line] = self.ends[line].max(end);
    }

    fn quote_markers(&mut self, node: &mut Node) {
        let first = self.lines.line_of(node.range.start);
        for line in self.line_span(node) {
            let from = if line == first {
                node.range.start
            } else {
                self.ends[line]
            };
            if let Some(marker) = self.quote_marker_at(from.max(self.ends[line]), node.range.end) {
                self.ends[line] = marker.end;
                node.add_markup(MarkupKind::QuoteMarker, marker);
            }
        }
    }

    fn quote_marker_at(&self, from: usize, limit: usize) -> Option<std::ops::Range<usize>> {
        let bytes = self.text.as_bytes();
        let spaces = bytes[from..limit]
            .iter()
            .take(3)
            .take_while(|&&b| b == b' ')
            .count();
        let at = from + spaces;
        if bytes.get(at) != Some(&b'>') || at >= limit {
            return None;
        }
        let end = super::build::skip_one_space(self.text, at + 1).min(limit);
        Some(at..end)
    }

    /// Continuation lines of a list item or footnote give up to `width`
    /// leading spaces to the container.
    fn consume_indentation(&mut self, node: &Node, width: usize) {
        let span = self.line_span(node);
        let bytes = self.text.as_bytes();
        for line in span.start + 1..span.end {
            let from = self.ends[line];
            let spaces = bytes[from..node.range.end.max(from)]
                .iter()
                .take(width)
                .take_while(|&&b| b == b' ')
                .count();
            self.ends[line] = from + spaces;
        }
    }
}

/// Runs the prefix pass over the whole tree.
pub(crate) fn add_container_markup(
    nodes: &mut [Node],
    order: &[NodeId],
    text: &str,
    lines: &LineIndex,
) {
    let mut prefixes = Prefixes::new(text, lines);
    for id in order {
        prefixes.visit(&mut nodes[id.0]);
    }
}
