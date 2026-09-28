//! The raw HTML found in notes: line breaks, rules, bold, italic,
//! strike-through, underline, super- and subscripts, highlights, keys,
//! links, styled spans (colour, fill, size, weight, slant, decoration),
//! aligned blocks, images and page breaks: the subset the editor draws.
//! Other tags are dropped and their text kept.

use editor_core::syntax::{Alignment, FontSize, HtmlStyle, Rgba8};

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

/// The value of attribute `name`, quoted (straight or curly) or bare, read
/// as the editor reads it.
pub(crate) fn attribute(attrs: &str, name: &str) -> Option<String> {
    let padded = format!(" {attrs}");
    editor_core::syntax::html_attribute(&padded, name).map(str::to_owned)
}

/// The styles an element's `style` attribute asks for, and for a block,
/// its old `align` attribute: only what the editor draws, so nothing
/// that moves boxes or runs code gets through.
pub(crate) fn element_style(name: &str, attrs: &str) -> HtmlStyle {
    let mut style =
        attribute(attrs, "style").map_or_else(HtmlStyle::default, |css| HtmlStyle::parse(&css));
    if style.align.is_none() && matches!(name, "p" | "div" | "center") {
        style.align =
            attribute(attrs, "align").and_then(|align| editor_core::syntax::parse_align(&align));
    }
    if name == "center" && style.align.is_none() {
        style.align = Some(Alignment::Center);
    }
    style
}

/// A link's address when it is safe to follow: a web or mail address,
/// never `javascript:` or `data:`.
pub(crate) fn safe_href(attrs: &str) -> Option<String> {
    attribute(attrs, "href").filter(|href| {
        editor_core::syntax::is_safe_href(href)
            && (href.contains("://") || href.starts_with("mailto:"))
    })
}

/// The value of CSS property `name` in a `style` attribute.
pub(crate) fn css_property(style: &str, name: &str) -> Option<String> {
    style.split(';').find_map(|declaration| {
        let (property, value) = declaration.split_once(':')?;
        (property.trim().eq_ignore_ascii_case(name)).then(|| value.trim().to_ascii_lowercase())
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

/// The Typst opener for an element that wraps content, and how many
/// brackets it opens.
fn element_opener(name: &str, attrs: &str) -> (String, usize) {
    if let Some((_, opener)) = SIMPLE_OPENERS.iter().find(|(tag, _)| *tag == name) {
        return ((*opener).to_owned(), 1);
    }
    if name == "a"
        && let Some(href) = safe_href(attrs)
    {
        return (format!("#link({})[", escape::string(&href)), 1);
    }
    let openers = style_openers(&element_style(name, attrs));
    match openers.is_empty() {
        true => ("#[".to_owned(), 1),
        false => (openers.concat(), openers.len()),
    }
}

/// One Typst opener per part of a style, outermost first.
fn style_openers(style: &HtmlStyle) -> Vec<String> {
    let mut openers = Vec::new();
    if let Some(align) = style.align.and_then(typst_align) {
        openers.push(format!("#align({align})["));
    }
    let text = text_arguments(style);
    if !text.is_empty() {
        openers.push(format!("#text({})[", text.join(", ")));
    }
    if let Some(color) = style.background {
        openers.push(format!("#highlight(fill: {})[", typst_color(color)));
    }
    let decorations = [
        (style.underline, "#underline["),
        (style.strikethrough, "#strike["),
    ];
    openers.extend(
        decorations
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, opener)| (*opener).to_owned()),
    );
    openers
}

/// The arguments of the `#text` a style asks for: colour, size, weight
/// and slant.
fn text_arguments(style: &HtmlStyle) -> Vec<String> {
    let mut arguments = Vec::new();
    if let Some(color) = style.color {
        arguments.push(format!("fill: {}", typst_color(color)));
    }
    if let Some(FontSize::Absolute(percent) | FontSize::Relative(percent)) = style.font_size {
        arguments.push(format!("size: {}em", f32::from(percent) / 100.));
    }
    if let Some(bold) = style.bold {
        let weight = if bold { "bold" } else { "regular" };
        arguments.push(format!("weight: \"{weight}\""));
    }
    if let Some(italic) = style.italic {
        let slant = if italic { "italic" } else { "normal" };
        arguments.push(format!("style: \"{slant}\""));
    }
    arguments
}

fn typst_color(color: Rgba8) -> String {
    format!("rgb(\"{}\")", color.to_hex())
}

fn typst_align(align: Alignment) -> Option<&'static str> {
    match align {
        Alignment::Left => Some("left"),
        Alignment::Center => Some("center"),
        Alignment::Right => Some("right"),
        Alignment::None => None,
    }
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
            _ => {
                let (opener, brackets) = element_opener(name, attrs);
                self.open_html(name, opener, brackets);
            }
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

    fn open_html(&mut self, name: &str, opener: String, brackets: usize) {
        self.inline_markup(&opener);
        self.open.push(Open {
            kind: OpenKind::Html {
                tag: name.to_owned(),
                opener,
            },
            closer: format!("{};", "]".repeat(brackets)),
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
        assert_eq!(
            attribute("style=”color:red;”", "style").as_deref(),
            Some("color:red;")
        );
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
        let opener = |name: &str, attrs: &str| element_opener(name, attrs);
        assert_eq!(opener("u", ""), ("#underline[".into(), 1));
        assert_eq!(
            opener("span", r#" style="color: #b5452c""#),
            ("#text(fill: rgb(\"#b5452c\"))[".into(), 1)
        );
        assert_eq!(
            opener("div", r#" style="text-align: right""#),
            ("#align(right)[".into(), 1)
        );
        assert_eq!(opener("center", ""), ("#align(center)[".into(), 1));
        assert_eq!(opener("span", ""), ("#[".into(), 1));
        assert_eq!(
            opener(
                "p",
                " style=\"text-align:center; color:red; font-size:2em; font-weight:bold; \
                 background-color:yellow; text-decoration:underline; position:fixed\""
            ),
            (
                "#align(center)[#text(fill: rgb(\"#ff0000\"), size: 2em, weight: \"bold\")[\
                 #highlight(fill: rgb(\"#ffff00\"))[#underline["
                    .into(),
                4
            )
        );
        assert_eq!(
            opener("a", r#" href="javascript://x%0aalert(1)""#),
            ("#[".into(), 1)
        );
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(decode_entities("a &amp;lt; b &lt;"), "a &lt; b <");
    }
}
