//! Turn Markdown into footnote references, definitions and `^[n]` typos, with
//! byte offsets so callers can edit precisely.

use std::ops::Range;

use super::mask::{line_ranges, mask_code_and_math, strip_line_prefix};

/// A `[^label]` reference in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteRef {
    pub label: String,
    /// From the `[` to just past the `]`.
    pub range: Range<usize>,
}

/// A `[^label]: body` definition block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteDef {
    pub label: String,
    /// Start of the definition's first line.
    pub line_start: usize,
    /// Just past the last character of the block (no trailing line break).
    pub end: usize,
    /// Leading whitespace and callout markers before `[^label]:`.
    pub indent: String,
    /// Everything after `[^label]:`, including continuation lines.
    pub body: String,
}

impl FootnoteDef {
    /// Range of the `[^label]:` head.
    pub fn head_range(&self) -> Range<usize> {
        let start = self.line_start + self.indent.len();
        start..start + self.label.len() + 4
    }
}

/// A `^[digits]` typo that was probably meant to be `[^digits]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineTypo {
    pub label: String,
    /// From the `^` to just past the `]`.
    pub range: Range<usize>,
}

/// Every footnote reference, definition and typo in a document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedFootnotes {
    pub refs: Vec<FootnoteRef>,
    pub defs: Vec<FootnoteDef>,
    pub inline_typos: Vec<InlineTypo>,
}

/// True when the label is all ASCII digits.
pub fn is_numeric(label: &str) -> bool {
    !label.is_empty() && label.bytes().all(|b| b.is_ascii_digit())
}

/// Parses footnotes, ignoring anything inside code or math.
pub fn parse_footnotes(text: &str) -> ParsedFootnotes {
    let masked = mask_code_and_math(text);
    ParsedFootnotes {
        refs: find_refs(&masked),
        defs: find_defs(text, &masked),
        inline_typos: find_inline_typos(&masked),
    }
}

/// The largest numeric label among references and definitions, or 0.
pub fn max_numeric_label(parsed: &ParsedFootnotes) -> u64 {
    let refs = parsed.refs.iter().map(|r| r.label.as_str());
    let defs = parsed.defs.iter().map(|d| d.label.as_str());
    refs.chain(defs)
        .filter(|label| is_numeric(label))
        .map(|label| label.parse::<u64>().unwrap_or(u64::MAX - 1))
        .max()
        .unwrap_or(0)
}

/// The first definition with this label.
pub fn find_def<'a>(defs: &'a [FootnoteDef], label: &str) -> Option<&'a FootnoteDef> {
    defs.iter().find(|d| d.label == label)
}

/// The first reference with this label.
pub fn first_ref<'a>(refs: &'a [FootnoteRef], label: &str) -> Option<&'a FootnoteRef> {
    refs.iter().find(|r| r.label == label)
}

/// Length of a footnote label starting at `start`: bytes up to `]`, with no
/// brackets or line breaks. Returns `None` if there is no closing `]`.
fn label_len(bytes: &[u8], start: usize) -> Option<usize> {
    let len = bytes[start..]
        .iter()
        .take_while(|&&b| !matches!(b, b'[' | b']' | b'\n'))
        .count();
    let closed = bytes.get(start + len) == Some(&b']');
    (len > 0 && closed).then_some(len)
}

fn ref_at(masked: &str, start: usize) -> Option<FootnoteRef> {
    let bytes = masked.as_bytes();
    if !bytes[start..].starts_with(b"[^") {
        return None;
    }
    let len = label_len(bytes, start + 2)?;
    let end = start + 2 + len + 1;
    if bytes.get(end) == Some(&b':') {
        return None;
    }
    Some(FootnoteRef {
        label: masked[start + 2..end - 1].to_string(),
        range: start..end,
    })
}

fn find_refs(masked: &str) -> Vec<FootnoteRef> {
    let mut refs = Vec::new();
    let mut index = 0;
    while index < masked.len() {
        match ref_at(masked, index) {
            Some(found) => {
                index = found.range.end;
                refs.push(found);
            }
            None => index += 1,
        }
    }
    refs
}

fn typo_at(masked: &str, start: usize) -> Option<InlineTypo> {
    let bytes = masked.as_bytes();
    if !bytes[start..].starts_with(b"^[") {
        return None;
    }
    let digits = bytes[start + 2..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    let end = start + 2 + digits;
    if digits == 0 || bytes.get(end) != Some(&b']') {
        return None;
    }
    Some(InlineTypo {
        label: masked[start + 2..end].to_string(),
        range: start..end + 1,
    })
}

fn find_inline_typos(masked: &str) -> Vec<InlineTypo> {
    let mut typos = Vec::new();
    let mut index = 0;
    while index < masked.len() {
        match typo_at(masked, index) {
            Some(found) => {
                index = found.range.end;
                typos.push(found);
            }
            None => index += 1,
        }
    }
    typos
}

/// A definition head on one line: prefix length, label and head length.
pub(crate) struct DefHead {
    pub prefix_len: usize,
    pub label: String,
    pub head_len: usize,
}

/// Recognises `[^label]:` at the start of a line, after whitespace or
/// callout markers.
pub(crate) fn def_head(line: &str) -> Option<DefHead> {
    let bytes = line.as_bytes();
    let prefix_len = bytes.len() - strip_line_prefix(bytes).len();
    if !bytes[prefix_len..].starts_with(b"[^") {
        return None;
    }
    let len = label_len(bytes, prefix_len + 2)?;
    let colon = prefix_len + 2 + len + 1;
    if bytes.get(colon) != Some(&b':') {
        return None;
    }
    Some(DefHead {
        prefix_len,
        label: line[prefix_len + 2..colon - 1].to_string(),
        head_len: len + 4,
    })
}

fn is_blank(line: &str) -> bool {
    strip_line_prefix(line.as_bytes())
        .iter()
        .all(|b| b.is_ascii_whitespace())
}

fn is_indented_continuation(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with("    ")
}

struct Lines<'a> {
    masked: &'a str,
    ranges: Vec<Range<usize>>,
}

impl Lines<'_> {
    fn masked(&self, index: usize) -> &str {
        &self.masked[self.ranges[index].clone()]
    }

    /// Continues a definition over non-blank lines until the next head.
    fn skip_lazy_lines(&self, mut index: usize) -> usize {
        while index < self.ranges.len() {
            let line = self.masked(index);
            if is_blank(line) || def_head(line).is_some() {
                break;
            }
            index += 1;
        }
        index
    }

    fn first_non_blank(&self, mut index: usize) -> usize {
        while index < self.ranges.len() && is_blank(self.masked(index)) {
            index += 1;
        }
        index
    }

    /// One past the last line of the definition starting at `head`. A blank
    /// line ends it unless the next paragraph is indented, which Markdown
    /// treats as another paragraph of the same footnote.
    fn block_end(&self, head: usize) -> usize {
        let mut end = head + 1;
        loop {
            end = self.skip_lazy_lines(end);
            let next = self.first_non_blank(end);
            if next == end || next >= self.ranges.len() {
                return end;
            }
            let line = self.masked(next);
            if !is_indented_continuation(line) || def_head(line).is_some() {
                return end;
            }
            end = next;
        }
    }
}

fn line_content_end(text: &str, range: &Range<usize>) -> usize {
    if text[range.clone()].ends_with('\r') {
        range.end - 1
    } else {
        range.end
    }
}

fn find_defs(text: &str, masked: &str) -> Vec<FootnoteDef> {
    let lines = Lines {
        masked,
        ranges: line_ranges(text),
    };
    let mut defs = Vec::new();
    let mut index = 0;
    while index < lines.ranges.len() {
        let Some(head) = def_head(lines.masked(index)) else {
            index += 1;
            continue;
        };
        let last = lines.block_end(index) - 1;
        let line_start = lines.ranges[index].start;
        let end = line_content_end(text, &lines.ranges[last]).max(line_start + head.prefix_len);
        let body_start = (line_start + head.prefix_len + head.head_len).min(end);
        defs.push(FootnoteDef {
            label: head.label,
            line_start,
            end,
            indent: text[line_start..line_start + head.prefix_len].to_string(),
            body: text[body_start..end].to_string(),
        });
        index = last + 1;
    }
    defs
}
