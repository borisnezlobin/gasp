//! Searching by tag: `tag:physics` or `#physics` finds the notes tagged
//! physics or a tag nested under it, whether the tag is written in the
//! text or only in the frontmatter's `tags:`. Which notes have a tag
//! comes from the vault's link index, which reads both; the notes' texts
//! only give the lines to show.

use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use memchr::memmem::Finder;

use super::engine::{Note, NoteResult, TAG_WEIGHT, fold, line_hits};

/// The tag a query asks for, without its `#`: the query is `tag:name`,
/// `tag:#name` or a lone `#name`.
pub fn tag_query(query: &str) -> Option<&str> {
    let query = query.trim();
    let tag = match query.get(..4) {
        Some(prefix) if prefix.eq_ignore_ascii_case("tag:") => query[4..].trim_start(),
        _ => query.strip_prefix('#')?,
    };
    let tag = tag.strip_prefix('#').unwrap_or(tag);
    let is_tag = tag
        .chars()
        .all(|ch| ch.is_alphanumeric() || "_-/".contains(ch))
        && tag.chars().any(|ch| !ch.is_ascii_digit());
    is_tag.then_some(tag)
}

/// The notes in `tagged` (vault-relative, `/`-separated paths), each with
/// the lines where the tag is written, by path.
pub fn search_tagged(
    notes: &[Note],
    tag: &str,
    tagged: &HashSet<String>,
    generation: &AtomicUsize,
    current: usize,
) -> Vec<NoteResult> {
    // Paths compare by their parts, so a `/` matches a Windows `\`.
    let tagged: HashSet<&Path> = tagged.iter().map(Path::new).collect();
    let name = fold(tag).text;
    let mut results = Vec::new();
    for note in notes {
        if generation.load(Ordering::Relaxed) != current {
            return Vec::new();
        }
        if tagged.contains(note.path.as_path()) {
            let matches = tag_matches(note, &name);
            results.push(NoteResult {
                path: note.path.clone(),
                score: TAG_WEIGHT,
                match_count: matches.len(),
                hits: line_hits(&note.text, &matches),
            });
        }
    }
    results.sort_by(|a, b| a.path.cmp(&b.path));
    results
}

/// Where the tag (`name`, folded) is written in `note`: each `#name` (or
/// a tag nested under it) in the body, and the name on the frontmatter's
/// lines.
fn tag_matches(note: &Note, name: &str) -> Vec<Range<usize>> {
    if name.is_empty() {
        return Vec::new();
    }
    let text = &*note.text;
    let front = frontmatter_end(text);
    let written = format!("#{name}");
    let (written, name) = (Finder::new(&written), Finder::new(name));
    let in_body = note.matches(&written).filter(|range| {
        range.start >= front && ends_tag(text, range.end) && starts_word(text, range.start)
    });
    let in_front = note
        .matches(&name)
        .filter(|range| range.end <= front && ends_tag(text, range.end));
    let mut matches: Vec<Range<usize>> = in_front.chain(in_body).collect();
    matches.sort_by_key(|range| range.start);
    matches
}

/// Whether a tag match ends the tag, or a nested tag goes on after it.
fn ends_tag(text: &str, end: usize) -> bool {
    text[end..]
        .chars()
        .next()
        .is_none_or(|ch| ch == '/' || !(ch.is_alphanumeric() || ch == '_' || ch == '-'))
}

fn starts_word(text: &str, start: usize) -> bool {
    text[..start]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace)
}

/// Where the frontmatter ends, or 0 when the note has none.
fn frontmatter_end(text: &str) -> usize {
    let Some(rest) = text.strip_prefix("---\n") else {
        return 0;
    };
    rest.find("\n---")
        .map_or(0, |at| "---\n".len() + at + "\n---".len())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn tag_queries_are_recognised() {
        assert_eq!(tag_query("tag:physics"), Some("physics"));
        assert_eq!(tag_query("TAG: #physics/waves"), Some("physics/waves"));
        assert_eq!(tag_query(" #todo "), Some("todo"));
        assert_eq!(tag_query("#1"), None, "a number isn't a tag");
        assert_eq!(tag_query("physics"), None);
        assert_eq!(tag_query("#two words"), None);
    }

    #[test]
    fn frontmatter_and_inline_tags_are_found() {
        let text = "---\ntags: [physics, maths]\n---\nWaves #physics/waves and #physicsy.\n";
        let found: Vec<&str> =
            tag_matches(&Note::new(PathBuf::from("n.md"), text.into()), "physics")
                .into_iter()
                .map(|range| &text[range])
                .collect();
        assert_eq!(found, ["physics", "#physics"]);
    }

    #[test]
    fn only_tagged_notes_come_back() {
        let notes = vec![
            Note::new(
                PathBuf::from("a/Front.md"),
                "---\ntags: physics\n---\nBody".into(),
            ),
            Note::new(PathBuf::from("Other.md"), "No tag, just physics.".into()),
        ];
        let tagged: HashSet<String> = ["a/Front.md".to_string()].into();
        let results = search_tagged(&notes, "physics", &tagged, &AtomicUsize::new(0), 0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, PathBuf::from("a/Front.md"));
        assert_eq!(results[0].hits[0].line, 1, "the frontmatter's tags line");
    }
}
