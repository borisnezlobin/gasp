//! A small line styler for the spike: headings, strong, emphasis, inline
//! code and images. Phase 2 replaces it with the core render planner.

use std::ops::Range;

/// What an inline span means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpanKind {
    Strong,
    Emphasis,
    Code,
    Image { target: String },
}

/// An inline span. Offsets are bytes within the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// The whole span, markers included.
    pub range: Range<usize>,
    /// The part between the markers.
    pub content: Range<usize>,
    pub kind: SpanKind,
}

impl Span {
    pub fn is_image(&self) -> bool {
        matches!(self.kind, SpanKind::Image { .. })
    }
}

/// How one source line is styled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineStyle {
    /// 1 to 6 for headings, 0 for body text.
    pub heading_level: u8,
    /// The `#` markers and the space after them.
    pub heading_marker: Option<Range<usize>>,
    pub spans: Vec<Span>,
}

impl LineStyle {
    pub fn has_image(&self) -> bool {
        self.spans.iter().any(Span::is_image)
    }

    /// The span covering `offset`, if any.
    pub fn span_at(&self, offset: usize) -> Option<&Span> {
        self.spans.iter().find(|span| span.range.contains(&offset))
    }

    /// Whether `offset` is a marker (a heading `#` or a span delimiter).
    pub fn is_marker(&self, offset: usize) -> bool {
        if self
            .heading_marker
            .as_ref()
            .is_some_and(|marker| marker.contains(&offset))
        {
            return true;
        }
        self.span_at(offset)
            .is_some_and(|span| !span.content.contains(&offset))
    }
}

/// Styles one line of Markdown.
pub fn style_line(text: &str) -> LineStyle {
    let (heading_level, heading_marker) = heading(text);
    let body_start = heading_marker.as_ref().map_or(0, |marker| marker.end);
    LineStyle {
        heading_level,
        heading_marker,
        spans: scan_spans(text, body_start),
    }
}

/// Heading level from the line's `#` prefix, without styling the rest.
pub fn heading_level(text: &str) -> u8 {
    heading(text).0
}

/// Whether the line has an image, without building the rest of the style.
pub fn line_has_image(text: &str) -> bool {
    text.contains("![") && style_line(text).has_image()
}

fn heading(text: &str) -> (u8, Option<Range<usize>>) {
    let hashes = text.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&hashes) || text.as_bytes().get(hashes) != Some(&b' ') {
        return (0, None);
    }
    (hashes as u8, Some(0..hashes + 1))
}

type Matcher = fn(&str, usize) -> Option<Span>;

const MATCHERS: &[Matcher] = &[
    wiki_image,
    markdown_image,
    inline_code,
    strong,
    emphasis_star,
    emphasis_underscore,
];

fn scan_spans(text: &str, from: usize) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut at = from;
    while at < text.len() {
        if let Some(span) = MATCHERS.iter().find_map(|matcher| matcher(text, at)) {
            at = span.range.end;
            spans.push(span);
            continue;
        }
        at += text[at..].chars().next().map_or(1, char::len_utf8);
    }
    spans
}

/// Finds `open … close` starting at `at`, with non-empty content that
/// doesn't start with a space.
fn delimited(
    text: &str,
    at: usize,
    open: &str,
    close: &str,
) -> Option<(Range<usize>, Range<usize>)> {
    if !text[at..].starts_with(open) {
        return None;
    }
    let content_start = at + open.len();
    let content_len = text[content_start..].find(close)?;
    let starts_with_space = text[content_start..].starts_with(' ');
    if content_len == 0 || starts_with_space {
        return None;
    }
    let content = content_start..content_start + content_len;
    Some((at..content.end + close.len(), content))
}

fn simple(text: &str, at: usize, marker: &str, kind: SpanKind) -> Option<Span> {
    let (range, content) = delimited(text, at, marker, marker)?;
    Some(Span {
        range,
        content,
        kind,
    })
}

fn inline_code(text: &str, at: usize) -> Option<Span> {
    simple(text, at, "`", SpanKind::Code)
}

fn strong(text: &str, at: usize) -> Option<Span> {
    simple(text, at, "**", SpanKind::Strong)
}

fn emphasis_star(text: &str, at: usize) -> Option<Span> {
    simple(text, at, "*", SpanKind::Emphasis)
}

fn emphasis_underscore(text: &str, at: usize) -> Option<Span> {
    simple(text, at, "_", SpanKind::Emphasis)
}

fn wiki_image(text: &str, at: usize) -> Option<Span> {
    let (range, content) = delimited(text, at, "![[", "]]")?;
    let inner = &text[content.clone()];
    let target = inner.split('|').next().unwrap_or(inner).trim().to_owned();
    Some(Span {
        range,
        content,
        kind: SpanKind::Image { target },
    })
}

fn markdown_image(text: &str, at: usize) -> Option<Span> {
    let (alt_range, _) = delimited(text, at, "![", "]").or_else(|| empty_alt(text, at))?;
    let (target_range, target) = delimited(text, alt_range.end, "(", ")")?;
    Some(Span {
        range: at..target_range.end,
        content: target.clone(),
        kind: SpanKind::Image {
            target: text[target].trim().to_owned(),
        },
    })
}

fn empty_alt(text: &str, at: usize) -> Option<(Range<usize>, Range<usize>)> {
    text[at..]
        .starts_with("![]")
        .then(|| (at..at + 3, at + 2..at + 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(String, SpanKind)> {
        style_line(text)
            .spans
            .into_iter()
            .map(|span| (text[span.range].to_owned(), span.kind))
            .collect()
    }

    #[test]
    fn headings_have_a_level_and_a_marker() {
        let style = style_line("## Kinematics");
        assert_eq!(style.heading_level, 2);
        assert_eq!(style.heading_marker, Some(0..3));
        assert!(style.is_marker(0));
        assert!(!style.is_marker(3));
    }

    #[test]
    fn hashes_without_a_space_are_not_a_heading() {
        assert_eq!(heading_level("#tag"), 0);
        assert_eq!(heading_level("####### seven"), 0);
        assert_eq!(heading_level("###### six"), 6);
    }

    #[test]
    fn finds_strong_emphasis_and_code() {
        assert_eq!(
            kinds("a **bold** and *it* or _it_ with `code`"),
            vec![
                ("**bold**".to_owned(), SpanKind::Strong),
                ("*it*".to_owned(), SpanKind::Emphasis),
                ("_it_".to_owned(), SpanKind::Emphasis),
                ("`code`".to_owned(), SpanKind::Code),
            ]
        );
    }

    #[test]
    fn unclosed_or_spaced_markers_are_plain() {
        assert!(kinds("2 * 3 = 6 and a ** b").is_empty());
        assert!(kinds("a `b").is_empty());
    }

    #[test]
    fn finds_wiki_and_markdown_images() {
        let image = |target: &str| SpanKind::Image {
            target: target.to_owned(),
        };
        assert_eq!(
            kinds("see ![[plot.png|200]] and ![alt](images/a.png) or ![](b.png)"),
            vec![
                ("![[plot.png|200]]".to_owned(), image("plot.png")),
                ("![alt](images/a.png)".to_owned(), image("images/a.png")),
                ("![](b.png)".to_owned(), image("b.png")),
            ]
        );
        assert!(line_has_image("x ![[a.png]] y"));
        assert!(!line_has_image("x ![ y"));
    }

    #[test]
    fn markers_are_offsets_outside_the_content() {
        let style = style_line("x **b** y");
        assert!(style.is_marker(2));
        assert!(style.is_marker(3));
        assert!(!style.is_marker(4));
        assert!(style.is_marker(6));
        assert!(!style.is_marker(8));
    }

    #[test]
    fn handles_multibyte_text() {
        assert_eq!(
            kinds("日本語 **太字** です"),
            vec![("**太字**".to_owned(), SpanKind::Strong)]
        );
    }
}
