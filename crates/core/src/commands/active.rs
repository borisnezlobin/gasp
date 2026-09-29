//! Which toggle commands are "on" where the cursor is, so a toolbar can
//! show bold as pressed while the cursor is in bold text.

use crate::syntax::{HtmlKind, NodeKind, SyntaxTree};

/// The inline format a node around the cursor turns on, if any.
fn format_for(kind: &NodeKind) -> Option<&'static str> {
    let id = match kind {
        NodeKind::Strong | NodeKind::Html(HtmlKind::Bold) => "format.bold",
        NodeKind::Emphasis | NodeKind::Html(HtmlKind::Italic) => "format.italic",
        NodeKind::Html(HtmlKind::Underline) => "format.underline",
        NodeKind::Strikethrough | NodeKind::Html(HtmlKind::Strike) => "format.strikethrough",
        NodeKind::Highlight | NodeKind::Html(HtmlKind::Mark) => "format.highlight",
        NodeKind::Code => "format.code",
        NodeKind::Math { display: false } => "format.math-inline",
        NodeKind::Comment | NodeKind::CommentBlock => "format.comment",
        NodeKind::Link(_) | NodeKind::WikiLink(_) => "format.link",
        _ => return None,
    };
    Some(id)
}

/// The command a block around the cursor turns on, if any.
fn block_command_for(kind: &NodeKind) -> Option<&'static str> {
    let id = match kind {
        NodeKind::List { ordered: true, .. } => "format.numbered-list",
        NodeKind::List { ordered: false, .. } => "format.bullet-list",
        NodeKind::ListItem { task: Some(_) } => "edit.toggle-task",
        NodeKind::Callout(_) => "format.callout",
        NodeKind::Table { .. } => "table.insert",
        _ => return None,
    };
    Some(id)
}

/// The command a node around the cursor turns on, if any.
fn command_for(kind: &NodeKind) -> Option<&'static str> {
    format_for(kind).or_else(|| block_command_for(kind))
}

/// The toggle commands in effect at `offset`, innermost last, each once.
pub fn active_commands(tree: &SyntaxTree, offset: usize) -> Vec<&'static str> {
    let mut active = Vec::new();
    for id in tree.path_at(offset) {
        if let Some(command) = command_for(&tree.node(id).kind)
            && !active.contains(&command)
        {
            active.push(command);
        }
    }
    active
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn active_at(text: &str, marker: &str) -> Vec<&'static str> {
        let offset = text.find(marker).expect("marker is in the text");
        active_commands(&parse(text), offset)
    }

    #[test]
    fn bold_and_italic_are_on_inside_them() {
        let text = "plain **bold *both* here** after";
        assert!(active_at(text, "plain").is_empty());
        assert_eq!(active_at(text, "old"), ["format.bold"]);
        assert_eq!(active_at(text, "oth"), ["format.bold", "format.italic"]);
    }

    #[test]
    fn lists_say_which_kind() {
        assert_eq!(active_at("- one\n- two\n", "two"), ["format.bullet-list"]);
        assert_eq!(
            active_at("1. one\n2. two\n", "two"),
            ["format.numbered-list"]
        );
        assert_eq!(
            active_at("- [x] done\n", "done"),
            ["format.bullet-list", "edit.toggle-task"]
        );
    }
}
