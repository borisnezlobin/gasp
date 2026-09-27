//! Lists nested with a tab, as Obsidian writes them, start at their marker.

use editor_core::syntax::{MarkupKind, NodeKind, parse};

#[test]
fn a_tab_nested_list_item_starts_at_its_marker() {
    let text = "- a\n\t- b\n\t\t- c\n";
    let tree = parse(text);
    let items: Vec<(&str, Vec<&str>)> = tree
        .preorder()
        .into_iter()
        .map(|id| tree.node(id))
        .filter(|node| matches!(node.kind, NodeKind::ListItem { .. }))
        .map(|node| {
            let markers = node
                .markup
                .iter()
                .filter(|markup| markup.kind == MarkupKind::ListMarker)
                .map(|markup| &text[markup.range.clone()])
                .collect();
            (text[node.range.clone()].lines().next().unwrap(), markers)
        })
        .collect();
    assert_eq!(
        items,
        vec![
            ("- a", vec!["- "]),
            ("- b", vec!["- "]),
            ("- c", vec!["- "])
        ]
    );
}
