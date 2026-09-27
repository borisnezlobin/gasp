//! Raw HTML in notes. The article keeps the handful of elements notes use
//! for meaning (underline, super- and subscripts, keys, colour, alignment,
//! images) with only the attributes that carry it; everything else is
//! dropped and its text kept, so an article never carries scripts, styles
//! or Obsidian's own markup.

use crate::pdf::convert::html::{
    Token, attribute, css_property, is_hex_color, is_page_break, tokenize,
};

/// Elements kept as they are, without attributes.
const PLAIN: [&str; 16] = [
    "u",
    "ins",
    "sup",
    "sub",
    "kbd",
    "mark",
    "b",
    "strong",
    "i",
    "em",
    "s",
    "del",
    "small",
    "blockquote",
    "details",
    "summary",
];

/// Elements whose content is not text and is dropped with them.
const HIDDEN: [&str; 4] = ["script", "style", "template", "iframe"];

/// A piece of filtered HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    /// Markup to write as it is.
    Markup(String),
    /// Text as written in the HTML source (entities and all).
    Text(String),
    /// An `<img>` whose source still has to be resolved.
    Image { src: String, width: Option<String> },
    /// A kept element opened: its tag and the markup that opens it.
    Open { tag: String, markup: String },
    /// The end of an element; written only if the element was kept and is
    /// still open.
    Close { tag: String },
    /// The start of an element whose content is dropped with it, such as a
    /// script. Its content can span several Markdown events.
    Hide(String),
    /// The end of a hidden element.
    Unhide(String),
}

/// The kept parts of a stretch of raw HTML.
pub(crate) fn filter(html: &str) -> Vec<Piece> {
    tokenize(html)
        .into_iter()
        .filter_map(|token| match token {
            Token::Text(text) => Some(Piece::Text(text.to_owned())),
            Token::Open { name, .. } if HIDDEN.contains(&name.as_str()) => Some(Piece::Hide(name)),
            Token::Close(name) if HIDDEN.contains(&name.as_str()) => Some(Piece::Unhide(name)),
            Token::Open {
                name,
                attrs,
                self_closing,
            } => open(&name, attrs, self_closing),
            Token::Close(name) => kept_name(&name).map(|tag| Piece::Close { tag }),
        })
        .collect()
}

/// The name an element is written with, when it is kept.
fn kept_name(name: &str) -> Option<String> {
    match name {
        "strike" => Some("s".to_owned()),
        "span" | "div" | "a" => Some(name.to_owned()),
        _ if PLAIN.contains(&name) => Some(name.to_owned()),
        _ => None,
    }
}

fn open(name: &str, attrs: &str, self_closing: bool) -> Option<Piece> {
    match name {
        "br" => Some(Piece::Markup("<br>".to_owned())),
        "hr" => Some(Piece::Markup("<hr>".to_owned())),
        "img" => attribute(attrs, "src").map(|src| Piece::Image {
            src,
            width: attribute(attrs, "width").map(|width| width.trim_end_matches("px").to_owned()),
        }),
        _ if is_page_break(attrs) || self_closing => None,
        _ => Some(Piece::Open {
            tag: kept_name(name)?,
            markup: opener(name, attrs)?,
        }),
    }
}

/// The opening tag written for element `name`, if it is kept.
fn opener(name: &str, attrs: &str) -> Option<String> {
    let style = attribute(attrs, "style").unwrap_or_default();
    match name {
        "strike" => Some("<s>".to_owned()),
        "span" => css_property(&style, "color")
            .filter(|color| is_hex_color(color))
            .map(|color| format!("<span style=\"color: {color}\">")),
        "div" => css_property(&style, "text-align")
            .or_else(|| attribute(attrs, "align"))
            .filter(|align| matches!(align.as_str(), "left" | "right" | "center"))
            .map(|align| format!("<div style=\"text-align: {align}\">")),
        "a" => attribute(attrs, "href")
            .filter(|href| href.contains("://") || href.starts_with("mailto:"))
            .map(|href| format!("<a href=\"{}\">", super::escape_attribute(&href))),
        _ if PLAIN.contains(&name) => Some(format!("<{name}>")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_hidden_elements_and_drops_unsafe_attributes() {
        let pieces =
            filter("<script>x</script><b onclick=\"x()\">b</b><a href=\"javascript:x()\">a</a>");
        assert_eq!(
            pieces,
            vec![
                Piece::Hide("script".into()),
                Piece::Text("x".into()),
                Piece::Unhide("script".into()),
                Piece::Open {
                    tag: "b".into(),
                    markup: "<b>".into()
                },
                Piece::Text("b".into()),
                Piece::Close { tag: "b".into() },
                Piece::Text("a".into()),
                Piece::Close { tag: "a".into() },
            ]
        );
    }
}
