//! Building widget descriptions from nodes.

use std::ops::Range;

use crate::syntax::html_attribute as attribute;
use crate::syntax::{ConflictSide, HtmlKind, Markup, MarkupKind, Node, NodeId, NodeKind, WikiInfo};

use super::output::WidgetKind;
use super::reveal::Revealer;
use crate::link_card::LinkCard;

/// File extensions an embed draws as a picture. Any other embed, such as
/// a note or a PDF, reads as a link to it rather than a picture of
/// nothing.
const IMAGE_EXTENSIONS: [&str; 11] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif", "tif", "tiff", "heic",
];

/// Whether an embed's target names a picture.
pub fn embeds_image(target: &str) -> bool {
    std::path::Path::new(target)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|image| image.eq_ignore_ascii_case(extension))
        })
}

/// Standalone HTML tags that become widgets.
pub(crate) fn html_replaceable(kind: HtmlKind) -> bool {
    matches!(
        kind,
        HtmlKind::LineBreak | HtmlKind::HorizontalRule | HtmlKind::Image | HtmlKind::Comment
    )
}

/// The widget drawn instead of a node, if it has one. Hidden comments have
/// none: they simply disappear.
pub(crate) fn replacement(revealer: &Revealer<'_>, id: NodeId) -> Option<WidgetKind> {
    let node = revealer.tree.node(id);
    let text = revealer.text;
    let kind = match &node.kind {
        NodeKind::Math { display } => WidgetKind::InlineMath {
            tex: tex(revealer, id),
            display: *display,
        },
        NodeKind::MathBlock => WidgetKind::MathBlock {
            tex: tex(revealer, id),
        },
        NodeKind::Image(info) => markdown_image(
            &info.destination,
            &joined(text, &node.content_without_destination()),
        ),
        NodeKind::Embed(info) if !embeds_image(&info.target) => {
            return note_embed(revealer, info);
        }
        NodeKind::Embed(info) => WidgetKind::Image {
            target: info.target.clone(),
            alt: info.alias.clone().unwrap_or_default(),
            width: info.size.map(|size| size.0),
            height: info.size.and_then(|size| size.1),
            embed: true,
        },
        NodeKind::FootnoteReference { label } => WidgetKind::FootnoteSuperscript {
            label: label.clone(),
        },
        NodeKind::CodeBlock(_) => return link_card(text, node),
        _ => return simple_replacement(node, text),
    };
    Some(kind)
}

/// The card for an embedded note, when the app draws them; other files,
/// such as PDFs, stay links.
fn note_embed(revealer: &Revealer<'_>, info: &WikiInfo) -> Option<WidgetKind> {
    let wanted = revealer.settings.embed_notes && crate::embed::embeds_note(&info.target);
    wanted.then(|| WidgetKind::EmbeddedNote {
        target: info.target.clone(),
        subpath: info.subpath.clone(),
    })
}

fn simple_replacement(node: &Node, text: &str) -> Option<WidgetKind> {
    match node.kind {
        NodeKind::ThematicBreak | NodeKind::Html(HtmlKind::HorizontalRule) => {
            Some(WidgetKind::HorizontalRule)
        }
        NodeKind::Html(HtmlKind::LineBreak) => Some(WidgetKind::LineBreak),
        NodeKind::Html(HtmlKind::Image) => Some(html_image(&text[node.range.clone()])),
        _ => None,
    }
}

trait ImageContent {
    fn content_without_destination(&self) -> Vec<Range<usize>>;
}

impl ImageContent for Node {
    /// The alt text: content that isn't a destination or bracket.
    fn content_without_destination(&self) -> Vec<Range<usize>> {
        let destination_start = self
            .markup
            .iter()
            .find(|m| m.kind == MarkupKind::LinkDestination)
            .map_or(self.range.end, |m| m.range.start);
        self.content
            .iter()
            .filter(|piece| piece.end <= destination_start)
            .cloned()
            .collect()
    }
}

fn joined(text: &str, ranges: &[Range<usize>]) -> String {
    ranges.iter().map(|range| &text[range.clone()]).collect()
}

/// A Markdown image; Obsidian reads `![alt|300x200](src)` as a size.
fn markdown_image(target: &str, alt: &str) -> WidgetKind {
    let (alt, size) = match alt.rsplit_once('|') {
        Some((rest, size)) => match parse_size(size) {
            Some(size) => (rest, Some(size)),
            None => (alt, None),
        },
        None => (alt, None),
    };
    WidgetKind::Image {
        target: target.to_owned(),
        alt: alt.to_owned(),
        width: size.map(|size| size.0),
        height: size.and_then(|size| size.1),
        embed: false,
    }
}

fn parse_size(value: &str) -> Option<(u32, Option<u32>)> {
    match value.trim().split_once('x') {
        Some((width, height)) => Some((width.parse().ok()?, Some(height.parse().ok()?))),
        None => Some((value.trim().parse().ok()?, None)),
    }
}

fn html_image(tag: &str) -> WidgetKind {
    WidgetKind::Image {
        target: attribute(tag, "src").unwrap_or_default().to_owned(),
        alt: attribute(tag, "alt").unwrap_or_default().to_owned(),
        width: attribute(tag, "width").and_then(|w| w.parse().ok()),
        height: attribute(tag, "height").and_then(|h| h.parse().ok()),
        embed: false,
    }
}

/// The TeX source of a math node, without quote markers of enclosing
/// quotes and callouts.
pub(crate) fn tex(revealer: &Revealer<'_>, id: NodeId) -> String {
    let tree = revealer.tree;
    let node = tree.node(id);
    let (Some(first), Some(last)) = (node.content.first(), node.content.last()) else {
        return String::new();
    };
    let range = first.start..last.end;
    let markers: Vec<Range<usize>> = tree
        .ancestors(id)
        .flat_map(|a| tree.node(a).markup.iter())
        .filter(|m| {
            m.kind == MarkupKind::QuoteMarker
                && m.range.start >= range.start
                && m.range.end <= range.end
        })
        .map(|m| m.range.clone())
        .collect();
    let text = revealer.text;
    if markers.is_empty() {
        return text[range].trim().to_owned();
    }
    let mut tex = String::with_capacity(range.len());
    let mut at = range.start;
    let mut sorted = markers;
    sorted.sort_by_key(|m| m.start);
    for marker in sorted {
        tex.push_str(&text[at..marker.start]);
        at = marker.end;
    }
    tex.push_str(&text[at..range.end]);
    tex.trim().to_owned()
}

/// The code between a code block's fences.
pub(crate) fn code_lines(node: &Node) -> Option<Range<usize>> {
    let fences: Vec<&Markup> = node.markup.iter().collect();
    let start = fences
        .first()
        .map_or(node.range.start, |open| open.range.end + 1);
    let end = match fences.get(1) {
        Some(close) => close.range.start,
        None => node.range.end,
    };
    (start < end).then_some(start..end)
}

/// The card an `embed` code block describes, when it names a URL.
pub(crate) fn link_card(text: &str, node: &Node) -> Option<WidgetKind> {
    let NodeKind::CodeBlock(info) = &node.kind else {
        return None;
    };
    if info.language.as_deref() != Some(crate::link_card::LANGUAGE) {
        return None;
    }
    let body = code_lines(node).map_or("", |range| &text[range]);
    LinkCard::parse(body).map(WidgetKind::LinkCard)
}

pub(crate) fn code_block(node: &Node) -> WidgetKind {
    let NodeKind::CodeBlock(info) = &node.kind else {
        unreachable!("code fences belong to code blocks")
    };
    WidgetKind::CodeBlock {
        language: info.language.clone(),
        title: info.title.clone(),
        line_numbers: info.line_numbers,
        highlighted_lines: info.highlighted_lines.clone(),
        content: code_lines(node).unwrap_or(node.range.end..node.range.end),
    }
}

/// The first line of a task item after its markers.
pub(crate) fn task_text(text: &str, node: &Node) -> Option<Range<usize>> {
    let start = node.markup.iter().map(|m| m.range.end).max()?;
    let end = text[start..node.range.end]
        .find('\n')
        .map_or(node.range.end, |at| start + at);
    (start < end).then_some(start..end)
}

/// The widget drawn instead of a hidden list or task marker.
pub(crate) fn marker_widget(
    revealer: &Revealer<'_>,
    id: NodeId,
    token: &Markup,
) -> Option<WidgetKind> {
    let node = revealer.tree.node(id);
    match (token.kind, &node.kind) {
        (
            MarkupKind::TaskMarker,
            NodeKind::ListItem {
                task: Some(checked),
            },
        ) => Some(WidgetKind::Checkbox { checked: *checked }),
        (MarkupKind::ListMarker, _) => Some(list_bullet(revealer, id)),
        (MarkupKind::FootnoteMarker, NodeKind::FootnoteDefinition { label }) => {
            Some(WidgetKind::FootnoteSuperscript {
                label: label.clone(),
            })
        }
        (MarkupKind::ConflictMarker, NodeKind::Conflict) => conflict_label(&node.markup, token),
        // `[[#Heading]]` names a heading in this note: it just hides.
        (MarkupKind::WikiSubpath, NodeKind::WikiLink(_)) => {
            (token.range.start != node.range.start + 2).then_some(WidgetKind::SubpathSeparator)
        }
        _ => None,
    }
}

/// The opening marker names this device's version and the separator the
/// other device's; the closing marker just hides.
fn conflict_label(markers: &[Markup], token: &Markup) -> Option<WidgetKind> {
    let index = markers.iter().position(|marker| marker == token)?;
    let side = match index {
        0 => ConflictSide::ThisDevice,
        _ if index + 1 < markers.len() => ConflictSide::OtherDevice,
        _ => return None,
    };
    Some(WidgetKind::ConflictLabel { side })
}

fn list_bullet(revealer: &Revealer<'_>, id: NodeId) -> WidgetKind {
    let tree = revealer.tree;
    let depth = tree
        .ancestors(id)
        .filter(|&a| matches!(tree.node(a).kind, NodeKind::List { .. }))
        .count();
    let parent = tree.node(id).parent.map(|p| tree.node(p));
    let (ordered, number) = match parent.map(|p| (&p.kind, p)) {
        Some((
            NodeKind::List {
                ordered: true,
                start,
            },
            list,
        )) => {
            let index = list.children.iter().position(|&c| c == id).unwrap_or(0) as u64;
            (true, Some(start.unwrap_or(1) + index))
        }
        _ => (false, None),
    };
    WidgetKind::ListBullet {
        ordered,
        number,
        depth: depth.max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_attributes_are_read() {
        let tag = r#"<img src="a b.png" width=300 alt='cat'>"#;
        assert_eq!(attribute(tag, "src"), Some("a b.png"));
        assert_eq!(attribute(tag, "width"), Some("300"));
        assert_eq!(attribute(tag, "alt"), Some("cat"));
        assert_eq!(attribute(tag, "height"), None);
    }

    #[test]
    fn markdown_image_alt_can_carry_a_size() {
        let WidgetKind::Image {
            alt, width, height, ..
        } = markdown_image("a.png", "cat|200x100")
        else {
            panic!("expected an image");
        };
        assert_eq!((alt.as_str(), width, height), ("cat", Some(200), Some(100)));
    }
}
