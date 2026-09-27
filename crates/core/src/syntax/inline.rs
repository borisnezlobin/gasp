//! Inline passes over the tree for what CommonMark doesn't know:
//! `==highlight==`, `%%comment%%`, `#tags`, bare URLs, paired HTML elements
//! and HTML tags inside HTML blocks.

use std::ops::Range;

use super::html::{self, HtmlTag};
use super::kinds::{HtmlKind, LinkInfo, LinkKind, MarkupKind, NodeKind};
use super::tree::{Node, NodeId};

pub(crate) struct Arena<'a> {
    pub nodes: &'a mut Vec<Node>,
    pub text: &'a str,
}

impl Arena<'_> {
    fn push(&mut self, kind: NodeKind, range: Range<usize>, parent: NodeId) -> NodeId {
        let id = NodeId(self.nodes.len());
        let mut node = Node::new(kind, range);
        node.parent = Some(parent);
        self.nodes.push(node);
        id
    }

    fn is_text(&self, id: NodeId) -> bool {
        self.nodes[id.0].kind == NodeKind::Text
    }

    fn range(&self, id: NodeId) -> Range<usize> {
        self.nodes[id.0].range.clone()
    }

    /// Merges runs of text children whose ranges touch.
    pub fn merge_texts(&mut self, parent: NodeId) {
        let children = std::mem::take(&mut self.nodes[parent.0].children);
        let mut merged: Vec<NodeId> = Vec::with_capacity(children.len());
        for id in children {
            match merged.last().copied() {
                Some(last)
                    if self.is_text(last)
                        && self.is_text(id)
                        && self.nodes[last.0].range.end == self.nodes[id.0].range.start =>
                {
                    self.nodes[last.0].range.end = self.nodes[id.0].range.end;
                }
                _ => merged.push(id),
            }
        }
        self.nodes[parent.0].children = merged;
    }

    /// Gives an HTML block one child per tag.
    pub fn add_html_block_tags(&mut self, block: NodeId) {
        let range = self.range(block);
        let tags = html::scan_tags(self.text, range);
        let children = tags
            .into_iter()
            .map(|tag| self.push(NodeKind::Html(tag.kind), tag.range, block))
            .collect();
        self.nodes[block.0].children = children;
    }

    /// Pairs opening and closing HTML tags among `parent`'s children into
    /// element nodes, such as `<u>…</u>`.
    pub fn pair_html(&mut self, parent: NodeId) {
        let mut index = 0;
        while index < self.nodes[parent.0].children.len() {
            if let Some(close) = self.matching_close(parent, index) {
                self.wrap_element(parent, index, close);
            }
            index += 1;
        }
    }

    fn tag_of(&self, id: NodeId) -> Option<HtmlTag> {
        match self.nodes[id.0].kind {
            NodeKind::Html(_) if self.nodes[id.0].children.is_empty() => html::tag_at(
                &self.text[..self.nodes[id.0].range.end],
                self.nodes[id.0].range.start,
            ),
            _ => None,
        }
    }

    fn matching_close(&self, parent: NodeId, open_index: usize) -> Option<usize> {
        let children = &self.nodes[parent.0].children;
        let open = self
            .tag_of(children[open_index])
            .filter(HtmlTag::opens_element)?;
        let mut depth = 0usize;
        for (index, &id) in children.iter().enumerate().skip(open_index + 1) {
            let Some(tag) = self.tag_of(id).filter(|tag| tag.name == open.name) else {
                continue;
            };
            match (tag.closing, depth) {
                (true, 0) => return Some(index),
                (true, _) => depth -= 1,
                (false, _) => depth += usize::from(tag.opens_element()),
            }
        }
        None
    }

    fn wrap_element(&mut self, parent: NodeId, open: usize, close: usize) {
        let children = self.nodes[parent.0].children.clone();
        let (open_id, close_id) = (children[open], children[close]);
        let range = self.nodes[open_id.0].range.start..self.nodes[close_id.0].range.end;
        let kind = self.nodes[open_id.0].kind.clone();
        let element = self.push(kind, range, parent);
        let open_range = self.range(open_id);
        let close_range = self.range(close_id);
        let node = &mut self.nodes[element.0];
        node.add_markup(MarkupKind::HtmlTag, open_range);
        node.add_markup(MarkupKind::HtmlTag, close_range);
        node.children = children[open + 1..close].to_vec();
        let mut new_children = children[..open].to_vec();
        new_children.push(element);
        new_children.extend_from_slice(&children[close + 1..]);
        self.nodes[parent.0].children = new_children;
    }

    /// Wraps `delimiter`-delimited spans among `parent`'s text children into
    /// nodes of `kind`.
    pub fn wrap_delimited(&mut self, parent: NodeId, delimiter: Delimiter) {
        let mut from_child = 0;
        while let Some(pair) = self.find_pair(parent, from_child, &delimiter) {
            from_child = self.wrap_pair(parent, &pair, &delimiter);
        }
    }

    fn delimiter_positions(
        &self,
        parent: NodeId,
        from_child: usize,
        delimiter: &Delimiter,
    ) -> Vec<(usize, usize)> {
        let children = &self.nodes[parent.0].children;
        let mut positions = Vec::new();
        for (index, &id) in children.iter().enumerate().skip(from_child) {
            if !self.is_text(id) {
                continue;
            }
            let range = self.range(id);
            let mut at = range.start;
            while let Some(found) = self.text[at..range.end].find(delimiter.token) {
                positions.push((index, at + found));
                at += found + delimiter.token.len();
            }
        }
        positions
    }

    fn find_pair(&self, parent: NodeId, from_child: usize, delimiter: &Delimiter) -> Option<Pair> {
        let positions = self.delimiter_positions(parent, from_child, delimiter);
        let len = delimiter.token.len();
        for (open_index, &(open_child, open)) in positions.iter().enumerate() {
            if !(delimiter.can_open)(self.text, open, len) {
                continue;
            }
            let close = positions[open_index + 1..].iter().find(|&&(_, close)| {
                close >= open + len + delimiter.min_content
                    && (delimiter.can_close)(self.text, close)
            });
            if let Some(&(close_child, close)) = close {
                return Some(Pair {
                    open_child,
                    open,
                    close_child,
                    close,
                });
            }
        }
        None
    }

    /// Wraps one pair and returns the child index to continue scanning from.
    fn wrap_pair(&mut self, parent: NodeId, pair: &Pair, delimiter: &Delimiter) -> usize {
        let len = delimiter.token.len();
        let children = self.nodes[parent.0].children.clone();
        let open_text = self.range(children[pair.open_child]);
        let close_text = self.range(children[pair.close_child]);
        let wrapper = self.push(delimiter.kind.clone(), pair.open..pair.close + len, parent);
        let mut inner = Vec::new();
        if pair.open_child == pair.close_child {
            self.push_text(&mut inner, pair.open + len..pair.close, wrapper);
        } else {
            self.push_text(&mut inner, pair.open + len..open_text.end, wrapper);
            inner.extend_from_slice(&children[pair.open_child + 1..pair.close_child]);
            self.push_text(&mut inner, close_text.start..pair.close, wrapper);
        }
        let node = &mut self.nodes[wrapper.0];
        node.add_markup(delimiter.markup, pair.open..pair.open + len);
        node.add_markup(delimiter.markup, pair.close..pair.close + len);
        node.children = inner;
        let mut new_children = children[..pair.open_child].to_vec();
        self.push_text(&mut new_children, open_text.start..pair.open, parent);
        new_children.push(wrapper);
        let resume = new_children.len();
        self.push_text(&mut new_children, pair.close + len..close_text.end, parent);
        new_children.extend_from_slice(&children[pair.close_child + 1..]);
        self.nodes[parent.0].children = new_children;
        resume
    }

    fn push_text(&mut self, list: &mut Vec<NodeId>, range: Range<usize>, parent: NodeId) {
        if !range.is_empty() {
            let id = self.push(NodeKind::Text, range, parent);
            list.push(id);
        }
    }

    /// Splits text children into tags, bare URLs and empty `$$` math.
    pub fn split_special_text(&mut self, parent: NodeId) {
        let children = std::mem::take(&mut self.nodes[parent.0].children);
        let mut result = Vec::with_capacity(children.len());
        for id in children {
            if !self.is_text(id) {
                result.push(id);
                continue;
            }
            let range = self.range(id);
            let pieces = special_pieces(self.text, range.clone());
            if pieces.is_empty() {
                result.push(id);
                continue;
            }
            self.push_pieces(&mut result, range, pieces, parent);
        }
        self.nodes[parent.0].children = result;
    }

    fn push_pieces(
        &mut self,
        list: &mut Vec<NodeId>,
        range: Range<usize>,
        pieces: Vec<(NodeKind, Range<usize>)>,
        parent: NodeId,
    ) {
        let mut at = range.start;
        for (kind, piece) in pieces {
            self.push_text(list, at..piece.start, parent);
            at = piece.end;
            let id = self.push(kind, piece, parent);
            if matches!(self.nodes[id.0].kind, NodeKind::Math { .. }) {
                super::markup::add_pair(&mut self.nodes[id.0], MarkupKind::MathDelimiter, 1);
            }
            list.push(id);
        }
        self.push_text(list, at..range.end, parent);
    }

    /// Trims text children so they don't overlap the parent's markup, and
    /// drops the ones that end up empty.
    pub fn clip_texts(&mut self, parent: NodeId) {
        let node = &self.nodes[parent.0];
        if node.markup.is_empty() && node.kind != NodeKind::CalloutTitle {
            return;
        }
        let bounds = node.range.clone();
        let markup: Vec<Range<usize>> = node.markup.iter().map(|m| m.range.clone()).collect();
        let children = std::mem::take(&mut self.nodes[parent.0].children);
        let kept = children
            .into_iter()
            .filter(|&id| {
                if !self.is_text(id) {
                    return true;
                }
                let clipped = clip(self.range(id), &bounds, &markup);
                self.nodes[id.0].range = clipped.clone();
                !clipped.is_empty()
            })
            .collect();
        self.nodes[parent.0].children = kept;
    }
}

fn clip(range: Range<usize>, bounds: &Range<usize>, markup: &[Range<usize>]) -> Range<usize> {
    let mut start = range.start.max(bounds.start);
    let mut end = range.end.min(bounds.end);
    for token in markup {
        if token.start <= start && start < token.end {
            start = token.end;
        }
        if token.start < end && end <= token.end {
            end = token.start;
        }
    }
    start..end.max(start)
}

struct Pair {
    open_child: usize,
    open: usize,
    close_child: usize,
    close: usize,
}

/// A delimiter pair such as `==` or `%%`.
pub(crate) struct Delimiter {
    pub token: &'static str,
    pub kind: NodeKind,
    pub markup: MarkupKind,
    pub min_content: usize,
    pub can_open: fn(&str, usize, usize) -> bool,
    pub can_close: fn(&str, usize) -> bool,
}

impl Delimiter {
    pub fn comment() -> Self {
        Self {
            token: "%%",
            kind: NodeKind::Comment,
            markup: MarkupKind::CommentDelimiter,
            min_content: 0,
            can_open: |_, _, _| true,
            can_close: |_, _| true,
        }
    }

    pub fn highlight() -> Self {
        Self {
            token: "==",
            kind: NodeKind::Highlight,
            markup: MarkupKind::HighlightDelimiter,
            min_content: 1,
            can_open: |text, at, len| {
                text[at + len..]
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_whitespace() && c != '=')
            },
            can_close: |text, at| {
                text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| !c.is_whitespace() && c != '=')
            },
        }
    }
}

/// Tags, bare URLs and empty `$$` in a text range, in order.
fn special_pieces(text: &str, range: Range<usize>) -> Vec<(NodeKind, Range<usize>)> {
    let mut pieces = Vec::new();
    let mut at = range.start;
    while at < range.end {
        match special_at(text, at, range.end) {
            Some((kind, piece)) => {
                at = piece.end;
                pieces.push((kind, piece));
            }
            None => at += text[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    pieces
}

fn special_at(text: &str, at: usize, end: usize) -> Option<(NodeKind, Range<usize>)> {
    let byte = text.as_bytes()[at];
    let preceded_by_space = text[..at]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    match byte {
        b'#' if preceded_by_space => tag_at(text, at, end),
        b'h' | b'w' if !preceded_by_word(text, at) => url_at(text, at, end),
        b'$' => empty_math_at(text, at, end),
        _ => None,
    }
}

fn preceded_by_word(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_some_and(char::is_alphanumeric)
}

fn is_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/')
}

fn tag_at(text: &str, at: usize, end: usize) -> Option<(NodeKind, Range<usize>)> {
    let body: usize = text[at + 1..end]
        .chars()
        .take_while(|&ch| is_tag_char(ch))
        .map(char::len_utf8)
        .sum();
    let name = &text[at + 1..at + 1 + body];
    let valid =
        !name.is_empty() && !name.starts_with('/') && !name.chars().all(|c| c.is_ascii_digit());
    valid.then(|| {
        (
            NodeKind::Tag {
                name: name.to_owned(),
            },
            at..at + 1 + body,
        )
    })
}

const URL_PREFIXES: &[&str] = &["https://", "http://", "www."];

fn url_at(text: &str, at: usize, end: usize) -> Option<(NodeKind, Range<usize>)> {
    let rest = &text[at..end];
    let prefix = URL_PREFIXES
        .iter()
        .find(|prefix| rest.starts_with(**prefix))?;
    let raw_len = rest
        .find(|c: char| c.is_whitespace() || c == '<')
        .unwrap_or(rest.len());
    let len = trim_url_end(&rest[..raw_len]);
    if len <= prefix.len() {
        return None;
    }
    let url = &rest[..len];
    let destination = if url.starts_with("www.") {
        format!("http://{url}")
    } else {
        url.to_owned()
    };
    let info = LinkInfo {
        kind: LinkKind::BareUrl,
        destination,
        title: String::new(),
    };
    Some((NodeKind::Link(Box::new(info)), at..at + len))
}

/// Drops trailing punctuation and unbalanced closing parentheses.
fn trim_url_end(url: &str) -> usize {
    let mut end = url.len();
    loop {
        let trimmed =
            url[..end].trim_end_matches(['.', ',', ':', ';', '!', '?', '"', '\'', '*', '_', '~']);
        end = trimmed.len();
        let unbalanced =
            trimmed.ends_with(')') && trimmed.matches(')').count() > trimmed.matches('(').count();
        if !unbalanced {
            return end;
        }
        end -= 1;
    }
}

/// `$$` with nothing between, left over after math parsing, as empty math.
fn empty_math_at(text: &str, at: usize, end: usize) -> Option<(NodeKind, Range<usize>)> {
    let bytes = text.as_bytes();
    let is_pair = text[at..end].starts_with("$$")
        && bytes.get(at + 2) != Some(&b'$')
        && (at == 0 || !matches!(bytes[at - 1], b'$' | b'\\'));
    is_pair.then_some((NodeKind::Math { display: false }, at..at + 2))
}

/// Whether text under this node may hold tags and bare URLs.
pub(crate) fn allows_special_text(kind: &NodeKind) -> bool {
    kind.holds_inlines() && !matches!(kind, NodeKind::Html(HtmlKind::Comment))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(text: &str) -> Vec<(String, &str)> {
        special_pieces(text, 0..text.len())
            .into_iter()
            .map(|(kind, range)| (format!("{kind:?}").chars().take(4).collect(), &text[range]))
            .collect()
    }

    #[test]
    fn tags_need_a_space_before_and_a_non_digit() {
        let found = pieces("#tag a#b #1 #a/b-c, #2024x");
        let texts: Vec<_> = found.iter().map(|(_, t)| *t).collect();
        assert_eq!(texts, vec!["#tag", "#a/b-c", "#2024x"]);
    }

    #[test]
    fn bare_urls_drop_trailing_punctuation() {
        let found = pieces("see https://a.b/c_(d). and (www.x.org)");
        let texts: Vec<_> = found.iter().map(|(_, t)| *t).collect();
        assert_eq!(texts, vec!["https://a.b/c_(d)", "www.x.org"]);
    }

    #[test]
    fn empty_double_dollar_is_math() {
        let found = pieces("a $$ b $$$ c");
        let texts: Vec<_> = found.iter().map(|(_, t)| *t).collect();
        assert_eq!(texts, vec!["$$"]);
    }
}
