//! What each node contributes to the plan: styles, hidden ranges, widgets,
//! line decorations and collapsed lines.

use std::ops::Range;

use crate::syntax::{
    CalloutKind, ConflictSide, Markup, MarkupKind, Node, NodeId, NodeKind, SyntaxKind,
};

use super::html;
use super::output::{LineStyle, Placement, StyleKey, TableRowPlan, Widget, WidgetKind};
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
    /// Lines drawn as rows of a table's grid.
    pub table_rows: Vec<(usize, TableRowPlan)>,
    /// List and task markers shown as source.
    pub shown_markers: Vec<Range<usize>>,
}

pub(crate) struct Planner<'a> {
    pub revealer: Revealer<'a>,
    pub effects: Effects,
    /// The text of the lines being planned, so a long table plans only
    /// its rows among them.
    pub span: Range<usize>,
}

/// The syntax kind that decides whether a node is replaced by a widget.
fn replaceable_syntax(node: &Node, text: &str) -> Option<SyntaxKind> {
    let kind = match &node.kind {
        NodeKind::CodeBlock(_) if widgets::link_card(text, node).is_some() => SyntaxKind::CodeBlock,
        NodeKind::Math { .. } | NodeKind::MathBlock => SyntaxKind::Math,
        NodeKind::Image(_) => SyntaxKind::Image,
        NodeKind::Embed(info) if widgets::embeds_image(&info.target) => SyntaxKind::Image,
        NodeKind::ThematicBreak => SyntaxKind::ThematicBreak,
        NodeKind::FootnoteReference { .. } => SyntaxKind::Footnote,
        NodeKind::Comment | NodeKind::CommentBlock => SyntaxKind::Comment,
        NodeKind::Html(kind) if node.children.is_empty() && widgets::html_replaceable(*kind) => {
            SyntaxKind::Html
        }
        _ => return None,
    };
    Some(kind)
}

/// A span for each bracket in `source`, which starts at `offset`, styled
/// by its depth. `\\{` and `\\}` count as brackets; a closing bracket takes
/// the colour of the one it closes.
fn bracket_spans(source: &str, offset: usize) -> Vec<(Range<usize>, StyleKey)> {
    let mut spans = Vec::new();
    let mut depth: u8 = 0;
    let mut escaped = false;
    for (at, c) in source.char_indices() {
        let start = offset + if escaped { at - 1 } else { at };
        let end = offset + at + 1;
        match c {
            '(' | '[' | '{' => {
                spans.push((start..end, StyleKey::MathBracket(depth % 3)));
                depth = depth.wrapping_add(1);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                spans.push((start..end, StyleKey::MathBracket(depth % 3)));
            }
            _ => {}
        }
        escaped = c == '\\' && !escaped;
    }
    spans
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
        if matches!(node.kind, NodeKind::Table { .. }) {
            return self.visit_table(id);
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
        if self.revealer.settings.bracket_colours {
            self.bracket_colours(id);
        }
        if self.node(id).kind == NodeKind::Frontmatter {
            self.frontmatter_properties(id);
        }
        for token in &self.node(id).markup {
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
            NodeKind::Html(_) => {
                let mut spans = html::element_spans(self.revealer.tree, id);
                spans.extend(
                    node.markup
                        .iter()
                        .map(|m| (m.range.clone(), StyleKey::Html)),
                );
                spans
            }
            NodeKind::ListItem { task: Some(true) } => widgets::task_text(self.revealer.text, node)
                .map(|range| vec![(range, StyleKey::TaskDone)])
                .unwrap_or_default(),
            kind => node_style(kind)
                .map(|style| vec![(node.range.clone(), style)])
                .unwrap_or_default(),
        };
        self.effects.spans.extend(spans);
    }

    /// Colours the brackets in shown math source by nesting depth.
    fn bracket_colours(&mut self, id: NodeId) {
        let node = self.node(id);
        if !matches!(node.kind, NodeKind::Math { .. } | NodeKind::MathBlock) {
            return;
        }
        let text = self.revealer.text;
        for range in &node.content {
            let spans = bracket_spans(&text[range.clone()], range.start);
            self.effects.spans.extend(spans);
        }
    }

    pub(super) fn add_line_styles(&mut self, id: NodeId) {
        let node = self.node(id);
        let style = match &node.kind {
            NodeKind::Heading { level, .. } => LineStyle::Heading(*level),
            NodeKind::BlockQuote => LineStyle::Quote {
                depth: self.quote_depth(id),
            },
            NodeKind::Callout(info) => return self.callout_line_styles(id, info.kind),
            NodeKind::CodeBlock(_) => return self.code_line_styles(node),
            NodeKind::Conflict => {
                let styles = self.conflict_line_styles(node);
                return self.effects.line_styles.extend(styles);
            }
            NodeKind::Html(_) => match html::alignment(node) {
                Some(align) => LineStyle::Align(align),
                None => return,
            },
            kind => match simple_line_style(kind) {
                Some(style) => style,
                None => return,
            },
        };
        let lines = self.lines_of(&node.range);
        self.effects.line_styles.push((lines, style));
    }

    fn callout_line_styles(&mut self, id: NodeId, kind: CalloutKind) {
        let lines = self.lines_of(&self.node(id).range);
        let depth = self.quote_depth(id);
        let header = lines.start..lines.start + 1;
        self.effects
            .line_styles
            .push((lines, LineStyle::Callout { kind, depth }));
        self.effects
            .line_styles
            .push((header, LineStyle::CalloutHeader { kind }));
    }

    /// Each line of a code block gets its index in the block; only the
    /// lines being planned are recorded.
    fn code_line_styles(&mut self, node: &Node) {
        let lines = self.lines_of(&node.range);
        let planned = self.lines_of(&self.span);
        let shown = lines.start.max(planned.start)..lines.end.min(planned.end);
        let styles = shown.map(|line| {
            let index = line - lines.start;
            (line..line + 1, LineStyle::CodeBlock { index })
        });
        self.effects.line_styles.extend(styles);
    }

    /// This device's lines run from the opening marker to the separator,
    /// the other device's from the separator to the closing marker.
    fn conflict_line_styles(&self, node: &Node) -> Vec<(Range<usize>, LineStyle)> {
        let lines = self.lines_of(&node.range);
        let separator = match &node.markup[..] {
            [_, separator, _] => self.lines_of(&separator.range).start,
            _ => lines.end,
        };
        vec![
            (
                lines.start..separator,
                LineStyle::Conflict {
                    side: ConflictSide::ThisDevice,
                },
            ),
            (
                separator..lines.end,
                LineStyle::Conflict {
                    side: ConflictSide::OtherDevice,
                },
            ),
        ]
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

    /// Property names in the frontmatter are muted. While its symbols are
    /// hidden, each line is a property row: the colon after a name hides
    /// and the app sets values in a column. A block list under a name,
    /// `topics:` then `- optics` lines, joins the name's row as chips.
    fn frontmatter_properties(&mut self, id: NodeId) {
        let node = self.node(id);
        let (text, tree) = (self.revealer.text, self.revealer.tree);
        let revealed = self.revealer.revealed(id, SyntaxKind::Frontmatter, None);
        let lines = self.lines_of(&node.range);
        let body = lines.start + 1..lines.end.saturating_sub(1);
        let mut tags = false;
        let mut joined_until = body.start;
        for line in body.clone() {
            let range = tree.lines().line_range(text, line);
            let key = property_key(&text[range.clone()]);
            if let Some((name, _)) = key {
                tags = TAG_KEYS.contains(&&text[range.start..range.start + name]);
                let name = range.start..range.start + name;
                self.effects.spans.push((name, StyleKey::FrontmatterKey));
            }
            if revealed || line < joined_until {
                continue;
            }
            let value = match key {
                Some((name, value)) => {
                    let colon = range.start + name..range.start + value;
                    self.effects.hidden.push(colon);
                    range.start + value..range.end
                }
                None => range.clone(),
            };
            if key.is_some() && value.is_empty() {
                joined_until = self.join_block_list(line, body.end, tags);
            }
            if joined_until <= line {
                self.property_chips(value, key.is_some(), tags);
            }
            let style = LineStyle::Property {
                keyed: key.is_some(),
            };
            self.effects.line_styles.push((line..line + 1, style));
        }
    }

    /// Draws the block list under the name on `line` as chips on the
    /// name's row, collapsing the item lines. Answers the first line after
    /// the list, or `line` itself when no list follows.
    fn join_block_list(&mut self, line: usize, end: usize, tags: bool) -> usize {
        let (text, tree) = (self.revealer.text, self.revealer.tree);
        let items: Vec<String> = (line + 1..end)
            .map_while(|item| {
                let range = tree.lines().line_range(text, item);
                let chip = block_item_chip(&text[range.clone()])?.pop()?;
                Some(text[range.start + chip.start..range.start + chip.end].to_owned())
            })
            .collect();
        if items.is_empty() {
            return line;
        }
        let after = line + 1 + items.len();
        let at = tree.lines().line_range(text, line).end;
        self.effects.widgets.push(Widget {
            kind: WidgetKind::PropertyList { items, tags },
            range: at..at,
            placement: Placement::Replace,
        });
        self.effects.collapsed.push(line + 1..after);
        after
    }

    /// A list value, `[physics, review]` after a name or `- physics` on a
    /// line of its own, reads as chips: brackets, commas, dashes and
    /// quotes hide, and each item gets a fill. Tags are drawn as tags.
    fn property_chips(&mut self, value: Range<usize>, keyed: bool, tags: bool) {
        let text = &self.revealer.text[value.clone()];
        let chips = match keyed {
            true => flow_list_chips(text),
            false => block_item_chip(text),
        };
        let Some(items) = chips else {
            return;
        };
        let shift = |range: Range<usize>| value.start + range.start..value.start + range.end;
        let gaps = gaps_between(&items, text.len());
        self.effects.hidden.extend(gaps.into_iter().map(shift));
        for item in items {
            self.effects
                .spans
                .push((shift(item.clone()), StyleKey::PropertyChip));
            if tags {
                self.effects.spans.push((shift(item), StyleKey::Tag));
            }
        }
    }

    /// Whether a token shows: a table's pipes while the table shows its
    /// source, other markup as its node's reveal mode says.
    fn markup_shown(&self, id: NodeId, token: &Markup) -> bool {
        let node = self.node(id);
        if matches!(
            token.kind,
            MarkupKind::TablePipe | MarkupKind::TableDelimiterRow
        ) {
            let tree = self.revealer.tree;
            let table = std::iter::once(id)
                .chain(tree.ancestors(id))
                .find(|&a| matches!(tree.node(a).kind, NodeKind::Table { .. }));
            return table.is_some_and(|table| self.revealer.table_shows_source(table));
        }
        let line_marker = is_line_marker(&node.kind, token.kind).then_some(&token.range);
        matches!(node.kind, NodeKind::LinkDefinition { .. })
            || self
                .revealer
                .revealed(id, token.kind.syntax_kind(), line_marker)
    }

    pub(super) fn markup_effect(&mut self, id: NodeId, token: &Markup) {
        if self.markup_shown(id, token) {
            self.effects
                .spans
                .push((token.range.clone(), StyleKey::MarkupDimmed));
            if matches!(token.kind, MarkupKind::ListMarker | MarkupKind::TaskMarker) {
                self.effects.shown_markers.push(token.range.clone());
            }
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

/// Where a top-level YAML key on `line` ends and its value starts: `name:`
/// followed by a space or the line's end. Indented lines, list items and
/// comments have none.
/// Properties whose values are tags.
const TAG_KEYS: [&str; 2] = ["tags", "tag"];

/// The items of a list value, as ranges of it. Everything else hides.
type Chips = Vec<Range<usize>>;

/// The items of a one-line list value, `[a, "b c"]`, when it is one.
fn flow_list_chips(value: &str) -> Option<Chips> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    let mut items = Vec::new();
    let mut at = 1;
    for part in inner.split(',') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            let start = at + part.len() - part.trim_start().len();
            let item = unquoted(value, start..start + trimmed.len());
            if !item.is_empty() {
                items.push(item);
            }
        }
        at += part.len() + 1;
    }
    (!items.is_empty()).then_some(items)
}

/// The item of a block list line, `  - a`, when the line is one.
fn block_item_chip(line: &str) -> Option<Chips> {
    let rest = line.trim_start().strip_prefix("- ")?;
    let item = rest.trim();
    if item.is_empty() || item.starts_with(['[', '{']) {
        return None;
    }
    let start = line.len() - rest.trim_start().len();
    let item = unquoted(line, start..start + item.len());
    (!item.is_empty()).then(|| vec![item])
}

/// `range` without the quotes around it, when it has a matching pair.
fn unquoted(value: &str, range: Range<usize>) -> Range<usize> {
    let bytes = &value.as_bytes()[range.clone()];
    let quoted =
        bytes.len() >= 2 && matches!(bytes[0], b'"' | b'\'') && bytes[0] == bytes[bytes.len() - 1];
    match quoted {
        true => range.start + 1..range.end - 1,
        false => range,
    }
}

/// Everything in `0..len` that isn't one of `items`, which are in order.
fn gaps_between(items: &[Range<usize>], len: usize) -> Vec<Range<usize>> {
    let mut gaps = Vec::new();
    let mut at = 0;
    for item in items {
        if item.start > at {
            gaps.push(at..item.start);
        }
        at = item.end;
    }
    if len > at {
        gaps.push(at..len);
    }
    gaps
}

fn property_key(line: &str) -> Option<(usize, usize)> {
    let first = line.chars().next()?;
    if first.is_whitespace() || matches!(first, '-' | '#' | '[' | '{') {
        return None;
    }
    let colon = line.find(':')?;
    let rest = &line[colon + 1..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let value = colon + 1 + (rest.len() - rest.trim_start().len());
    Some((colon, value))
}

/// `$$` with nothing inside stays literal text, though it counts as math
/// for input.
fn is_empty_math(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Math { .. }) && node.content.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_of_brackets_shares_a_colour() {
        let spans = bracket_spans("\\frac{(a)}{\\{b\\}}", 10);
        let depths: Vec<(usize, u8)> = spans
            .iter()
            .map(|(range, style)| match style {
                StyleKey::MathBracket(depth) => (range.start - 10, *depth),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            depths,
            vec![
                (5, 0),
                (6, 1),
                (8, 1),
                (9, 0),
                (10, 0),
                (11, 1),
                (14, 1),
                (16, 0)
            ]
        );
        assert_eq!(spans[5].0, 21..23, "an escaped brace takes its backslash");
    }
}
