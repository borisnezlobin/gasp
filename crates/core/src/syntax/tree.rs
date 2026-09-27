//! The syntax tree: an arena of nodes with exact byte ranges.

use std::ops::Range;

use super::kinds::{MarkupKind, NodeKind};

/// Index of a node in a [`SyntaxTree`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

/// A markup token: the symbols that the render planner can hide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Markup {
    pub kind: MarkupKind,
    pub range: Range<usize>,
}

/// One node of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    /// The whole source range, markup included, without trailing newlines.
    pub range: Range<usize>,
    /// Markup tokens in source order.
    pub markup: Vec<Markup>,
    /// The node's range minus its markup, in source order.
    pub content: Vec<Range<usize>>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

impl Node {
    pub(crate) fn new(kind: NodeKind, range: Range<usize>) -> Self {
        Self {
            kind,
            range,
            markup: Vec::new(),
            content: Vec::new(),
            parent: None,
            children: Vec::new(),
        }
    }

    pub(crate) fn add_markup(&mut self, kind: MarkupKind, range: Range<usize>) {
        if !range.is_empty() {
            self.markup.push(Markup { kind, range });
        }
    }
}

/// Byte offsets of line starts, for mapping offsets to lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(memchr_newlines(text).map(|at| at + 1));
        Self {
            starts,
            len: text.len(),
        }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The zero-based line containing `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|&start| start <= offset) - 1
    }

    /// The line's range without its line terminator.
    pub fn line_range(&self, text: &str, line: usize) -> Range<usize> {
        let start = self.starts[line];
        let next = self.starts.get(line + 1).copied().unwrap_or(self.len);
        let mut end = next;
        if end > start && text.as_bytes()[end - 1] == b'\n' {
            end -= 1;
            if end > start && text.as_bytes()[end - 1] == b'\r' {
                end -= 1;
            }
        }
        start..end
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.starts[line]
    }
}

fn memchr_newlines(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.bytes()
        .enumerate()
        .filter(|(_, byte)| *byte == b'\n')
        .map(|(at, _)| at)
}

/// A parsed document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxTree {
    pub(crate) nodes: Vec<Node>,
    pub(crate) lines: LineIndex,
}

impl SyntaxTree {
    pub const ROOT: NodeId = NodeId(0);

    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn lines(&self) -> &LineIndex {
        &self.lines
    }

    /// Top-level block ids, in order.
    pub fn blocks(&self) -> &[NodeId] {
        &self.nodes[0].children
    }

    /// All node ids in document order (parents before children).
    pub fn preorder(&self) -> Vec<NodeId> {
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![Self::ROOT];
        while let Some(id) = stack.pop() {
            order.push(id);
            stack.extend(self.node(id).children.iter().rev());
        }
        order
    }

    /// Ids of the nodes whose range contains `offset`, outermost first.
    /// A node contains the offsets from its start up to and including its end.
    pub fn path_at(&self, offset: usize) -> Vec<NodeId> {
        let mut path = vec![Self::ROOT];
        let mut current = Self::ROOT;
        while let Some(child) = self.child_containing(current, offset) {
            path.push(child);
            current = child;
        }
        path
    }

    fn child_containing(&self, parent: NodeId, offset: usize) -> Option<NodeId> {
        let children = &self.node(parent).children;
        let after = children.partition_point(|&id| self.node(id).range.start <= offset);
        children[..after]
            .iter()
            .rev()
            .take(2)
            .copied()
            .find(|&id| self.node(id).range.end >= offset)
    }

    /// Ancestors of `id`, nearest first, excluding `id` itself.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(self.node(id).parent, |&parent| self.node(parent).parent)
    }

    /// Ids of the nodes overlapping `range`, in document order.
    pub fn nodes_overlapping(&self, range: Range<usize>) -> Vec<NodeId> {
        let mut found = Vec::new();
        self.collect_overlapping(Self::ROOT, &range, &mut found);
        found
    }

    fn collect_overlapping(&self, id: NodeId, range: &Range<usize>, found: &mut Vec<NodeId>) {
        found.push(id);
        let children = &self.node(id).children;
        let first = children.partition_point(|&child| self.node(child).range.end < range.start);
        for &child in &children[first..] {
            if self.node(child).range.start > range.end {
                break;
            }
            self.collect_overlapping(child, range, found);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_maps_offsets_to_lines() {
        let text = "ab\ncd\r\n\nx";
        let lines = LineIndex::new(text);
        assert_eq!(lines.line_count(), 4);
        assert_eq!(lines.line_of(0), 0);
        assert_eq!(lines.line_of(2), 0);
        assert_eq!(lines.line_of(3), 1);
        assert_eq!(lines.line_range(text, 1), 3..5);
        assert_eq!(lines.line_range(text, 2), 7..7);
        assert_eq!(lines.line_range(text, 3), 8..9);
    }
}
