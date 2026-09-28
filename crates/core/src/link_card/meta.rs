//! A web page's card, read from its `<head>`: the Open Graph and Twitter
//! tags most sites set, then `<title>`, `<meta name="description">` and
//! the declared icon. No HTML parser: the head is scanned for its `<meta>`,
//! `<title>` and `<link>` tags, which is all the card needs.

use super::LinkCard;
use url::Url;

/// How much of a page is read: heads are small, and some pages aren't.
pub const MAX_PAGE_BYTES: usize = 512 * 1024;

/// The card for the page at `url` whose HTML is `html`.
pub fn card_from_html(url: &str, html: &str) -> LinkCard {
    let head = head_of(html);
    let metas = tags(head, "meta");
    let meta = |keys: &[&str]| {
        keys.iter().find_map(|key| {
            metas.iter().find_map(|tag| {
                let named = attribute(tag, "property").or_else(|| attribute(tag, "name"))?;
                named
                    .eq_ignore_ascii_case(key)
                    .then(|| attribute(tag, "content"))
                    .flatten()
                    .filter(|content| !content.trim().is_empty())
            })
        })
    };
    let title = meta(&["og:title", "twitter:title"])
        .or_else(|| title_tag(head))
        .map(|title| clean(&title))
        .unwrap_or_default();
    let description = meta(&["og:description", "twitter:description", "description"])
        .map(|description| clean(&description))
        .unwrap_or_default();
    let base = Url::parse(url).ok();
    let absolute = |link: String| resolve(base.as_ref(), &decode_entities(&link));
    let image = meta(&[
        "og:image:secure_url",
        "og:image",
        "twitter:image",
        "twitter:image:src",
    ])
    .and_then(absolute);
    let favicon = icon_link(head).and_then(absolute).or_else(|| {
        let base = base.as_ref()?;
        base.join("/favicon.ico").ok().map(String::from)
    });
    let mut card = LinkCard {
        url: url.to_owned(),
        title,
        description,
        image,
        favicon,
    };
    if card.title.is_empty() {
        card.title = card.domain().to_owned();
    }
    card
}

/// Everything before `</head>`, or the whole page when it has none.
fn head_of(html: &str) -> &str {
    find_ignoring_case(html, "</head").map_or(html, |end| &html[..end])
}

/// Each `<name …>` tag in `html`, from its `<` to its `>`.
fn tags<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name}");
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(at) = find_ignoring_case(rest, &open) {
        let tag = &rest[at..];
        let end = tag.find('>').map_or(tag.len(), |end| end + 1);
        let after = tag.as_bytes().get(open.len()).copied();
        if after.is_none_or(|ch| ch.is_ascii_whitespace() || ch == b'/' || ch == b'>') {
            found.push(&tag[..end]);
        }
        rest = &tag[end.max(1)..];
    }
    found
}

fn title_tag(head: &str) -> Option<String> {
    let start = find_ignoring_case(head, "<title")?;
    let open_end = start + head[start..].find('>')? + 1;
    let close = find_ignoring_case(&head[open_end..], "</title")?;
    Some(head[open_end..open_end + close].to_owned())
}

/// The `href` of the first `<link rel="icon">`, `shortcut icon` or
/// `apple-touch-icon`.
fn icon_link(head: &str) -> Option<String> {
    tags(head, "link").into_iter().find_map(|tag| {
        let rel = attribute(tag, "rel")?.to_ascii_lowercase();
        let is_icon =
            rel.split_whitespace().any(|word| word == "icon") || rel.contains("apple-touch-icon");
        is_icon.then(|| attribute(tag, "href")).flatten()
    })
}

/// The value of `name="…"`, `name='…'` or `name=bare` in a tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let before = lower.as_bytes().get(at.wrapping_sub(1)).copied();
        if !before.is_some_and(|ch| ch.is_ascii_whitespace()) {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(value) = rest.strip_prefix('=') else {
            continue;
        };
        return Some(unquoted(value.trim_start()).to_owned());
    }
    None
}

fn unquoted(value: &str) -> &str {
    match value.chars().next() {
        Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or(""),
        _ => value
            .split(|ch: char| ch.is_whitespace() || ch == '>')
            .next()
            .unwrap_or(""),
    }
}

fn find_ignoring_case(haystack: &str, needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

fn resolve(base: Option<&Url>, link: &str) -> Option<String> {
    let link = link.trim();
    if link.is_empty() {
        return None;
    }
    match base {
        Some(base) => base.join(link).ok().map(String::from),
        None => Url::parse(link).ok().map(String::from),
    }
}

/// Entities decoded and whitespace collapsed, as the card shows it.
fn clean(text: &str) -> String {
    decode_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The entities pages actually use in titles and descriptions.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = rest
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| entity(&rest[1..end]).map(|ch| (ch, end + 1)));
        match decoded {
            Some((ch, len)) => {
                out.push(ch);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    const NAMED: [(&str, char); 10] = [
        ("amp", '&'),
        ("lt", '<'),
        ("gt", '>'),
        ("quot", '"'),
        ("apos", '\''),
        ("nbsp", ' '),
        ("mdash", '—'),
        ("ndash", '–'),
        ("hellip", '…'),
        ("rsquo", '’'),
    ];
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code);
    }
    NAMED
        .iter()
        .find(|(entity, _)| *entity == name)
        .map(|(_, ch)| *ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN_GRAPH: &str = r#"<!doctype html><html><head>
        <meta charset="utf-8">
        <title>Fallback title</title>
        <meta property="og:title" content="Wave packets &amp; group velocity">
        <meta name="description" content="Plain description">
        <meta property='og:description' content='How a   group of waves
            moves together.'>
        <meta property="og:image" content="/images/cover.png">
        <link rel="shortcut icon" href="/static/icon.png">
        </head><body><meta property="og:title" content="Not in the head"></body></html>"#;

    #[test]
    fn open_graph_tags_win() {
        let card = card_from_html("https://physics.example.org/waves/", OPEN_GRAPH);
        assert_eq!(card.title, "Wave packets & group velocity");
        assert_eq!(card.description, "How a group of waves moves together.");
        assert_eq!(
            card.image.as_deref(),
            Some("https://physics.example.org/images/cover.png")
        );
        assert_eq!(
            card.favicon.as_deref(),
            Some("https://physics.example.org/static/icon.png")
        );
        assert_eq!(card.url, "https://physics.example.org/waves/");
    }

    #[test]
    fn plain_pages_fall_back_to_title_and_description() {
        let html = "<HTML><HEAD><TITLE>\n  Old &#8212; school \n</TITLE>\
            <META NAME=description CONTENT=\"Just HTML.\"></HEAD></HTML>";
        let card = card_from_html("http://old.example.com/a", html);
        assert_eq!(card.title, "Old — school");
        assert_eq!(card.description, "Just HTML.");
        assert_eq!(card.image, None);
        assert_eq!(
            card.favicon.as_deref(),
            Some("http://old.example.com/favicon.ico")
        );
    }

    #[test]
    fn a_page_without_a_title_is_named_by_its_domain() {
        let card = card_from_html("https://www.example.com/x", "<p>hi</p>");
        assert_eq!(card.title, "example.com");
        assert_eq!(card.description, "");
    }

    #[test]
    fn attributes_need_a_word_boundary() {
        let tag = r#"<meta data-name="x" name="description" content="yes">"#;
        assert_eq!(attribute(tag, "name").as_deref(), Some("description"));
        assert_eq!(
            decode_entities("a &bogus; b &#x27;c&#39;"),
            "a &bogus; b 'c'"
        );
    }
}
