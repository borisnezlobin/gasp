//! Obsidian callout headers: `> [!type]± Optional title`.

use pulldown_cmark::{Event, TagEnd};

/// A parsed callout header line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CalloutHeader {
    /// The callout type, lowercased.
    pub kind: String,
    /// Text after the marker on the header line (may be empty).
    pub rest_offset: usize,
}

/// Parses the start of the first line of a block quote.
pub(crate) fn parse_header(text: &str) -> Option<CalloutHeader> {
    let inner = text.strip_prefix("[!")?;
    let close = inner.find(']')?;
    let kind = &inner[..close];
    if kind.is_empty() || kind.contains(char::is_whitespace) {
        return None;
    }
    let mut offset = 2 + close + 1;
    if text[offset..].starts_with(['+', '-']) {
        offset += 1;
    }
    offset += text[offset..].len() - text[offset..].trim_start().len();
    Some(CalloutHeader {
        kind: kind.to_lowercase(),
        rest_offset: offset,
    })
}

/// The title Obsidian shows when a callout has none: the type with its first
/// letter capitalised.
pub(crate) fn default_title(kind: &str) -> String {
    let mut characters = kind.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

fn is_line_end(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph)
    )
}

/// Index of the event ending the first line of the paragraph whose inline
/// content starts at `from`: the end of a callout's title.
pub(crate) fn header_line_end(events: &[Event<'_>], from: usize) -> usize {
    let mut depth = 0usize;
    for (index, event) in events.iter().enumerate().skip(from) {
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) if depth > 0 => depth -= 1,
            event if depth == 0 && is_line_end(event) => return index,
            _ => {}
        }
    }
    events.len()
}

/// Whether the events in `from..to` are only whitespace.
pub(crate) fn is_blank_span(events: &[Event<'_>], from: usize, to: usize) -> bool {
    events[from..to.min(events.len())]
        .iter()
        .all(|event| matches!(event, Event::Text(text) if text.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_header() {
        let header = parse_header("[!NOTE] Title here").unwrap();
        assert_eq!(header.kind, "note");
        assert_eq!(&"[!NOTE] Title here"[header.rest_offset..], "Title here");
    }

    #[test]
    fn parses_fold_marker() {
        let text = "[!tip]- Folded";
        let header = parse_header(text).unwrap();
        assert_eq!(header.kind, "tip");
        assert_eq!(&text[header.rest_offset..], "Folded");
    }

    #[test]
    fn rejects_plain_quotes() {
        assert_eq!(parse_header("Just a quote"), None);
        assert_eq!(parse_header("[!] empty"), None);
        assert_eq!(parse_header("[link] text"), None);
    }

    #[test]
    fn capitalises_default_title() {
        assert_eq!(default_title("tldr"), "Tldr");
        assert_eq!(default_title("faq"), "Faq");
    }
}
