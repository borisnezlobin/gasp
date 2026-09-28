//! Link cards: a web page shown as a card with its title, description and
//! image, stored the way the Link Embed plugin stores it, so notes stay
//! readable in Obsidian:
//!
//! ````text
//! ```embed
//! title: "The page's title"
//! image: "https://example.com/preview.png"
//! description: "What the page says about itself."
//! url: "https://example.com/page"
//! favicon: "https://example.com/favicon.ico"
//! ```
//! ````
//!
//! Only `url` is required. Values are double-quoted, with `\"` and `\\`
//! escaped.

pub mod meta;

use std::ops::Range;

/// The code block language Link Embed writes.
pub const LANGUAGE: &str = "embed";

/// Whether `text` is a web address a card can be made from.
pub fn is_web_url(text: &str) -> bool {
    ["http://", "https://"]
        .iter()
        .any(|scheme| text.starts_with(scheme) && text.len() > scheme.len())
        && !text.contains(char::is_whitespace)
}

/// The web address on the line of `text` holding `offset`, when the
/// address is all the line holds.
pub fn url_on_line(text: &str, offset: usize) -> Option<String> {
    let offset = text.floor_char_boundary(offset.min(text.len()));
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |at| offset + at);
    let address = text[start..end].trim();
    is_web_url(address).then(|| address.to_owned())
}

/// What turns the line holding `url` into its card: the range to replace,
/// its line break included, and the card's Markdown.
pub fn card_replacement(text: &str, url: &str, card: &LinkCard) -> Option<(Range<usize>, String)> {
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        if line.trim() == url {
            return Some((start..start + line.len(), card.to_markdown()));
        }
        start += line.len();
    }
    None
}

/// What a card shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkCard {
    pub url: String,
    pub title: String,
    pub description: String,
    pub image: Option<String>,
    pub favicon: Option<String>,
}

impl LinkCard {
    /// Reads the body of an `embed` block. `None` without a `url`.
    pub fn parse(body: &str) -> Option<LinkCard> {
        let mut card = LinkCard::default();
        for line in body.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = unquote(value.trim());
            match key.trim() {
                "url" => card.url = value,
                "title" => card.title = value,
                "description" => card.description = value,
                "image" => card.image = Some(value).filter(|v| !v.is_empty()),
                "favicon" => card.favicon = Some(value).filter(|v| !v.is_empty()),
                _ => {}
            }
        }
        (!card.url.is_empty()).then_some(card)
    }

    /// The whole block, fences included, ending in a line break.
    pub fn to_markdown(&self) -> String {
        let mut out = format!("```{LANGUAGE}\n");
        let fields = [
            ("title", Some(&self.title)),
            ("image", self.image.as_ref()),
            ("description", Some(&self.description)),
            ("url", Some(&self.url)),
            ("favicon", self.favicon.as_ref()),
        ];
        for (key, value) in fields {
            if let Some(value) = value.filter(|value| !value.is_empty() || key == "url") {
                out.push_str(&format!("{key}: {}\n", quote(value)));
            }
        }
        out.push_str("```\n");
        out
    }

    /// The page's host without `www.`, shown under the description.
    pub fn domain(&self) -> &str {
        let rest = self
            .url
            .split_once("://")
            .map_or(self.url.as_str(), |(_, rest)| rest);
        let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
        host.strip_prefix("www.").unwrap_or(host)
    }
}

/// `"text"` with quotes and backslashes escaped, on one line.
fn quote(value: &str) -> String {
    let flat = value.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("\"{}\"", flat.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The value inside quotes, unescaped; a bare value as it is.
fn unquote(value: &str) -> String {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return value.to_owned();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.extend(chars.next()),
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_link_embed_writes() {
        let body = "title: \"Rust \\\"book\\\"\"\nimage: \"https://x.org/a.png\"\n\
            description: \"Learn Rust.\"\nurl: \"https://www.x.org/book/\"\n";
        let card = LinkCard::parse(body).unwrap();
        assert_eq!(card.title, "Rust \"book\"");
        assert_eq!(card.image.as_deref(), Some("https://x.org/a.png"));
        assert_eq!(card.description, "Learn Rust.");
        assert_eq!(card.domain(), "x.org");
        assert_eq!(card.favicon, None);
    }

    #[test]
    fn a_url_is_required() {
        assert_eq!(LinkCard::parse("title: \"x\"\n"), None);
    }

    #[test]
    fn writes_a_block_that_reads_back() {
        let card = LinkCard {
            url: "https://x.org".into(),
            title: "A \"quoted\"\ntitle".into(),
            description: String::new(),
            image: None,
            favicon: Some("https://x.org/icon.png".into()),
        };
        let markdown = card.to_markdown();
        assert_eq!(
            markdown,
            "```embed\ntitle: \"A \\\"quoted\\\" title\"\nurl: \"https://x.org\"\n\
             favicon: \"https://x.org/icon.png\"\n```\n"
        );
        let body = markdown.lines().skip(1).collect::<Vec<_>>().join("\n");
        let read = LinkCard::parse(&body).unwrap();
        assert_eq!(read.title, "A \"quoted\" title");
        assert_eq!(read.favicon, card.favicon);
    }
}
