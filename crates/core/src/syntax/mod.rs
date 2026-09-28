//! Markdown parser for CommonMark, GFM and the Obsidian extensions.
//!
//! [`parse`] builds a [`SyntaxTree`] whose nodes carry exact byte ranges and,
//! separately, the ranges of their markup tokens, so the render planner can
//! hide or reveal the symbols while the content stays in place.

mod build;
mod callout;
mod code_info;
mod context;
mod css;
mod html;
mod incremental;
mod inline;
mod kinds;
mod markup;
mod prefix;
mod segments;
mod tree;
mod wikilink;

#[cfg(test)]
mod tests;

use std::ops::Range;

pub use crate::pipeline::InputContext;
pub use css::{FONT_SCALE_RANGE, FontSize, HtmlStyle, Rgba8, parse_align, parse_color};
pub use html::{attribute as html_attribute, in_unclosed_tag, is_safe_href, straighten_tag_quotes};
pub use incremental::Edit;
pub use kinds::{
    Alignment, CalloutInfo, CalloutKind, CodeBlockInfo, ConflictSide, Fold, HtmlKind, LinkInfo,
    LinkKind, MarkupKind, NodeKind, SyntaxKind, WikiInfo,
};
pub use tree::{LineIndex, Markup, Node, NodeId, SyntaxTree};

use inline::{Arena, Delimiter};

/// Parses a whole document.
pub fn parse(text: &str) -> SyntaxTree {
    let lines = LineIndex::new(text);
    let nodes = process(build::build_nodes(text), text, &lines);
    SyntaxTree {
        nodes,
        lines,
        definitions: Default::default(),
    }
}

/// A document read as plain lines with no structure yet, as a long note
/// shows while its real parse runs in the background. Any edit to it
/// parses the whole document.
pub fn plain(text: &str) -> SyntaxTree {
    SyntaxTree {
        nodes: vec![Node::new(NodeKind::Document, 0..text.len())],
        lines: LineIndex::new(text),
        definitions: Default::default(),
    }
}

/// Runs every pass after pulldown-cmark on a raw arena.
fn process(mut nodes: Vec<Node>, text: &str, lines: &LineIndex) -> Vec<Node> {
    trim_text_newlines(&mut nodes, text);
    trim_block_ranges(&mut nodes, text);
    convert_math_blocks(&mut nodes);
    add_leaf_markup(&mut nodes, text);
    add_table_markup(&mut nodes, text);
    let order = preorder(&nodes);
    prefix::add_container_markup(&mut nodes, &order, text, lines);
    callout::convert_callouts(&mut nodes, text);
    let mut arena = Arena {
        nodes: &mut nodes,
        text,
    };
    inline_pass(&mut arena, SyntaxTree::ROOT, false);
    finish(nodes)
}

fn preorder(nodes: &[Node]) -> Vec<NodeId> {
    let mut order = Vec::with_capacity(nodes.len());
    let mut stack = vec![SyntaxTree::ROOT];
    while let Some(id) = stack.pop() {
        order.push(id);
        stack.extend(nodes[id.0].children.iter().rev());
    }
    order
}

/// Drops line terminators from the end of text nodes, which pulldown-cmark
/// includes for code block lines.
fn trim_text_newlines(nodes: &mut [Node], text: &str) {
    for node in nodes.iter_mut().filter(|node| node.kind == NodeKind::Text) {
        let trimmed = text[node.range.clone()]
            .trim_end_matches(['\n', '\r'])
            .len();
        node.range.end = node.range.start + trimmed;
    }
}

/// Drops trailing newlines and spaces from block ranges, never cutting into
/// a child. Children come after their parents in the arena, so walking it
/// backwards trims children first.
fn trim_block_ranges(nodes: &mut [Node], text: &str) {
    for index in (1..nodes.len()).rev() {
        if !nodes[index].kind.is_block() {
            continue;
        }
        let range = nodes[index].range.clone();
        let trimmed_end = range.start + text[range.clone()].trim_end().len();
        let children_end = nodes[index]
            .children
            .iter()
            .map(|child| nodes[child.0].range.end)
            .max()
            .unwrap_or(range.start);
        nodes[index].range.end = trimmed_end.max(children_end).min(range.end);
    }
}

/// A paragraph holding nothing but `$$…$$` becomes a math block.
fn convert_math_blocks(nodes: &mut [Node]) {
    for index in 0..nodes.len() {
        if nodes[index].kind != NodeKind::Paragraph {
            continue;
        }
        let meaningful: Vec<NodeId> = nodes[index]
            .children
            .iter()
            .copied()
            .filter(|id| nodes[id.0].kind != NodeKind::SoftBreak)
            .collect();
        if let [only] = meaningful[..]
            && nodes[only.0].kind == (NodeKind::Math { display: true })
        {
            let range = nodes[only.0].range.clone();
            nodes[index].kind = NodeKind::MathBlock;
            nodes[index].range = range;
            nodes[index].children.clear();
        }
    }
}

fn add_leaf_markup(nodes: &mut [Node], text: &str) {
    for index in 0..nodes.len() {
        let children_end = nodes[index]
            .children
            .last()
            .map(|child| nodes[child.0].range.end);
        markup::add_markup(&mut nodes[index], text, children_end);
    }
}

fn add_table_markup(nodes: &mut [Node], text: &str) {
    for index in 0..nodes.len() {
        match nodes[index].kind {
            NodeKind::Table { .. } => {
                let head_end = nodes[index]
                    .children
                    .first()
                    .map_or(nodes[index].range.start, |head| nodes[head.0].range.end);
                markup::add_table_delimiter_row(&mut nodes[index], text, head_end);
            }
            NodeKind::TableHead | NodeKind::TableRow => {
                let cells: Vec<Range<usize>> = nodes[index]
                    .children
                    .iter()
                    .map(|cell| nodes[cell.0].range.clone())
                    .collect();
                markup::add_table_pipes(&mut nodes[index], text, &cells);
            }
            _ => {}
        }
    }
}

fn inline_pass(arena: &mut Arena<'_>, id: NodeId, in_link: bool) {
    let kind = arena.nodes[id.0].kind.clone();
    if matches!(kind, NodeKind::HtmlBlock(_)) {
        arena.add_html_block_tags(id);
        arena.pair_html(id);
    }
    arena.merge_texts(id);
    arena.clip_texts(id);
    if matches!(kind, NodeKind::Link(_)) {
        arena.pair_html(id);
    }
    if kind.holds_inlines() {
        arena.split_curly_tags(id);
        arena.pair_html(id);
        arena.wrap_delimited(id, Delimiter::comment());
        arena.wrap_delimited(id, Delimiter::highlight());
        if !in_link && inline::allows_special_text(&kind) {
            arena.split_special_text(id);
        }
    }
    let children = arena.nodes[id.0].children.clone();
    for child in children {
        let child_kind = &arena.nodes[child.0].kind;
        if *child_kind == NodeKind::Comment {
            continue;
        }
        let child_in_link = in_link || child_kind.is_link_like();
        inline_pass(arena, child, child_in_link);
    }
}

/// Drops unreachable nodes, renumbers the rest in document order, fixes
/// parent links, sorts markup and computes content ranges.
fn finish(mut nodes: Vec<Node>) -> Vec<Node> {
    let order = preorder(&nodes);
    let mut new_ids = vec![NodeId(usize::MAX); nodes.len()];
    for (new, old) in order.iter().enumerate() {
        new_ids[old.0] = NodeId(new);
    }
    let mut result: Vec<Node> = order
        .iter()
        .map(|old| std::mem::replace(&mut nodes[old.0], Node::new(NodeKind::Document, 0..0)))
        .collect();
    for node in &mut result {
        finish_node(node, &new_ids);
    }
    for index in 0..result.len() {
        for child in 0..result[index].children.len() {
            let child_id = result[index].children[child];
            result[child_id.0].parent = Some(NodeId(index));
        }
    }
    result[0].parent = None;
    result
}

fn finish_node(node: &mut Node, new_ids: &[NodeId]) {
    node.children
        .iter_mut()
        .for_each(|child| *child = new_ids[child.0]);
    let bounds = node.range.clone();
    node.markup.iter_mut().for_each(|m| {
        m.range = m.range.start.max(bounds.start)..m.range.end.min(bounds.end);
    });
    node.markup.retain(|m| m.range.start < m.range.end);
    node.markup.sort_by_key(|m| (m.range.start, m.range.end));
    node.content = subtract(&node.range, node.markup.iter().map(|m| m.range.clone()));
}

/// `range` minus the sorted `holes`, as non-empty pieces.
fn subtract(range: &Range<usize>, holes: impl Iterator<Item = Range<usize>>) -> Vec<Range<usize>> {
    let mut pieces = Vec::new();
    let mut at = range.start;
    for hole in holes {
        if hole.start > at {
            pieces.push(at..hole.start.min(range.end));
        }
        at = at.max(hole.end);
    }
    if at < range.end {
        pieces.push(at..range.end);
    }
    pieces
}
