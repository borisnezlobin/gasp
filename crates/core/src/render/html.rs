//! Styles of the HTML elements a note may use: `<b>`, `<sup>`, `<kbd>`,
//! `<span style="color: red">`, `<center>` and the rest. Each paired
//! element styles its text, not its tags; a lone or mismatched tag styles
//! nothing.

use std::ops::Range;

use crate::syntax::{
    Alignment, FONT_SCALE_RANGE, FontSize, HtmlKind, HtmlStyle, Node, NodeId, NodeKind, SyntaxTree,
};

use super::output::StyleKey;

/// Whether `node` is a paired element, `<b>…</b>`, rather than a lone tag.
fn is_element(node: &Node) -> bool {
    node.markup.len() == 2
}

fn html_kind(node: &Node) -> Option<HtmlKind> {
    match node.kind {
        NodeKind::Html(kind) if is_element(node) => Some(kind),
        _ => None,
    }
}

/// The spans an HTML element gives its text.
pub(crate) fn element_spans(tree: &SyntaxTree, id: NodeId) -> Vec<(Range<usize>, StyleKey)> {
    let node = tree.node(id);
    let Some(kind) = html_kind(node) else {
        return Vec::new();
    };
    let mut keys = kind_keys(kind);
    if let Some(style) = kind.style() {
        style_keys(tree, id, style, &mut keys);
    }
    node.content
        .iter()
        .flat_map(|range| keys.iter().map(move |key| (range.clone(), *key)))
        .collect()
}

fn kind_keys(kind: HtmlKind) -> Vec<StyleKey> {
    let key = match kind {
        HtmlKind::Underline => StyleKey::Underline,
        HtmlKind::Bold => StyleKey::Strong,
        HtmlKind::Italic => StyleKey::Emphasis,
        HtmlKind::Strike => StyleKey::Strikethrough,
        HtmlKind::Superscript => StyleKey::Superscript,
        HtmlKind::Subscript => StyleKey::Subscript,
        HtmlKind::Mark => StyleKey::Highlight,
        HtmlKind::Kbd => StyleKey::Kbd,
        _ => return Vec::new(),
    };
    vec![key]
}

fn style_keys(tree: &SyntaxTree, id: NodeId, style: &HtmlStyle, keys: &mut Vec<StyleKey>) {
    let depth = nesting(tree, id);
    if let Some(color) = style.color {
        let rgba = color.to_u32();
        keys.push(StyleKey::TextColor { depth, rgba });
    }
    if let Some(color) = style.background {
        let rgba = color.to_u32();
        keys.push(StyleKey::TextBackground { depth, rgba });
    }
    if style.font_size.is_some() {
        let percent = font_scale(tree, id);
        keys.push(StyleKey::FontScale { depth, percent });
    }
    let flags = [
        (style.bold == Some(true), StyleKey::Strong),
        (style.italic == Some(true), StyleKey::Emphasis),
        (style.underline, StyleKey::Underline),
        (style.strikethrough, StyleKey::Strikethrough),
    ];
    keys.extend(flags.iter().filter(|(on, _)| *on).map(|(_, key)| *key));
}

/// How many HTML elements hold this one.
fn nesting(tree: &SyntaxTree, id: NodeId) -> u8 {
    let depth = tree
        .ancestors(id)
        .filter(|&ancestor| html_kind(tree.node(ancestor)).is_some())
        .count();
    u8::try_from(depth).unwrap_or(u8::MAX)
}

fn font_size(node: &Node) -> Option<FontSize> {
    html_kind(node)?.style()?.font_size
}

/// The size of an element's text as a percentage of the line's, with
/// `em` and `%` sizes taken relative to the element around it.
fn font_scale(tree: &SyntaxTree, id: NodeId) -> u16 {
    let mut percent = 100.;
    let mut sizes: Vec<FontSize> = std::iter::once(id)
        .chain(tree.ancestors(id))
        .filter_map(|at| font_size(tree.node(at)))
        .collect();
    let nearest_absolute = sizes
        .iter()
        .position(|size| matches!(size, FontSize::Absolute(_)))
        .map_or(sizes.len(), |at| at + 1);
    sizes.truncate(nearest_absolute);
    for size in sizes.iter().rev() {
        percent = match size {
            FontSize::Absolute(own) => f32::from(*own),
            FontSize::Relative(own) => percent * f32::from(*own) / 100.,
        };
    }
    let (min, max) = FONT_SCALE_RANGE;
    percent.clamp(f32::from(min), f32::from(max)).round() as u16
}

/// How a block element aligns its lines, when it does.
pub(crate) fn alignment(node: &Node) -> Option<Alignment> {
    let kind = html_kind(node).filter(HtmlKind::is_block)?;
    let written = kind.style().and_then(|style| style.align);
    match kind {
        HtmlKind::Center(_) => Some(written.unwrap_or(Alignment::Center)),
        _ => written,
    }
}
