//! Recognising raw HTML tags.

use std::ops::Range;

use super::kinds::HtmlKind;

/// A single HTML tag found in source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlTag {
    pub range: Range<usize>,
    pub name: String,
    pub closing: bool,
    pub self_closing: bool,
    pub kind: HtmlKind,
}

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

impl HtmlTag {
    /// Whether this tag opens an element that needs a matching closing tag.
    pub fn opens_element(&self) -> bool {
        !self.closing
            && !self.self_closing
            && self.kind != HtmlKind::Comment
            && !VOID_ELEMENTS.contains(&self.name.as_str())
    }
}

/// Parses the tag starting at `start`, if there is one.
pub(crate) fn tag_at(text: &str, start: usize) -> Option<HtmlTag> {
    let rest = &text[start..];
    if rest.starts_with("<!--") {
        let end = rest.find("-->")? + 3;
        return Some(comment_tag(start..start + end));
    }
    let after_open = rest.strip_prefix('<')?;
    let closing = after_open.starts_with('/');
    let name_start = usize::from(closing);
    let name_len = tag_name_len(&after_open[name_start..]);
    if name_len == 0 {
        return None;
    }
    let name = after_open[name_start..name_start + name_len].to_ascii_lowercase();
    let end = tag_end(rest)?;
    Some(HtmlTag {
        kind: HtmlKind::from_tag_name(&name),
        name,
        closing,
        self_closing: rest[..end].ends_with("/>"),
        range: start..start + end,
    })
}

fn comment_tag(range: Range<usize>) -> HtmlTag {
    HtmlTag {
        range,
        name: String::new(),
        closing: false,
        self_closing: true,
        kind: HtmlKind::Comment,
    }
}

fn tag_name_len(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return 0;
    }
    bytes
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
        .count()
}

/// Length of the tag at the start of `rest` up to and including `>`,
/// skipping `>` inside quoted attribute values.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (at, byte) in rest.bytes().enumerate() {
        match (quote, byte) {
            (Some(open), _) if byte == open => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(byte),
            (None, b'>') => return Some(at + 1),
            (None, b'\n') if rest[at + 1..].starts_with('\n') => return None,
            _ => {}
        }
    }
    None
}

/// Every tag in `range` of `text`, in order.
pub(crate) fn scan_tags(text: &str, range: Range<usize>) -> Vec<HtmlTag> {
    let mut tags = Vec::new();
    let mut at = range.start;
    while let Some(found) = text[at..range.end].find('<') {
        let start = at + found;
        match tag_at(&text[..range.end], start) {
            Some(tag) => {
                at = tag.range.end;
                tags.push(tag);
            }
            None => at = start + 1,
        }
    }
    tags
}

/// Classifies a single inline HTML tag.
pub(crate) fn classify_tag(source: &str) -> HtmlKind {
    tag_at(source, 0).map_or(HtmlKind::Other, |tag| tag.kind)
}

/// Classifies an HTML block by its first tag.
pub(crate) fn classify_block(source: &str) -> HtmlKind {
    let start = source.len() - source.trim_start().len();
    tag_at(source, start).map_or(HtmlKind::Other, |tag| tag.kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_classified_by_name() {
        assert_eq!(classify_tag("<br>"), HtmlKind::LineBreak);
        assert_eq!(classify_tag("<BR/>"), HtmlKind::LineBreak);
        assert_eq!(classify_tag("<hr />"), HtmlKind::HorizontalRule);
        assert_eq!(classify_tag("</u>"), HtmlKind::Underline);
        assert_eq!(classify_tag("<img src=\"a>b.png\">"), HtmlKind::Image);
        assert_eq!(classify_tag("<span>"), HtmlKind::Other);
        assert_eq!(classify_tag("<!-- x -->"), HtmlKind::Comment);
    }

    #[test]
    fn scanning_finds_every_tag() {
        let text = "<div align=\"c\">\n**x** < 3\n</div>";
        let tags = scan_tags(text, 0..text.len());
        let names: Vec<_> = tags.iter().map(|t| (t.name.as_str(), t.closing)).collect();
        assert_eq!(names, vec![("div", false), ("div", true)]);
        assert_eq!(tags[1].range, 26..32);
    }

    #[test]
    fn void_and_self_closing_tags_do_not_open_elements() {
        assert!(!tag_at("<br>", 0).unwrap().opens_element());
        assert!(!tag_at("<x/>", 0).unwrap().opens_element());
        assert!(tag_at("<u>", 0).unwrap().opens_element());
    }
}
