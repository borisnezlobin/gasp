//! Turning a note's matches into the lines a result shows: one hit per
//! line, each with an excerpt that leaves out markup and the ranges of
//! the matches in it.

use std::ops::Range;

/// Lines shown per note; the match count still covers every match.
pub const MAX_HITS_PER_NOTE: usize = 20;
/// Characters of context kept before a match in a long line.
const EXCERPT_LEAD: usize = 40;
/// Longest excerpt, in characters.
const EXCERPT_CHARS: usize = 160;
/// Inline markup an excerpt drops so it reads as text: emphasis, strike,
/// highlight and code marks. Table pipes become spaces.
const INLINE_MARKS: [&str; 5] = ["**", "__", "~~", "==", "`"];

/// A matching line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineHit {
    /// Zero-based line number.
    pub line: usize,
    /// Byte offset of the first match on the line, in the note.
    pub offset: usize,
    pub excerpt: String,
    /// Matches within `excerpt`.
    pub ranges: Vec<Range<usize>>,
}

/// Groups matches by line, one hit per line, up to [`MAX_HITS_PER_NOTE`].
pub fn line_hits(text: &str, matches: &[Range<usize>]) -> Vec<LineHit> {
    collect_hits(text, matches.iter().cloned()).0
}

/// The hits for `matches` (in order, not overlapping), and how many
/// matches there were. Matches past the last line shown are only counted.
pub(crate) fn collect_hits(
    text: &str,
    mut matches: impl Iterator<Item = Range<usize>>,
) -> (Vec<LineHit>, usize) {
    let mut lines = LineCursor::new(text);
    let mut hits: Vec<LineHit> = Vec::new();
    let mut spans: Vec<Range<usize>> = Vec::new();
    let mut count = 0;
    for range in matches.by_ref() {
        count += 1;
        lines.advance_to(range.start);
        let local = range.start - lines.start..range.end.min(lines.end) - lines.start;
        if let Some(hit) = hits.last_mut().filter(|hit| hit.line == lines.number) {
            hit.ranges.push(local);
            continue;
        }
        if hits.len() == MAX_HITS_PER_NOTE {
            break;
        }
        hits.push(LineHit {
            line: lines.number,
            offset: range.start,
            excerpt: String::new(),
            ranges: vec![local],
        });
        spans.push(lines.start..lines.end);
    }
    count += matches.count();
    let mut scratch = Vec::new();
    for (hit, span) in hits.iter_mut().zip(spans) {
        hit.excerpt = excerpt(&text[span], &mut hit.ranges, &mut scratch);
    }
    (hits, count)
}

/// The line a match is on, moving forward through a text.
struct LineCursor<'a> {
    text: &'a str,
    number: usize,
    start: usize,
    /// Where the line's `\n` is, or the text's end.
    end: usize,
}

impl<'a> LineCursor<'a> {
    fn new(text: &'a str) -> Self {
        let mut cursor = LineCursor {
            text,
            number: 0,
            start: 0,
            end: 0,
        };
        cursor.end = cursor.line_end();
        cursor
    }

    fn line_end(&self) -> usize {
        self.text[self.start..]
            .find('\n')
            .map_or(self.text.len(), |end| self.start + end)
    }

    /// Moves to the line holding byte `at`.
    fn advance_to(&mut self, at: usize) {
        while self.end < at {
            self.start = self.end + 1;
            self.number += 1;
            self.end = self.line_end();
        }
    }
}

/// A line's excerpt: shortened around its first match, without the markup
/// that starts it or inline marks, with `ranges` (matches in the line)
/// moved to where they are in the excerpt. `scratch` is reused between
/// excerpts.
fn excerpt(line: &str, ranges: &mut Vec<Range<usize>>, scratch: &mut Vec<usize>) -> String {
    let line = line.trim_end_matches('\r');
    let window = Window::around(line, ranges[0].start);
    let shift = |offset: usize| offset.clamp(window.start, window.end) - window.start;
    let prefix = window.prefix();
    for range in ranges.iter_mut() {
        *range = shift(range.start) + prefix.len()..shift(range.end) + prefix.len();
    }
    ranges.retain(|range| !range.is_empty());
    let pieces = [prefix, &line[window.start..window.end], window.suffix(line)];
    strip_inline_markup(&pieces, ranges, scratch)
}

/// The part of a long line an excerpt keeps: from a little before the
/// first match, or from after the line's markup, for at most
/// [`EXCERPT_CHARS`] characters.
struct Window {
    start: usize,
    end: usize,
    /// Where the line's text starts after its indent and markup.
    body: usize,
}

impl Window {
    fn around(line: &str, first_match: usize) -> Window {
        let first = first_match.min(line.len());
        let indent = line.len() - line.trim_start().len();
        let lead = chars_back(&line[..first], EXCERPT_LEAD);
        let body = indent + markup_prefix_len(&line[indent..]);
        let start = lead.max(body).min(first);
        let end = start + chars_forward(&line[start..], EXCERPT_CHARS);
        Window { start, end, body }
    }

    fn prefix(&self) -> &'static str {
        if self.start > self.body { "…" } else { "" }
    }

    fn suffix(&self, line: &str) -> &'static str {
        if self.end < line.len() { "…" } else { "" }
    }
}

/// Where the `count`th character from the end of `text` starts, or 0 when
/// it has fewer.
fn chars_back(text: &str, count: usize) -> usize {
    let tail = text.len().saturating_sub(count);
    if text.as_bytes()[tail..].is_ascii() {
        return if text.len() > count { tail } else { 0 };
    }
    text.char_indices()
        .rev()
        .nth(count - 1)
        .map_or(0, |(index, _)| index)
}

/// The length of the first `count` characters of `text`, or all of it.
fn chars_forward(text: &str, count: usize) -> usize {
    let head = count.min(text.len());
    if text.as_bytes()[..head].is_ascii() {
        return head;
    }
    text.char_indices()
        .nth(count)
        .map_or(text.len(), |(index, _)| index)
}

/// The markup that starts a line, such as `- [x] `, `## ` or `> `, which
/// an excerpt leaves out so it reads as text.
pub(crate) fn markup_prefix_len(line: &str) -> usize {
    let mut rest = line;
    loop {
        let next = strip_one_marker(rest);
        if next.len() == rest.len() {
            return line.len() - rest.len();
        }
        rest = next;
    }
}

fn strip_one_marker(text: &str) -> &str {
    const MARKERS: [&str; 7] = ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ ", "> "];
    if let Some(marker) = MARKERS.iter().find(|marker| text.starts_with(*marker)) {
        return &text[marker.len()..];
    }
    let hashes = text.len() - text.trim_start_matches('#').len();
    if (1..=6).contains(&hashes) && text[hashes..].starts_with(' ') {
        return &text[hashes + 1..];
    }
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 && text[digits..].starts_with(". ") {
        return &text[digits + 2..];
    }
    text
}

/// The excerpt the `pieces` spell out, without inline markup, and with
/// `ranges` (sorted, apart, in the pieces joined) moved along with the
/// text. A mark inside a match stays, so the match still reads.
fn strip_inline_markup(
    pieces: &[&str],
    ranges: &mut Vec<Range<usize>>,
    moved: &mut Vec<usize>,
) -> String {
    let mut out = String::with_capacity(pieces.iter().map(|piece| piece.len()).sum());
    let mut edges = RangeEdges::new(ranges, moved);
    let mut base = 0;
    for piece in pieces {
        copy_piece(piece, base, &mut out, &mut edges);
        base += piece.len();
    }
    edges.land(usize::MAX, out.len());
    for (range, landed) in ranges.iter_mut().zip(moved.chunks(2)) {
        *range = landed[0]..landed[1];
    }
    ranges.retain(|range| !range.is_empty());
    out
}

/// Copies `piece`, which starts at byte `base` of the excerpt, into `out`
/// without its inline marks. Runs of plain ASCII, which most of a note
/// is, are copied whole.
fn copy_piece(piece: &str, base: usize, out: &mut String, edges: &mut RangeEdges) {
    let bytes = piece.as_bytes();
    let mut at = 0;
    while at < piece.len() {
        let space_kept = !out.is_empty() && !out.ends_with(' ');
        let plain = plain_run(&bytes[at..], space_kept);
        if plain > 0 {
            edges.land_run(base + at..base + at + plain, out.len());
            out.push_str(&piece[at..at + plain]);
            at += plain;
            continue;
        }
        let start = out.len();
        let rest = &piece[at..];
        let dropped = INLINE_MARKS
            .iter()
            .find(|mark| rest.starts_with(*mark) && !edges.inside(base + at));
        let taken = match dropped {
            Some(mark) => mark.len(),
            None => push_char(rest, out),
        };
        edges.land(base + at + taken, start);
        at += taken;
    }
}

/// How many of `bytes` an excerpt keeps as they are: ASCII other than
/// blanks and the marks' characters, and single spaces after them.
/// `space_kept` says whether a space at the start would be kept.
fn plain_run(bytes: &[u8], mut space_kept: bool) -> usize {
    let mut length = 0;
    for &byte in bytes {
        if byte == b' ' && space_kept {
            space_kept = false;
        } else if is_plain(byte) {
            space_kept = true;
        } else {
            break;
        }
        length += 1;
    }
    length
}

/// An ASCII byte an excerpt keeps as it is: not a blank, a pipe or the
/// start of an inline mark.
fn is_plain(byte: u8) -> bool {
    byte.is_ascii() && !byte.is_ascii_whitespace() && !b"|*_~=`\x0b".contains(&byte)
}

/// The ends of sorted, separate ranges, visited in order as the text
/// they're in is rewritten, each landing where its byte went.
struct RangeEdges<'a> {
    ranges: &'a [Range<usize>],
    /// Where each edge landed, starts and ends in turn.
    moved: &'a mut Vec<usize>,
    /// The first range that doesn't end before the text read so far.
    current: usize,
}

impl<'a> RangeEdges<'a> {
    fn new(ranges: &'a [Range<usize>], moved: &'a mut Vec<usize>) -> Self {
        moved.clear();
        RangeEdges {
            ranges,
            moved,
            current: 0,
        }
    }

    /// Whether byte `at` of the text is inside a range, `at` only growing.
    fn inside(&mut self, at: usize) -> bool {
        while self
            .ranges
            .get(self.current)
            .is_some_and(|range| range.end <= at)
        {
            self.current += 1;
        }
        self.ranges
            .get(self.current)
            .is_some_and(|range| range.start <= at)
    }

    fn edge(&self, index: usize) -> Option<usize> {
        let range = self.ranges.get(index / 2)?;
        Some(if index.is_multiple_of(2) {
            range.start
        } else {
            range.end
        })
    }

    /// Every edge before byte `before` lands at `at` in the new text.
    fn land(&mut self, before: usize, at: usize) {
        while self
            .edge(self.moved.len())
            .is_some_and(|edge| edge < before)
        {
            self.moved.push(at);
        }
    }

    /// The edges in `run`, copied as it is to `at` in the new text, land
    /// where their bytes went.
    fn land_run(&mut self, run: Range<usize>, at: usize) {
        while let Some(edge) = self.edge(self.moved.len()).filter(|edge| *edge < run.end) {
            self.moved.push(at + edge - run.start);
        }
    }
}

/// Copies the char at the start of `rest` and returns its length in
/// bytes. A pipe counts as a space, and a space never follows another or
/// starts the excerpt.
fn push_char(rest: &str, out: &mut String) -> usize {
    let ch = rest.chars().next().unwrap_or(' ');
    let blank = ch == '|' || ch.is_whitespace();
    if !blank {
        out.push(ch);
    } else if !out.is_empty() && !out.ends_with(' ') {
        out.push(' ');
    }
    ch.len_utf8()
}
