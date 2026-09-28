//! Raw HTML in notes. The article keeps the handful of elements notes use
//! for meaning (bold, italic, strike-through, underline, super- and
//! subscripts, highlights, keys, links, styled spans and paragraphs,
//! alignment, images) with only the attributes that carry it, styles
//! rebuilt from the subset the editor draws; everything else is dropped
//! and its text kept, so an article never carries scripts, event
//! handlers, layout-breaking CSS or Obsidian's own markup.

use crate::pdf::convert::html::{
    Token, attribute, element_style, is_page_break, safe_href, tokenize,
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

/// Elements kept only with a style, which is rebuilt from what the editor
/// understands of theirs, so no other CSS gets through.
const STYLED: [&str; 4] = ["span", "div", "p", "center"];

/// The name an element is written with, when it is kept.
fn kept_name(name: &str) -> Option<String> {
    match name {
        "strike" => Some("s".to_owned()),
        "a" => Some(name.to_owned()),
        _ if PLAIN.contains(&name) || STYLED.contains(&name) => Some(name.to_owned()),
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
    match name {
        "strike" => Some("<s>".to_owned()),
        "a" => {
            safe_href(attrs).map(|href| format!("<a href=\"{}\">", super::escape_attribute(&href)))
        }
        _ if STYLED.contains(&name) => styled_opener(name, attrs),
        _ if PLAIN.contains(&name) => Some(format!("<{name}>")),
        _ => None,
    }
}

/// `<span>`, `<div>`, `<p>` or `<center>` with only the styles the editor
/// draws. A span or div with none says nothing and is dropped.
fn styled_opener(name: &str, attrs: &str) -> Option<String> {
    let mut style = element_style(name, attrs);
    if name == "center" {
        style.align = None;
    }
    let css = style.to_css();
    match (css.is_empty(), name) {
        (true, "span" | "div") => None,
        (true, _) => Some(format!("<{name}>")),
        (false, _) => Some(format!(
            "<{name} style=\"{}\">",
            super::escape_attribute(&css)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_are_rebuilt_from_the_safe_subset() {
        let opened = |html: &str| match filter(html).into_iter().next() {
            Some(Piece::Open { markup, .. }) => Some(markup),
            _ => None,
        };
        assert_eq!(
            opened("<span style=\"color:red; position:fixed; onclick:x\" onmouseover=\"x()\">"),
            Some("<span style=\"color: #ff0000\">".into())
        );
        assert_eq!(
            opened("<p style=\"text-align: center; width: 9999px\">"),
            Some("<p style=\"text-align: center\">".into())
        );
        assert_eq!(opened("<center>"), Some("<center>".into()));
        assert_eq!(opened("<span style=\"display:none\">"), None);
        assert_eq!(opened("<a href=\"javascript://%0aalert(1)\">"), None);
    }

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
