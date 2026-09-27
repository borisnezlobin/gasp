//! The raw HTML found in notes: line breaks, rules, underline, super- and
//! subscripts, keys, coloured spans, aligned divs, images and page breaks.
//! Other tags are dropped and their text kept.

use super::{Converter, Open, OpenKind};
use crate::pdf::escape;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token<'s> {
    Open {
        name: String,
        attrs: &'s str,
        self_closing: bool,
    },
    Close(String),
    Text(&'s str),
}

pub(crate) fn tokenize(html: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        let Some(start) = rest.find('<') else {
            tokens.push(Token::Text(rest));
            break;
        };
        if start > 0 {
            tokens.push(Token::Text(&rest[..start]));
        }
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        match rest
            .find('>')
            .and_then(|end| tag_token(&rest[1..end]).map(|token| (end, token)))
        {
            Some((end, token)) => {
                tokens.push(token);
                rest = &rest[end + 1..];
            }
            None => {
                tokens.push(Token::Text("<"));
                rest = &rest[1..];
            }
        }
    }
    tokens
}

fn tag_token(inner: &str) -> Option<Token<'_>> {
    if let Some(name) = inner.strip_prefix('/') {
        return Some(Token::Close(name.trim().to_ascii_lowercase()));
    }
    let self_closing = inner.ends_with('/');
    let inner = inner.trim_end_matches('/');
    let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
    let name = &inner[..name_end];
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(Token::Open {
        name: name.to_ascii_lowercase(),
        attrs: &inner[name_end..],
        self_closing,
    })
}

/// The value of attribute `name`, quoted or bare.
pub(crate) fn attribute(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut search = 0;
    while let Some(found) = lower[search..].find(name) {
        let at = search + found;
        search = at + name.len();
        let preceded_by_space = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = attrs[search..].trim_start();
        let Some(value) = rest.strip_prefix('=').filter(|_| preceded_by_space) else {
            continue;
        };
        return Some(attribute_value(value.trim_start()));
    }
    None
}

fn attribute_value(text: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(rest) = text.strip_prefix(quote) {
            return rest.split(quote).next().unwrap_or_default().to_owned();
        }
    }
    text.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// The value of CSS property `name` in a `style` attribute.
pub(crate) fn css_property(style: &str, name: &str) -> Option<String> {
    style.split(';').find_map(|declaration| {
        let (property, value) = declaration.split_once(':')?;
        (property.trim().eq_ignore_ascii_case(name)).then(|| value.trim().to_ascii_lowercase())
    })
}

pub(crate) fn is_hex_color(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|hex| {
        matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
    })
}

pub(crate) fn is_page_break(attrs: &str) -> bool {
    let class = attribute(attrs, "class").unwrap_or_default();
    let style = attribute(attrs, "style").unwrap_or_default();
    class.split_whitespace().any(|name| name == "page-break")
        || [
            "page-break-after",
            "page-break-before",
            "break-after",
            "break-before",
        ]
        .iter()
        .filter_map(|property| css_property(&style, property))
        .any(|value| value == "always" || value == "page")
}

const SIMPLE_OPENERS: [(&str, &str); 14] = [
    ("u", "#underline["),
    ("ins", "#underline["),
    ("sup", "#super["),
    ("sub", "#sub["),
    ("kbd", "#kbd["),
    ("mark", "#mark["),
    ("b", "#strong["),
    ("strong", "#strong["),
    ("i", "#emph["),
    ("em", "#emph["),
    ("s", "#strike["),
    ("del", "#strike["),
    ("strike", "#strike["),
    ("blockquote", "#quote-block["),
];

/// The Typst opener for an element that wraps content.
fn element_opener(name: &str, attrs: &str) -> String {
    if let Some((_, opener)) = SIMPLE_OPENERS.iter().find(|(tag, _)| *tag == name) {
        return (*opener).to_owned();
    }
    let style = attribute(attrs, "style").unwrap_or_default();
    if let Some(color) = css_property(&style, "color").filter(|color| is_hex_color(color)) {
        return format!("#text(fill: rgb(\"{color}\"))[");
    }
    let align = css_property(&style, "text-align").or_else(|| attribute(attrs, "align"));
    if let Some(align @ ("right" | "center" | "left")) = align.as_deref() {
        return format!("#align({align})[");
    }
    if name == "a"
        && let Some(href) = attribute(attrs, "href").filter(|href| href.contains("://"))
    {
        return format!("#link({})[", escape::string(&href));
    }
    "#[".to_owned()
}

/// Elements that never have content.
fn is_void(name: &str) -> bool {
    matches!(
        name,
        "input" | "wbr" | "meta" | "link" | "source" | "col" | "area" | "embed" | "track"
    )
}

pub(crate) fn decode_entities(text: &str) -> String {
    const ENTITIES: [(&str, &str); 7] = [
        ("&nbsp;", "\u{a0}"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&amp;", "&"),
    ];
    ENTITIES
        .iter()
        .fold(text.to_owned(), |text, (entity, plain)| {
            text.replace(entity, plain)
        })
}

impl Converter<'_> {
    pub(super) fn html(&mut self, html: &str) {
        for token in tokenize(html) {
            match token {
                Token::Text(text) => self.html_text(text),
                Token::Open {
                    name,
                    attrs,
                    self_closing,
                } => self.html_open(&name, attrs, self_closing),
                Token::Close(name) => self.html_close(&name),
            }
        }
    }

    fn html_text(&mut self, text: &str) {
        let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            if !text.is_empty() {
                self.text(" ");
            }
            return;
        }
        let leading = text.starts_with(char::is_whitespace);
        let trailing = text.ends_with(char::is_whitespace);
        let mut spaced = String::new();
        spaced.push_str(if leading { " " } else { "" });
        spaced.push_str(&decode_entities(&collapsed));
        spaced.push_str(if trailing { " " } else { "" });
        self.text(&spaced);
    }

    fn html_open(&mut self, name: &str, attrs: &str, self_closing: bool) {
        match name {
            "br" => self.inline_markup("#linebreak();"),
            "hr" => self.inline_markup("#hrule();"),
            "img" => self.html_image(attrs),
            _ if is_page_break(attrs) => self.inline_markup("#page-break();"),
            _ if self_closing || is_void(name) => {}
            _ => self.open_html(name, element_opener(name, attrs)),
        }
    }

    fn html_image(&mut self, attrs: &str) {
        let Some(source) = attribute(attrs, "src") else {
            return;
        };
        let width =
            attribute(attrs, "width").and_then(|width| width.trim_end_matches("px").parse().ok());
        let markup = self.image_markup(&source, width);
        self.inline_markup(&markup);
    }

    fn open_html(&mut self, name: &str, opener: String) {
        self.inline_markup(&opener);
        self.open.push(Open {
            kind: OpenKind::Html {
                tag: name.to_owned(),
                opener,
            },
            closer: "];".to_owned(),
        });
    }

    /// Closes element `name` and anything opened inside it, if it is open in
    /// the current Markdown construct.
    fn html_close(&mut self, name: &str) {
        let mut depth = None;
        for (index, open) in self.open.iter().enumerate().rev() {
            match &open.kind {
                OpenKind::Html { tag, .. } if tag == name => {
                    depth = Some(index);
                    break;
                }
                OpenKind::Html { .. } | OpenKind::Highlight => {}
                _ => break,
            }
        }
        let Some(depth) = depth else {
            return;
        };
        while self.open.len() > depth {
            if let Some(open) = self.open.pop() {
                self.out.push_str(&open.closer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_tags_and_text() {
        let tokens = tokenize("a<br/>b<u>c</u><!-- x -->d");
        assert_eq!(tokens.len(), 7);
        assert!(matches!(&tokens[1], Token::Open { name, self_closing: true, .. } if name == "br"));
        assert_eq!(tokens[5], Token::Close("u".into()));
        assert_eq!(tokens[6], Token::Text("d"));
    }

    #[test]
    fn stray_angle_bracket_is_text() {
        assert_eq!(
            tokenize("a < b"),
            vec![Token::Text("a "), Token::Text("<"), Token::Text(" b")]
        );
    }

    #[test]
    fn reads_attributes() {
        let attrs = r#" src="images/a b.png" width=240 style='color: #b5452c'"#;
        assert_eq!(attribute(attrs, "src").as_deref(), Some("images/a b.png"));
        assert_eq!(attribute(attrs, "width").as_deref(), Some("240"));
        assert_eq!(
            css_property(&attribute(attrs, "style").unwrap(), "color").as_deref(),
            Some("#b5452c")
        );
    }

    #[test]
    fn recognises_page_breaks() {
        assert!(is_page_break(r#" class="page-break""#));
        assert!(is_page_break(r#" style="page-break-after: always;""#));
        assert!(!is_page_break(r#" class="note""#));
    }

    #[test]
    fn picks_openers() {
        assert_eq!(element_opener("u", ""), "#underline[");
        assert_eq!(
            element_opener("span", r#" style="color: #b5452c""#),
            "#text(fill: rgb(\"#b5452c\"))["
        );
        assert_eq!(
            element_opener("div", r#" style="text-align: right""#),
            "#align(right)["
        );
        assert_eq!(element_opener("span", ""), "#[");
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(decode_entities("a &amp;lt; b &lt;"), "a &lt; b <");
    }
}
