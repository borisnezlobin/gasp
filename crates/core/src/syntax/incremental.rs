//! Reparsing after an edit, one run of top-level blocks at a time.
//!
//! The edited blocks are reparsed together with one unchanged block on each
//! side. When the unchanged blocks come back exactly as before, the blocks
//! beyond them can't have changed either, so the new nodes are spliced into
//! the old tree. Otherwise, and whenever the edit could change how the rest
//! of the document parses (comment blocks, frontmatter, link or footnote
//! definitions), the whole document is parsed again.

use std::ops::Range;

use super::kinds::NodeKind;
use super::tree::{Node, NodeId, SyntaxTree};

/// A text change: the bytes in `old` were replaced by `new_len` bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub old: Range<usize>,
    pub new_len: usize,
}

impl Edit {
    fn delta(&self) -> isize {
        self.new_len as isize - self.old.len() as isize
    }
}

/// The run of top-level blocks to reparse.
struct Region {
    /// Indices into the root's children.
    first: usize,
    last: usize,
    old: Range<usize>,
    new: Range<usize>,
    edit_end: usize,
    delta: isize,
}

impl SyntaxTree {
    /// Updates the tree for `new_text`, which is the old text with `edit`
    /// applied. Gives the same tree as [`super::parse`] on `new_text`.
    pub fn edit(&mut self, new_text: &str, edit: &Edit) {
        if !self.reparse_table_row(new_text, edit) && !self.splice_reparse(new_text, edit) {
            *self = super::parse(new_text);
        }
    }

    fn splice_reparse(&mut self, new_text: &str, edit: &Edit) -> bool {
        let Some(region) = self.region_for(new_text, edit) else {
            return false;
        };
        // A failed reparse parses the whole document again, lines included,
        // so the lines can move now.
        self.lines.edit(new_text, edit);
        let context = self
            .definitions
            .0
            .get_or_init(|| self.definitions_outside(new_text, &region));
        let Some(raw) = super::build::build_region(new_text, region.new.clone(), context) else {
            return false;
        };
        let nodes = super::process(raw, new_text, &self.lines);
        if !self.boundaries_match(&region, &nodes) {
            return false;
        }
        self.splice(&region, nodes, new_text.len());
        true
    }

    fn block_range(&self, index: usize) -> Range<usize> {
        self.node(self.blocks()[index]).range.clone()
    }

    fn region_for(&self, new_text: &str, edit: &Edit) -> Option<Region> {
        let blocks = self.blocks();
        if blocks.len() < 3 {
            return None;
        }
        let ends_before = blocks.partition_point(|&id| self.node(id).range.end < edit.old.start);
        let starts_before = blocks.partition_point(|&id| self.node(id).range.start <= edit.old.end);
        let low = ends_before
            .min(starts_before.saturating_sub(1))
            .min(blocks.len() - 1);
        let high = starts_before
            .saturating_sub(1)
            .max(ends_before)
            .min(blocks.len() - 1);
        let first = low.saturating_sub(1);
        let last = (high + 1).min(blocks.len() - 1);
        let delta = edit.delta();
        let old_start = if first == 0 {
            0
        } else {
            self.line_start_of(self.block_range(first).start)
        };
        let old_end = if last + 1 == blocks.len() {
            self.root().range.end
        } else {
            self.block_range(last).end
        };
        let covers_edit = old_start <= edit.old.start && edit.old.end <= old_end;
        let new = old_start..(old_end as isize + delta) as usize;
        let region = Region {
            first,
            last,
            old: old_start..old_end,
            new,
            edit_end: edit.old.end,
            delta,
        };
        (covers_edit && self.region_is_safe(new_text, &region)).then_some(region)
    }

    fn line_start_of(&self, offset: usize) -> usize {
        self.lines.line_start(self.lines.line_of(offset))
    }

    /// Whether reparsing the region alone can give the right answer.
    fn region_is_safe(&self, new_text: &str, region: &Region) -> bool {
        let Some(new_source) = new_text.get(region.new.clone()) else {
            return false;
        };
        let risky_text = ["%%", "]:", "<<<<<<<", "=======", ">>>>>>>"]
            .iter()
            .any(|risky| new_source.contains(risky));
        let frontmatter = region.new.start == 0 && new_text.starts_with("---");
        let risky_blocks = self.blocks()[region.first..=region.last]
            .iter()
            .any(|&id| self.has_definition_or_segment(id));
        !(risky_text || frontmatter || risky_blocks)
    }

    fn has_definition_or_segment(&self, id: NodeId) -> bool {
        let node = self.node(id);
        let risky = matches!(
            node.kind,
            NodeKind::Frontmatter
                | NodeKind::CommentBlock
                | NodeKind::Conflict
                | NodeKind::FootnoteDefinition { .. }
                | NodeKind::LinkDefinition { .. }
        );
        risky
            || node
                .children
                .iter()
                .any(|&child| self.has_definition_or_segment(child))
    }

    /// Link and footnote definitions outside the region, as Markdown to
    /// parse after it so references inside the region still resolve. A
    /// region with definitions is never reparsed alone, so these are all
    /// the document's definitions, and they stay so through such edits.
    fn definitions_outside(&self, new_text: &str, region: &Region) -> String {
        let mut context = String::from("\n\n");
        for node in self.definition_candidates() {
            let outside = node.range.end <= region.old.start || node.range.start >= region.old.end;
            if !outside {
                continue;
            }
            match &node.kind {
                NodeKind::FootnoteDefinition { label } => {
                    context.push_str(&format!("[^{label}]: x\n\n"));
                }
                NodeKind::LinkDefinition { .. } => {
                    let range = shift_range(&node.range, region.old.end, region.delta);
                    context.push_str(&new_text[range]);
                    context.push_str("\n\n");
                }
                _ => {}
            }
        }
        context
    }

    /// Nodes that are or may hold link and footnote definitions, in
    /// document order: blocks, descending only into blocks that hold
    /// other blocks, so the inlines of a long note are never visited.
    fn definition_candidates(&self) -> Vec<&Node> {
        self.block_preorder()
            .into_iter()
            .map(|id| self.node(id))
            .collect()
    }

    /// Every block below the root in document order, descending only into
    /// blocks that hold other blocks, so the inlines of a long note are
    /// never visited.
    pub fn block_preorder(&self) -> Vec<NodeId> {
        let mut found = Vec::new();
        let mut stack: Vec<NodeId> = self.blocks().iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let node = self.node(id);
            found.push(id);
            if holds_blocks(&node.kind) {
                let blocks = node.children.iter().rev().copied();
                stack.extend(blocks.filter(|&child| self.node(child).kind.is_block()));
            }
        }
        found
    }

    /// The unchanged blocks at the region's edges must parse as before.
    fn boundaries_match(&self, region: &Region, nodes: &[Node]) -> bool {
        let new_blocks = &nodes[0].children;
        let (Some(&new_first), Some(&new_last)) = (new_blocks.first(), new_blocks.last()) else {
            return false;
        };
        let old_first = self.node(self.blocks()[region.first]);
        let old_last = self.node(self.blocks()[region.last]);
        let expected_last = shift_range(&old_last.range, region.edit_end, region.delta);
        let first_matches = region.first == 0
            || nodes[new_first.0].kind == old_first.kind
                && nodes[new_first.0].range == old_first.range;
        let last_matches = region.last + 1 == self.blocks().len()
            || nodes[new_last.0].kind == old_last.kind && nodes[new_last.0].range == expected_last;
        first_matches && last_matches
    }

    fn splice(&mut self, region: &Region, region_nodes: Vec<Node>, new_len: usize) {
        let blocks = self.blocks();
        let removed_start = blocks[region.first].0;
        let removed_end = blocks
            .get(region.last + 1)
            .map_or(self.nodes.len(), |id| id.0);
        let inserted = region_nodes.len() - 1;
        let id_shift = inserted as isize - (removed_end - removed_start) as isize;
        let new_top: Vec<NodeId> = region_nodes[0]
            .children
            .iter()
            .map(|id| NodeId(id.0 + removed_start - 1))
            .collect();
        let incoming = region_nodes.into_iter().skip(1).map(|mut node| {
            renumber(&mut node, removed_start - 1);
            node
        });
        self.nodes.splice(removed_start..removed_end, incoming);
        for node in &mut self.nodes[removed_start + inserted..] {
            shift_node(node, region.delta, id_shift);
        }
        let root = &mut self.nodes[0];
        for id in &mut root.children[region.last + 1..] {
            id.0 = shift(id.0, id_shift);
        }
        root.children.splice(region.first..=region.last, new_top);
        root.range = 0..new_len;
        root.content.clear();
        root.content.push(0..new_len);
    }
}

/// Whether a node's children can be blocks, and so definitions.
fn holds_blocks(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::BlockQuote
            | NodeKind::Callout(_)
            | NodeKind::List { .. }
            | NodeKind::ListItem { .. }
            | NodeKind::FootnoteDefinition { .. }
            | NodeKind::Conflict
    )
}

fn shift(offset: usize, delta: isize) -> usize {
    (offset as isize + delta) as usize
}

/// Shifts a range that lies wholly before or wholly after `edit_end`.
fn shift_range(range: &Range<usize>, edit_end: usize, delta: isize) -> Range<usize> {
    if range.start >= edit_end {
        shift(range.start, delta)..shift(range.end, delta)
    } else {
        range.clone()
    }
}

/// Moves a node from the region arena, where the region root is id 0, into
/// the tree, where its first node lands at `base + 1`.
fn renumber(node: &mut Node, base: usize) {
    node.children.iter_mut().for_each(|id| id.0 += base);
    node.parent = match node.parent {
        Some(NodeId(0)) | None => Some(SyntaxTree::ROOT),
        Some(parent) => Some(NodeId(parent.0 + base)),
    };
}

fn shift_node(node: &mut Node, delta: isize, id_shift: isize) {
    let move_range =
        |range: &mut Range<usize>| *range = shift(range.start, delta)..shift(range.end, delta);
    move_range(&mut node.range);
    node.markup
        .iter_mut()
        .for_each(|m| move_range(&mut m.range));
    node.content.iter_mut().for_each(move_range);
    if id_shift == 0 {
        return;
    }
    node.children
        .iter_mut()
        .for_each(|id| id.0 = shift(id.0, id_shift));
    if let Some(parent) = node.parent.as_mut().filter(|p| p.0 != 0) {
        parent.0 = shift(parent.0, id_shift);
    }
}
