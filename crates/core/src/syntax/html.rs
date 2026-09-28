//! Recognising raw HTML tags.

use std::ops::Range;

use super::kinds::{HtmlKind, HtmlStyle};

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
    let source = &rest[..end];
    Some(HtmlTag {
        kind: classify(&name, source, closing),
        name,
        closing,
        self_closing: source.ends_with("/>"),
        range: start..start + end,
    })
}

/// The element a tag named `name` opens or closes, with the style its
/// attributes ask for. A closing tag carries no style.
fn classify(name: &str, source: &str, closing: bool) -> HtmlKind {
    let kind = HtmlKind::from_tag_name(name);
    if closing || kind.style().is_none() {
        return kind;
    }
    let mut style = attribute(source, "style").map_or_else(HtmlStyle::default, HtmlStyle::parse);
    if style.align.is_none() && kind.is_block() {
        style.align = attribute(source, "align").and_then(super::css::parse_align);
    }
    kind.with_style(style)
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

/// The quote that closes a value opened with `open`: straight quotes close
/// themselves, and a curly double or single quote, as Smart Typography
/// writes them, closes with either of its pair.
fn closes(open: char, candidate: char) -> bool {
    match open {
        '“' | '”' => matches!(candidate, '“' | '”'),
        '‘' | '’' => matches!(candidate, '‘' | '’'),
        _ => candidate == open,
    }
}

fn is_quote(character: char) -> bool {
    matches!(character, '"' | '\'' | '“' | '”' | '‘' | '’')
}

/// Length of the tag at the start of `rest` up to and including `>`,
/// skipping `>` inside quoted attribute values.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut previous = '<';
    for (at, character) in rest.char_indices() {
        match quote {
            Some(open) if closes(open, character) => quote = None,
            Some(_) => {}
            None if is_quote(character) && previous == '=' => quote = Some(character),
            None if character == '>' => return Some(at + 1),
            None if character == '\n' && rest[at + 1..].starts_with('\n') => return None,
            None => {}
        }
        if !character.is_whitespace() {
            previous = character;
        }
    }
    None
}

/// The value of attribute `name` in the tag `source` (`<a href="…">`),
/// quoted with straight or curly quotes, or bare.
pub fn attribute<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let mut search = source.find(char::is_whitespace)?;
    while let Some(found) = find_ignoring_case(&source[search..], name) {
        let at = search + found;
        search = at + name.len();
        let preceded_by_space = source[..at].ends_with(char::is_whitespace);
        let Some(value) = source[search..]
            .trim_start()
            .strip_prefix('=')
            .filter(|_| preceded_by_space)
        else {
            continue;
        };
        return Some(attribute_value(value.trim_start()));
    }
    None
}

fn find_ignoring_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn attribute_value(text: &str) -> &str {
    let mut characters = text.chars();
    match characters.next() {
        Some(open) if is_quote(open) => {
            let rest = &text[open.len_utf8()..];
            let end = rest
                .char_indices()
                .find(|(_, character)| closes(open, *character))
                .map_or(rest.len(), |(at, _)| at);
            &rest[..end]
        }
        _ => {
            let end = text
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(text.len());
            let value = &text[..end];
            match text[end..].starts_with('>') {
                true => value.strip_suffix('/').unwrap_or(value),
                false => value,
            }
        }
    }
}

/// Whether an `<a href>` may become a link: web and mail addresses and
/// relative paths, never `javascript:` or `data:` URLs.
pub fn is_safe_href(href: &str) -> bool {
    let href = href.trim();
    let scheme = href
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|scheme| !scheme.contains(['/', '?', '#']));
    !href.is_empty()
        && scheme.is_none_or(|scheme| {
            ["http", "https", "mailto", "obsidian"]
                .iter()
                .any(|safe| safe.eq_ignore_ascii_case(scheme))
        })
}

/// The curly quotes Smart Typography puts around attribute values.
pub(crate) const CURLY_QUOTES: [char; 4] = ['“', '”', '‘', '’'];

/// `text` with the curly quotes around attribute values in its HTML tags
/// made straight, `<span style=”color: red”>` becoming `<span
/// style="color: red">`, for exporters whose HTML readers only know
/// straight quotes. Quotes anywhere else are left alone.
pub fn straighten_tag_quotes(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains(CURLY_QUOTES) {
        return text.into();
    }
    let tree = super::parse(text);
    let mut tags: Vec<Range<usize>> = tree
        .nodes()
        .iter()
        .flat_map(|node| &node.markup)
        .filter(|markup| markup.kind == super::MarkupKind::HtmlTag)
        .map(|markup| markup.range.clone())
        .filter(|range| text[range.clone()].contains(CURLY_QUOTES))
        .collect();
    if tags.is_empty() {
        return text.into();
    }
    tags.sort_by_key(|range| range.start);
    tags.dedup();
    let mut straight = String::with_capacity(text.len());
    let mut at = 0;
    for tag in tags {
        if tag.start < at {
            continue;
        }
        straight.push_str(&text[at..tag.start]);
        straight.push_str(&straighten_tag(&text[tag.clone()]));
        at = tag.end;
    }
    straight.push_str(&text[at..]);
    straight.into()
}

/// One tag with the quotes around its attribute values made straight.
fn straighten_tag(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len());
    let mut open: Option<char> = None;
    let mut previous = '<';
    for character in tag.chars() {
        let straight = match open {
            Some(quote) if closes(quote, character) => {
                open = None;
                straight_quote(quote)
            }
            None if is_quote(character) && previous == '=' => {
                open = Some(character);
                straight_quote(character)
            }
            _ => character,
        };
        out.push(straight);
        if !character.is_whitespace() {
            previous = character;
        }
    }
    out
}

fn straight_quote(quote: char) -> char {
    match quote {
        '‘' | '’' | '\'' => '\'',
        _ => '"',
    }
}

/// Other elements notes use, besides those [`HtmlKind`] names, whose
/// tags are recognised while they are still being typed.
const COMMON_ELEMENTS: [&str; 22] = [
    "table",
    "thead",
    "tbody",
    "tr",
    "td",
    "th",
    "font",
    "small",
    "big",
    "details",
    "summary",
    "blockquote",
    "ul",
    "ol",
    "li",
    "pre",
    "code",
    "abbr",
    "q",
    "cite",
    "figure",
    "iframe",
];

/// Whether `before`, the start of a line up to the cursor, ends inside an
/// opening tag still being typed, such as `<span style="`. The parser
/// can't see a tag until its `>` is typed, so typing replacements ask
/// this too and leave the tag's quotes and dashes alone.
pub fn in_unclosed_tag(before: &str) -> bool {
    let Some(open) = before.rfind('<') else {
        return false;
    };
    let rest = &before[open + 1..];
    let name_len = tag_name_len(rest);
    let name = &rest[..name_len];
    let known = HtmlKind::from_tag_name(name) != HtmlKind::Other
        || COMMON_ELEMENTS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(name));
    name_len > 0
        && known
        && rest[name_len..].starts_with(char::is_whitespace)
        && !rest.contains('>')
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
        assert_eq!(classify_tag("<span>"), HtmlKind::Span(HtmlStyle::NONE));
        assert_eq!(classify_tag("<STRONG>"), HtmlKind::Bold);
        assert_eq!(classify_tag("<table>"), HtmlKind::Other);
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

    #[test]
    fn attributes_take_straight_curly_or_no_quotes() {
        let tag = "<span style=”color: red;” title='a b' data-x=bare>";
        assert_eq!(attribute(tag, "style"), Some("color: red;"));
        assert_eq!(attribute(tag, "TITLE"), Some("a b"));
        assert_eq!(attribute(tag, "data-x"), Some("bare"));
        assert_eq!(attribute(tag, "x"), None);
        assert_eq!(attribute("<img src=a/b.png/>", "src"), Some("a/b.png"));
        assert_eq!(
            attribute("<a href=“https://x.org”>", "href"),
            Some("https://x.org")
        );
    }

    #[test]
    fn curly_quotes_hide_a_greater_than_sign() {
        let tag = tag_at("<span title=“a > b”>x", 0).unwrap();
        assert_eq!(tag.range.end, "<span title=“a > b”>".len());
    }

    #[test]
    fn styles_are_read_from_the_opening_tag() {
        let style = |source: &str| classify_tag(source).style().copied();
        let red = style("<span style=\"color:red;\">").and_then(|s| s.color);
        assert_eq!(red, super::super::css::parse_color("red"));
        assert_eq!(style("</span>"), Some(HtmlStyle::NONE));
        assert_eq!(
            style("<b style=\"color:red\">"),
            None,
            "only span, p, div and center"
        );
        let aligned = style("<p align=\"right\">").and_then(|s| s.align);
        assert_eq!(aligned, Some(super::super::kinds::Alignment::Right));
    }

    #[test]
    fn a_tag_being_typed_is_recognised() {
        assert!(in_unclosed_tag("see <span style=\""));
        assert!(in_unclosed_tag("<p style=\"a--"));
        assert!(in_unclosed_tag("x <table "));
        assert!(!in_unclosed_tag("<span style=\"x\">then \""));
        assert!(!in_unclosed_tag("a < b and \""));
        assert!(!in_unclosed_tag("x<y and \""));
        assert!(!in_unclosed_tag("<span"));
    }

    #[test]
    fn curly_attribute_quotes_are_straightened_for_export() {
        let text = "<span style=”color: red;”>a “quote”</span> and <b title=‘x’>“prose”</b>";
        assert_eq!(
            straighten_tag_quotes(text),
            "<span style=\"color: red;\">a “quote”</span> and <b title='x'>“prose”</b>"
        );
        assert!(matches!(
            straighten_tag_quotes("plain “text”"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn only_safe_links_are_followed() {
        for safe in [
            "https://x.org",
            "mailto:a@b.c",
            "Notes/Other.md",
            "#heading",
            "dir/a:b.md",
        ] {
            assert!(is_safe_href(safe), "{safe}");
        }
        for unsafe_href in [
            "javascript:alert(1)",
            " JavaScript:x",
            "data:text/html,x",
            "",
        ] {
            assert!(!is_safe_href(unsafe_href), "{unsafe_href}");
        }
    }
}
