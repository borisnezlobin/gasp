//! Reading one note for the vault index: its links (wikilinks, embeds and
//! Markdown links), its tags (inline `#tags` and the frontmatter's
//! `tags:`), and the aliases its frontmatter gives it.
//!
//! Links and tags inside code are left out, as Obsidian does.

use std::ops::Range;

use crate::link_update::{Destination, code_ranges, find_from, find_markdown_destinations};

/// A link in a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The path part as written, such as `Projects/Plan` or `../Plan.md`,
    /// with Markdown links' percent-encoding decoded.
    pub target: String,
    /// The heading or block after `#`, without the `#`.
    pub subpath: Option<String>,
    /// A wikilink's alias after `|`, or a Markdown link's text.
    pub display: Option<String>,
    /// The whole link, from `[[` or `[` (or the `!` before it) to its end.
    pub range: Range<usize>,
    /// `![[…]]` or `![…](…)`.
    pub embed: bool,
    /// `[text](target)` rather than `[[target]]`.
    pub markdown: bool,
}

impl Link {
    /// What the link shows in a list: its alias, else its target.
    pub fn label(&self) -> &str {
        self.display
            .as_deref()
            .filter(|display| !display.trim().is_empty())
            .unwrap_or(&self.target)
    }
}

/// A tag, without its `#`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// Where `#name` is in the text; `None` for frontmatter tags.
    pub range: Option<Range<usize>>,
}

/// Everything the index keeps from one note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedNote {
    pub links: Vec<Link>,
    pub tags: Vec<Tag>,
    pub aliases: Vec<String>,
    /// Where the body starts, after any frontmatter.
    pub body_start: usize,
}

/// Reads `text`'s links, tags and aliases.
pub fn parse_note(text: &str) -> ParsedNote {
    let code = CodeRanges::new(text);
    let frontmatter = frontmatter(text);
    let body_start = frontmatter.as_ref().map_or(0, |found| found.end);
    let mut links = wikilinks(text, &code);
    links.extend(markdown_links(text, &code));
    links.sort_by_key(|link| link.range.start);
    let mut tags = Vec::new();
    let mut aliases = Vec::new();
    if let Some(found) = &frontmatter {
        tags.extend(found.tags.iter().map(|name| Tag {
            name: name.clone(),
            range: None,
        }));
        aliases.clone_from(&found.aliases);
    }
    tags.extend(inline_tags(text, body_start, &code));
    ParsedNote {
        links,
        tags,
        aliases,
        body_start,
    }
}

/// Code spans and blocks, sorted, for quick "is this in code" checks.
pub struct CodeRanges(Vec<Range<usize>>);

impl CodeRanges {
    pub fn new(text: &str) -> CodeRanges {
        let mut ranges = code_ranges(text);
        ranges.sort_by_key(|range| range.start);
        CodeRanges(ranges)
    }

    pub fn contains(&self, at: usize) -> bool {
        // Fences and inline spans never overlap, so only the last range
        // starting at or before `at` can hold it.
        let after = self.0.partition_point(|range| range.start <= at);
        after > 0 && self.0[after - 1].contains(&at)
    }
}

// ---- Wikilinks ----

fn wikilinks(text: &str, code: &CodeRanges) -> Vec<Link> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = find_from(text, from, "[[") {
        let body_start = open + 2;
        let Some(close) = find_from(text, body_start, "]]") else {
            break;
        };
        let body = &text[body_start..close];
        if body.contains('\n') || body.contains("[[") {
            from = body_start;
            continue;
        }
        from = close + 2;
        let escaped = open > 0 && text.as_bytes()[open - 1] == b'\\';
        if escaped || code.contains(open) {
            continue;
        }
        let embed = open > 0 && text.as_bytes()[open - 1] == b'!';
        let start = if embed { open - 1 } else { open };
        if let Some(link) = wikilink(body, start..close + 2, embed) {
            found.push(link);
        }
    }
    found
}

/// A wikilink from its body, the text between `[[` and `]]`.
fn wikilink(body: &str, range: Range<usize>, embed: bool) -> Option<Link> {
    // In a table the pipe is escaped as `\|`.
    let (before_alias, alias) = match body.split_once('|') {
        Some((before, alias)) => (before.strip_suffix('\\').unwrap_or(before), Some(alias)),
        None => (body, None),
    };
    let (target, subpath) = match before_alias.split_once('#') {
        Some((target, subpath)) => (target, Some(subpath.trim())),
        None => (before_alias, None),
    };
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    Some(Link {
        target: target.to_string(),
        subpath: subpath.filter(|s| !s.is_empty()).map(str::to_string),
        display: alias.map(|alias| alias.trim().to_string()),
        range,
        embed,
        markdown: false,
    })
}

// ---- Markdown links ----

fn markdown_links(text: &str, code: &CodeRanges) -> Vec<Link> {
    find_markdown_destinations(text)
        .into_iter()
        .filter(|dest| !code.contains(dest.start))
        .filter_map(|dest| markdown_link(text, dest))
        .collect()
}

/// The link whose destination is at `dest`, if its `[text]` is on the
/// same line and the destination points inside the vault.
fn markdown_link(text: &str, dest: Range<usize>) -> Option<Link> {
    let bracket_close = dest.start - 2;
    let line_start = text[..bracket_close].rfind('\n').map_or(0, |at| at + 1);
    let open = bracket_open(&text[line_start..bracket_close])? + line_start;
    let parsed = Destination::parse(&text[dest.clone()])?;
    let line_end = text[dest.end..]
        .find('\n')
        .map_or(text.len(), |at| at + dest.end);
    let close = text[dest.end..line_end].find(')')? + dest.end;
    let embed = open > 0 && text.as_bytes()[open - 1] == b'!';
    let start = if embed { open - 1 } else { open };
    let subpath = parsed.fragment.trim_start_matches('#');
    Some(Link {
        target: parsed.path,
        subpath: (!subpath.is_empty()).then(|| subpath.to_string()),
        display: Some(text[open + 1..bracket_close].to_string()),
        range: start..close + 1,
        embed,
        markdown: true,
    })
    .filter(|link| !link.target.is_empty())
}

/// Where the `[` that the text's final `]` closes is.
fn bracket_open(before_close: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (at, ch) in before_close.char_indices().rev() {
        match ch {
            ']' => depth += 1,
            '[' if depth == 0 => return Some(at),
            '[' => depth -= 1,
            _ => {}
        }
    }
    None
}

// ---- Tags ----

/// Whether `ch` can be part of a tag's name.
fn is_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/')
}

fn inline_tags(text: &str, body_start: usize, code: &CodeRanges) -> Vec<Tag> {
    let mut tags = Vec::new();
    let body = &text[body_start..];
    for (at, _) in body.match_indices('#') {
        let start = body_start + at;
        let after_space = body[..at]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        if !after_space || code.contains(start) || in_html_tag(&body[..at]) {
            continue;
        }
        let name_len = body[at + 1..]
            .find(|ch: char| !is_tag_char(ch))
            .unwrap_or(body.len() - at - 1);
        let name = body[at + 1..at + 1 + name_len].trim_end_matches('/');
        if is_tag_name(name) {
            tags.push(Tag {
                name: name.to_string(),
                range: Some(start..start + 1 + name.len()),
            });
        }
    }
    tags
}

/// Whether the line so far has an HTML tag open, as in
/// `<span style="color: #b5452c">`, whose `#` isn't a tag.
fn in_html_tag(before: &str) -> bool {
    let line = before.rsplit('\n').next().unwrap_or(before);
    match (line.rfind('<'), line.rfind('>')) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        _ => false,
    }
}

/// A tag needs a character that isn't a digit, so `#1` isn't one.
fn is_tag_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('/') && name.chars().any(|ch| !ch.is_ascii_digit())
}

// ---- Frontmatter ----

struct Frontmatter {
    end: usize,
    tags: Vec<String>,
    aliases: Vec<String>,
}

/// The YAML block at the top of a note, between `---` lines. Only the
/// `tags` and `aliases` keys are read, in their inline, comma and list
/// forms.
fn frontmatter(text: &str) -> Option<Frontmatter> {
    let first = text.lines().next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut offset = first.len() + 1;
    let mut lines = Vec::new();
    let mut end = None;
    for line in text[offset.min(text.len())..].split_inclusive('\n') {
        offset += line.len();
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            end = Some(offset);
            break;
        }
        lines.push(trimmed);
    }
    let end = end?;
    Some(Frontmatter {
        end,
        tags: yaml_list(&lines, &["tags", "tag"])
            .into_iter()
            .map(|tag| tag.trim_start_matches('#').to_string())
            .filter(|tag| is_tag_name(tag))
            .collect(),
        aliases: yaml_list(&lines, &["aliases", "alias"]),
    })
}

/// The values of the first of `keys` present: `[a, b]`, `a, b`, or a
/// `- a` list on the following lines.
fn yaml_list(lines: &[&str], keys: &[&str]) -> Vec<String> {
    let Some((index, value)) = lines.iter().enumerate().find_map(|(index, line)| {
        let (key, value) = line.split_once(':')?;
        keys.contains(&key.trim())
            .then_some((index, value.trim()))
            .filter(|_| !line.starts_with([' ', '\t']))
    }) else {
        return Vec::new();
    };
    if !value.is_empty() {
        let inner = value.trim_start_matches('[').trim_end_matches(']');
        return inner.split(',').filter_map(yaml_scalar).collect();
    }
    lines[index + 1..]
        .iter()
        .map_while(|line| line.trim_start().strip_prefix("- "))
        .filter_map(yaml_scalar)
        .collect()
}

fn yaml_scalar(raw: &str) -> Option<String> {
    let value = raw.trim().trim_matches(['"', '\'']).trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(text: &str) -> Vec<(String, Option<String>, Option<String>, bool)> {
        parse_note(text)
            .links
            .into_iter()
            .map(|l| (l.target, l.subpath, l.display, l.embed))
            .collect()
    }

    #[test]
    fn wikilinks_split_into_target_heading_and_alias() {
        let text = "See [[Plan#Goals|the plan]] and ![[chart.png]] and [[Projects/Road map]].";
        assert_eq!(
            targets(text),
            vec![
                (
                    "Plan".into(),
                    Some("Goals".into()),
                    Some("the plan".into()),
                    false
                ),
                ("chart.png".into(), None, None, true),
                ("Projects/Road map".into(), None, None, false),
            ]
        );
        let first = &parse_note(text).links[0];
        assert_eq!(&text[first.range.clone()], "[[Plan#Goals|the plan]]");
        let embed = &parse_note(text).links[1];
        assert_eq!(&text[embed.range.clone()], "![[chart.png]]");
    }

    #[test]
    fn table_pipes_and_same_note_headings() {
        assert_eq!(
            targets("| [[Plan\\|p]] |"),
            vec![("Plan".into(), None, Some("p".into()), false)]
        );
        assert!(targets("Up to [[#Intro]].").is_empty());
    }

    #[test]
    fn markdown_links_are_decoded_and_web_links_skipped() {
        let text = "[the plan](Projects/My%20Plan.md#Goals) [web](https://x.org) ![c](img/c.png)";
        let links = parse_note(text).links;
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Projects/My Plan.md");
        assert_eq!(links[0].subpath.as_deref(), Some("Goals"));
        assert_eq!(links[0].display.as_deref(), Some("the plan"));
        assert_eq!(
            &text[links[0].range.clone()],
            "[the plan](Projects/My%20Plan.md#Goals)"
        );
        assert!(links[1].embed && links[1].markdown);
    }

    #[test]
    fn code_hides_links_and_tags() {
        let text = "`[[Nope]]` #real\n```\n[[Also nope]] #fake\n```\n[[Yes]]";
        assert_eq!(targets(text).len(), 1);
        let tags: Vec<String> = parse_note(text).tags.into_iter().map(|t| t.name).collect();
        assert_eq!(tags, ["real"]);
    }

    #[test]
    fn inline_tags_nest_and_skip_headings_numbers_and_anchors() {
        let text = "# Heading\n#physics/waves and #todo, not #1 or a#b or [[N#h]] #2024-plan <span style=\"color: #b5452c\">x</span>";
        let tags: Vec<String> = parse_note(text).tags.into_iter().map(|t| t.name).collect();
        assert_eq!(tags, ["physics/waves", "todo", "2024-plan"]);
    }

    #[test]
    fn frontmatter_tags_and_aliases_in_every_form() {
        let inline = "---\ntags: [maths, \"#analysis\"]\naliases: Limits, Limit\n---\nBody #inline";
        let parsed = parse_note(inline);
        let tags: Vec<&str> = parsed.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(tags, ["maths", "analysis", "inline"]);
        assert_eq!(parsed.aliases, ["Limits", "Limit"]);
        assert_eq!(&inline[parsed.body_start..], "Body #inline");
        let listed = "---\ntitle: x\ntags:\n  - one\n  - two/three\n---\n";
        let tags: Vec<String> = parse_note(listed)
            .tags
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(tags, ["one", "two/three"]);
    }

    #[test]
    fn an_unclosed_frontmatter_is_just_text() {
        let parsed = parse_note("---\ntags: x\n#real");
        assert_eq!(parsed.body_start, 0);
        assert_eq!(parsed.tags.len(), 1);
    }
}
