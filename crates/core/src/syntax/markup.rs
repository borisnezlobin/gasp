//! Markup tokens of leaf and inline nodes, found by looking at the source
//! around the ranges pulldown-cmark reports.

use std::ops::Range;

use super::kinds::{LinkInfo, LinkKind, MarkupKind, NodeKind};
use super::segments::fence_of;
use super::tree::Node;

/// Adds the markup of a node that doesn't depend on container prefixes.
/// `children_end` is where the last child ends, if there is a child.
pub(crate) fn add_markup(node: &mut Node, text: &str, children_end: Option<usize>) {
    if add_delimited(node, text) || add_block_markup(node, text) {
        return;
    }
    add_reference_markup(node, text, children_end);
}

fn add_delimited(node: &mut Node, text: &str) -> bool {
    let source = &text[node.range.clone()];
    let (kind, len) = match node.kind {
        NodeKind::Emphasis => (MarkupKind::EmphasisDelimiter, 1),
        NodeKind::Strong => (MarkupKind::StrongDelimiter, 2),
        NodeKind::Strikethrough => (MarkupKind::StrikethroughDelimiter, run_len(source, b'~')),
        NodeKind::Code => (MarkupKind::CodeDelimiter, run_len(source, b'`')),
        NodeKind::Math { display } => (MarkupKind::MathDelimiter, 1 + usize::from(display)),
        NodeKind::MathBlock => (MarkupKind::MathDelimiter, 2),
        _ => return false,
    };
    add_pair(node, kind, len);
    true
}

/// Adds `len`-byte delimiters at both ends of the node.
pub(crate) fn add_pair(node: &mut Node, kind: MarkupKind, len: usize) {
    let Range { start, end } = node.range;
    let len = len.min((end - start) / 2);
    node.add_markup(kind, start..start + len);
    node.add_markup(kind, end - len..end);
}

fn run_len(source: &str, byte: u8) -> usize {
    source.bytes().take_while(|&b| b == byte).count()
}

fn add_block_markup(node: &mut Node, text: &str) -> bool {
    match node.kind {
        NodeKind::Heading { .. } => add_heading(node, text),
        NodeKind::CodeBlock(ref info) if info.fenced => add_fences(node, text),
        NodeKind::ThematicBreak => {
            let range = trimmed(text, node.range.clone());
            node.add_markup(MarkupKind::ThematicBreak, range);
        }
        NodeKind::HardBreak => {
            let source = &text[node.range.clone()];
            let len = source.trim_end_matches(['\n', '\r']).len();
            node.add_markup(
                MarkupKind::HardBreakMarker,
                node.range.start..node.range.start + len,
            );
        }
        NodeKind::Frontmatter => add_frontmatter(node, text),
        NodeKind::CommentBlock => add_comment_block(node, text),
        NodeKind::Conflict => add_conflict_markers(node, text),
        NodeKind::Html(_) => node.add_markup(MarkupKind::HtmlTag, node.range.clone()),
        _ => return false,
    }
    true
}

/// The opening, separator and closing lines of a sync conflict: its
/// first line, the first `=======` after it and its last line.
fn add_conflict_markers(node: &mut Node, text: &str) {
    let lines: Vec<Range<usize>> = super::segments::lines_from(text, node.range.start)
        .take_while(|(start, _)| *start < node.range.end)
        .map(|(start, line)| start..start + line.len())
        .collect();
    let (Some(open), Some(close)) = (lines.first(), lines.last()) else {
        return;
    };
    let separator = lines
        .iter()
        .skip(1)
        .find(|line| &text[(*line).clone()] == super::segments::SEPARATOR);
    node.add_markup(MarkupKind::ConflictMarker, open.clone());
    if let Some(separator) = separator.filter(|separator| *separator != close) {
        node.add_markup(MarkupKind::ConflictMarker, separator.clone());
    }
    node.add_markup(MarkupKind::ConflictMarker, close.clone());
}

fn add_reference_markup(node: &mut Node, text: &str, children_end: Option<usize>) {
    match &node.kind {
        NodeKind::Link(info) => add_link(node, text, &info.clone(), children_end, 1),
        NodeKind::Image(info) => add_link(node, text, &info.clone(), children_end, 2),
        NodeKind::WikiLink(_) => super::wikilink::add_markup(node, text, false),
        NodeKind::Embed(_) => super::wikilink::add_markup(node, text, true),
        NodeKind::FootnoteReference { .. } => {
            let Range { start, end } = node.range;
            node.add_markup(MarkupKind::FootnoteMarker, start..start + 2);
            node.add_markup(MarkupKind::FootnoteMarker, end - 1..end);
        }
        NodeKind::FootnoteDefinition { .. } => {
            add_label_prefix(node, text, MarkupKind::FootnoteMarker)
        }
        NodeKind::LinkDefinition { .. } => {
            add_label_prefix(node, text, MarkupKind::LinkBracket);
            let rest = node.markup.last().map_or(node.range.start, |m| m.range.end);
            node.add_markup(MarkupKind::LinkDestination, rest..node.range.end);
        }
        _ => {}
    }
}

/// The range with trailing and leading whitespace removed.
pub(crate) fn trimmed(text: &str, range: Range<usize>) -> Range<usize> {
    let source = &text[range.clone()];
    let start = range.start + (source.len() - source.trim_start().len());
    let end = range.start + source.trim_end().len();
    start..end.max(start)
}

/// `[label]:` and one following space at the node's start.
fn add_label_prefix(node: &mut Node, text: &str, kind: MarkupKind) {
    let start = node.range.start;
    if let Some(close) = text[start..node.range.end].find("]:") {
        let end = super::build::skip_one_space(text, start + close + 2);
        node.add_markup(kind, start..end.min(node.range.end));
    }
}

fn add_heading(node: &mut Node, text: &str) {
    let Range { start, end } = node.range;
    let source = &text[start..end];
    let NodeKind::Heading { level, .. } = node.kind else {
        return;
    };
    if let Some(newline) = source.rfind('\n') {
        node.kind = NodeKind::Heading {
            level,
            setext: true,
        };
        let underline = trimmed_marker_line(text, start + newline + 1..end);
        node.add_markup(MarkupKind::HeadingMarker, underline);
        return;
    }
    add_atx_markers(node, text);
}

/// The underline of a setext heading, skipping quote prefixes.
fn trimmed_marker_line(text: &str, line: Range<usize>) -> Range<usize> {
    let source = &text[line.clone()];
    let offset = source.find(['=', '-']).unwrap_or(0);
    trimmed(text, line.start + offset..line.end)
}

fn add_atx_markers(node: &mut Node, text: &str) {
    let Range { start, end } = trimmed(text, node.range.clone());
    let hashes_end = start + run_len(&text[start..end], b'#');
    let open_end = hashes_end + run_len_of(&text[hashes_end..end], b" \t");
    node.add_markup(MarkupKind::HeadingMarker, start..open_end);
    let content = &text[open_end..end];
    let without_hashes = content.trim_end_matches('#');
    let close_start = open_end + without_hashes.len();
    let separated = without_hashes.is_empty() || without_hashes.ends_with([' ', '\t']);
    if close_start < end && separated {
        let spaced = open_end + without_hashes.trim_end().len();
        node.add_markup(MarkupKind::HeadingMarker, spaced..end);
    }
}

fn run_len_of(source: &str, bytes: &[u8]) -> usize {
    source.bytes().take_while(|b| bytes.contains(b)).count()
}

fn line_end(text: &str, from: usize, limit: usize) -> usize {
    text[from..limit].find('\n').map_or(limit, |at| from + at)
}

fn add_fences(node: &mut Node, text: &str) {
    let Range { start, end } = node.range;
    let open_end = line_end(text, start, end);
    let open_line = &text[start..open_end];
    node.add_markup(MarkupKind::CodeFence, trimmed(text, start..open_end));
    let Some((fence_char, fence_len)) = fence_of(open_line.trim_start()) else {
        return;
    };
    if open_end >= end {
        return;
    }
    let last_line_start = start + text[start..end].rfind('\n').map_or(0, |at| at + 1);
    let last_line = &text[last_line_start..end];
    let Some(fence_at) = last_line.find(fence_char as char) else {
        return;
    };
    let is_closing = fence_of(&last_line[fence_at..])
        .is_some_and(|(ch, len)| ch == fence_char && len >= fence_len)
        && last_line[fence_at..]
            .trim_end_matches([fence_char as char, ' ', '\t'])
            .is_empty();
    if is_closing {
        let range = trimmed(text, last_line_start + fence_at..end);
        node.add_markup(MarkupKind::CodeFence, range);
    }
}

fn add_frontmatter(node: &mut Node, text: &str) {
    let Range { start, end } = node.range;
    let open_end = line_end(text, start, end);
    node.add_markup(MarkupKind::FrontmatterFence, start..open_end);
    if let Some(newline) = text[start..end].rfind('\n') {
        node.add_markup(
            MarkupKind::FrontmatterFence,
            trimmed(text, start + newline + 1..end),
        );
    }
}

fn add_comment_block(node: &mut Node, text: &str) {
    let Range { start, end } = node.range;
    node.add_markup(MarkupKind::CommentDelimiter, start..start + 2);
    if end >= start + 4 && text[..end].ends_with("%%") {
        node.add_markup(MarkupKind::CommentDelimiter, end - 2..end);
    }
}

/// Brackets and destination of a Markdown link or image. `open_len` is 1
/// for `[` and 2 for `![`.
fn add_link(
    node: &mut Node,
    text: &str,
    info: &LinkInfo,
    children_end: Option<usize>,
    open_len: usize,
) {
    let Range { start, end } = node.range;
    match info.kind {
        LinkKind::BareUrl => {}
        LinkKind::Autolink | LinkKind::Email => {
            node.add_markup(MarkupKind::LinkBracket, start..start + 1);
            node.add_markup(MarkupKind::LinkBracket, end - 1..end);
        }
        _ => add_bracketed_link(node, text, children_end, open_len),
    }
}

fn add_bracketed_link(node: &mut Node, text: &str, children_end: Option<usize>, open_len: usize) {
    let Range { start, end } = node.range;
    let text_end = children_end
        .unwrap_or(start + open_len)
        .max(start + open_len);
    node.add_markup(MarkupKind::LinkBracket, start..start + open_len);
    let Some(close) = text[text_end..end].find(']').map(|at| text_end + at) else {
        return;
    };
    node.add_markup(MarkupKind::LinkBracket, close..close + 1);
    node.add_markup(MarkupKind::LinkDestination, close + 1..end);
}

/// Pipes between the cells of a table row.
pub(crate) fn add_table_pipes(row: &mut Node, text: &str, cells: &[Range<usize>]) {
    let Range { start, end } = row.range;
    let bytes = text.as_bytes();
    let pipes: Vec<usize> = (start..end)
        .filter(|&at| bytes[at] == b'|')
        .filter(|&at| at == 0 || bytes[at - 1] != b'\\')
        .filter(|at| !cells.iter().any(|cell| cell.contains(at)))
        .collect();
    for at in pipes {
        row.add_markup(MarkupKind::TablePipe, at..at + 1);
    }
}

/// The delimiter row of a table, the line after its head.
pub(crate) fn add_table_delimiter_row(table: &mut Node, text: &str, head_end: usize) {
    let end = table.range.end;
    let Some(newline) = text[head_end..end].find('\n') else {
        return;
    };
    let row_start = head_end + newline + 1;
    let row_end = line_end(text, row_start, end);
    let offset = text[row_start..row_end].find(['|', '-', ':']).unwrap_or(0);
    let range = trimmed(text, row_start + offset..row_end);
    table.add_markup(MarkupKind::TableDelimiterRow, range);
}

/// The marker of a list item and the spaces after it.
pub(crate) fn list_marker(text: &str, start: usize, end: usize) -> Range<usize> {
    let source = &text[start..end];
    let digits = source.bytes().take_while(u8::is_ascii_digit).count();
    let marker_len = digits + 1;
    let after = &source[marker_len.min(source.len())..];
    let line_rest = &after[..after.find('\n').unwrap_or(after.len())];
    let spaces = run_len_of(line_rest, b" \t");
    let spaces = match spaces {
        _ if spaces == line_rest.len() => 0,
        0..=4 => spaces,
        _ => 1,
    };
    start..start + marker_len + spaces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heading_markup(text: &str) -> Vec<Range<usize>> {
        let mut node = Node::new(
            NodeKind::Heading {
                level: 1,
                setext: false,
            },
            0..text.len(),
        );
        add_markup(&mut node, text, None);
        node.markup.into_iter().map(|m| m.range).collect()
    }

    #[test]
    fn atx_heading_markers() {
        assert_eq!(heading_markup("## Title"), vec![0..3]);
        assert_eq!(heading_markup("# Title ##"), vec![0..2, 7..10]);
        assert_eq!(heading_markup("# C#"), vec![0..2]);
        assert_eq!(heading_markup("#"), vec![0..1]);
    }

    #[test]
    fn setext_underline_is_markup() {
        assert_eq!(heading_markup("Title\n==="), vec![6..9]);
    }

    #[test]
    fn list_markers_include_the_following_space() {
        assert_eq!(list_marker("- a", 0, 3), 0..2);
        assert_eq!(list_marker("12. a", 0, 5), 0..4);
        assert_eq!(list_marker("-", 0, 1), 0..1);
        assert_eq!(list_marker("-      code", 0, 11), 0..2);
    }
}
