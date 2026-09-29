//! Finding the link at an offset, for following it or previewing it.

use std::ops::Range;

use gasp_core::syntax::{LinkKind, NodeKind, SyntaxTree, WikiInfo};

/// Something a hover preview can show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoverTarget {
    /// A note, as a wikilink target (`note#heading`) or a Markdown link
    /// destination.
    Note { link: String },
    /// A footnote reference's label.
    Footnote { label: String },
    /// An underlined footnote problem, such as an unused definition.
    Problem { message: String },
    /// A word or phrase the grammar checker underlined.
    Flag,
}

/// URL schemes that never point at a note.
const WEB_SCHEMES: [&str; 3] = ["http://", "https://", "mailto:"];

/// What the innermost link or footnote reference containing `offset`
/// would preview, and the node's range. Web links preview nothing.
pub fn hover_target_at(tree: &SyntaxTree, offset: usize) -> Option<(HoverTarget, Range<usize>)> {
    tree.path_at(offset).into_iter().rev().find_map(|id| {
        let node = tree.node(id);
        let target = match &node.kind {
            NodeKind::WikiLink(info) => HoverTarget::Note {
                link: wiki_target(info),
            },
            NodeKind::Link(info) if is_note_link(info.kind, &info.destination) => {
                HoverTarget::Note {
                    link: info.destination.clone(),
                }
            }
            NodeKind::FootnoteReference { label } => HoverTarget::Footnote {
                label: label.clone(),
            },
            _ => return None,
        };
        Some((target, node.range.clone()))
    })
}

fn is_note_link(kind: LinkKind, destination: &str) -> bool {
    let web = WEB_SCHEMES
        .iter()
        .any(|scheme| destination.starts_with(scheme));
    let bare = matches!(
        kind,
        LinkKind::BareUrl | LinkKind::Autolink | LinkKind::Email
    );
    !web && !bare && !destination.contains("://") && !destination.is_empty()
}

/// The target of the innermost link or wikilink containing `offset`, or
/// ending right before it, as a caret after a link typed or picked is: a
/// URL or path for Markdown links, and `note#heading` for wikilinks.
pub fn link_target_at(tree: &SyntaxTree, offset: usize) -> Option<String> {
    link_containing(tree, offset).or_else(|| link_containing(tree, offset.checked_sub(1)?))
}

/// The link whose text holds the character at `offset`.
fn link_containing(tree: &SyntaxTree, offset: usize) -> Option<String> {
    tree.path_at(offset)
        .into_iter()
        .rev()
        .find_map(|id| link_target(&tree.node(id).kind))
}

fn link_target(kind: &NodeKind) -> Option<String> {
    match kind {
        NodeKind::Link(info) => Some(info.destination.clone()),
        NodeKind::WikiLink(info) => Some(wiki_target(info)),
        _ => None,
    }
}

fn wiki_target(info: &WikiInfo) -> String {
    match &info.subpath {
        Some(subpath) => format!("{}#{subpath}", info.target),
        None => info.target.clone(),
    }
}

#[cfg(test)]
mod tests {
    use gasp_core::syntax::parse;

    use super::*;

    #[test]
    fn finds_markdown_and_wiki_links() {
        let text = "see [site](https://example.com) and [[Note#Part|alias]] or https://x.org";
        let tree = parse(text);
        assert_eq!(
            link_target_at(&tree, 6).as_deref(),
            Some("https://example.com")
        );
        assert_eq!(link_target_at(&tree, 40).as_deref(), Some("Note#Part"));
        assert_eq!(link_target_at(&tree, 65).as_deref(), Some("https://x.org"));
        assert_eq!(link_target_at(&tree, 1), None);
    }

    #[test]
    fn a_caret_just_after_a_link_is_on_it() {
        let text = "see [[Note]] then";
        let tree = parse(text);
        let end = text.find(" then").unwrap();
        assert_eq!(link_target_at(&tree, end).as_deref(), Some("Note"));
        assert_eq!(link_target_at(&tree, end + 1), None);
    }

    #[test]
    fn hover_targets_are_notes_and_footnotes_not_web_pages() {
        let text =
            "[[Note#Part|alias]] [doc](Folder/Doc%20Two.md) [web](https://x.org) a[^1]\n\n[^1]: x";
        let tree = parse(text);
        let note = |link: &str| HoverTarget::Note { link: link.into() };
        assert_eq!(hover_target_at(&tree, 3), Some((note("Note#Part"), 0..19)));
        assert_eq!(
            hover_target_at(&tree, 22).map(|found| found.0),
            Some(note("Folder/Doc%20Two.md"))
        );
        assert_eq!(hover_target_at(&tree, 50), None);
        let footnote = HoverTarget::Footnote { label: "1".into() };
        assert_eq!(
            hover_target_at(&tree, 71).map(|found| found.0),
            Some(footnote)
        );
    }
}
