//! Splits a document into frontmatter, `%%` comment blocks and Markdown
//! segments before the CommonMark parser sees it.

use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Segment {
    Frontmatter(Range<usize>),
    Comment(Range<usize>),
    Markdown(Range<usize>),
}

/// Splits `text` into segments that cover it completely, in order.
pub(crate) fn split(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let body_start = match frontmatter_end(text) {
        Some(end) => {
            segments.push(Segment::Frontmatter(0..end));
            end
        }
        None => 0,
    };
    let mut markdown_start = body_start;
    for comment in comment_blocks(text, body_start) {
        push_markdown(&mut segments, markdown_start..comment.start);
        markdown_start = comment.end;
        segments.push(Segment::Comment(comment));
    }
    push_markdown(&mut segments, markdown_start..text.len());
    segments
}

fn push_markdown(segments: &mut Vec<Segment>, range: Range<usize>) {
    if !range.is_empty() {
        segments.push(Segment::Markdown(range));
    }
}

/// Lines of `text` from `from`, as (start, line without terminator).
fn lines_from(text: &str, from: usize) -> impl Iterator<Item = (usize, &str)> {
    let rest = &text[from..];
    rest.split_inclusive('\n').scan(from, |offset, line| {
        let start = *offset;
        *offset += line.len();
        Some((start, line.trim_end_matches(['\n', '\r'])))
    })
}

/// End offset of the closing fence of YAML frontmatter at the very start.
fn frontmatter_end(text: &str) -> Option<usize> {
    let mut lines = lines_from(text, 0);
    let (_, first) = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    lines
        .find(|(_, line)| matches!(line.trim_end(), "---" | "..."))
        .map(|(start, line)| start + line.len())
}

struct FenceTracker {
    open: Option<(u8, usize)>,
}

impl FenceTracker {
    /// Updates the state with `line` and returns whether the line is part of
    /// a fenced code block.
    fn consume(&mut self, line: &str) -> bool {
        let fence = fence_of(line);
        match (self.open, fence) {
            (None, Some(found)) => {
                self.open = Some(found);
                true
            }
            (Some((ch, len)), Some((found_ch, found_len))) => {
                if ch == found_ch && found_len >= len && is_bare_fence(line) {
                    self.open = None;
                }
                true
            }
            (Some(_), None) => true,
            (None, None) => false,
        }
    }
}

/// The fence character and length if `line` opens or closes a fence.
pub(crate) fn fence_of(line: &str) -> Option<(u8, usize)> {
    let trimmed = strip_indent(line)?;
    let ch = *trimmed.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = trimmed.bytes().take_while(|&b| b == ch).count();
    (len >= 3).then_some((ch, len))
}

fn is_bare_fence(line: &str) -> bool {
    strip_indent(line).is_some_and(|rest| {
        let ch = rest.as_bytes()[0];
        rest.trim_start_matches(ch as char).trim().is_empty()
    })
}

/// The line with up to three leading spaces removed, or `None` if it is
/// indented further.
fn strip_indent(line: &str) -> Option<&str> {
    let spaces = line.bytes().take_while(|&b| b == b' ').count();
    (spaces <= 3).then(|| &line[spaces..])
}

/// Ranges of `%%` comments that start a line and run over several lines.
fn comment_blocks(text: &str, from: usize) -> Vec<Range<usize>> {
    let mut blocks = Vec::new();
    let mut fences = FenceTracker { open: None };
    let mut open: Option<usize> = None;
    for (start, line) in lines_from(text, from) {
        if let Some(block_start) = open {
            if let Some(at) = line.find("%%") {
                blocks.push(block_start..start + at + 2);
                open = None;
            }
            continue;
        }
        if fences.consume(line) {
            continue;
        }
        open = comment_opening(line).map(|at| start + at);
    }
    if let Some(block_start) = open {
        blocks.push(block_start..text.len());
    }
    blocks
}

/// Offset of `%%` if the line opens a multi-line comment: it starts the line
/// and isn't closed on the same line.
fn comment_opening(line: &str) -> Option<usize> {
    let trimmed = strip_indent(line)?;
    let rest = trimmed.strip_prefix("%%")?;
    let at = line.len() - trimmed.len();
    (!rest.contains("%%")).then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_and_comment_blocks_are_split_out() {
        let text = "---\na: 1\n---\ntext\n%%\nhidden\n%%\nmore\n";
        let segments = split(text);
        assert_eq!(
            segments,
            vec![
                Segment::Frontmatter(0..12),
                Segment::Markdown(12..18),
                Segment::Comment(18..30),
                Segment::Markdown(30..36),
            ]
        );
    }

    #[test]
    fn inline_comments_and_fenced_percent_signs_stay_in_markdown() {
        let text = "%% one line %%\n```\n%%\n```\n";
        assert_eq!(split(text), vec![Segment::Markdown(0..text.len())]);
    }

    #[test]
    fn unclosed_comment_runs_to_the_end() {
        let text = "a\n\n%% open\nrest";
        assert_eq!(
            split(text),
            vec![Segment::Markdown(0..3), Segment::Comment(3..text.len())]
        );
    }

    #[test]
    fn frontmatter_needs_a_closing_fence() {
        assert_eq!(frontmatter_end("---\na: 1\n"), None);
        assert_eq!(frontmatter_end("---\n---\n"), Some(7));
    }
}
