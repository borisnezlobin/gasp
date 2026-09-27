//! What each node contributes to the plan: styles, hidden ranges, widgets,
//! line decorations and collapsed lines.

use std::ops::Range;

use crate::syntax::{Markup, MarkupKind, Node, NodeId, NodeKind, SyntaxKind};

use super::output::{LineStyle, Placement, StyleKey, Widget, WidgetKind};
use super::reveal::{Revealer, is_line_marker};
use super::widgets;

#[derive(Default)]
pub(crate) struct Effects {
    pub spans: Vec<(Range<usize>, StyleKey)>,
    pub hidden: Vec<Range<usize>>,
    pub widgets: Vec<Widget>,
    /// Line numbers and the decoration they get.
    pub line_styles: Vec<(Range<usize>, LineStyle)>,
    /// Line numbers that take no space.
    pub collapsed: Vec<Range<usize>>,
}

pub(crate) struct Planner<'a> {
    pub revealer: Revealer<'a>,
    pub effects: Effects,
}

/// The syntax kind that decides whether a node is replaced by a widget.
fn replaceable_syntax(node: &Node, text: &str) -> Option<SyntaxKind> {
    let kind = match &node.kind {
        NodeKind::CodeBlock(_) if widgets::link_card(text, node).is_some() => SyntaxKind::CodeBlock,
        NodeKind::Math { .. } | NodeKind::MathBlock => SyntaxKind::Math,
        NodeKind::Image(_) | NodeKind::Embed(_) => SyntaxKind::Image,
        NodeKind::ThematicBreak => SyntaxKind::ThematicBreak,
        NodeKind::FootnoteReference { .. } => SyntaxKind::Footnote,
        NodeKind::Table { .. } => SyntaxKind::Table,
        NodeKind::Comment | NodeKind::CommentBlock => SyntaxKind::Comment,
        NodeKind::Html(kind) if node.children.is_empty() && widgets::html_replaceable(*kind) => {
            SyntaxKind::Html
        }
        _ => return None,
    };
    Some(kind)
}

fn node_style(kind: &NodeKind) -> Option<StyleKey> {
    let style = match kind {
        NodeKind::Strong => StyleKey::Strong,
        NodeKind::Emphasis => StyleKey::Emphasis,
        NodeKind::Strikethrough => StyleKey::Strikethrough,
        NodeKind::Highlight => StyleKey::Highlight,
        NodeKind::Code => StyleKey::Code,
        NodeKind::Link(_) | NodeKind::WikiLink(_) | NodeKind::Image(_) | NodeKind::Embed(_) => {
            StyleKey::Link
        }
        NodeKind::Tag { .. } => StyleKey::Tag,
        NodeKind::Comment | NodeKind::CommentBlock => StyleKey::Comment,
        NodeKind::FootnoteReference { .. } => StyleKey::FootnoteRef,
        _ => return block_style(kind),
    };
    Some(style)
}

fn block_style(kind: &NodeKind) -> Option<StyleKey> {
    let style = match kind {
        NodeKind::Math { .. } | NodeKind::MathBlock => StyleKey::MathSource,
        NodeKind::Heading { level, .. } => StyleKey::Heading(*level),
        NodeKind::Html(crate::syntax::HtmlKind::Underline) => StyleKey::Underline,
        NodeKind::Frontmatter => StyleKey::Frontmatter,
        NodeKind::CalloutTitle => StyleKey::CalloutTitle,
        _ => return None,
    };
    Some(style)
}

impl<'a> Planner<'a> {
    fn node(&self, id: NodeId) -> &'a Node {
        let tree: &'a crate::syntax::SyntaxTree = self.revealer.tree;
        tree.node(id)
    }

    fn lines_of(&self, range: &Range<usize>) -> Range<usize> {
        let lines = self.revealer.tree.lines();
        lines.line_of(range.start)..lines.line_of(range.end) + 1
    }

    pub fn visit(&mut self, id: NodeId) {
        let node = self.node(id);
        if is_empty_math(node) {
            return;
        }
        let replaceable = replaceable_syntax(node, self.revealer.text);
        let replaced = replaceable.is_some_and(|syntax| !self.revealer.revealed(id, syntax, None));
        // A link card draws its own surface, not a code block's.
        if !(replaced && matches!(node.kind, NodeKind::CodeBlock(_))) {
            self.add_line_styles(id);
        }
        if replaced {
            self.replace(id);
            return;
        }
        if replaceable.is_some() {
            self.revealed_extras(id);
        }
        self.add_node_styles(id);
        let markup = self.node(id).markup.clone();
        for token in &markup {
            self.markup_effect(id, token);
        }
    }

    /// Replaces the node with its widget, or hides it when it has none.
    fn replace(&mut self, id: NodeId) {
        let node = self.node(id);
        let range = node.range.clone();
        let widget = widgets::replacement(&self.revealer, id);
        let has_widget = widget.is_some();
        if let Some(kind) = widget {
            self.effects.widgets.push(Widget {
                kind,
                range: range.clone(),
                placement: Placement::Replace,
            });
        }
        self.effects.hidden.push(range.clone());
        self.collapse_covered(&range, has_widget);
    }

    /// Collapses lines that `range` covers completely, keeping the first
    /// line when a widget is drawn there.
    fn collapse_covered(&mut self, range: &Range<usize>, keep_first: bool) {
        let lines = self.lines_of(range);
        let tree = self.revealer.tree;
        let first_covered = lines.start + usize::from(keep_first);
        let covered = (first_covered..lines.end).filter(|&line| {
            let line_range = tree.lines().line_range(self.revealer.text, line);
            range.start <= line_range.start && line_range.end <= range.end
        });
        for line in covered {
            self.effects.collapsed.push(line..line + 1);
        }
    }

    fn revealed_extras(&mut self, id: NodeId) {
        let node = self.node(id);
        let touched = self.revealer.touches(&node.range);
        let range = node.range.clone();
        let extra = match &node.kind {
            NodeKind::Math { display } if touched && !node.content.is_empty() => {
                let tex = widgets::tex(&self.revealer, id);
                Some((
                    WidgetKind::MathPreview {
                        tex,
                        display: *display,
                    },
                    Placement::Above,
                ))
            }
            NodeKind::MathBlock if touched => {
                let tex = widgets::tex(&self.revealer, id);
                Some((
                    WidgetKind::MathPreview { tex, display: true },
                    Placement::Below,
                ))
            }
            NodeKind::Image(_) | NodeKind::Embed(_) => {
                widgets::replacement(&self.revealer, id).map(|kind| (kind, Placement::Below))
            }
            _ => None,
        };
        if let Some((kind, placement)) = extra {
            self.effects.widgets.push(Widget {
                kind,
                range,
                placement,
            });
        }
    }

    fn add_node_styles(&mut self, id: NodeId) {
        let node = self.node(id);
        let spans: Vec<(Range<usize>, StyleKey)> = match &node.kind {
            NodeKind::CodeBlock(_) => widgets::code_lines(node)
                .map(|range| vec![(range, StyleKey::CodeBlock)])
                .unwrap_or_default(),
            NodeKind::Html(kind) if *kind != crate::syntax::HtmlKind::Underline => node
                .markup
                .iter()
                .map(|m| (m.range.clone(), StyleKey::Html))
                .collect(),
            NodeKind::ListItem { task: Some(true) } => widgets::task_text(self.revealer.text, node)
                .map(|range| vec![(range, StyleKey::TaskDone)])
                .unwrap_or_default(),
            kind => node_style(kind)
                .map(|style| vec![(node.range.clone(), style)])
                .unwrap_or_default(),
        };
        self.effects.spans.extend(spans);
    }

    fn add_line_styles(&mut self, id: NodeId) {
        let node = self.node(id);
        let lines = self.lines_of(&node.range);
        let styles: Vec<(Range<usize>, LineStyle)> = match &node.kind {
            NodeKind::Heading { level, .. } => vec![(lines, LineStyle::Heading(*level))],
            NodeKind::BlockQuote => vec![(
                lines,
                LineStyle::Quote {
                    depth: self.quote_depth(id),
                },
            )],
            NodeKind::Callout(info) => vec![
                (
                    lines.clone(),
                    LineStyle::Callout {
                        kind: info.kind,
                        depth: self.quote_depth(id),
                    },
                ),
                (
                    lines.start..lines.start + 1,
                    LineStyle::CalloutHeader { kind: info.kind },
                ),
            ],
            NodeKind::CodeBlock(_) => lines
                .clone()
                .map(|line| {
                    (
                        line..line + 1,
                        LineStyle::CodeBlock {
                            index: line - lines.start,
                        },
                    )
                })
                .collect(),
            kind => simple_line_style(kind)
                .map(|style| vec![(lines, style)])
                .unwrap_or_default(),
        };
        self.effects.line_styles.extend(styles);
    }

    fn quote_depth(&self, id: NodeId) -> usize {
        let tree = self.revealer.tree;
        1 + tree
            .ancestors(id)
            .filter(|&a| {
                matches!(
                    tree.node(a).kind,
                    NodeKind::BlockQuote | NodeKind::Callout(_)
                )
            })
            .count()
    }

    /// Tables reveal their pipes as a whole; other markup reveals with its node.
    fn reveal_owner(&self, id: NodeId, kind: MarkupKind) -> NodeId {
        if kind != MarkupKind::TablePipe {
            return id;
        }
        let tree = self.revealer.tree;
        tree.ancestors(id)
            .find(|&a| matches!(tree.node(a).kind, NodeKind::Table { .. }))
            .unwrap_or(id)
    }

    fn markup_effect(&mut self, id: NodeId, token: &Markup) {
        let owner = self.reveal_owner(id, token.kind);
        let line_marker = is_line_marker(&self.node(id).kind, token.kind).then_some(&token.range);
        let syntax = token.kind.syntax_kind();
        let always_shown = matches!(self.node(id).kind, NodeKind::LinkDefinition { .. });
        if always_shown || self.revealer.revealed(owner, syntax, line_marker) {
            self.effects
                .spans
                .push((token.range.clone(), StyleKey::MarkupDimmed));
            return;
        }
        self.effects.hidden.push(token.range.clone());
        self.hidden_markup_widget(id, token);
    }

    fn hidden_markup_widget(&mut self, id: NodeId, token: &Markup) {
        match token.kind {
            MarkupKind::CodeFence => self.hidden_fence(id, token),
            MarkupKind::CalloutHeader => self.hidden_callout_header(id, token),
            _ => {
                if let Some(kind) = widgets::marker_widget(&self.revealer, id, token) {
                    self.push_replace(kind, token.range.clone());
                }
            }
        }
    }

    fn push_replace(&mut self, kind: WidgetKind, range: Range<usize>) {
        self.effects.widgets.push(Widget {
            kind,
            range,
            placement: Placement::Replace,
        });
    }

    fn hidden_fence(&mut self, id: NodeId, token: &Markup) {
        let node = self.node(id);
        if node.markup.first() == Some(token) {
            let kind = widgets::code_block(node);
            self.push_replace(kind, token.range.clone());
        }
    }

    fn hidden_callout_header(&mut self, id: NodeId, token: &Markup) {
        let node = self.node(id);
        let NodeKind::Callout(info) = &node.kind else {
            return;
        };
        let folded =
            info.fold == Some(crate::syntax::Fold::Closed) && !self.revealer.touches(&node.range);
        let title = node
            .children
            .first()
            .map(|&child| self.node(child))
            .filter(|child| child.kind == NodeKind::CalloutTitle)
            .map(|child| child.range.clone());
        let kind = WidgetKind::CalloutHeader {
            kind: info.kind,
            type_name: info.type_name.clone(),
            title,
            default_title: info.default_title(),
            fold: info.fold,
            folded,
        };
        let body_lines = self.lines_of(&node.range);
        self.push_replace(kind, token.range.clone());
        if folded && body_lines.len() > 1 {
            self.effects
                .collapsed
                .push(body_lines.start + 1..body_lines.end);
        }
    }
}

fn simple_line_style(kind: &NodeKind) -> Option<LineStyle> {
    let style = match kind {
        NodeKind::MathBlock => LineStyle::MathBlock,
        NodeKind::Table { .. } => LineStyle::Table,
        NodeKind::Frontmatter => LineStyle::Frontmatter,
        NodeKind::CommentBlock => LineStyle::Comment,
        NodeKind::FootnoteDefinition { .. } => LineStyle::FootnoteDefinition,
        _ => return None,
    };
    Some(style)
}

/// `$$` with nothing inside stays literal text, though it counts as math
/// for input.
fn is_empty_math(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Math { .. }) && node.content.is_empty()
}
