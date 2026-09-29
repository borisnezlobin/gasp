//! The prose in a Markdown note: which blocks hold sentences, and what
//! each inline node reads as.
//!
//! Paragraphs and list items are units of their own, so a sentence never
//! runs from one item into the next. Code, math, tables, raw HTML blocks,
//! comments and frontmatter hold no prose. Headings are titles rather
//! than sentences, so only the grammar checker reads them, and it leaves
//! quotes and callouts alone because they're someone else's words.

use std::ops::Range;

use gasp_core::syntax::{NodeId, NodeKind, SyntaxTree};

use crate::projection::{Piece, PieceKind, Projection};
use crate::segment::{Length, Thresholds, sentences};

/// What the prose is being read for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Purpose {
    /// Sentence-length highlighting.
    Rhythm,
    /// Spelling and mechanical checks.
    Grammar,
}

/// A block of prose: a paragraph, a list item's own text or a heading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// From its first piece to its last.
    pub range: Range<usize>,
    pub pieces: Vec<Piece>,
}

impl Unit {
    /// The unit's pieces as the reader sees them.
    pub fn project(&self, source: &str) -> Projection {
        Projection::new(source, &self.pieces)
    }
}

/// The units that overlap `within`, in order.
pub fn units(tree: &SyntaxTree, within: Range<usize>, purpose: Purpose) -> Vec<Unit> {
    tree.nodes_overlapping(within)
        .into_iter()
        .filter(|&id| is_unit(&tree.node(id).kind, purpose))
        .filter(|&id| {
            !tree
                .ancestors(id)
                .any(|up| excludes(&tree.node(up).kind, purpose))
        })
        .filter_map(|id| unit_of(tree, id, purpose))
        .collect()
}

fn is_unit(kind: &NodeKind, purpose: Purpose) -> bool {
    match kind {
        NodeKind::Paragraph | NodeKind::ListItem { .. } => true,
        NodeKind::Heading { .. } => purpose == Purpose::Grammar,
        _ => false,
    }
}

/// Whether nothing inside a block of this kind is read.
fn excludes(kind: &NodeKind, purpose: Purpose) -> bool {
    match kind {
        NodeKind::Table { .. }
        | NodeKind::CodeBlock(_)
        | NodeKind::MathBlock
        | NodeKind::HtmlBlock(_)
        | NodeKind::Frontmatter
        | NodeKind::CommentBlock => true,
        NodeKind::BlockQuote | NodeKind::Callout(_) => purpose == Purpose::Grammar,
        _ => false,
    }
}

fn unit_of(tree: &SyntaxTree, id: NodeId, purpose: Purpose) -> Option<Unit> {
    let mut pieces = Vec::new();
    for &child in &tree.node(id).children {
        // A list item's nested lists and paragraphs are units of their own.
        if tree.node(child).kind.is_block() {
            continue;
        }
        collect(tree, child, purpose, &mut pieces);
    }
    if !pieces.iter().any(|piece| piece.kind == PieceKind::Text) {
        return None;
    }
    let range = pieces.first()?.range.start..pieces.last()?.range.end;
    Some(Unit { range, pieces })
}

/// What an inline node becomes.
enum Reading {
    /// Its text, as it is.
    Text,
    /// Its children, without its markup.
    Children,
    Atom,
    Space,
    /// Nothing: it isn't read, like an image or a comment.
    Skip,
}

fn reading(tree: &SyntaxTree, id: NodeId, purpose: Purpose) -> Reading {
    let node = tree.node(id);
    let has_children = !node.children.is_empty();
    match &node.kind {
        NodeKind::Text => Reading::Text,
        NodeKind::SoftBreak | NodeKind::HardBreak => Reading::Space,
        NodeKind::Emphasis | NodeKind::Strong | NodeKind::Strikethrough | NodeKind::Highlight => {
            Reading::Children
        }
        NodeKind::Code | NodeKind::Math { .. } | NodeKind::Tag { .. } => Reading::Atom,
        // Links read as their text for rhythm; the checker skips them.
        NodeKind::Link(_) | NodeKind::WikiLink(_) | NodeKind::Html(_)
            if has_children && purpose == Purpose::Rhythm =>
        {
            Reading::Children
        }
        NodeKind::Link(_) | NodeKind::WikiLink(_) => Reading::Atom,
        NodeKind::Html(_) if has_children => Reading::Atom,
        // A lone tag such as <br> separates words.
        NodeKind::Html(_) => Reading::Space,
        _ => Reading::Skip,
    }
}

fn collect(tree: &SyntaxTree, id: NodeId, purpose: Purpose, pieces: &mut Vec<Piece>) {
    let range = tree.node(id).range.clone();
    match reading(tree, id, purpose) {
        Reading::Text => pieces.push(Piece::new(range, PieceKind::Text)),
        Reading::Atom => pieces.push(Piece::new(range, PieceKind::Atom)),
        Reading::Space => pieces.push(Piece::new(range, PieceKind::Space)),
        Reading::Children => {
            for &child in &tree.node(id).children {
                collect(tree, child, purpose, pieces);
            }
        }
        Reading::Skip => {}
    }
}

/// Each sentence of `unit` with its length, as ranges of the note.
pub fn sentence_lengths(
    source: &str,
    unit: &Unit,
    thresholds: Thresholds,
) -> Vec<(Range<usize>, Length)> {
    let projection = unit.project(source);
    sentences(projection.text())
        .into_iter()
        .map(|sentence| {
            (
                projection.source_range(sentence.range),
                thresholds.classify(sentence.words),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use gasp_core::syntax::parse;

    use super::*;

    fn unit_texts(text: &str, purpose: Purpose) -> Vec<String> {
        let tree = parse(text);
        units(&tree, 0..text.len(), purpose)
            .iter()
            .map(|unit| unit.project(text).text().to_owned())
            .collect()
    }

    #[test]
    fn paragraphs_and_list_items_are_units() {
        let text = "One *two* three.\nFour.\n\n- item one. Two\n  - nested\n- item **b**\n";
        assert_eq!(
            unit_texts(text, Purpose::Rhythm),
            ["One two three. Four.", "item one. Two", "nested", "item b"]
        );
    }

    #[test]
    fn code_math_tables_and_headings_are_not_prose() {
        let text = "# Title\n\n```\ncode here.\n```\n\n$$\nx = 1.\n$$\n\n| a. | b. |\n|---|---|\n| c | d |\n\nReal text.\n";
        assert_eq!(unit_texts(text, Purpose::Rhythm), ["Real text."]);
        assert_eq!(unit_texts(text, Purpose::Grammar), ["Title", "Real text."]);
    }

    #[test]
    fn inline_code_math_and_tags_are_single_words() {
        let text = "Run `cargo test` on $x + y$ with #tag now.";
        assert_eq!(
            unit_texts(text, Purpose::Rhythm),
            ["Run x on x with x now."]
        );
    }

    #[test]
    fn links_read_as_their_text_for_rhythm_only() {
        let text = "See [the docs](https://a.b/c.d) and [[Note|my note]] or https://x.y/z today.";
        assert_eq!(
            unit_texts(text, Purpose::Rhythm),
            ["See the docs and my note or x today."]
        );
        assert_eq!(
            unit_texts(text, Purpose::Grammar),
            ["See x and x or x today."]
        );
    }

    #[test]
    fn quotes_are_read_for_rhythm_but_not_checked() {
        let text = "> Quoted words\n> go on.\n\nMine.";
        assert_eq!(
            unit_texts(text, Purpose::Rhythm),
            ["Quoted words go on.", "Mine."]
        );
        assert_eq!(unit_texts(text, Purpose::Grammar), ["Mine."]);
    }

    #[test]
    fn images_comments_and_breaks() {
        let text = "Before ![[img.png]] %%note%% after<br>next.";
        assert_eq!(unit_texts(text, Purpose::Rhythm), ["Before   after next."]);
    }

    #[test]
    fn sentence_ranges_map_back_through_markup() {
        let text = "> A **long** one. Short!\n> Next line.";
        let tree = parse(text);
        let unit = &units(&tree, 0..text.len(), Purpose::Rhythm)[0];
        let found: Vec<&str> = sentence_lengths(text, unit, Thresholds::default())
            .into_iter()
            .map(|(range, _)| &text[range])
            .collect();
        assert_eq!(found, ["A **long** one.", "Short!", "Next line."]);
    }
}
