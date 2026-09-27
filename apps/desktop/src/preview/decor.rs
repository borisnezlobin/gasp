//! Insets, padding and the fills and bars drawn behind a line: quote bars,
//! callout tints, code block and frontmatter surfaces, and code line
//! numbers.

use editor_core::render::{LinePlan, LineStyle};
use editor_core::syntax::{ConflictSide, Node, NodeKind, SyntaxTree};
use gpui::{Pixels, px};

use crate::line_layout::{Bar, LineDecor, Surface};
use crate::preview::source::Source;
use crate::theme::Theme;

/// Where a line's content sits within the column, and what is behind it.
#[derive(Clone, Debug, Default)]
pub struct LineFrame {
    /// Content inset from the column's left edge.
    pub left: Pixels,
    /// Content inset from the column's right edge.
    pub right: Pixels,
    pub pad_top: Pixels,
    pub pad_bottom: Pixels,
    pub decor: LineDecor,
    /// The number shown in a code block's gutter.
    pub line_number: Option<usize>,
}

struct FrameBuilder<'a> {
    plan: &'a LinePlan,
    source: &'a Source,
    theme: &'a Theme,
    column: Pixels,
    /// Whether code blocks number their lines unless a block says.
    line_numbers: bool,
    frame: LineFrame,
}

/// The frame for a planned line. Code blocks number their lines when
/// `line_numbers` is set, unless a block's `ln:` says otherwise.
pub fn line_frame(
    plan: &LinePlan,
    source: &Source,
    theme: &Theme,
    column: Pixels,
    line_numbers: bool,
) -> LineFrame {
    let mut builder = FrameBuilder {
        plan,
        source,
        theme,
        column,
        line_numbers,
        frame: LineFrame::default(),
    };
    for style in &plan.line_styles {
        builder.quote_style(style);
    }
    for style in &plan.line_styles {
        builder.block_style(style);
    }
    builder.frame
}

impl<'a> FrameBuilder<'a> {
    /// Nodes containing the line's start, outermost first, that `want` accepts.
    fn enclosing(&self, want: impl Fn(&NodeKind) -> bool) -> Vec<&'a Node> {
        let tree: &'a SyntaxTree = self.source.tree();
        tree.path_at(self.plan.range.start)
            .into_iter()
            .map(|id| tree.node(id))
            .filter(|node| want(&node.kind))
            .collect()
    }

    fn is_first_line(&self, node: &Node) -> bool {
        self.source.line_of(node.range.start) == self.plan.line
    }

    fn is_last_line(&self, node: &Node) -> bool {
        self.source.line_of(node.range.end) == self.plan.line
    }

    fn pad_edges(&mut self, node: &Node, padding: Pixels) {
        if self.is_first_line(node) {
            self.frame.pad_top = self.frame.pad_top.max(padding);
        }
        if self.is_last_line(node) {
            self.frame.pad_bottom = self.frame.pad_bottom.max(padding);
        }
    }

    fn surface(&mut self, group: usize, left: Pixels, color: gpui::Hsla) {
        self.frame.decor.surfaces.push(Surface {
            group,
            left,
            width: (self.column - left).max(px(0.)),
            color,
        });
    }

    fn quote_style(&mut self, style: &LineStyle) {
        let theme = self.theme;
        match *style {
            LineStyle::Quote { depth } => {
                self.frame.decor.bars.push(Bar {
                    x: theme.quote_indent * (depth - 1) as f32,
                    width: theme.quote_bar_width,
                    color: theme.divider,
                });
                self.frame.left = self.frame.left.max(theme.quote_indent * depth as f32);
            }
            LineStyle::Callout { kind, depth } => self.callout(kind, depth),
            LineStyle::Conflict { side } => self.conflict(side),
            _ => {}
        }
    }

    /// Each version of a sync conflict is a tinted block of its own, inset
    /// like a callout.
    fn conflict(&mut self, side: ConflictSide) {
        let theme = self.theme;
        let conflicts = self.enclosing(|kind| matches!(kind, NodeKind::Conflict));
        let Some(node) = conflicts.last().copied() else {
            return;
        };
        let group = node.range.start * 2 + side as usize;
        self.surface(group, px(0.), theme.conflict_surface(side));
        self.frame.left = self.frame.left.max(theme.space_lg);
        self.frame.right = self.frame.right.max(theme.space_lg);
        let lines = self.side_lines(node, side);
        if lines.start == self.plan.line {
            self.frame.pad_top = self.frame.pad_top.max(theme.space_sm);
        }
        // The closing marker's line collapses while the cursor is away,
        // so the line above it pads the block's bottom too.
        let closing = side == ConflictSide::OtherDevice;
        let last = lines.end - 1 - usize::from(closing && lines.len() > 1);
        if self.plan.line >= last {
            self.frame.pad_bottom = self.frame.pad_bottom.max(theme.space_sm);
        }
    }

    /// The lines of one version, marker lines included.
    fn side_lines(&self, node: &Node, side: ConflictSide) -> std::ops::Range<usize> {
        let first = self.source.line_of(node.range.start);
        let last = self.source.line_of(node.range.end) + 1;
        let separator = match &node.markup[..] {
            [_, separator, _] => self.source.line_of(separator.range.start),
            _ => last,
        };
        match side {
            ConflictSide::ThisDevice => first..separator,
            ConflictSide::OtherDevice => separator..last,
        }
    }

    fn callout(&mut self, kind: editor_core::syntax::CalloutKind, depth: usize) {
        let theme = self.theme;
        let quotes =
            self.enclosing(|kind| matches!(kind, NodeKind::BlockQuote | NodeKind::Callout(_)));
        let Some(node) = quotes.get(depth - 1).copied() else {
            return;
        };
        let left = theme.quote_indent * (depth - 1) as f32;
        let group = node.range.start;
        self.surface(group, left, theme.callout_surface(kind));
        self.frame.left = self.frame.left.max(left + theme.space_lg);
        self.frame.right = self.frame.right.max(theme.space_lg);
        self.pad_edges(node, theme.space_sm);
    }

    fn block_style(&mut self, style: &LineStyle) {
        match *style {
            LineStyle::CodeBlock { index } => self.code_line(index),
            LineStyle::Frontmatter => self.frontmatter(),
            LineStyle::Heading(level) => self.heading(level),
            _ => {}
        }
    }

    /// Room above a heading's first line, so it sits nearer the text it
    /// introduces than the text before it.
    fn heading(&mut self, level: u8) {
        let theme = self.theme;
        let headings = self.enclosing(|kind| matches!(kind, NodeKind::Heading { .. }));
        if headings.last().is_some_and(|node| self.is_first_line(node)) {
            let space = theme.font_size(level) * theme.heading_space_above;
            self.frame.pad_top = self.frame.pad_top.max(space);
        }
    }

    fn code_line(&mut self, index: usize) {
        let theme = self.theme;
        let blocks = self.enclosing(|kind| matches!(kind, NodeKind::CodeBlock(_)));
        let Some(node) = blocks.last().copied() else {
            return;
        };
        let NodeKind::CodeBlock(info) = &node.kind else {
            return;
        };
        let left = self.frame.left;
        self.surface(node.range.start, left, theme.code_background);
        let numbered = info.line_numbers.unwrap_or(self.line_numbers);
        let closing_fence = node.markup.len() > 1 && self.is_last_line(node);
        let code_line = index > 0 && !closing_fence;
        if numbered && code_line {
            self.frame.line_number = Some(index);
        }
        if code_line && is_highlighted(&info.highlighted_lines, index) {
            self.frame.decor.bands.push(Surface {
                group: node.range.start,
                left,
                width: (self.column - left).max(px(0.)),
                color: theme.code_highlight,
            });
        }
        let gutter = if numbered {
            theme.body_font_size * 2.
        } else {
            px(0.)
        };
        self.frame.left = left + theme.space_lg + gutter;
        self.frame.right = self.frame.right.max(theme.space_lg);
        self.pad_edges(node, theme.space_sm);
    }

    fn frontmatter(&mut self) {
        let theme = self.theme;
        let blocks = self.enclosing(|kind| matches!(kind, NodeKind::Frontmatter));
        let Some(node) = blocks.last().copied() else {
            return;
        };
        let left = self.frame.left;
        self.surface(node.range.start, left, theme.surface);
        self.frame.left = left + theme.space_md;
        self.frame.right = self.frame.right.max(theme.space_md);
        self.pad_edges(node, theme.space_sm);
    }
}

/// Whether code line `index` (from 1) is in one of the fence's ranges.
fn is_highlighted(ranges: &[(u32, u32)], index: usize) -> bool {
    ranges
        .iter()
        .any(|&(first, last)| (first as usize..=last as usize).contains(&index))
}

#[cfg(test)]
mod tests {
    use editor_core::render::{RenderInput, RevealSettings, plan};

    use super::*;

    fn frames(text: &str) -> Vec<LineFrame> {
        let source = Source::new(text);
        let settings = RevealSettings::default();
        let cursor = text.len();
        let plan = plan(&RenderInput {
            text,
            tree: source.tree(),
            selections: std::slice::from_ref(&(cursor..cursor)),
            settings: &settings,
        });
        let theme = Theme::default();
        plan.lines
            .iter()
            .map(|line| line_frame(line, &source, &theme, px(600.), false))
            .collect()
    }

    #[test]
    fn quotes_get_bars_and_insets() {
        let frames = frames("> a\n>> b\n\nx");
        let theme = Theme::default();
        assert_eq!(frames[0].decor.bars.len(), 1);
        assert_eq!(frames[1].decor.bars.len(), 2);
        assert_eq!(frames[1].left, theme.quote_indent * 2.);
        assert!(frames[3].decor.bars.is_empty());
    }

    #[test]
    fn callouts_share_one_tinted_group() {
        let frames = frames("> [!tip] Title\n> body\n\nx");
        let (header, body) = (&frames[0], &frames[1]);
        assert_eq!(header.decor.surfaces.len(), 1);
        assert_eq!(header.decor.surfaces[0].group, body.decor.surfaces[0].group);
        assert!(header.pad_top > px(0.) && body.pad_bottom > px(0.));
        assert_eq!(header.pad_bottom, px(0.));
    }

    #[test]
    fn numbered_code_lines_get_numbers() {
        let frames = frames("```rust ln:true\nfn a() {}\nfn b() {}\n```\n\nx");
        let numbers: Vec<Option<usize>> = frames.iter().map(|frame| frame.line_number).collect();
        assert_eq!(numbers, vec![None, Some(1), Some(2), None, None, None]);
        assert_eq!(frames[1].decor.surfaces.len(), 1);
    }
}
