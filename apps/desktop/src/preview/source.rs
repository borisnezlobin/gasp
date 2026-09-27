//! The note's text as one string plus its syntax tree, kept current after
//! every change by reparsing only the edited blocks.

use std::ops::Range;

use editor_core::document::Document;
use editor_core::syntax::{self, Edit, SyntaxTree};

/// The text and tree the render planner reads.
#[derive(Clone, Debug)]
pub struct Source {
    text: String,
    tree: SyntaxTree,
    /// The tree is [`syntax::plain`], waiting for a real parse.
    plain: bool,
}

/// What an update changed, in line numbers before and after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceChange {
    pub edit: Edit,
    pub old_lines: Range<usize>,
    pub new_lines: Range<usize>,
}

impl Source {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            tree: syntax::parse(text),
            plain: false,
        }
    }

    /// `text` as plain lines, to show at once while [`Source::new`] runs
    /// elsewhere. The first edit parses it for real.
    pub fn unparsed(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            tree: syntax::plain(text),
            plain: true,
        }
    }

    /// Whether the tree is still the plain one of [`Source::unparsed`].
    pub fn is_plain(&self) -> bool {
        self.plain
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tree(&self) -> &SyntaxTree {
        &self.tree
    }

    pub fn line_count(&self) -> usize {
        self.tree.lines().line_count()
    }

    /// The line's range without its terminator.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        self.tree.lines().line_range(&self.text, line)
    }

    pub fn line_text(&self, line: usize) -> &str {
        &self.text[self.line_range(line)]
    }

    pub fn line_of(&self, offset: usize) -> usize {
        self.tree.lines().line_of(offset.min(self.text.len()))
    }

    /// Replaces `range` with `insert`, as a single known edit.
    pub fn replace(&mut self, range: Range<usize>, insert: &str) -> SourceChange {
        let edit = Edit {
            old: range.clone(),
            new_len: insert.len(),
        };
        let old_lines = self.lines_of(&range);
        self.text.replace_range(range, insert);
        self.apply(edit, old_lines)
    }

    /// Catches up with `doc` after any change, such as undo, by finding the
    /// one range that differs. Returns `None` when nothing changed.
    pub fn sync(&mut self, doc: &Document) -> Option<SourceChange> {
        let new_text = doc.to_string();
        let edit = differing_range(&self.text, &new_text)?;
        let old_lines = self.lines_of(&edit.old);
        self.text = new_text;
        Some(self.apply(edit, old_lines))
    }

    fn apply(&mut self, edit: Edit, old_lines: Range<usize>) -> SourceChange {
        // A plain tree has no blocks to reparse, so this parses it all.
        self.tree.edit(&self.text, &edit);
        self.plain = false;
        let new_end = edit.old.start + edit.new_len;
        let new_lines = self.lines_of(&(edit.old.start..new_end));
        SourceChange {
            edit,
            old_lines,
            new_lines,
        }
    }

    fn lines_of(&self, range: &Range<usize>) -> Range<usize> {
        let lines = self.tree.lines();
        lines.line_of(range.start)..lines.line_of(range.end) + 1
    }
}

/// The smallest edit that turns `old` into `new`, on character boundaries.
pub fn differing_range(old: &str, new: &str) -> Option<Edit> {
    if old == new {
        return None;
    }
    let mut prefix = common_prefix(old.as_bytes(), new.as_bytes());
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let room = old.len().min(new.len()) - prefix;
    let mut suffix = common_suffix(old.as_bytes(), new.as_bytes()).min(room);
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    Some(Edit {
        old: prefix..old.len() - suffix,
        new_len: new.len() - suffix - prefix,
    })
}

/// Compared in blocks first so long equal stretches cost a memcmp.
const BLOCK: usize = 256;

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    let limit = a.len().min(b.len());
    let mut at = 0;
    while at + BLOCK <= limit && a[at..at + BLOCK] == b[at..at + BLOCK] {
        at += BLOCK;
    }
    at + a[at..limit]
        .iter()
        .zip(&b[at..limit])
        .take_while(|(x, y)| x == y)
        .count()
}

fn common_suffix(a: &[u8], b: &[u8]) -> usize {
    let limit = a.len().min(b.len());
    let mut matched = 0;
    while matched + BLOCK <= limit
        && a[a.len() - matched - BLOCK..a.len() - matched]
            == b[b.len() - matched - BLOCK..b.len() - matched]
    {
        matched += BLOCK;
    }
    matched
        + a[..a.len() - matched]
            .iter()
            .rev()
            .zip(b[..b.len() - matched].iter().rev())
            .take_while(|(x, y)| x == y)
            .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_one_changed_range() {
        let edit = differing_range("hello world", "hello brave world").unwrap();
        assert_eq!(edit.old, 6..6);
        assert_eq!(edit.new_len, 6);
        let edit = differing_range("abc", "ac").unwrap();
        assert_eq!((edit.old, edit.new_len), (1..2, 0));
        assert!(differing_range("same", "same").is_none());
    }

    #[test]
    fn repeated_characters_do_not_overlap() {
        let edit = differing_range("aa", "aaa").unwrap();
        assert_eq!((edit.old, edit.new_len), (2..2, 1));
        let edit = differing_range("aaa", "a").unwrap();
        assert_eq!((edit.old, edit.new_len), (1..3, 0));
    }

    #[test]
    fn edits_stay_on_character_boundaries() {
        let edit = differing_range("aé", "aè").unwrap();
        assert_eq!(edit.old, 1..3);
        assert_eq!(edit.new_len, 2);
    }

    #[test]
    fn long_texts_compare_by_blocks() {
        let old = "x".repeat(1000) + "a" + &"y".repeat(700);
        let new = "x".repeat(1000) + "bb" + &"y".repeat(700);
        let edit = differing_range(&old, &new).unwrap();
        assert_eq!((edit.old, edit.new_len), (1000..1001, 2));
    }

    #[test]
    fn an_unparsed_source_has_lines_and_parses_on_its_first_edit() {
        let mut source = Source::unparsed("# a\n\ntext");
        assert!(source.is_plain());
        assert_eq!(source.line_count(), 3);
        assert_eq!(source.line_text(2), "text");
        source.replace(0..0, "x");
        assert!(!source.is_plain());
        assert_eq!(*source.tree(), syntax::parse(source.text()));
    }

    #[test]
    fn replace_and_sync_keep_the_tree_current() {
        let mut source = Source::new("# a\n\ntext\n\nmore");
        let change = source.replace(5..5, "**bold** ");
        assert_eq!(source.text(), "# a\n\n**bold** text\n\nmore");
        assert_eq!(change.old_lines, 2..3);
        assert_eq!(*source.tree(), syntax::parse(source.text()));
        let doc = Document::from("# a\n\nnew\nlines\n\nmore");
        let change = source.sync(&doc).unwrap();
        assert_eq!(change.new_lines, 2..4);
        assert_eq!(*source.tree(), syntax::parse(source.text()));
        assert_eq!(source.line_text(3), "lines");
    }
}
