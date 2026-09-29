//! Context around a link or mention for the backlinks panel, finding
//! plain-text mentions of a note's title that aren't links yet, and
//! turning one into a link.

use std::ops::Range;
use std::sync::Arc;

use super::index::LinkIndex;
use super::parse::{CodeRanges, Link, parse_note};
use crate::link_update::{file_name, strip_note_extension};

/// How much text an excerpt keeps before and after what it's about:
/// about three lines of the sidebar at its usual width. Excerpts are cut
/// here, at words, rather than clamped when drawn, which cuts mid-letter.
const CONTEXT_BEFORE: usize = 50;
const CONTEXT_AFTER: usize = 70;

/// A line of context with the part it's about marked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excerpt {
    pub text: String,
    /// The link's label or the mention, within `text`.
    pub highlight: Range<usize>,
    /// Whether text was cut off before or after.
    pub cut_start: bool,
    pub cut_end: bool,
}

/// The line holding `range` of `text`, cut down to about
/// [`CONTEXT_BEFORE`] and [`CONTEXT_AFTER`], with the range swapped for `shown` (a
/// link's label, so `[[Plan|the plan]]` reads as "the plan").
pub fn excerpt(text: &str, range: Range<usize>, shown: &str) -> Excerpt {
    let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = text[range.end..]
        .find('\n')
        .map_or(text.len(), |at| at + range.end);
    let before = plain_inline(strip_line_markup(&text[line_start..range.start]));
    let after = plain_inline(text[range.end..line_end].trim_end());
    let (before, cut_start) = keep_end(&before, CONTEXT_BEFORE);
    let (after, cut_end) = keep_start(&after, CONTEXT_AFTER);
    let mut out = String::with_capacity(before.len() + shown.len() + after.len() + 6);
    if cut_start {
        out.push('…');
    }
    out.push_str(before.trim_start());
    let start = out.len();
    out.push_str(shown);
    let highlight = start..out.len();
    out.push_str(after);
    if cut_end {
        out.push('…');
    }
    Excerpt {
        text: out,
        highlight,
        cut_start,
        cut_end,
    }
}

/// Emphasis markers an excerpt leaves out.
const EMPHASIS: [&str; 4] = ["**", "__", "==", "~~"];

/// `text` as it reads: links as their labels, without emphasis markers.
fn plain_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if let Some((label, len)) = inline_link(rest) {
            out.push_str(&label);
            rest = &rest[len..];
        } else if let Some(marker) = EMPHASIS.iter().find(|m| rest.starts_with(**m)) {
            rest = &rest[marker.len()..];
        } else {
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

/// A wikilink or Markdown link at the start of `text`: its label and
/// how many bytes it takes.
fn inline_link(text: &str) -> Option<(String, usize)> {
    let body = text.strip_prefix('!').unwrap_or(text);
    let bang = text.len() - body.len();
    if let Some(inner) = body.strip_prefix("[[") {
        let close = inner.find("]]")?;
        let link = &inner[..close];
        let label = match link.split_once('|') {
            Some((_, alias)) => alias,
            None => link.split('#').next().unwrap_or(link),
        };
        return Some((label.to_string(), bang + 2 + close + 2));
    }
    let inner = body.strip_prefix('[')?;
    let close = inner.find("](")?;
    let end = inner[close..].find(')')? + close;
    let label = &inner[..close];
    (!label.contains(']')).then(|| (label.to_string(), bang + 1 + end + 1))
}

/// A line without its list, quote, heading or task marker.
fn strip_line_markup(line: &str) -> &str {
    let mut rest = line.trim_start();
    loop {
        let next = rest
            .strip_prefix("> ")
            .or_else(|| rest.strip_prefix("- [ ] "))
            .or_else(|| rest.strip_prefix("- [x] "))
            .or_else(|| rest.strip_prefix("- "))
            .or_else(|| rest.strip_prefix("* "))
            .or_else(|| rest.strip_prefix("+ "))
            .or_else(|| strip_heading_marker(rest));
        match next {
            Some(next) => rest = next.trim_start(),
            None => return rest,
        }
    }
}

fn strip_heading_marker(line: &str) -> Option<&str> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    (1..=6)
        .contains(&hashes)
        .then(|| line[hashes..].strip_prefix(' '))
        .flatten()
}

/// The last `chars` characters of `text`, starting at a word if cut.
fn keep_end(text: &str, chars: usize) -> (&str, bool) {
    let count = text.chars().count();
    if count <= chars {
        return (text, false);
    }
    let cut = text
        .char_indices()
        .nth(count - chars)
        .map_or(0, |(at, _)| at);
    let word = text[cut..].find(' ').map_or(cut, |at| cut + at + 1);
    (&text[word..], true)
}

/// The first `chars` characters of `text`, ending at a word if cut.
fn keep_start(text: &str, chars: usize) -> (&str, bool) {
    let Some((cut, _)) = text.char_indices().nth(chars) else {
        return (text, false);
    };
    let word = text[..cut].rfind(' ').unwrap_or(cut);
    (&text[..word], true)
}

/// A link's excerpt, showing the link as its label.
pub fn link_excerpt(text: &str, link: &Link) -> Excerpt {
    excerpt(text, link.range.clone(), link.label())
}

/// A plain-text mention of a note in another note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    pub source: String,
    /// The mentioned words in `source`.
    pub range: Range<usize>,
    pub excerpt: Excerpt,
}

/// A note an unlinked-mentions search looks through.
struct SearchedNote {
    path: String,
    text: Arc<str>,
}

/// What an unlinked-mentions search needs from the index, taken on the
/// main thread so the search itself can run anywhere. Taking it only
/// shares each note's text; a note is parsed again for its links only
/// when it has a mention.
pub struct MentionSearch {
    target: String,
    names: Vec<String>,
    notes: Vec<SearchedNote>,
}

impl MentionSearch {
    /// Mentions of the note at `target` by its title or aliases.
    pub fn new(index: &LinkIndex, target: &str) -> MentionSearch {
        let mut names = vec![note_title(target).to_string()];
        if let Some(entry) = index.note(target) {
            names.extend(entry.parsed.aliases.iter().cloned());
        }
        names.retain(|name| name.trim().chars().count() >= 2);
        let notes = index
            .note_paths()
            .filter(|path| *path != target)
            .filter_map(|path| {
                Some(SearchedNote {
                    path: path.to_string(),
                    text: index.note(path)?.text.clone(),
                })
            })
            .collect();
        MentionSearch {
            target: target.to_string(),
            names,
            notes,
        }
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    /// Every mention, grouped by note path in order, at most `limit`.
    pub fn run(&self, limit: usize) -> Vec<Mention> {
        let mut found = Vec::new();
        let mut notes: Vec<&SearchedNote> = self.notes.iter().collect();
        notes.sort_by(|a, b| a.path.cmp(&b.path));
        for note in notes {
            if found.len() >= limit {
                break;
            }
            let text = &note.text;
            let mut ranges = mentions_in(text, &self.names);
            ranges.truncate(limit - found.len());
            found.extend(ranges.into_iter().map(|range| Mention {
                source: note.path.clone(),
                excerpt: excerpt(text, range.clone(), &text[range.clone()]),
                range,
            }));
        }
        found
    }
}

/// Where `names` appear in `text` as whole words, outside links, code
/// and frontmatter, ignoring case.
fn mentions_in(text: &str, names: &[String]) -> Vec<Range<usize>> {
    let candidates: Vec<Range<usize>> = names
        .iter()
        .flat_map(|name| {
            find_ignoring_case(text, name)
                .into_iter()
                .map(|start| start..start + name.len())
        })
        .filter(|range| is_whole_word(text, range))
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }
    // Only a note that mentions the name is parsed for its links.
    let parsed = parse_note(text);
    let code = CodeRanges::new(text);
    let mut found: Vec<Range<usize>> = Vec::new();
    for range in candidates {
        let inside_link = parsed
            .links
            .iter()
            .any(|link| link.range.start < range.end && range.start < link.range.end);
        let overlaps = found
            .iter()
            .any(|seen| seen.start < range.end && range.start < seen.end);
        if range.start < parsed.body_start || inside_link || overlaps || code.contains(range.start)
        {
            continue;
        }
        found.push(range);
    }
    found.sort_by_key(|range| range.start);
    found
}

/// Byte offsets of `needle` in `haystack`, with ASCII letters matched in
/// either case. Other characters must match exactly.
fn find_ignoring_case(haystack: &str, needle: &str) -> Vec<usize> {
    let (hay, pin) = (haystack.as_bytes(), needle.as_bytes());
    let Some(&first) = pin.first() else {
        return Vec::new();
    };
    let (lower, upper) = (first.to_ascii_lowercase(), first.to_ascii_uppercase());
    let mut found = Vec::new();
    let mut at = 0;
    while at + pin.len() <= hay.len() {
        let Some(next) = memchr::memchr2(lower, upper, &hay[at..]) else {
            break;
        };
        at += next;
        if at + pin.len() > hay.len() {
            break;
        }
        if hay[at..at + pin.len()].eq_ignore_ascii_case(pin)
            && haystack.is_char_boundary(at)
            && haystack.is_char_boundary(at + pin.len())
        {
            found.push(at);
            at += pin.len();
        } else {
            at += 1;
        }
    }
    found
}

fn is_whole_word(text: &str, range: &Range<usize>) -> bool {
    let before = text[..range.start].chars().next_back();
    let after = text[range.end..].chars().next();
    !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
}

/// A note's title: its file name without `.md`.
pub fn note_title(path: &str) -> &str {
    strip_note_extension(file_name(path))
}

/// The wikilink that turns `mentioned` (the words as written) into a link
/// to `target`, keeping the words when they differ from the link.
pub fn wikilink_for(linkpath: &str, mentioned: &str) -> String {
    if note_title(linkpath) == mentioned {
        format!("[[{linkpath}]]")
    } else {
        format!("[[{linkpath}|{mentioned}]]")
    }
}

/// How to link to `target` from anywhere: its title, or its path without
/// `.md` when another file has the same name.
pub fn linkpath_for(index: &LinkIndex, target: &str) -> String {
    let title = note_title(target);
    let probe = Link {
        target: title.to_string(),
        subpath: None,
        display: None,
        range: 0..0,
        embed: false,
        markdown: false,
    };
    // A name that resolves to this note from the vault root is safe.
    if index.resolve("", &probe).as_deref() == Some(target) {
        title.to_string()
    } else {
        strip_note_extension(target).to_string()
    }
}

/// `text` with the mention at `range` made a link, if the words there
/// are still `expected`.
pub fn link_mention(text: &str, range: Range<usize>, expected: &str, link: &str) -> Option<String> {
    let current = text.get(range.clone())?;
    if !current.eq_ignore_ascii_case(expected) {
        return None;
    }
    Some(format!(
        "{}{link}{}",
        &text[..range.start],
        &text[range.end..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_show_the_label_and_drop_markup() {
        let text = "intro\n- See [[Plan#Goals|the plan]] for more.\nnext";
        let link = super::super::parse::parse_note(text).links.remove(0);
        let found = link_excerpt(text, &link);
        assert_eq!(found.text, "See the plan for more.");
        assert_eq!(&found.text[found.highlight.clone()], "the plan");
    }

    #[test]
    fn other_links_and_emphasis_read_as_text() {
        let text =
            "**Bold** [[A|alpha]], ![[pic.png]] and [the plan](Plan.md) near [[Target]] ==x==.";
        let link = super::super::parse::parse_note(text)
            .links
            .into_iter()
            .find(|link| link.target == "Target")
            .unwrap();
        let found = link_excerpt(text, &link);
        assert_eq!(
            found.text,
            "Bold alpha, pic.png and the plan near Target x."
        );
        assert_eq!(&found.text[found.highlight.clone()], "Target");
    }

    #[test]
    fn long_lines_are_cut_at_words() {
        let before = "word ".repeat(40);
        let text = format!("{before}[[Target]] {}", "after ".repeat(40));
        let link = super::super::parse::parse_note(&text).links.remove(0);
        let found = link_excerpt(&text, &link);
        assert!(found.cut_start && found.cut_end);
        assert!(found.text.starts_with("…word"));
        assert!(found.text.ends_with("after…"));
        assert!(found.text.chars().count() < 200);
    }

    #[test]
    fn unlinked_mentions_skip_links_code_and_parts_of_words() {
        let mut index = LinkIndex::new();
        index.set_note("Wave Packets.md", "---\naliases: [packets]\n---\n");
        index.set_note(
            "Notes.md",
            "About wave packets. [[Wave Packets]] `Wave Packets` Wave Packetsy.\nPackets too.",
        );
        let search = MentionSearch::new(&index, "Wave Packets.md");
        let found = search.run(10);
        let words: Vec<&str> = found
            .iter()
            .map(|m| &index.note("Notes.md").unwrap().text[m.range.clone()])
            .collect();
        assert_eq!(words, ["wave packets", "Packets"]);
        assert_eq!(
            found[0].excerpt.text,
            "About wave packets. Wave Packets `Wave Packets` Wave Packetsy."
        );
    }

    #[test]
    fn linking_a_mention_keeps_its_words() {
        let text = "About wave packets here.";
        let range = 6..18;
        let link = wikilink_for("Wave Packets", &text[range.clone()]);
        assert_eq!(link, "[[Wave Packets|wave packets]]");
        let linked = link_mention(text, range.clone(), "Wave Packets", &link).unwrap();
        assert_eq!(linked, "About [[Wave Packets|wave packets]] here.");
        assert!(link_mention("Changed text", range, "Wave Packets", &link).is_none());
    }

    #[test]
    fn duplicate_names_link_by_path() {
        let mut index = LinkIndex::new();
        index.set_note("A/Plan.md", "");
        index.set_note("Plan.md", "");
        assert_eq!(linkpath_for(&index, "Plan.md"), "Plan");
        assert_eq!(linkpath_for(&index, "A/Plan.md"), "A/Plan");
    }
}
