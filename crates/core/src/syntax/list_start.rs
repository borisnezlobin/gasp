//! A paragraph whose last line is an empty list item, such as `- ` typed on
//! the line under some text, ends there, and the item starts a list.
//! Markdown keeps an empty item from interrupting a paragraph, so without
//! this the bullet would only appear once the item had text.

use std::ops::Range;

use super::kinds::NodeKind;
use super::tree::{Node, NodeId};

/// An empty list item on a paragraph's last line: the line break before
/// it, the item's range (the marker and one space) and the list it starts.
struct EmptyItem {
    newline: usize,
    range: Range<usize>,
    list: NodeKind,
}

/// Splits every paragraph that ends in an empty list item.
pub(crate) fn split_starting_lists(nodes: &mut Vec<Node>, text: &str) {
    for index in 0..nodes.len() {
        if nodes[index].kind != NodeKind::Paragraph {
            continue;
        }
        if let Some(item) = empty_last_line_item(nodes, text, NodeId(index)) {
            start_list(nodes, text, NodeId(index), item);
        }
    }
}

fn empty_last_line_item(nodes: &[Node], text: &str, paragraph: NodeId) -> Option<EmptyItem> {
    let node = &nodes[paragraph.0];
    let newline = node.range.start + text[node.range.clone()].rfind('\n')?;
    let line_start = newline + 1;
    let line_end = text[line_start..]
        .find(['\n', '\r'])
        .map_or(text.len(), |at| line_start + at);
    let prefix = text[line_start..line_end]
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'>'))
        .count();
    let marker_start = line_start + prefix;
    let column = node.range.start - line_start_of(text, node.range.start);
    if prefix > column + 3 || straddles(nodes, paragraph, newline) {
        return None;
    }
    let (marker_len, list) = empty_marker(&text[marker_start..line_end])?;
    Some(EmptyItem {
        newline,
        range: marker_start..marker_start + marker_len + 1,
        list,
    })
}

fn line_start_of(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

/// Whether a child of `paragraph` other than a line break runs across the
/// break at `newline`, so the paragraph can't end there.
fn straddles(nodes: &[Node], paragraph: NodeId, newline: usize) -> bool {
    nodes[paragraph.0].children.iter().any(|child| {
        let node = &nodes[child.0];
        let is_break = matches!(node.kind, NodeKind::SoftBreak | NodeKind::HardBreak);
        !is_break && node.range.start < newline && node.range.end > newline + 1
    })
}

/// The marker's length and its list's kind when `line` is a list marker
/// followed by whitespace and nothing else. An ordered list must start at
/// 1, as Markdown asks of one interrupting a paragraph.
fn empty_marker(line: &str) -> Option<(usize, NodeKind)> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let marker_len = digits + 1;
    let rest = line.get(marker_len..)?;
    if rest.is_empty() || !rest.bytes().all(|byte| matches!(byte, b' ' | b'\t')) {
        return None;
    }
    let delimiter = line.as_bytes()[digits];
    match digits {
        0 if matches!(delimiter, b'-' | b'*' | b'+') => Some((
            1,
            NodeKind::List {
                ordered: false,
                start: None,
            },
        )),
        1..=9 if matches!(delimiter, b'.' | b')') && line[..digits].parse() == Ok(1) => Some((
            marker_len,
            NodeKind::List {
                ordered: true,
                start: Some(1),
            },
        )),
        _ => None,
    }
}

/// Ends `paragraph` before its last line and puts a list holding one empty
/// item after it.
fn start_list(nodes: &mut Vec<Node>, text: &str, paragraph: NodeId, item: EmptyItem) {
    end_paragraph_before(nodes, text, paragraph, item.newline);
    let parent = nodes[paragraph.0]
        .parent
        .expect("a paragraph sits inside a block");
    let list = push_node(nodes, parent, item.list, item.range.clone());
    let list_item = push_node(
        nodes,
        list,
        NodeKind::ListItem { task: None },
        item.range.clone(),
    );
    nodes[list.0].children.push(list_item);
    let siblings = &mut nodes[parent.0].children;
    let after_paragraph = siblings
        .iter()
        .position(|&sibling| sibling == paragraph)
        .map_or(siblings.len(), |at| at + 1);
    siblings.insert(after_paragraph, list);
    widen_ancestors(nodes, parent, item.range.end);
}

fn end_paragraph_before(nodes: &mut [Node], text: &str, paragraph: NodeId, newline: usize) {
    let start = nodes[paragraph.0].range.start;
    let kept: Vec<NodeId> = nodes[paragraph.0]
        .children
        .iter()
        .copied()
        .filter(|child| nodes[child.0].range.end <= newline)
        .collect();
    nodes[paragraph.0].children = kept;
    nodes[paragraph.0].range = start..start + text[start..newline].trim_end().len();
}

fn push_node(nodes: &mut Vec<Node>, parent: NodeId, kind: NodeKind, range: Range<usize>) -> NodeId {
    let id = NodeId(nodes.len());
    let mut node = Node::new(kind, range);
    node.parent = Some(parent);
    nodes.push(node);
    id
}

/// Ranges were trimmed before the list existed, so the blocks around it
/// may end before the space after its marker.
fn widen_ancestors(nodes: &mut [Node], from: NodeId, end: usize) {
    let mut at = Some(from);
    while let Some(id) = at {
        let range = &mut nodes[id.0].range;
        range.end = range.end.max(end);
        at = nodes[id.0].parent;
    }
}
