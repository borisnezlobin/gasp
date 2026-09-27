//! Turns pulldown-cmark events into arena nodes.

use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{BrokenLink, CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag};

use super::kinds::{Alignment, CodeBlockInfo, LinkInfo, LinkKind, NodeKind};
use super::segments::{self, Segment};
use super::tree::{Node, NodeId};

pub(crate) fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS
}

type LinkDefs = HashMap<String, (String, String)>;

/// Builds the raw arena: the document root, frontmatter, comment blocks and
/// everything pulldown-cmark reports for the Markdown segments.
pub(crate) fn build_nodes(text: &str) -> Vec<Node> {
    let segments = segments::split(text);
    let defs = shared_link_defs(text, &segments);
    let mut builder = Builder {
        nodes: vec![Node::new(NodeKind::Document, 0..text.len())],
        stack: vec![NodeId(0)],
        text,
        overflowed: false,
        skipping: 0,
    };
    for segment in segments {
        builder.add_segment(segment, &defs);
    }
    builder.nodes
}

/// Builds the raw arena for `region` of `text` alone, parsed as Markdown
/// followed by `context` (definitions it may refer to).
/// Returns `None` when the region's last block runs on into the context.
pub(crate) fn build_region(text: &str, region: Range<usize>, context: &str) -> Option<Vec<Node>> {
    let mut builder = Builder {
        nodes: vec![Node::new(NodeKind::Document, region.clone())],
        stack: vec![NodeId(0)],
        text,
        overflowed: false,
        skipping: 0,
    };
    let source = format!("{}{context}", &text[region.clone()]);
    builder.add_source(&source, region, &LinkDefs::new());
    (!builder.overflowed).then_some(builder.nodes)
}

/// Clamps an event's end to `limit` when it only overshoots by whitespace.
fn clamp_end(source: &str, end: usize, limit: usize) -> Option<usize> {
    if end <= limit {
        return Some(end);
    }
    source[limit..end].trim().is_empty().then_some(limit)
}

/// Link reference definitions of every segment, so a reference in one
/// segment resolves against a definition in another. Only needed when a
/// comment block or frontmatter splits the document.
fn shared_link_defs(text: &str, segments: &[Segment]) -> LinkDefs {
    let markdown: Vec<_> = segments
        .iter()
        .filter_map(|segment| match segment {
            Segment::Markdown(range) => Some(range.clone()),
            _ => None,
        })
        .collect();
    if markdown.len() < 2 {
        return LinkDefs::new();
    }
    let mut defs = LinkDefs::new();
    for range in markdown {
        let parser = Parser::new_ext(&text[range], options());
        for (label, def) in parser.reference_definitions().iter() {
            let title = def.title.as_deref().unwrap_or_default().to_owned();
            defs.insert(label.to_lowercase(), (def.dest.to_string(), title));
        }
    }
    defs
}

struct Builder<'a> {
    nodes: Vec<Node>,
    stack: Vec<NodeId>,
    text: &'a str,
    /// Set when a region parse produced a node past the region's end.
    overflowed: bool,
    /// Depth of a dropped subtree being skipped.
    skipping: usize,
}

impl Builder<'_> {
    fn add_segment(&mut self, segment: Segment, defs: &LinkDefs) {
        match segment {
            Segment::Frontmatter(range) => {
                self.push_leaf(NodeKind::Frontmatter, range);
            }
            Segment::Comment(range) => {
                self.push_leaf(NodeKind::CommentBlock, range);
            }
            Segment::Markdown(range) => self.add_markdown(range, defs),
        }
        self.stack.truncate(1);
    }

    fn add_markdown(&mut self, range: Range<usize>, defs: &LinkDefs) {
        let source = &self.text[range.clone()];
        self.add_source(source, range, defs);
    }

    /// Parses `source`, whose first `range.len()` bytes are `range` of the
    /// document. Anything after that is context, such as definitions the
    /// region refers to, and produces no nodes.
    fn add_source(&mut self, source: &str, range: Range<usize>, defs: &LinkDefs) {
        let base = range.start;
        let limit = range.len();
        let callback = |link: BrokenLink<'_>| {
            defs.get(&link.reference.to_lowercase())
                .map(|(dest, title)| (CowStr::from(dest.clone()), CowStr::from(title.clone())))
        };
        let mut events = Parser::new_with_broken_link_callback(source, options(), Some(callback))
            .into_offset_iter();
        for (event, span) in events.by_ref() {
            if span.start >= limit && self.stack.len() == 1 {
                break;
            }
            let Some(end) = clamp_end(source, span.end, limit).filter(|_| span.start <= limit)
            else {
                self.overflowed = true;
                return;
            };
            self.add_event(event, span.start + base..end + base);
        }
        let mut definitions: Vec<_> = events
            .reference_definitions()
            .iter()
            .filter(|(_, def)| def.span.end <= limit)
            .map(|(label, def)| (label.to_owned(), def.span.start + base..def.span.end + base))
            .collect();
        definitions.sort_by_key(|(_, span)| span.start);
        for (label, span) in definitions {
            self.insert_nested(NodeKind::LinkDefinition { label }, span);
        }
    }

    fn add_event(&mut self, event: Event<'_>, range: Range<usize>) {
        if self.skipping > 0 {
            self.skip(&event);
            return;
        }
        let range = if matches!(event, Event::Start(Tag::List(_) | Tag::Item)) {
            skip_leading_blanks(self.text, range)
        } else {
            range
        };
        if let Event::TaskListMarker(checked) = event {
            self.mark_task(checked, range);
            return;
        }
        let misplaced = !self.fits(&range);
        match event {
            Event::Start(_) if misplaced => self.skipping = 1,
            Event::Start(tag) => self.start(tag, range),
            Event::End(_) => {
                self.stack.pop();
            }
            _ if misplaced => {}
            Event::Html(_) => {}
            other => {
                if let Some(kind) = leaf_kind(&other, self.text, &range) {
                    self.push_leaf(kind, range);
                }
            }
        }
    }

    /// Whether a node with `range` fits after the current parent's last
    /// child and inside the parent. pulldown-cmark occasionally reports
    /// events outside their parent for malformed wikilinks; those are
    /// dropped along with their children.
    fn fits(&self, range: &Range<usize>) -> bool {
        let parent = &self.nodes[self.stack.last().expect("the root stays on the stack").0];
        let previous_end = parent
            .children
            .last()
            .map_or(parent.range.start, |child| self.nodes[child.0].range.end);
        previous_end <= range.start && range.end <= parent.range.end
    }

    fn skip(&mut self, event: &Event<'_>) {
        match event {
            Event::Start(_) => self.skipping += 1,
            Event::End(_) => self.skipping -= 1,
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) {
        let kind = tag_kind(tag, &self.text[range.clone()]);
        let id = self.push_leaf(kind, range);
        self.stack.push(id);
    }

    fn push_leaf(&mut self, kind: NodeKind, range: Range<usize>) -> NodeId {
        let id = NodeId(self.nodes.len());
        let parent = *self.stack.last().expect("the root stays on the stack");
        let mut node = Node::new(kind, range);
        node.parent = Some(parent);
        self.nodes.push(node);
        self.nodes[parent.0].children.push(id);
        id
    }

    fn mark_task(&mut self, checked: bool, range: Range<usize>) {
        let item = self
            .stack
            .iter()
            .rev()
            .copied()
            .find(|id| matches!(self.nodes[id.0].kind, NodeKind::ListItem { .. }));
        if let Some(item) = item {
            self.nodes[item.0].kind = NodeKind::ListItem {
                task: Some(checked),
            };
            let end = skip_one_space(self.text, range.end);
            self.nodes[item.0].add_markup(super::kinds::MarkupKind::TaskMarker, range.start..end);
        }
    }

    /// Inserts a node under the deepest existing node that contains it.
    fn insert_nested(&mut self, kind: NodeKind, range: Range<usize>) {
        let mut parent = NodeId(0);
        while let Some(child) = self.nodes[parent.0].children.iter().copied().find(|id| {
            let node = &self.nodes[id.0];
            node.kind.is_block() && node.range.start <= range.start && range.end <= node.range.end
        }) {
            parent = child;
        }
        let id = NodeId(self.nodes.len());
        let mut node = Node::new(kind, range.clone());
        node.parent = Some(parent);
        self.nodes.push(node);
        let siblings = &self.nodes[parent.0].children;
        let at =
            siblings.partition_point(|sibling| self.nodes[sibling.0].range.start < range.start);
        self.nodes[parent.0].children.insert(at, id);
    }
}

pub(crate) fn skip_one_space(text: &str, at: usize) -> usize {
    match text.as_bytes().get(at) {
        Some(b' ' | b'\t') => at + 1,
        _ => at,
    }
}

fn leaf_kind(event: &Event<'_>, text: &str, range: &Range<usize>) -> Option<NodeKind> {
    let kind = match event {
        Event::Text(_) => NodeKind::Text,
        Event::Code(_) => NodeKind::Code,
        Event::InlineMath(_) => NodeKind::Math { display: false },
        Event::DisplayMath(_) => NodeKind::Math { display: true },
        Event::InlineHtml(_) => NodeKind::Html(super::html::classify_tag(&text[range.clone()])),
        Event::FootnoteReference(label) => NodeKind::FootnoteReference {
            label: label.to_string(),
        },
        Event::SoftBreak => NodeKind::SoftBreak,
        Event::HardBreak => NodeKind::HardBreak,
        Event::Rule => NodeKind::ThematicBreak,
        _ => return None,
    };
    Some(kind)
}

/// A list or list item starts at its marker. pulldown-cmark starts a list
/// nested with a tab at the line break before it, so the range moves past
/// any leading whitespace.
fn skip_leading_blanks(text: &str, range: Range<usize>) -> Range<usize> {
    let blank = text[range.clone()]
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .count();
    (range.start + blank).min(range.end)..range.end
}

fn tag_kind(tag: Tag<'_>, source: &str) -> NodeKind {
    block_tag_kind(&tag, source).unwrap_or_else(|| inline_tag_kind(tag, source))
}

fn block_tag_kind(tag: &Tag<'_>, source: &str) -> Option<NodeKind> {
    let kind = match tag {
        Tag::Heading { level, .. } => NodeKind::Heading {
            level: *level as u8,
            setext: false,
        },
        Tag::BlockQuote(_) => NodeKind::BlockQuote,
        Tag::CodeBlock(kind) => NodeKind::CodeBlock(Box::new(code_block_info(kind))),
        Tag::HtmlBlock => NodeKind::HtmlBlock(super::html::classify_block(source)),
        Tag::List(start) => NodeKind::List {
            ordered: start.is_some(),
            start: *start,
        },
        Tag::Item => NodeKind::ListItem { task: None },
        Tag::FootnoteDefinition(label) => NodeKind::FootnoteDefinition {
            label: label.to_string(),
        },
        Tag::Table(alignments) => NodeKind::Table {
            alignments: alignments.iter().map(|a| alignment(*a)).collect(),
        },
        _ => return table_part_kind(tag),
    };
    Some(kind)
}

fn table_part_kind(tag: &Tag<'_>) -> Option<NodeKind> {
    match tag {
        Tag::TableHead => Some(NodeKind::TableHead),
        Tag::TableRow => Some(NodeKind::TableRow),
        Tag::TableCell => Some(NodeKind::TableCell),
        _ => None,
    }
}

fn inline_tag_kind(tag: Tag<'_>, source: &str) -> NodeKind {
    match tag {
        Tag::Emphasis => NodeKind::Emphasis,
        Tag::Strong => NodeKind::Strong,
        Tag::Strikethrough => NodeKind::Strikethrough,
        Tag::Link {
            link_type,
            dest_url,
            title,
            ..
        } => link_node(link_type, dest_url, title, source, false),
        Tag::Image {
            link_type,
            dest_url,
            title,
            ..
        } => link_node(link_type, dest_url, title, source, true),
        _ => NodeKind::Paragraph,
    }
}

fn link_node(
    link_type: LinkType,
    destination: CowStr<'_>,
    title: CowStr<'_>,
    source: &str,
    image: bool,
) -> NodeKind {
    if let LinkType::WikiLink { .. } = link_type {
        let info = Box::new(super::wikilink::parse_info(source, image));
        return if image {
            NodeKind::Embed(info)
        } else {
            NodeKind::WikiLink(info)
        };
    }
    let info = Box::new(LinkInfo {
        kind: link_kind(link_type),
        destination: destination.to_string(),
        title: title.to_string(),
    });
    if image {
        NodeKind::Image(info)
    } else {
        NodeKind::Link(info)
    }
}

fn link_kind(link_type: LinkType) -> LinkKind {
    match link_type {
        LinkType::Reference | LinkType::ReferenceUnknown => LinkKind::Reference,
        LinkType::Collapsed | LinkType::CollapsedUnknown => LinkKind::Collapsed,
        LinkType::Shortcut | LinkType::ShortcutUnknown => LinkKind::Shortcut,
        LinkType::Autolink => LinkKind::Autolink,
        LinkType::Email => LinkKind::Email,
        _ => LinkKind::Inline,
    }
}

fn alignment(value: pulldown_cmark::Alignment) -> Alignment {
    match value {
        pulldown_cmark::Alignment::None => Alignment::None,
        pulldown_cmark::Alignment::Left => Alignment::Left,
        pulldown_cmark::Alignment::Center => Alignment::Center,
        pulldown_cmark::Alignment::Right => Alignment::Right,
    }
}

fn code_block_info(kind: &CodeBlockKind<'_>) -> CodeBlockInfo {
    match kind {
        CodeBlockKind::Indented => CodeBlockInfo::default(),
        CodeBlockKind::Fenced(info) => super::code_info::parse(info),
    }
}
