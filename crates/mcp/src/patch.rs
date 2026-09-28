//! Changing part of a note without sending all of it: replacing exact
//! text a known number of times, or adding text at the start or end of
//! the note or of a heading's section.
//!
//! Headings are named by their text, and a heading that appears more
//! than once is named through its parents with `::`, as Obsidian's Local
//! REST API does: `Projects::Next steps`.

use std::ops::Range;

use crate::frontmatter;

/// One change to a note's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Patch<'a> {
    /// Replaces every match of `find`, which must match `expected` times.
    Replace {
        find: &'a str,
        with: &'a str,
        expected: usize,
    },
    /// Adds `content` at the end of the note, or of a heading's section.
    Append {
        content: &'a str,
        heading: Option<&'a str>,
    },
    /// Adds `content` at the start of the note's body (after any
    /// frontmatter), or just under a heading.
    Prepend {
        content: &'a str,
        heading: Option<&'a str>,
    },
}

/// `text` with `patch` applied, or why it can't be.
pub fn apply(text: &str, patch: Patch<'_>) -> Result<String, String> {
    match patch {
        Patch::Replace {
            find,
            with,
            expected,
        } => replace(text, find, with, expected),
        Patch::Append { content, heading } => {
            let at = match heading {
                Some(heading) => section(text, heading)?.end,
                None => text.len(),
            };
            Ok(insert_lines(text, at, content))
        }
        Patch::Prepend { content, heading } => {
            let at = match heading {
                Some(heading) => section(text, heading)?.start,
                None => frontmatter::split(text).map_or(0, |block| block.body_start),
            };
            Ok(insert_lines(text, at, content))
        }
    }
}

fn replace(text: &str, find: &str, with: &str, expected: usize) -> Result<String, String> {
    if find.is_empty() {
        return Err("`find` is empty".into());
    }
    let count = text.matches(find).count();
    if count == expected {
        return Ok(text.replace(find, with));
    }
    Err(match count {
        0 => "`find` isn't in the note; read it again and copy the text exactly".to_string(),
        _ => format!(
            "`find` matches {count} times, not {expected}; include more of the text around \
             it, or set `expected_count` to {count} to change them all"
        ),
    })
}

/// `content` put in at `at`, a line start or the text's end, on lines of
/// its own.
fn insert_lines(text: &str, at: usize, content: &str) -> String {
    let mut out = String::with_capacity(text.len() + content.len() + 2);
    out.push_str(&text[..at]);
    if at > 0 && !text[..at].ends_with('\n') {
        out.push('\n');
    }
    out.push_str(content);
    if !content.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&text[at..]);
    out
}

/// An ATX heading found in a note.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Heading {
    level: usize,
    text: String,
    /// The heading's line, with its line break.
    line: Range<usize>,
}

/// Where a heading's content goes: from the line after the heading to
/// the end of its last non-blank line before the next heading of the
/// same or a higher level.
fn section(text: &str, path: &str) -> Result<Range<usize>, String> {
    let headings = headings(text);
    let parents = parents(&headings);
    let mut found: Option<usize> = None;
    for (depth, name) in path.split("::").map(str::trim).enumerate() {
        // After the first name, only the found heading's own subheadings.
        let candidates: Vec<usize> = (0..headings.len())
            .filter(|index| depth == 0 || parents[*index] == found)
            .collect();
        found = Some(find_heading(&headings, &candidates, name, path)?);
    }
    let at = found.ok_or_else(|| "the heading is empty".to_string())?;
    let start = headings[at].line.end;
    let limit = headings
        .get(section_end_index(&headings, at))
        .map_or(text.len(), |next| next.line.start);
    Ok(start..content_end(text, start, limit))
}

/// Each heading's parent: the nearest heading before it of a higher
/// level.
fn parents(headings: &[Heading]) -> Vec<Option<usize>> {
    let mut open: Vec<usize> = Vec::new();
    let mut parents = Vec::with_capacity(headings.len());
    for (index, heading) in headings.iter().enumerate() {
        while open
            .last()
            .is_some_and(|last| headings[*last].level >= heading.level)
        {
            open.pop();
        }
        parents.push(open.last().copied());
        open.push(index);
    }
    parents
}

/// The index of the heading after `at` that ends its section.
fn section_end_index(headings: &[Heading], at: usize) -> usize {
    let level = headings[at].level;
    headings[at + 1..]
        .iter()
        .position(|heading| heading.level <= level)
        .map_or(headings.len(), |offset| at + 1 + offset)
}

/// The end of the last non-blank line in `start..limit`, or `start` when
/// every line there is blank.
fn content_end(text: &str, start: usize, limit: usize) -> usize {
    let body = &text[start..limit];
    let kept = body.trim_end_matches(['\n', '\r', ' ', '\t']);
    if kept.is_empty() {
        return start;
    }
    // Keep the last line's own break, so what's added starts a new line.
    let rest = &body[kept.len()..];
    let line_break = rest.find('\n').map_or(0, |at| at + 1);
    start + kept.len() + line_break
}

fn find_heading(
    headings: &[Heading],
    candidates: &[usize],
    name: &str,
    path: &str,
) -> Result<usize, String> {
    let level = name.bytes().take_while(|byte| *byte == b'#').count();
    let wanted = name[level..].trim();
    let matches: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|index| {
            let heading = &headings[*index];
            heading.text == wanted && (level == 0 || heading.level == level)
        })
        .collect();
    match matches.as_slice() {
        [only] => Ok(*only),
        [] => Err(format!(
            "there's no heading {wanted:?} (from {path:?}) in the note"
        )),
        many => Err(format!(
            "the heading {wanted:?} appears {} times; name its parent too, as \"Parent::{wanted}\"",
            many.len()
        )),
    }
}

/// Every ATX heading outside frontmatter and code blocks.
fn headings(text: &str) -> Vec<Heading> {
    let start = frontmatter::split(text).map_or(0, |block| block.body_start);
    let mut headings = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut offset = start;
    for line in text[start..].split_inclusive('\n') {
        let range = offset..offset + line.len();
        offset += line.len();
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if let Some(open) = fence {
            if closes_fence(trimmed, open) {
                fence = None;
            }
            continue;
        }
        fence = opens_fence(trimmed);
        if fence.is_none()
            && let Some((level, text)) = atx_heading(trimmed)
        {
            headings.push(Heading {
                level,
                text,
                line: range,
            });
        }
    }
    headings
}

/// `(level, text)` for an ATX heading line.
fn atx_heading(line: &str) -> Option<(usize, String)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let level = rest.bytes().take_while(|byte| *byte == b'#').count();
    let after = &rest[level..];
    let spaced = after.is_empty() || after.starts_with([' ', '\t']);
    if !(1..=6).contains(&level) || !spaced {
        return None;
    }
    let text = after.trim().trim_end_matches('#').trim_end();
    Some((level, text.to_string()))
}

fn opens_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let mark = trimmed
        .chars()
        .next()
        .filter(|ch| *ch == '`' || *ch == '~')?;
    let count = trimmed.chars().take_while(|ch| *ch == mark).count();
    (count >= 3).then_some((mark, count))
}

fn closes_fence(line: &str, (mark, count): (char, usize)) -> bool {
    let trimmed = line.trim();
    trimmed.chars().all(|ch| ch == mark) && trimmed.chars().count() >= count
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ntitle: Plan\n---\n# Plan\n\nIntro.\n\n## Tasks\n\n- one\n- two\n\n\
                        ## Notes\n\nSome notes.\n\n### Tasks\n\nNested.\n";

    fn append(text: &str, content: &str, heading: Option<&str>) -> Result<String, String> {
        apply(text, Patch::Append { content, heading })
    }

    fn prepend(text: &str, content: &str, heading: Option<&str>) -> Result<String, String> {
        apply(text, Patch::Prepend { content, heading })
    }

    #[test]
    fn replace_checks_the_count() {
        let replace = |find, with, expected| {
            apply(
                "a cat and a cat",
                Patch::Replace {
                    find,
                    with,
                    expected,
                },
            )
        };
        assert_eq!(replace("cat", "dog", 2).unwrap(), "a dog and a dog");
        let error = replace("cat", "dog", 1).unwrap_err();
        assert!(error.contains("matches 2 times, not 1"), "{error}");
        assert!(
            replace("bird", "dog", 1)
                .unwrap_err()
                .contains("isn't in the note")
        );
        assert!(replace("", "dog", 1).is_err());
    }

    #[test]
    fn append_and_prepend_to_the_whole_note() {
        assert_eq!(append("a\n", "b", None).unwrap(), "a\nb\n");
        assert_eq!(append("a", "b\n", None).unwrap(), "a\nb\n");
        assert_eq!(append("", "b", None).unwrap(), "b\n");
        // Prepending goes after the frontmatter.
        assert_eq!(
            prepend("---\nx: 1\n---\nbody\n", "top", None).unwrap(),
            "---\nx: 1\n---\ntop\nbody\n"
        );
        assert_eq!(prepend("body\n", "top", None).unwrap(), "top\nbody\n");
    }

    #[test]
    fn append_goes_after_the_sections_last_line() {
        let result = append(NOTE, "- three", Some("Plan::Tasks")).unwrap();
        assert!(
            result.contains("- one\n- two\n- three\n\n## Notes"),
            "{result}"
        );
        // A section's subsections are part of it.
        let result = append(NOTE, "End.", Some("Notes")).unwrap();
        assert!(result.ends_with("Nested.\nEnd.\n"), "{result}");
    }

    #[test]
    fn prepend_goes_just_under_the_heading() {
        let result = prepend(NOTE, "First.", Some("## Notes")).unwrap();
        assert!(
            result.contains("## Notes\nFirst.\n\nSome notes."),
            "{result}"
        );
    }

    #[test]
    fn repeated_headings_are_named_through_their_parents() {
        let error = append(NOTE, "x", Some("Tasks")).unwrap_err();
        assert!(error.contains("appears 2 times"), "{error}");
        let result = append(NOTE, "More.", Some("Notes::Tasks")).unwrap();
        assert!(result.ends_with("Nested.\nMore.\n"), "{result}");
        // A level picks one too.
        assert!(append(NOTE, "x", Some("### Tasks")).is_ok());
        let error = append(NOTE, "x", Some("Missing")).unwrap_err();
        assert!(error.contains("no heading \"Missing\""), "{error}");
    }

    #[test]
    fn headings_in_code_and_empty_sections() {
        let text = "# Top\n```\n# not a heading\n```\n## Empty\n## Next\ntext";
        let result = append(text, "added", Some("Empty")).unwrap();
        assert_eq!(
            result,
            "# Top\n```\n# not a heading\n```\n## Empty\nadded\n## Next\ntext"
        );
        assert!(append(text, "x", Some("not a heading")).is_err());
        // A heading on the last line, with no line break after it.
        assert_eq!(append("# End", "x", Some("End")).unwrap(), "# End\nx\n");
    }
}
