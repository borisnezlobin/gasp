//! Find and replace inside one note: plain or regex queries, with case and
//! whole-word options. Matches are byte ranges in the text searched, never
//! overlapping and never empty.

use std::fmt;
use std::ops::Range;

use regex::{Regex, RegexBuilder};

use crate::document::{Document, Selection};
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

/// The command id replace transactions are recorded under.
pub const REPLACE: &str = "find.replace";

/// How a query is matched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FindOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

/// Why a query could not be compiled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindError(pub String);

impl fmt::Display for FindError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for FindError {}

/// A compiled query.
#[derive(Clone, Debug)]
pub struct FindQuery {
    pattern: Regex,
    options: FindOptions,
}

impl FindQuery {
    /// Compiles `query`. An empty query finds nothing and gives `None`.
    pub fn new(query: &str, options: FindOptions) -> Result<Option<FindQuery>, FindError> {
        if query.is_empty() {
            return Ok(None);
        }
        let source = if options.regex {
            query.to_owned()
        } else {
            regex::escape(query)
        };
        let pattern = RegexBuilder::new(&source)
            .case_insensitive(!options.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|error| FindError(error.to_string()))?;
        Ok(Some(FindQuery { pattern, options }))
    }

    pub fn options(&self) -> FindOptions {
        self.options
    }

    /// Every match in `text`, in order.
    pub fn matches(&self, text: &str) -> Vec<Range<usize>> {
        self.pattern
            .find_iter(text)
            .map(|found| found.range())
            .filter(|range| !range.is_empty())
            .filter(|range| !self.options.whole_word || is_whole_word(text, range))
            .collect()
    }

    /// The text that replaces the match at `range`. Regex replacements
    /// expand `$1` and `${name}` groups; plain ones are taken literally.
    pub fn replacement_for(&self, text: &str, range: &Range<usize>, replacement: &str) -> String {
        if !self.options.regex {
            return replacement.to_owned();
        }
        let Some(captures) = self
            .pattern
            .captures_at(text, range.start)
            .filter(|captures| captures.get(0).is_some_and(|all| all.range() == *range))
        else {
            return replacement.to_owned();
        };
        let mut expanded = String::new();
        captures.expand(replacement, &mut expanded);
        expanded
    }

    /// The change that replaces every match in `text`.
    pub fn replace_all(&self, text: &str, replacement: &str) -> ChangeSet {
        let edits = self
            .matches(text)
            .into_iter()
            .map(|range| {
                let insert = self.replacement_for(text, &range, replacement);
                TextEdit::new(range, insert)
            })
            .collect();
        ChangeSet::new(edits).expect("matches never overlap")
    }
}

/// Whether `range` starts and ends on word boundaries in `text`.
fn is_whole_word(text: &str, range: &Range<usize>) -> bool {
    let before = text[..range.start].chars().next_back();
    let after = text[range.end..].chars().next();
    let first = text[range.clone()].chars().next();
    let last = text[range.clone()].chars().next_back();
    let joins = |outside: Option<char>, inside: Option<char>| {
        outside.is_some_and(is_word_char) && inside.is_some_and(is_word_char)
    };
    !joins(before, first) && !joins(after, last)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// The index of the first match at or after `offset`, wrapping to the first
/// match. `None` when there are no matches.
pub fn match_at_or_after(matches: &[Range<usize>], offset: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(
        matches
            .iter()
            .position(|range| range.start >= offset)
            .unwrap_or(0),
    )
}

/// The index after `current`, wrapping around.
pub fn next_index(current: usize, count: usize) -> usize {
    if count == 0 { 0 } else { (current + 1) % count }
}

/// The index before `current`, wrapping around.
pub fn previous_index(current: usize, count: usize) -> usize {
    if count == 0 {
        0
    } else {
        (current + count - 1) % count
    }
}

/// One undoable transaction that replaces every match in `doc`, leaving
/// the cursor after the last replacement. `None` when nothing matches.
pub fn replace_all_transaction(
    doc: &Document,
    query: &FindQuery,
    replacement: &str,
    timestamp_ms: u64,
) -> Option<Transaction> {
    let text = doc.to_string();
    let changes = query.replace_all(&text, replacement);
    let last = changes.edits().last()?;
    let cursor = changes.map_offset(last.range.end, crate::transaction::Assoc::Before);
    Some(
        Transaction::new(changes, Origin::command(REPLACE), timestamp_ms)
            .with_selection(Selection::cursor(cursor)),
    )
}

/// One transaction that replaces the match at `range`, selecting the
/// inserted text. `None` when `range` is no longer a match.
pub fn replace_one_transaction(
    doc: &Document,
    query: &FindQuery,
    range: Range<usize>,
    replacement: &str,
    timestamp_ms: u64,
) -> Option<Transaction> {
    let text = doc.to_string();
    if !query.matches(&text).contains(&range) {
        return None;
    }
    let insert = query.replacement_for(&text, &range, replacement);
    let end = range.start + insert.len();
    let changes = ChangeSet::replace(range.clone(), insert);
    Some(
        Transaction::new(changes, Origin::command(REPLACE), timestamp_ms)
            .with_selection(Selection::cursor(end)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::EditorState;

    fn find(text: &str, query: &str, options: FindOptions) -> Vec<Range<usize>> {
        FindQuery::new(query, options)
            .unwrap()
            .map_or_else(Vec::new, |query| query.matches(text))
    }

    fn plain() -> FindOptions {
        FindOptions::default()
    }

    #[test]
    fn plain_queries_ignore_case_by_default() {
        assert_eq!(find("Cat cat CAT", "cat", plain()), vec![0..3, 4..7, 8..11]);
    }

    #[test]
    fn case_sensitive_queries_match_exactly() {
        let options = FindOptions {
            case_sensitive: true,
            ..plain()
        };
        assert_eq!(find("Cat cat CAT", "cat", options), vec![4..7]);
    }

    #[test]
    fn whole_word_skips_matches_inside_words() {
        let options = FindOptions {
            whole_word: true,
            ..plain()
        };
        assert_eq!(
            find("cat concat cat_x cat.", "cat", options),
            vec![0..3, 17..20]
        );
        assert_eq!(find("a.b a.bc", "a.b", options), vec![0..3]);
    }

    #[test]
    fn plain_queries_escape_regex_syntax() {
        assert_eq!(find("a.b axb", "a.b", plain()), vec![0..3]);
        assert_eq!(find("(x) x", "(x)", plain()), vec![0..3]);
    }

    #[test]
    fn regex_queries_use_the_pattern() {
        let options = FindOptions {
            regex: true,
            ..plain()
        };
        assert_eq!(find("a1 b22 c", r"\d+", options), vec![1..2, 4..6]);
        assert_eq!(find("one\ntwo", "^t", options), vec![4..5]);
    }

    #[test]
    fn invalid_regexes_are_errors() {
        let options = FindOptions {
            regex: true,
            ..plain()
        };
        assert!(FindQuery::new("(", options).is_err());
    }

    #[test]
    fn empty_queries_and_empty_matches_find_nothing() {
        assert!(FindQuery::new("", plain()).unwrap().is_none());
        let options = FindOptions {
            regex: true,
            ..plain()
        };
        assert_eq!(find("abc", "x*", options), Vec::<Range<usize>>::new());
    }

    #[test]
    fn unicode_matches_are_byte_ranges_on_char_boundaries() {
        let text = "Ünïcode ünïcode 日本語";
        assert_eq!(find(text, "ÜNÏ", plain()), vec![0..5, 10..15]);
        assert_eq!(find(text, "本", plain()), vec![23..26]);
        let options = FindOptions {
            whole_word: true,
            ..plain()
        };
        assert_eq!(find(text, "日本", options), Vec::<Range<usize>>::new());
    }

    #[test]
    fn repeated_text_matches_without_overlap() {
        assert_eq!(find("aaaa", "aa", plain()), vec![0..2, 2..4]);
        assert_eq!(find("aaa", "aa", plain()), vec![0..2]);
    }

    #[test]
    fn regex_replacements_expand_groups() {
        let options = FindOptions {
            regex: true,
            ..plain()
        };
        let query = FindQuery::new(r"(\w+)@(\w+)", options).unwrap().unwrap();
        let text = "a@b and c@d";
        let changes = query.replace_all(text, "$2 at $1");
        assert_eq!(changes.apply_to_string(text).unwrap(), "b at a and d at c");
    }

    #[test]
    fn plain_replacements_are_literal() {
        let query = FindQuery::new("x", plain()).unwrap().unwrap();
        let changes = query.replace_all("x y x", "$1");
        assert_eq!(changes.apply_to_string("x y x").unwrap(), "$1 y $1");
    }

    #[test]
    fn replace_all_is_one_undo_step() {
        let doc = Document::from("red, Red, blue");
        let query = FindQuery::new("red", plain()).unwrap().unwrap();
        let transaction = replace_all_transaction(&doc, &query, "green", 0).unwrap();
        let mut state = EditorState::new(doc);
        state.apply(transaction).unwrap();
        assert_eq!(state.doc().to_string(), "green, green, blue");
        assert_eq!(state.selection().primary().head, 12);
        assert!(state.undo(1));
        assert_eq!(state.doc().to_string(), "red, Red, blue");
        assert!(!state.undo(2));
    }

    #[test]
    fn replace_all_without_matches_is_none() {
        let doc = Document::from("abc");
        let query = FindQuery::new("z", plain()).unwrap().unwrap();
        assert!(replace_all_transaction(&doc, &query, "y", 0).is_none());
    }

    #[test]
    fn replace_one_checks_the_range_still_matches() {
        let doc = Document::from("one two one");
        let query = FindQuery::new("one", plain()).unwrap().unwrap();
        assert!(replace_one_transaction(&doc, &query, 1..4, "1", 0).is_none());
        let transaction = replace_one_transaction(&doc, &query, 8..11, "1", 0).unwrap();
        let mut state = EditorState::new(doc);
        state.apply(transaction).unwrap();
        assert_eq!(state.doc().to_string(), "one two 1");
        assert_eq!(state.selection().primary().head, 9);
    }

    #[test]
    fn indexes_wrap_around() {
        let matches = vec![2..3, 5..6, 9..10];
        assert_eq!(match_at_or_after(&matches, 0), Some(0));
        assert_eq!(match_at_or_after(&matches, 5), Some(1));
        assert_eq!(match_at_or_after(&matches, 10), Some(0));
        assert_eq!(match_at_or_after(&[], 0), None);
        assert_eq!(next_index(2, 3), 0);
        assert_eq!(previous_index(0, 3), 2);
        assert_eq!(next_index(0, 0), 0);
    }
}
