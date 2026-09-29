//! `edit.indent` and `edit.outdent`, which Tab, Shift+Tab and the
//! iPhone's keyboard bar run. They move whole blocks wherever the caret is
//! in them: a list item goes one level in or out with the items nested
//! under it, every line of a paragraph shifts, and a block quote or
//! callout nests in another quote or comes out of one. Fenced code and
//! math blocks keep Tab's own tab at the caret, and shift the lines a
//! selection touches. A selection moves every block it touches, and the caret and
//! selection stay on the text they were on.

use std::ops::RangeInclusive;

use crate::document::{Document, Selection, SelectionRange};
use crate::syntax::{NodeId, NodeKind, SyntaxTree, parse};
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

pub const INDENT: &str = "edit.indent";
pub const OUTDENT: &str = "edit.outdent";

/// Spaces that count as one level on lines indented with spaces.
const SPACES_PER_LEVEL: usize = 4;
const TAB: &str = "\t";
const QUOTE_LEVEL: &str = "> ";

/// Moves every block the selection touches one level in.
pub fn indent(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    shift(doc, selection, Direction::In, timestamp_ms)
}

/// Moves every block the selection touches one level out.
pub fn outdent(doc: &Document, selection: &Selection, timestamp_ms: u64) -> Transaction {
    shift(doc, selection, Direction::Out, timestamp_ms)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    In,
    Out,
}

impl Direction {
    fn command(self) -> &'static str {
        match self {
            Direction::In => INDENT,
            Direction::Out => OUTDENT,
        }
    }
}

/// The block a line belongs to, as far as indenting goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    /// A list item, with everything nested under it.
    Item(NodeId),
    /// A paragraph at the top of the note.
    Paragraph(NodeId),
    /// A block quote or callout.
    Quote(NodeId),
    /// A code or math block inside this many quotes.
    Verbatim(usize),
    /// A line that starts like a list item but that Markdown folds into
    /// the item above, such as an empty nested item, inside this many
    /// quotes. It moves on its own.
    MarkerLine(usize),
    /// A heading, table, rule or anything else with no indent of its own.
    Fixed,
}

/// What one place in the selection moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Block(Block),
    /// One line of a code or math block, inside `depth` quotes.
    Line {
        line: usize,
        depth: usize,
    },
    /// A tab typed over the selection range, as in code.
    Tab(usize, usize),
}

fn shift(
    doc: &Document,
    selection: &Selection,
    direction: Direction,
    timestamp_ms: u64,
) -> Transaction {
    let text = doc.slice(0..doc.len());
    let shifter = Shifter {
        doc,
        tree: parse(&text),
        direction,
    };
    let mut units: Vec<Unit> = Vec::new();
    for range in selection.ranges() {
        for unit in shifter.units(range) {
            if !units.contains(&unit) {
                units.push(unit);
            }
        }
    }
    let edits = units
        .into_iter()
        .flat_map(|unit| shifter.edits(unit))
        .collect();
    let changes = ChangeSet::new(dedup_edits(edits)).unwrap_or_default();
    Transaction::new(changes, Origin::command(direction.command()), timestamp_ms)
}

/// Drops an edit at the same place as one before it, as when two blocks
/// share a line.
fn dedup_edits(mut edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    edits.dedup_by(|later, earlier| {
        later.range.start < earlier.range.end.max(earlier.range.start + 1)
    });
    edits
}

struct Shifter<'a> {
    doc: &'a Document,
    tree: SyntaxTree,
    direction: Direction,
}

impl Shifter<'_> {
    // MARK: What moves

    fn units(&self, range: &SelectionRange) -> Vec<Unit> {
        if range.is_empty() {
            let line = self.doc.line_of_offset(range.head);
            return self.caret_unit(range, line).into_iter().collect();
        }
        self.range_lines(range)
            .filter(|&line| !self.is_blank(line))
            .filter_map(|line| self.line_unit(line))
            .collect()
    }

    fn caret_unit(&self, range: &SelectionRange, line: usize) -> Option<Unit> {
        let tab = Unit::Tab(range.from(), range.to());
        match (self.block_at_line(line), self.direction) {
            (Block::Verbatim(_) | Block::Fixed, Direction::In) => Some(tab),
            (Block::Verbatim(depth), Direction::Out) | (Block::MarkerLine(depth), _) => {
                Some(Unit::Line { line, depth })
            }
            (Block::Fixed, Direction::Out) => None,
            (block, _) => Some(Unit::Block(block)),
        }
    }

    fn line_unit(&self, line: usize) -> Option<Unit> {
        match self.block_at_line(line) {
            Block::Verbatim(depth) | Block::MarkerLine(depth) => Some(Unit::Line { line, depth }),
            Block::Fixed => None,
            block => Some(Unit::Block(block)),
        }
    }

    /// The lines a selection touches. One ending at the very start of a
    /// line leaves that line out, as it looks unselected.
    fn range_lines(&self, range: &SelectionRange) -> RangeInclusive<usize> {
        let first = self.doc.line_of_offset(range.from());
        let mut last = self.doc.line_of_offset(range.to());
        if last > first && self.doc.line_start(last) == range.to() {
            last -= 1;
        }
        first..=last
    }

    fn block_at_line(&self, line: usize) -> Block {
        let text = self.doc.line_text(line);
        let markers = text.len() - text.trim_start_matches([' ', '\t', '>']).len();
        let path = self.tree.path_at(self.doc.line_start(line) + markers);
        let block = path
            .iter()
            .rev()
            .find_map(|&id| self.block_of(id))
            .unwrap_or(Block::Fixed);
        let starts_like_item = marker_width(&text[markers..]) > 0;
        if !starts_like_item
            || !self.may_hide_an_item(block, &path)
            || self.item_starts_on(&path, line)
        {
            return block;
        }
        Block::MarkerLine(self.quotes_on(&path))
    }

    /// Whether a line in `block` could be a list item Markdown folded into
    /// the text above: an empty item under a paragraph or an item's text
    /// reads as more of that text, or as a setext heading's underline.
    fn may_hide_an_item(&self, block: Block, path: &[NodeId]) -> bool {
        match block {
            Block::Item(_) | Block::Paragraph(_) => true,
            Block::Fixed => path.last().is_some_and(|&id| {
                matches!(
                    self.tree.node(id).kind,
                    NodeKind::Heading { setext: true, .. }
                )
            }),
            _ => false,
        }
    }

    /// Whether a list item on `path` starts on `line`, rather than the
    /// line being folded into something above it.
    fn item_starts_on(&self, path: &[NodeId], line: usize) -> bool {
        path.iter().any(|&id| {
            matches!(self.tree.node(id).kind, NodeKind::ListItem { .. })
                && *self.node_lines(id).start() == line
        })
    }

    fn quotes_on(&self, path: &[NodeId]) -> usize {
        path.iter()
            .filter(|&&id| is_quote(&self.tree.node(id).kind))
            .count()
    }

    fn block_of(&self, id: NodeId) -> Option<Block> {
        let block = match &self.tree.node(id).kind {
            NodeKind::ListItem { .. } => Block::Item(id),
            NodeKind::BlockQuote | NodeKind::Callout(_) => Block::Quote(id),
            NodeKind::Paragraph => self.paragraph_block(id),
            // An indented paragraph reads as indented code, so indented
            // code moves the way the paragraph it came from would.
            NodeKind::CodeBlock(info) if !info.fenced => self.paragraph_block(id),
            NodeKind::CodeBlock(_) | NodeKind::MathBlock => Block::Verbatim(self.quote_depth(id)),
            kind if kind.is_block() => Block::Fixed,
            _ => return None,
        };
        Some(block)
    }

    /// A paragraph moves with the list item or quote it's in; one in a
    /// footnote or a sync conflict stays put.
    fn paragraph_block(&self, paragraph: NodeId) -> Block {
        let Some(parent) = self.tree.node(paragraph).parent else {
            return Block::Fixed;
        };
        match self.tree.node(parent).kind {
            NodeKind::Document => Block::Paragraph(paragraph),
            _ => self.block_of(parent).unwrap_or(Block::Fixed),
        }
    }

    fn quote_depth(&self, id: NodeId) -> usize {
        self.tree
            .ancestors(id)
            .filter(|&ancestor| is_quote(&self.tree.node(ancestor).kind))
            .count()
    }

    // MARK: Edits

    fn edits(&self, unit: Unit) -> Vec<TextEdit> {
        match unit {
            Unit::Block(Block::Item(item)) => self.item_edits(item),
            Unit::Block(Block::Paragraph(paragraph)) => {
                self.level_edits(self.node_lines(paragraph), 0, TAB, SPACES_PER_LEVEL)
            }
            Unit::Block(Block::Quote(quote)) => self.quote_edits(quote),
            Unit::Block(Block::Verbatim(_) | Block::MarkerLine(_) | Block::Fixed) => Vec::new(),
            Unit::Line { line, depth } => {
                self.level_edits(line..=line, depth, TAB, SPACES_PER_LEVEL)
            }
            Unit::Tab(from, to) => vec![TextEdit::new(from..to, TAB)],
        }
    }

    /// Nests an item under the one before it, or brings it out a level,
    /// with its children. The first item of a list has nothing to nest
    /// under, and an item at the list's edge has nowhere out to go.
    fn item_edits(&self, item: NodeId) -> Vec<TextEdit> {
        let depth = self.quote_depth(item);
        let lines = self.node_lines(item);
        let own = self.indentation(*lines.start(), depth);
        match self.direction {
            Direction::In => match self.previous_item(item) {
                Some(previous) => {
                    let level = self.nesting_level(&own, previous, depth);
                    self.level_edits(lines, depth, &level, SPACES_PER_LEVEL)
                }
                None => Vec::new(),
            },
            Direction::Out if own.is_empty() => Vec::new(),
            Direction::Out => {
                let spaces = self.outdent_spaces(item, &own, depth);
                self.level_edits(lines, depth, TAB, spaces)
            }
        }
    }

    fn previous_item(&self, item: NodeId) -> Option<NodeId> {
        let parent = self.tree.node(item).parent?;
        let siblings = &self.tree.node(parent).children;
        let at = siblings.iter().position(|&sibling| sibling == item)?;
        at.checked_sub(1).map(|before| siblings[before])
    }

    /// What nests an item under `previous`: a tab, or as many spaces as
    /// the previous item's marker is wide when the list uses spaces.
    fn nesting_level(&self, own: &str, previous: NodeId, depth: usize) -> String {
        if !own.starts_with(' ') {
            return TAB.to_owned();
        }
        let line = self
            .doc
            .line_of_offset(self.tree.node(previous).range.start);
        let start = self.after_quotes(line, depth) - self.doc.line_start(line);
        let text = self.doc.line_text(line);
        " ".repeat(marker_width(text[start..].trim_start()).max(1))
    }

    /// The spaces one level out takes on lines indented with spaces: back
    /// to the column of the item this one is nested in.
    fn outdent_spaces(&self, item: NodeId, own: &str, depth: usize) -> usize {
        let parent_item = self
            .tree
            .ancestors(item)
            .find(|&ancestor| matches!(self.tree.node(ancestor).kind, NodeKind::ListItem { .. }));
        let parent_width = parent_item.map_or(0, |parent| {
            let line = self.doc.line_of_offset(self.tree.node(parent).range.start);
            self.indentation(line, depth).len()
        });
        own.len()
            .saturating_sub(parent_width)
            .clamp(1, SPACES_PER_LEVEL)
    }

    /// Nests a quote or callout in one more quote, or takes one quote
    /// marker off each of its lines.
    fn quote_edits(&self, quote: NodeId) -> Vec<TextEdit> {
        let depth = self.quote_depth(quote);
        let lines = self.node_lines(quote);
        lines
            .filter_map(|line| {
                let at = self.after_quotes(line, depth);
                let text = self.doc.slice(at..self.doc.line_end(line));
                let marker = text.len() - text.trim_start_matches(' ').len();
                match self.direction {
                    Direction::In => Some(TextEdit::insert(at + marker, QUOTE_LEVEL)),
                    Direction::Out => quote_marker(&text[marker..]).map(|len| {
                        let start = at + marker;
                        TextEdit::delete(start..start + len)
                    }),
                }
            })
            .collect()
    }

    /// One level added to or taken off each non-blank line, just inside
    /// `depth` quotes: `level` going in, and a tab or up to `spaces`
    /// spaces coming out.
    fn level_edits(
        &self,
        lines: RangeInclusive<usize>,
        depth: usize,
        level: &str,
        spaces: usize,
    ) -> Vec<TextEdit> {
        lines
            .filter(|&line| !self.is_blank(line))
            .filter_map(|line| {
                let at = self.after_quotes(line, depth);
                match self.direction {
                    Direction::In => Some(TextEdit::insert(at, level)),
                    Direction::Out => {
                        let own = self.indentation(line, depth);
                        leading_level(&own, spaces).map(|len| TextEdit::delete(at..at + len))
                    }
                }
            })
            .collect()
    }

    // MARK: Lines

    fn node_lines(&self, id: NodeId) -> RangeInclusive<usize> {
        let range = &self.tree.node(id).range;
        let last = range.end.saturating_sub(1).max(range.start);
        self.doc.line_of_offset(range.start)..=self.doc.line_of_offset(last)
    }

    fn is_blank(&self, line: usize) -> bool {
        self.doc
            .line_text(line)
            .trim_start_matches([' ', '\t', '>'])
            .is_empty()
    }

    /// The whitespace a line starts with, just inside `depth` quotes.
    fn indentation(&self, line: usize, depth: usize) -> String {
        let at = self.after_quotes(line, depth);
        let text = self.doc.slice(at..self.doc.line_end(line));
        let width = text.len() - text.trim_start_matches([' ', '\t']).len();
        text[..width].to_owned()
    }

    /// Where a line's text starts after `depth` quote markers, each `>`
    /// with up to three spaces before it and one after.
    fn after_quotes(&self, line: usize, depth: usize) -> usize {
        let text = self.doc.line_text(line);
        let bytes = text.as_bytes();
        let mut at = 0;
        for _ in 0..depth {
            let spaces = bytes[at..]
                .iter()
                .take(3)
                .take_while(|&&byte| byte == b' ')
                .count();
            if bytes.get(at + spaces) != Some(&b'>') {
                break;
            }
            at += spaces + 1;
            if bytes.get(at) == Some(&b' ') {
                at += 1;
            }
        }
        self.doc.line_start(line) + at
    }
}

fn is_quote(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::BlockQuote | NodeKind::Callout(_))
}

/// How long a quote marker at the start of `text` is: `>` and the space
/// after it.
fn quote_marker(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('>')?;
    Some(1 + usize::from(rest.starts_with(' ')))
}

/// One level at the start of `indentation`: a tab, or up to `spaces`
/// spaces.
fn leading_level(indentation: &str, spaces: usize) -> Option<usize> {
    if indentation.starts_with('\t') {
        return Some(1);
    }
    let count = indentation
        .bytes()
        .take(spaces)
        .take_while(|&byte| byte == b' ')
        .count();
    (count > 0).then_some(count)
}

/// How wide a list marker is with the space after it: `- ` is 2 and
/// `10. ` is 4.
fn marker_width(line: &str) -> usize {
    if ["- ", "* ", "+ "]
        .iter()
        .any(|marker| line.starts_with(marker))
    {
        return 2;
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let punctuated = [". ", ") "]
        .iter()
        .any(|end| line[digits..].starts_with(end));
    if digits > 0 && punctuated {
        digits + 2
    } else {
        0
    }
}

#[cfg(test)]
mod tests;
