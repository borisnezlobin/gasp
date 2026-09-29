//! Embedded notes (`![[Note]]`, `![[Note#Heading]]`, `![[Note#^block]]`):
//! which embeds name a note rather than a file, and the part of the note
//! an embed shows.

use std::ops::Range;

use crate::render::folds::heading_sections;
use crate::syntax::{NodeKind, SyntaxTree};

/// The part of a note an embed shows, from what follows its `#`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbedPart<'a> {
    /// The whole note, less its frontmatter.
    Whole,
    /// A heading and the section under it. `A#B` names B.
    Heading(&'a str),
    /// The block marked `^id`.
    Block(&'a str),
}

impl<'a> EmbedPart<'a> {
    pub fn of(subpath: Option<&'a str>) -> EmbedPart<'a> {
        let Some(subpath) = subpath.map(str::trim).filter(|path| !path.is_empty()) else {
            return EmbedPart::Whole;
        };
        match subpath.strip_prefix('^') {
            Some(id) => EmbedPart::Block(id.trim()),
            None => EmbedPart::Heading(subpath.rsplit('#').next().unwrap_or(subpath).trim()),
        }
    }
}

/// Whether an embed's target names a note: no extension, or `.md`. A dot
/// in a note's name (`v2.0 plans`) isn't an extension.
pub fn embeds_note(target: &str) -> bool {
    let name = target.rsplit('/').next().unwrap_or(target);
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return true;
    };
    let is_extension = !stem.is_empty()
        && (1..=4).contains(&extension.len())
        && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
        && !extension.chars().all(|ch| ch.is_ascii_digit());
    !is_extension || extension.eq_ignore_ascii_case("md")
}

/// The text an embed of `part` shows from a note, or `None` when the
/// heading or block isn't there. A block's `^id` marker is left out.
pub fn embedded_text(text: &str, tree: &SyntaxTree, part: EmbedPart<'_>) -> Option<String> {
    match part {
        EmbedPart::Whole => Some(text[body_start(tree)..].trim_end().to_string()),
        EmbedPart::Heading(title) => {
            let range = heading_range(text, tree, title)?;
            Some(text[range].trim_end().to_string())
        }
        EmbedPart::Block(id) => block_text(text, tree, id),
    }
}

/// Where a note's text starts after its frontmatter.
fn body_start(tree: &SyntaxTree) -> usize {
    tree.blocks()
        .first()
        .map(|&id| tree.node(id))
        .filter(|node| node.kind == NodeKind::Frontmatter)
        .map_or(0, |node| {
            let line = tree.lines().line_of(node.range.end.saturating_sub(1));
            let next = line + 1;
            match next < tree.lines().line_count() {
                true => tree.lines().line_start(next),
                false => node.range.end,
            }
        })
}

/// The heading titled `title` (ignoring case and extra spaces) and its
/// section.
fn heading_range(text: &str, tree: &SyntaxTree, title: &str) -> Option<Range<usize>> {
    let wanted = normalized(title);
    let section = heading_sections(tree).into_iter().find(|section| {
        let line = tree.lines().line_range(text, section.line);
        normalized(heading_title(&text[line])) == wanted
    })?;
    let end = section.body_text.end.min(text.len());
    Some(section.line_start..end.max(section.line_start))
}

/// A heading line's text without its `#`s and any closing ones.
fn heading_title(line: &str) -> &str {
    let title = line.trim_start().trim_start_matches('#');
    title.trim().trim_end_matches('#').trim()
}

fn normalized(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The block marked `^id`: the list item or top-level block the marker
/// ends, without the marker.
fn block_text(text: &str, tree: &SyntaxTree, id: &str) -> Option<String> {
    let marker = block_marker(text, id)?;
    let range = block_around(tree, marker.start)?;
    let mut shown = String::with_capacity(range.len());
    shown.push_str(&text[range.start..marker.start]);
    shown.push_str(&text[marker.end.min(range.end)..range.end.max(marker.end)]);
    Some(shown.trim_end().to_string())
}

/// Where ` ^id` sits at the end of a line, the space before it included.
fn block_marker(text: &str, id: &str) -> Option<Range<usize>> {
    let needle = format!("^{id}");
    let mut from = 0;
    while let Some(found) = text[from..].find(&needle) {
        let start = from + found;
        let end = start + needle.len();
        let ends_line = text[end..]
            .chars()
            .next()
            .is_none_or(|ch| ch == '\n' || ch == '\r');
        let spaced = text[..start].ends_with([' ', '\t']) || start == 0;
        if ends_line && spaced {
            let before = text[..start].trim_end_matches([' ', '\t']).len();
            return Some(before..end);
        }
        from = end;
    }
    None
}

/// The innermost list item holding `offset`, else the top-level block.
fn block_around(tree: &SyntaxTree, offset: usize) -> Option<Range<usize>> {
    let path = tree.path_at(offset);
    let item = path
        .iter()
        .rev()
        .map(|&id| tree.node(id))
        .find(|node| matches!(node.kind, NodeKind::ListItem { .. }));
    let block = item.or_else(|| path.get(1).map(|&id| tree.node(id)))?;
    Some(block.range.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    const NOTE: &str = "---\ntags: [a]\n---\n# Intro\nHello.\n\n## Proof\nStep one.\nStep two.\n\n## Next\nA claim. ^claim\n\n- first\n- second ^item\n";

    fn shown(part: EmbedPart<'_>) -> Option<String> {
        embedded_text(NOTE, &parse(NOTE), part)
    }

    #[test]
    fn targets_name_notes_or_files() {
        assert!(embeds_note("Lemma"));
        assert!(embeds_note("Folder/Lemma.md"));
        assert!(embeds_note("v2.0 plans"));
        assert!(!embeds_note("Paper.pdf"));
        assert!(!embeds_note("song.mp3"));
    }

    #[test]
    fn subpaths_name_a_heading_or_a_block() {
        assert_eq!(EmbedPart::of(None), EmbedPart::Whole);
        assert_eq!(EmbedPart::of(Some("Proof")), EmbedPart::Heading("Proof"));
        assert_eq!(
            EmbedPart::of(Some("Intro#Proof")),
            EmbedPart::Heading("Proof")
        );
        assert_eq!(EmbedPart::of(Some("^claim")), EmbedPart::Block("claim"));
    }

    #[test]
    fn a_whole_note_leaves_its_frontmatter_out() {
        let whole = shown(EmbedPart::Whole).unwrap();
        assert!(whole.starts_with("# Intro"), "{whole}");
    }

    #[test]
    fn a_heading_shows_its_section() {
        assert_eq!(
            shown(EmbedPart::Heading("proof")).as_deref(),
            Some("## Proof\nStep one.\nStep two.")
        );
        assert_eq!(shown(EmbedPart::Heading("Missing")), None);
    }

    #[test]
    fn a_block_shows_without_its_marker() {
        assert_eq!(
            shown(EmbedPart::Block("claim")).as_deref(),
            Some("A claim.")
        );
        assert_eq!(shown(EmbedPart::Block("item")).as_deref(), Some("- second"));
        assert_eq!(shown(EmbedPart::Block("nope")), None);
    }
}
