//! Reparsing one row of a table after an edit inside it. A table is one
//! block, so the general reparse parses all of it again; typing in a cell
//! of a long table would pay for every row. A row's cells parse on their
//! own, though: only the header decides the columns. So an edit on one
//! row's line reparses the header, the delimiter row and that row alone,
//! as a table of their own, and splices the row's nodes in, as long as
//! the row stays a row of the table (and the header keeps its cells).

use std::ops::Range;

use super::kinds::NodeKind;
use super::tree::{LineIndex, Node, NodeId, SyntaxTree};
use super::{Edit, Reparsed, build, process};

/// Text that can change how the rest of the note parses, which the
/// general reparse handles.
const RISKY: [&str; 5] = ["%%", "]:", "<<<<<<<", "=======", ">>>>>>>"];

/// An edited row and what its reparse needs.
struct EditedRow {
    row: NodeId,
    table: NodeId,
    header: bool,
    /// The row's line in the edited text.
    line: Range<usize>,
    /// The table's header and delimiter lines in the edited text.
    head: Range<usize>,
    delimiter: Range<usize>,
}

impl SyntaxTree {
    /// Reparses the table row an edit is on, when that's all it can
    /// change. Answers the table's lines before and after, when it did.
    pub(super) fn reparse_table_row(&mut self, new_text: &str, edit: &Edit) -> Option<Reparsed> {
        let edited = self.edited_row(new_text, edit)?;
        let line_text = &new_text[edited.line.clone()];
        if RISKY.iter().any(|risky| line_text.contains(risky)) {
            return None;
        }
        let nodes = self.parse_row(new_text, &edited)?;
        let table = self.node(edited.table).range.clone();
        let start = self.lines.line_start(self.lines.line_of(table.start));
        self.splice_row(&edited, nodes, edit, new_text.len());
        self.lines.edit(new_text, edit);
        Some(Reparsed::Blocks {
            old: start..table.end,
            new: start..self.node(edited.table).range.end,
        })
    }

    /// The row an edit is on, when the edit stays on its line and the
    /// row's table is a top-level block.
    fn edited_row(&self, new_text: &str, edit: &Edit) -> Option<EditedRow> {
        let inserted = new_text.get(edit.old.start..edit.old.start + edit.new_len)?;
        if inserted.contains(['\n', '\r']) {
            return None;
        }
        let row =
            self.path_at(edit.old.start).into_iter().rev().find(|&id| {
                matches!(self.node(id).kind, NodeKind::TableRow | NodeKind::TableHead)
            })?;
        let node = self.node(row);
        let table = node.parent?;
        let top_level = self.node(table).parent == Some(SyntaxTree::ROOT);
        // A row's range can run past its line's break, to an empty cell
        // pulldown-cmark puts there, so the edit must be on its line too.
        let lines = &self.lines;
        let row_line = lines.line_of(node.range.start);
        let row_start = lines.line_start(row_line);
        let line_break = match row_line + 1 < lines.line_count() {
            true => lines.line_start(row_line + 1) - 1,
            false => self.root().range.end,
        };
        let within =
            node.range.start < edit.old.start && edit.old.end <= node.range.end.min(line_break);
        if !within || !top_level {
            return None;
        }
        // Lines up to the edited one start where they did; each runs to
        // its break in the edited text.
        let line_at = |start: usize| {
            let rest = &new_text[start..];
            let end = start + rest.find('\n').unwrap_or(rest.len());
            start..end - usize::from(new_text[start..end].ends_with('\r'))
        };
        let head_start = lines.line_start(lines.line_of(self.node(table).range.start));
        let head = line_at(head_start);
        let delimiter = line_at(head_start + new_text[head_start..].find('\n')? + 1);
        let line = line_at(row_start);
        Some(EditedRow {
            row,
            table,
            header: node.kind == NodeKind::TableHead,
            line,
            head,
            delimiter,
        })
    }

    /// Parses the header, the delimiter row and the edited row as a table
    /// of their own, and answers the row's nodes, moved to where the row
    /// is in the note, when it's still a row of the same shape.
    fn parse_row(&self, new_text: &str, edited: &EditedRow) -> Option<Vec<Node>> {
        let mut mini = format!(
            "{}\n{}",
            &new_text[edited.head.clone()],
            &new_text[edited.delimiter.clone()]
        );
        let row_at = match edited.header {
            true => 0,
            false => {
                mini.push('\n');
                let at = mini.len();
                mini.push_str(&new_text[edited.line.clone()]);
                at
            }
        };
        let context = self.definitions.0.get()?;
        let raw = build::build_region(&mini, 0..mini.len(), context)?;
        let nodes = process(raw, &mini, &LineIndex::new(&mini));
        let table = match nodes[0].children[..] {
            [table] => table,
            _ => return None,
        };
        let old_table = self.node(edited.table);
        let same_table = nodes[table.0].kind == old_table.kind
            && nodes[table.0].children.len() == 1 + usize::from(!edited.header);
        if !same_table {
            return None;
        }
        let row = *nodes[table.0].children.last()?;
        let old_row = self.node(edited.row);
        let same_row = nodes[row.0].kind == old_row.kind
            && (!edited.header || nodes[row.0].children.len() == old_row.children.len());
        if !same_row {
            return None;
        }
        let end = subtree_end(&nodes, row);
        let shift = edited.line.start as isize - row_at as isize;
        let renumber = edited.row.0 as isize - row.0 as isize;
        let mut moved: Vec<Node> = nodes[row.0..end].to_vec();
        for node in &mut moved {
            move_node(node, shift, renumber);
        }
        moved[0].parent = Some(edited.table);
        Some(moved)
    }

    /// Puts the row's new nodes in place of its old ones, moving what
    /// comes after by the edit.
    fn splice_row(&mut self, edited: &EditedRow, nodes: Vec<Node>, edit: &Edit, new_len: usize) {
        let start = edited.row.0;
        let end = subtree_end(&self.nodes, edited.row);
        let id_shift = nodes.len() as isize - (end - start) as isize;
        let delta = edit.new_len as isize - edit.old.len() as isize;
        let moved_id = |id: &mut NodeId| {
            if id.0 >= end {
                id.0 = (id.0 as isize + id_shift) as usize;
            }
        };
        let moved_offset = |offset: &mut usize| {
            if *offset >= edit.old.end {
                *offset = (*offset as isize + delta) as usize;
            }
        };
        let inserted = nodes.len();
        self.nodes.splice(start..end, nodes);
        // Before the row, only its table and the root reach past the edit;
        // everything after the row moves.
        let ancestors = [SyntaxTree::ROOT.0, edited.table.0];
        let after = start + inserted..self.nodes.len();
        for index in ancestors.into_iter().chain(after) {
            let node = &mut self.nodes[index];
            node.children.iter_mut().for_each(moved_id);
            node.parent.iter_mut().for_each(moved_id);
            let ranges = std::iter::once(&mut node.range)
                .chain(node.markup.iter_mut().map(|markup| &mut markup.range))
                .chain(node.content.iter_mut());
            // What starts where text went in (a table starting with its
            // edited row) keeps its start.
            for range in ranges {
                if range.start > edit.old.start {
                    moved_offset(&mut range.start);
                }
                moved_offset(&mut range.end);
            }
        }
        // A table with a body ends where its last row does, spaces after
        // it left out.
        let table = edited.table.0;
        if !edited.header && self.nodes[table].children.last() == Some(&edited.row) {
            let end = self.nodes[start].range.end;
            let table = &mut self.nodes[table];
            table.range.end = end;
            let markup = table.markup.iter().map(|markup| markup.range.clone());
            table.content = super::subtract(&table.range, markup);
        }
        let root = &mut self.nodes[0];
        root.range = 0..new_len;
        root.content.clear();
        root.content.push(0..new_len);
    }
}

/// Where the subtree of `id` ends in a preorder arena: its last
/// descendant's index, plus one.
fn subtree_end(nodes: &[Node], id: NodeId) -> usize {
    let mut last = id;
    while let Some(&child) = nodes[last.0].children.last() {
        last = child;
    }
    last.0 + 1
}

/// Moves a node's offsets by `shift` and its ids by `renumber`.
fn move_node(node: &mut Node, shift: isize, renumber: isize) {
    let offset = |at: usize| (at as isize + shift) as usize;
    let range = |range: &mut Range<usize>| *range = offset(range.start)..offset(range.end);
    range(&mut node.range);
    node.markup.iter_mut().for_each(|m| range(&mut m.range));
    node.content.iter_mut().for_each(range);
    let id = |id: &mut NodeId| id.0 = (id.0 as isize + renumber) as usize;
    node.children.iter_mut().for_each(id);
    node.parent.iter_mut().for_each(id);
}
