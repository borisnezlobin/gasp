//! Obsidian callouts: a blockquote whose first line is `[!type]±  Title`.

use std::ops::Range;

use super::kinds::{CalloutInfo, CalloutKind, Fold, MarkupKind, NodeKind};
use super::markup::trimmed;
use super::tree::{Node, NodeId};

struct Header {
    info: CalloutInfo,
    /// `[!type]±` and the spaces after it.
    token: Range<usize>,
    /// The rest of the header line, trimmed. May be empty.
    title: Range<usize>,
    line_end: usize,
}

/// Parses a callout header at `start`, the start of the quote's first
/// paragraph.
fn header_at(text: &str, start: usize, limit: usize) -> Option<Header> {
    let line_end = text[start..limit].find('\n').map_or(limit, |at| start + at);
    let line = &text[start..line_end];
    let after_open = line.strip_prefix("[!")?;
    let close = after_open.find(']')?;
    let type_name = &after_open[..close];
    let valid_name = !type_name.is_empty()
        && type_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid_name {
        return None;
    }
    let mut token_end = start + 2 + close + 1;
    let fold = match text.as_bytes().get(token_end) {
        Some(b'+') => Some(Fold::Open),
        Some(b'-') => Some(Fold::Closed),
        _ => None,
    };
    token_end += usize::from(fold.is_some());
    token_end += text[token_end..line_end].len() - text[token_end..line_end].trim_start().len();
    Some(Header {
        info: CalloutInfo {
            kind: CalloutKind::from_name(type_name),
            type_name: type_name.to_owned(),
            fold,
        },
        token: start..token_end,
        title: trimmed(text, token_end..line_end),
        line_end,
    })
}

/// Turns every blockquote with a callout header into a callout.
pub(crate) fn convert_callouts(nodes: &mut Vec<Node>, text: &str) {
    let quotes: Vec<NodeId> = (0..nodes.len())
        .map(NodeId)
        .filter(|id| nodes[id.0].kind == NodeKind::BlockQuote)
        .collect();
    for quote in quotes {
        convert(nodes, text, quote);
    }
}

fn convert(nodes: &mut Vec<Node>, text: &str, quote: NodeId) {
    let Some(&paragraph) = nodes[quote.0].children.first() else {
        return;
    };
    if nodes[paragraph.0].kind != NodeKind::Paragraph {
        return;
    }
    let paragraph_range = nodes[paragraph.0].range.clone();
    let Some(header) = header_at(text, paragraph_range.start, paragraph_range.end) else {
        return;
    };
    let quote_node = &mut nodes[quote.0];
    quote_node.kind = NodeKind::Callout(Box::new(header.info.clone()));
    quote_node.add_markup(MarkupKind::CalloutHeader, header.token.clone());
    let title_children = split_header_line(nodes, paragraph, header.line_end);
    let mut new_children = Vec::new();
    if !header.title.is_empty() {
        let title = NodeId(nodes.len());
        let mut node = Node::new(NodeKind::CalloutTitle, header.title.clone());
        node.parent = Some(quote);
        node.children = title_children;
        nodes.push(node);
        new_children.push(title);
    }
    if !nodes[paragraph.0].children.is_empty() {
        new_children.push(paragraph);
    }
    let rest = nodes[quote.0].children[1..].to_vec();
    new_children.extend(rest);
    nodes[quote.0].children = new_children;
}

/// Removes the children on the header line from the paragraph and returns
/// them, dropping the soft break that ends the line. The paragraph then
/// starts at its next child.
fn split_header_line(nodes: &mut [Node], paragraph: NodeId, line_end: usize) -> Vec<NodeId> {
    let children = std::mem::take(&mut nodes[paragraph.0].children);
    let (header, body): (Vec<NodeId>, Vec<NodeId>) = children
        .into_iter()
        .partition(|id| nodes[id.0].range.start <= line_end);
    if let Some(&first) = body.first() {
        nodes[paragraph.0].range.start = nodes[first.0].range.start;
    }
    nodes[paragraph.0].children = body;
    header
        .into_iter()
        .filter(|id| nodes[id.0].kind != NodeKind::SoftBreak)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_parses_type_fold_and_title() {
        let text = "[!Warning]- Be careful\nbody";
        let header = header_at(text, 0, text.len()).unwrap();
        assert_eq!(header.info.kind, CalloutKind::Warning);
        assert_eq!(header.info.fold, Some(Fold::Closed));
        assert_eq!(header.token, 0..12);
        assert_eq!(&text[header.title], "Be careful");
    }

    #[test]
    fn plain_brackets_are_not_headers() {
        assert!(header_at("[link] x", 0, 8).is_none());
        assert!(header_at("[!] x", 0, 5).is_none());
    }
}
