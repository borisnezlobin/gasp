//! Finding the link at an offset, for following it.

use editor_core::syntax::{NodeKind, SyntaxTree, WikiInfo};

/// The target of the innermost link or wikilink containing `offset`: a URL
/// or path for Markdown links, and `note#heading` for wikilinks.
pub fn link_target_at(tree: &SyntaxTree, offset: usize) -> Option<String> {
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
    use editor_core::syntax::parse;

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
}
