//! Wikilinks `[[target#subpath|alias]]` and embeds `![[file|300x200]]`.

use std::ops::Range;

use super::kinds::{MarkupKind, WikiInfo};
use super::tree::Node;

/// The pieces of a wikilink's source, relative to its start.
struct Parts<'a> {
    open_len: usize,
    inner: &'a str,
    pipe: Option<usize>,
}

fn parts(source: &str) -> Parts<'_> {
    let open_len = if source.starts_with('!') { 3 } else { 2 };
    let close_len = if source.ends_with("]]") { 2 } else { 0 };
    let inner = source
        .get(open_len..source.len().saturating_sub(close_len))
        .unwrap_or_default();
    Parts {
        open_len,
        inner,
        pipe: inner.find('|'),
    }
}

pub(crate) fn parse_info(source: &str, embed: bool) -> WikiInfo {
    let parts = parts(source);
    let (target_part, after_pipe) = match parts.pipe {
        Some(at) => (&parts.inner[..at], Some(&parts.inner[at + 1..])),
        None => (parts.inner, None),
    };
    let (target, subpath) = match target_part.split_once('#') {
        Some((target, subpath)) => (target, Some(subpath.trim().to_owned())),
        None => (target_part, None),
    };
    let size = after_pipe.filter(|_| embed).and_then(parse_size);
    WikiInfo {
        target: target.trim().to_owned(),
        subpath,
        alias: after_pipe
            .filter(|_| size.is_none())
            .map(|alias| alias.trim().to_owned()),
        size,
    }
}

fn parse_size(value: &str) -> Option<(u32, Option<u32>)> {
    let value = value.trim();
    match value.split_once('x') {
        Some((width, height)) => Some((width.parse().ok()?, Some(height.parse().ok()?))),
        None => Some((value.parse().ok()?, None)),
    }
}

/// Adds the bracket, target and size markup of a wikilink or embed node.
pub(crate) fn add_markup(node: &mut Node, text: &str, embed: bool) {
    let Range { start, end } = node.range.clone();
    let parts = parts(&text[start..end]);
    let inner_start = start + parts.open_len;
    let inner_end = inner_start + parts.inner.len();
    node.add_markup(MarkupKind::WikiBracket, start..inner_start);
    if !embed && parts.pipe.is_none() {
        for (at, _) in parts.inner.match_indices('#') {
            let at = inner_start + at;
            node.add_markup(MarkupKind::WikiSubpath, at..at + 1);
        }
    }
    if let Some(pipe) = parts.pipe.map(|at| inner_start + at) {
        let has_size =
            matches!(&node.kind, super::kinds::NodeKind::Embed(info) if info.size.is_some());
        if embed && has_size {
            node.add_markup(MarkupKind::EmbedSize, pipe..inner_end);
        } else {
            node.add_markup(MarkupKind::WikiTarget, inner_start..pipe + 1);
        }
    }
    node.add_markup(MarkupKind::WikiBracket, inner_end..end);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_subpath_and_alias_are_split() {
        let info = parse_info("[[note#Heading|shown]]", false);
        assert_eq!(info.target, "note");
        assert_eq!(info.subpath.as_deref(), Some("Heading"));
        assert_eq!(info.alias.as_deref(), Some("shown"));
    }

    #[test]
    fn embed_sizes_are_parsed() {
        assert_eq!(parse_info("![[a.png|300]]", true).size, Some((300, None)));
        assert_eq!(
            parse_info("![[a.png|300x200]]", true).size,
            Some((300, Some(200)))
        );
        let captioned = parse_info("![[a.png|a cat]]", true);
        assert_eq!(captioned.size, None);
        assert_eq!(captioned.alias.as_deref(), Some("a cat"));
    }
}
