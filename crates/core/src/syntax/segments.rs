//! Splits a document into frontmatter, `%%` comment blocks, sync
//! conflicts and Markdown segments before the CommonMark parser sees it.

use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Segment {
    Frontmatter(Range<usize>),
    Comment(Range<usize>),
    Conflict(ConflictRegion),
    Markdown(Range<usize>),
}

/// The separator line between a sync conflict's two versions.
pub(crate) const SEPARATOR: &str = "=======";

/// A sync conflict written into a note: this device's version and the
/// other device's, between marker lines. The versions are parsed as
/// Markdown on their own, so the separator can't turn the line above it
/// into a heading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConflictRegion {
    /// From the start of the `<<<<<<<` line to the end of the `>>>>>>>`
    /// line.
    pub range: Range<usize>,
    pub this_device: Range<usize>,
    pub other_device: Range<usize>,
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
        push_markdown(text, &mut segments, markdown_start..comment.start);
        markdown_start = comment.end;
        segments.push(Segment::Comment(comment));
    }
    push_markdown(text, &mut segments, markdown_start..text.len());
    segments
}

/// Pushes `range` as Markdown, with any sync conflicts in it split out.
fn push_markdown(text: &str, segments: &mut Vec<Segment>, range: Range<usize>) {
    let mut at = range.start;
    for conflict in conflicts(text, range.clone()) {
        push_plain(segments, at..conflict.range.start);
        at = conflict.range.end;
        segments.push(Segment::Conflict(conflict));
    }
    push_plain(segments, at..range.end);
}

fn push_plain(segments: &mut Vec<Segment>, range: Range<usize>) {
    if !range.is_empty() {
        segments.push(Segment::Markdown(range));
    }
}

/// A sync conflict's marker lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConflictMarker {
    Open,
    Separator,
    Close,
}

/// The marker `line` is: seven `<`, `=` or `>`, with any label after the
/// angle brackets set off by a space.
pub(crate) fn conflict_marker(line: &str) -> Option<ConflictMarker> {
    let labelled = |prefix: &str| {
        line.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    };
    if line == SEPARATOR {
        Some(ConflictMarker::Separator)
    } else if labelled("<<<<<<<") {
        Some(ConflictMarker::Open)
    } else if labelled(">>>>>>>") {
        Some(ConflictMarker::Close)
    } else {
        None
    }
}

/// Sync conflicts in `range`: an opening line, a separator and a closing
/// line, in that order. Markers inside a fenced code block, such as an
/// example of a merge in a note, don't open one.
fn conflicts(text: &str, range: Range<usize>) -> Vec<ConflictRegion> {
    let mut scanner = ConflictScanner::default();
    lines_from(text, range.start)
        .take_while(|(start, _)| *start < range.end)
        .filter_map(|(start, line)| {
            let after = text[start..]
                .find('\n')
                .map_or(range.end, |at| start + at + 1)
                .min(range.end);
            scanner.feed(start, line, after)
        })
        .collect()
}

#[derive(Default)]
struct ConflictScanner {
    fences: FenceTracker,
    /// The opening line's start and the offset after it.
    open: Option<(usize, usize)>,
    /// The separator line's start and the offset after it.
    separator: Option<(usize, usize)>,
}

impl ConflictScanner {
    /// Reads the line at `start`, which the next line follows at `after`,
    /// and returns the conflict it closes.
    fn feed(&mut self, start: usize, line: &str, after: usize) -> Option<ConflictRegion> {
        let marker = conflict_marker(line);
        let Some(open) = self.open else {
            let in_code = self.fences.consume(line);
            if !in_code && marker == Some(ConflictMarker::Open) {
                self.open = Some((start, after));
            }
            return None;
        };
        match (marker?, self.separator) {
            (ConflictMarker::Open, None) => self.open = Some((start, after)),
            (ConflictMarker::Separator, None) => self.separator = Some((start, after)),
            (ConflictMarker::Close, Some(separator)) => {
                self.open = None;
                self.separator = None;
                return Some(ConflictRegion {
                    range: open.0..start + line.len(),
                    this_device: open.1..separator.0,
                    other_device: separator.1..start,
                });
            }
            _ => {}
        }
        None
    }
}

/// Lines of `text` from `from`, as (start, line without terminator).
pub(crate) fn lines_from(text: &str, from: usize) -> impl Iterator<Item = (usize, &str)> {
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

#[derive(Default)]
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
    fn sync_conflicts_are_split_out() {
        let text = "a\n<<<<<<< this device\nmine\n=======\ntheirs\n>>>>>>> other device\nb";
        let segments = split(text);
        let open = text.find("<<<").unwrap();
        let close_end = text.find("device\nb").unwrap() + "device".len();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], Segment::Markdown(0..open));
        let Segment::Conflict(conflict) = &segments[1] else {
            panic!("the conflict is its own segment");
        };
        assert_eq!(conflict.range, open..close_end);
        assert_eq!(&text[conflict.this_device.clone()], "mine\n");
        assert_eq!(&text[conflict.other_device.clone()], "theirs\n");
        assert_eq!(segments[2], Segment::Markdown(close_end..text.len()));
    }

    #[test]
    fn incomplete_or_fenced_conflicts_stay_markdown() {
        let unclosed = "<<<<<<< this device\nmine\n=======\ntheirs\n";
        assert_eq!(split(unclosed), vec![Segment::Markdown(0..unclosed.len())]);
        let fenced = "```\n<<<<<<< a\nx\n=======\ny\n>>>>>>> b\n```\n";
        assert_eq!(split(fenced), vec![Segment::Markdown(0..fenced.len())]);
        let no_separator = "<<<<<<< a\nx\n>>>>>>> b\n";
        assert_eq!(split(no_separator).len(), 1);
    }

    #[test]
    fn markers_need_seven_symbols_and_a_label_after_a_space() {
        assert_eq!(
            conflict_marker("<<<<<<< this device"),
            Some(ConflictMarker::Open)
        );
        assert_eq!(conflict_marker(">>>>>>>"), Some(ConflictMarker::Close));
        assert_eq!(conflict_marker("======="), Some(ConflictMarker::Separator));
        assert_eq!(conflict_marker("========"), None);
        assert_eq!(conflict_marker("<<<<<<<<"), None);
        assert_eq!(conflict_marker("<<<<<<<x"), None);
    }

    #[test]
    fn frontmatter_needs_a_closing_fence() {
        assert_eq!(frontmatter_end("---\na: 1\n"), None);
        assert_eq!(frontmatter_end("---\n---\n"), Some(7));
    }
}
