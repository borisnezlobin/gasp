//! The input context at an offset: what kind of text the cursor is in.

use crate::document::Document;
use crate::pipeline::{ContextProvider, InputContext, MathSpan};

use super::kinds::{HtmlKind, LinkKind, MarkupKind, NodeKind};
use super::tree::{Node, SyntaxTree};

type Check = fn(&Node, usize) -> Option<InputContext>;

const CHECKS: &[Check] = &[
    math_context,
    code_context,
    link_context,
    comment_context,
    html_context,
    frontmatter_context,
    table_context,
];

impl SyntaxTree {
    /// The input context at `offset`. The innermost construct wins, so math
    /// inside a table cell is `Math`. Delimiters count as outside: the
    /// offset just before an opening `$` or after a closing `$` is text.
    pub fn context_at(&self, offset: usize) -> InputContext {
        self.path_at(offset)
            .into_iter()
            .rev()
            .find_map(|id| {
                let node = self.node(id);
                CHECKS.iter().find_map(|check| check(node, offset))
            })
            .unwrap_or(InputContext::Text)
    }
}

/// Answers from the tree, which must have been parsed from `doc`'s current
/// text.
impl ContextProvider for SyntaxTree {
    /// The tree's context, or `Html` inside a tag still being typed,
    /// which the tree can't see until its `>` is there.
    fn context_at(&self, doc: &Document, offset: usize) -> InputContext {
        let context = SyntaxTree::context_at(self, offset);
        if !matches!(context, InputContext::Text | InputContext::Table) {
            return context;
        }
        let line_start = doc.line_start(doc.line_of_offset(offset));
        match super::html::in_unclosed_tag(&doc.slice(line_start..offset)) {
            true => InputContext::Html,
            false => context,
        }
    }

    fn math_at(&self, _doc: &Document, offset: usize) -> Option<MathSpan> {
        SyntaxTree::math_at(self, offset)
    }
}

impl SyntaxTree {
    /// The innermost math strictly around `offset`, as [`math_context`]
    /// counts it.
    pub fn math_at(&self, offset: usize) -> Option<MathSpan> {
        let node = self
            .path_at(offset)
            .into_iter()
            .rev()
            .map(|id| self.node(id))
            .find(|node| math_context(node, offset).is_some())?;
        let delimiters: Vec<_> = node
            .markup
            .iter()
            .filter(|m| m.kind == MarkupKind::MathDelimiter)
            .map(|m| m.range.clone())
            .collect();
        let start = delimiters.first().map_or(node.range.start, |open| open.end);
        let end = match delimiters.as_slice() {
            [_, .., close] if close.start >= start => close.start,
            _ => node.range.end,
        };
        Some(MathSpan {
            outer: node.range.clone(),
            inner: start..end.max(start),
            block: matches!(
                node.kind,
                NodeKind::MathBlock | NodeKind::Math { display: true }
            ),
        })
    }
}

fn strictly_inside(node: &Node, offset: usize) -> bool {
    node.range.start < offset && offset < node.range.end
}

fn in_markup(node: &Node, kind: MarkupKind, offset: usize) -> bool {
    node.markup
        .iter()
        .any(|m| m.kind == kind && m.range.start < offset && offset < m.range.end)
}

fn when(condition: bool, context: InputContext) -> Option<InputContext> {
    condition.then_some(context)
}

fn math_context(node: &Node, offset: usize) -> Option<InputContext> {
    let is_math = matches!(node.kind, NodeKind::Math { .. } | NodeKind::MathBlock);
    when(is_math && strictly_inside(node, offset), InputContext::Math)
}

fn code_context(node: &Node, offset: usize) -> Option<InputContext> {
    match &node.kind {
        NodeKind::Code => when(strictly_inside(node, offset), InputContext::Code),
        NodeKind::CodeBlock(info) if info.fenced => {
            let fences: Vec<_> = node.markup.iter().map(|m| m.range.clone()).collect();
            let after_open = fences.first().is_some_and(|open| offset > open.end);
            let before_close = fences.get(1).is_none_or(|close| offset <= close.start);
            when(after_open && before_close, InputContext::Code)
        }
        NodeKind::CodeBlock(_) => Some(InputContext::Code),
        _ => None,
    }
}

fn link_context(node: &Node, offset: usize) -> Option<InputContext> {
    let whole_link = match &node.kind {
        NodeKind::WikiLink(_) | NodeKind::Embed(_) => true,
        NodeKind::Link(info) => matches!(
            info.kind,
            LinkKind::BareUrl | LinkKind::Autolink | LinkKind::Email
        ),
        _ => false,
    };
    let inside = if whole_link {
        strictly_inside(node, offset)
    } else {
        in_markup(node, MarkupKind::LinkDestination, offset)
    };
    when(inside, InputContext::Link)
}

fn comment_context(node: &Node, offset: usize) -> Option<InputContext> {
    let is_comment = matches!(
        node.kind,
        NodeKind::Comment | NodeKind::CommentBlock | NodeKind::Html(HtmlKind::Comment)
    );
    when(
        is_comment && strictly_inside(node, offset),
        InputContext::Comment,
    )
}

/// Inside a tag, and anywhere in an HTML block other than the text of a
/// `<p>`, `<div>` or `<center>` block, which is prose.
fn html_context(node: &Node, offset: usize) -> Option<InputContext> {
    match node.kind {
        NodeKind::HtmlBlock(kind) if !kind.is_block() => Some(InputContext::Html),
        _ => when(
            in_markup(node, MarkupKind::HtmlTag, offset),
            InputContext::Html,
        ),
    }
}

fn frontmatter_context(node: &Node, offset: usize) -> Option<InputContext> {
    let is_frontmatter = node.kind == NodeKind::Frontmatter;
    when(
        is_frontmatter && strictly_inside(node, offset),
        InputContext::Frontmatter,
    )
}

fn table_context(node: &Node, _offset: usize) -> Option<InputContext> {
    let is_table = matches!(
        node.kind,
        NodeKind::Table { .. } | NodeKind::TableHead | NodeKind::TableRow | NodeKind::TableCell
    );
    when(is_table, InputContext::Table)
}
