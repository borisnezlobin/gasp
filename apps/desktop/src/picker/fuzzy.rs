//! Fuzzy matching for the pickers.
//!
//! A query matches a candidate when its characters appear in order,
//! ignoring case and diacritics. Among the ways to place them, the scorer
//! picks the best one with a dynamic programme over (query char, candidate
//! char): every matched character scores, characters that start a path
//! segment, a word or a camelCase hump score extra, runs of adjacent matches
//! score extra, and gaps between matches cost a little. A space in the query
//! matches any separator, so `daily note` finds `Daily/notes.md`.

use unicode_normalization::char::{decompose_canonical, is_combining_mark};

const SCORE_MATCH: i32 = 16;
const BONUS_FIRST_CHAR: i32 = 10;
const BONUS_PATH_START: i32 = 10;
const BONUS_WORD_START: i32 = 8;
const BONUS_CAMEL: i32 = 7;
const BONUS_CONSECUTIVE: i32 = 5;
const BONUS_EXACT_CASE: i32 = 1;
const PENALTY_GAP_START: i32 = -3;
const PENALTY_GAP_EXTEND: i32 = -1;
const PENALTY_LEADING: i32 = -1;
const MAX_PENALTY_LEADING: i32 = -3;
const UNREACHABLE: i32 = i32::MIN / 4;
const NO_POSITION: u32 = u32::MAX;

/// Letters without a canonical decomposition that people type without the
/// mark.
const SPECIAL_FOLDS: [(char, char); 8] = [
    ('ø', 'o'),
    ('ł', 'l'),
    ('đ', 'd'),
    ('ß', 's'),
    ('æ', 'a'),
    ('œ', 'o'),
    ('ı', 'i'),
    ('ħ', 'h'),
];

/// Lowercases `c` and strips its diacritics, one char in and one out so
/// positions stay aligned.
pub fn fold(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    if lower.is_ascii() {
        return lower;
    }
    if let Some((_, plain)) = SPECIAL_FOLDS.iter().find(|(from, _)| *from == lower) {
        return *plain;
    }
    let mut base = None;
    decompose_canonical(lower, |part| {
        if base.is_none() && !is_combining_mark(part) {
            base = Some(part);
        }
    });
    base.unwrap_or(lower)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharClass {
    Lower,
    Upper,
    Digit,
    PathSeparator,
    Delimiter,
}

fn class_of(c: char) -> CharClass {
    if c == '/' || c == '\\' {
        CharClass::PathSeparator
    } else if !c.is_alphanumeric() {
        CharClass::Delimiter
    } else if c.is_uppercase() {
        CharClass::Upper
    } else if c.is_numeric() {
        CharClass::Digit
    } else {
        CharClass::Lower
    }
}

fn is_separator(class: CharClass) -> bool {
    matches!(class, CharClass::PathSeparator | CharClass::Delimiter)
}

/// The extra score for matching a character of class `current` that
/// follows one of class `previous`.
fn boundary_bonus(previous: Option<CharClass>, current: CharClass) -> i32 {
    if is_separator(current) {
        return 0;
    }
    match (previous, current) {
        (None, _) => BONUS_FIRST_CHAR,
        (Some(CharClass::PathSeparator), _) => BONUS_PATH_START,
        (Some(CharClass::Delimiter), _) => BONUS_WORD_START,
        (Some(CharClass::Lower), CharClass::Upper) => BONUS_CAMEL,
        (Some(CharClass::Lower | CharClass::Upper), CharClass::Digit) => BONUS_CAMEL,
        _ => 0,
    }
}

/// One character of a candidate, with what the scorer needs precomputed.
#[derive(Clone, Copy, Debug)]
struct CandidateChar {
    folded: char,
    original: char,
    separator: bool,
    bonus: i8,
    byte: u32,
}

/// A string prepared for matching. Build it once and match it against
/// every query.
#[derive(Clone, Debug, Default)]
pub struct Candidate {
    chars: Vec<CandidateChar>,
}

impl Candidate {
    pub fn new(text: &str) -> Candidate {
        let mut previous = None;
        let chars = text
            .char_indices()
            .map(|(byte, original)| {
                let class = class_of(original);
                let bonus = boundary_bonus(previous, class);
                previous = Some(class);
                CandidateChar {
                    folded: fold(original),
                    original,
                    separator: is_separator(class),
                    bonus: bonus as i8,
                    byte: byte as u32,
                }
            })
            .collect();
        Candidate { chars }
    }

    /// The number of chars.
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// The char index of the byte offset `byte`, or the length past the end.
    pub fn char_index_of_byte(&self, byte: usize) -> usize {
        self.chars.partition_point(|c| (c.byte as usize) < byte)
    }
}

#[derive(Clone, Copy, Debug)]
struct QueryChar {
    folded: char,
    original: char,
    separator: bool,
}

impl QueryChar {
    fn matches(&self, candidate: &CandidateChar) -> bool {
        if self.separator {
            candidate.separator
        } else {
            self.folded == candidate.folded
        }
    }
}

/// A prepared query. Runs of whitespace become one separator wildcard and
/// combining marks are dropped.
#[derive(Clone, Debug, Default)]
pub struct Query {
    chars: Vec<QueryChar>,
}

impl Query {
    pub fn new(text: &str) -> Query {
        let mut chars: Vec<QueryChar> = Vec::new();
        for original in text.trim().chars().filter(|c| !is_combining_mark(*c)) {
            let separator = original.is_whitespace();
            if separator && chars.last().is_some_and(|c| c.separator) {
                continue;
            }
            chars.push(QueryChar {
                folded: fold(original),
                original,
                separator,
            });
        }
        Query { chars }
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
}

/// Where and how well a query matched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i32,
    /// Byte offsets of the matched chars in the candidate, ascending.
    pub positions: Vec<usize>,
}

/// Scores queries against candidates, reusing its buffers between calls.
#[derive(Debug, Default)]
pub struct Matcher {
    /// Best score with query char `i` matched at candidate char `j`.
    matched: Vec<i32>,
    /// The candidate position of query char `i - 1` for that best score.
    previous: Vec<u32>,
    /// Best score of query chars `..=i` ending at or before `j`, with the
    /// gap since the last match already charged.
    reach: Vec<i32>,
    /// The position of the last match behind `reach`.
    reach_from: Vec<u32>,
    /// The bonus of the boundary where the run of adjacent matches ending
    /// at each cell started.
    run_bonus: Vec<i8>,
    /// The candidate length the buffers are laid out for.
    width: usize,
}

impl Matcher {
    pub fn new() -> Matcher {
        Matcher::default()
    }

    /// Scores `query` against the whole candidate.
    pub fn score(&mut self, query: &Query, candidate: &Candidate) -> Option<FuzzyMatch> {
        self.score_from(query, candidate, 0)
    }

    /// Scores `query` against the candidate's chars from char index `start`
    /// on, such as a file name inside a path. Positions stay relative to
    /// the whole candidate.
    pub fn score_from(
        &mut self,
        query: &Query,
        candidate: &Candidate,
        start: usize,
    ) -> Option<FuzzyMatch> {
        let chars = candidate.chars.get(start..)?;
        let query = &query.chars;
        if query.is_empty() {
            return Some(FuzzyMatch::default());
        }
        if !is_subsequence(query, chars) {
            return None;
        }
        self.fill(query, chars);
        self.best_match(query.len(), chars)
    }

    fn fill(&mut self, query: &[QueryChar], chars: &[CandidateChar]) {
        self.width = chars.len();
        let cells = query.len() * chars.len();
        for buffer in [&mut self.matched, &mut self.reach] {
            buffer.clear();
            buffer.resize(cells, UNREACHABLE);
        }
        for buffer in [&mut self.previous, &mut self.reach_from] {
            buffer.clear();
            buffer.resize(cells, NO_POSITION);
        }
        self.run_bonus.clear();
        self.run_bonus.resize(cells, 0);
        for (row, query_char) in query.iter().enumerate() {
            self.fill_row(row, query_char, chars);
        }
    }

    fn fill_row(&mut self, row: usize, query_char: &QueryChar, chars: &[CandidateChar]) {
        let width = chars.len();
        let mut reach = UNREACHABLE;
        let mut reach_from = NO_POSITION;
        for (column, candidate_char) in chars.iter().enumerate().skip(row) {
            let cell = row * width + column;
            if query_char.matches(candidate_char) {
                let best = self.cell_score(row, column, query_char, candidate_char);
                if reachable(best.score) {
                    self.matched[cell] = best.score;
                    self.previous[cell] = best.previous;
                    self.run_bonus[cell] = best.run_bonus as i8;
                }
            }
            let extended = reach.saturating_add(PENALTY_GAP_EXTEND);
            if self.matched[cell] >= extended && reachable(self.matched[cell]) {
                reach = self.matched[cell];
                reach_from = column as u32;
            } else {
                reach = extended;
            }
            self.reach[cell] = reach;
            self.reach_from[cell] = reach_from;
        }
    }

    /// The best way to match `query_char` at `column`.
    fn cell_score(
        &self,
        row: usize,
        column: usize,
        query_char: &QueryChar,
        candidate_char: &CandidateChar,
    ) -> Cell {
        let exact_case = if query_char.original == candidate_char.original {
            BONUS_EXACT_CASE
        } else {
            0
        };
        let own_bonus = i32::from(candidate_char.bonus);
        if row == 0 {
            let leading = (PENALTY_LEADING * column as i32).max(MAX_PENALTY_LEADING);
            return Cell::start(SCORE_MATCH + own_bonus + exact_case + leading, own_bonus);
        }
        let above = (row - 1) * self.width;
        let gapped = self.gapped_cell(
            above,
            column,
            SCORE_MATCH + own_bonus + exact_case,
            own_bonus,
        );
        // A run keeps the bonus of the boundary it started on.
        let run_bonus = own_bonus
            .max(i32::from(self.run_bonus[above + column - 1]))
            .max(BONUS_CONSECUTIVE);
        let consecutive = Cell {
            score: self.matched[above + column - 1]
                .saturating_add(SCORE_MATCH + run_bonus + exact_case),
            previous: column as u32 - 1,
            run_bonus,
        };
        if consecutive.score >= gapped.score {
            consecutive
        } else {
            gapped
        }
    }

    /// Matching at `column` after a gap since the previous query char,
    /// which sits in the row starting at `above`.
    fn gapped_cell(&self, above: usize, column: usize, gain: i32, own_bonus: i32) -> Cell {
        if column < 2 {
            return Cell::start(UNREACHABLE, own_bonus);
        }
        let from = above + column - 2;
        Cell {
            score: self.reach[from].saturating_add(PENALTY_GAP_START + gain),
            previous: self.reach_from[from],
            run_bonus: own_bonus,
        }
    }

    fn best_match(&self, rows: usize, chars: &[CandidateChar]) -> Option<FuzzyMatch> {
        let width = chars.len();
        let last_row = (rows - 1) * width;
        let (column, score) = self.matched[last_row..last_row + width]
            .iter()
            .enumerate()
            .filter(|(_, score)| reachable(**score))
            .max_by_key(|(column, score)| (**score, std::cmp::Reverse(*column)))?;
        let mut positions = vec![0; rows];
        let mut column = column;
        for row in (0..rows).rev() {
            positions[row] = chars[column].byte as usize;
            let previous = self.previous[row * width + column];
            if previous == NO_POSITION {
                break;
            }
            column = previous as usize;
        }
        Some(FuzzyMatch {
            score: *score,
            positions,
        })
    }
}

/// One way to match a query char at a candidate position.
struct Cell {
    score: i32,
    previous: u32,
    run_bonus: i32,
}

impl Cell {
    fn start(score: i32, run_bonus: i32) -> Cell {
        Cell {
            score,
            previous: NO_POSITION,
            run_bonus,
        }
    }
}

fn reachable(score: i32) -> bool {
    score > UNREACHABLE / 2
}

fn is_subsequence(query: &[QueryChar], chars: &[CandidateChar]) -> bool {
    let mut remaining = chars.iter();
    query
        .iter()
        .all(|query_char| remaining.any(|c| query_char.matches(c)))
}

/// Matches `query` against `text` in one call. Pickers that match many
/// candidates should keep a [`Matcher`] and prepared [`Candidate`]s.
pub fn fuzzy_match(query: &str, text: &str) -> Option<FuzzyMatch> {
    Matcher::new().score(&Query::new(query), &Candidate::new(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(query: &str, text: &str) -> i32 {
        fuzzy_match(query, text)
            .unwrap_or_else(|| panic!("{query:?} should match {text:?}"))
            .score
    }

    fn positions(query: &str, text: &str) -> Vec<usize> {
        fuzzy_match(query, text).unwrap().positions
    }

    fn ranks_above(query: &str, better: &str, worse: &str) {
        let (a, b) = (score(query, better), score(query, worse));
        assert!(
            a > b,
            "{query:?}: {better:?} ({a}) should beat {worse:?} ({b})"
        );
    }

    #[test]
    fn needs_every_query_char_in_order() {
        assert!(fuzzy_match("abc", "a-b-c").is_some());
        assert!(fuzzy_match("acb", "abc").is_none());
        assert!(fuzzy_match("abcd", "abc").is_none());
        assert!(fuzzy_match("x", "").is_none());
    }

    #[test]
    fn an_empty_query_matches_everything_with_no_positions() {
        assert_eq!(fuzzy_match("", "anything"), Some(FuzzyMatch::default()));
        assert_eq!(fuzzy_match("   ", ""), Some(FuzzyMatch::default()));
    }

    #[test]
    fn ignores_case() {
        assert_eq!(positions("READ", "readme"), [0, 1, 2, 3]);
        assert_eq!(positions("readme", "README.md"), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn exact_case_breaks_ties() {
        ranks_above("Read", "Readme", "readme");
    }

    #[test]
    fn ignores_diacritics_both_ways() {
        assert_eq!(positions("cafe", "Café crème"), [0, 1, 2, 3]);
        assert_eq!(positions("crème", "Cafe creme"), [5, 6, 7, 8, 9]);
        assert!(fuzzy_match("zurich", "Zürich").is_some());
        assert!(fuzzy_match("oslo", "Øslo").is_some());
        assert!(fuzzy_match("lodz", "Łódź").is_some());
        assert!(fuzzy_match("strasse", "Straße").is_none());
        assert!(fuzzy_match("strae", "Straße").is_some());
    }

    #[test]
    fn decomposed_accents_match_like_composed_ones() {
        let decomposed = "Cafe\u{301}";
        assert_eq!(positions("café", decomposed), [0, 1, 2, 3]);
        assert_eq!(positions("cafe\u{301}", "café"), [0, 1, 2, 3]);
    }

    #[test]
    fn positions_are_byte_offsets() {
        assert_eq!(positions("éa", "éta"), [0, 3]);
        assert_eq!(positions("日記", "私の日記"), [6, 9]);
    }

    #[test]
    fn contiguous_runs_beat_scattered_chars() {
        ranks_above("note", "notebook", "n-o-t-e");
        ranks_above("plan", "my plan", "pxlxaxn");
    }

    #[test]
    fn word_starts_beat_the_middle_of_words() {
        ranks_above("fb", "foo bar", "fxxbxx");
        ranks_above("mp", "meeting-plan", "mapper");
        ranks_above("ar", "annual review", "garden");
    }

    #[test]
    fn camel_case_humps_count_as_word_starts() {
        ranks_above("gp", "getPath", "grape");
        ranks_above("fnm", "fileNameMatch", "finnmark");
    }

    #[test]
    fn path_segment_starts_score_highest() {
        ranks_above("pn", "projects/notes", "projects-notes");
        assert_eq!(positions("pn", "projects/notes"), [0, 9]);
    }

    #[test]
    fn prefers_word_starts_over_the_first_occurrence() {
        assert_eq!(positions("rev", "archive/review"), [8, 9, 10]);
        assert_eq!(positions("mol", "my model list"), [3, 4, 9]);
    }

    #[test]
    fn earlier_matches_rank_slightly_higher() {
        ranks_above("plan", "plan b", "a plan");
    }

    #[test]
    fn spaces_match_any_separator() {
        assert_eq!(positions("daily n", "daily/notes"), [0, 1, 2, 3, 4, 5, 6]);
        assert!(fuzzy_match("a b", "a_b").is_some());
        assert!(fuzzy_match("a b", "ab").is_none());
        assert_eq!(positions("  a   b ", "a-b"), [0, 1, 2]);
    }

    #[test]
    fn scoring_part_of_a_candidate_keeps_whole_positions() {
        let candidate = Candidate::new("notes/plan.md");
        let start = candidate.char_index_of_byte(6);
        let mut matcher = Matcher::new();
        let found = matcher
            .score_from(&Query::new("pl"), &candidate, start)
            .unwrap();
        assert_eq!(found.positions, [6, 7]);
        assert!(
            matcher
                .score_from(&Query::new("no"), &candidate, start)
                .is_none()
        );
    }

    #[test]
    fn a_matcher_can_be_reused_across_lengths() {
        let mut matcher = Matcher::new();
        let long = Candidate::new("a very long candidate string with words");
        let short = Candidate::new("aw");
        let query = Query::new("aw");
        assert!(matcher.score(&query, &long).is_some());
        assert_eq!(matcher.score(&query, &short).unwrap().positions, [0, 1]);
        assert!(matcher.score(&Query::new("zz"), &long).is_none());
    }

    #[test]
    fn every_position_is_a_matching_char_boundary() {
        let texts = ["Ünïcödé ==日本語== #タグ", "a/b\\c_d-e.f", "x"];
        for text in texts {
            for query in ["u", "ab", "cdef", "タ", "e f"] {
                let Some(found) = fuzzy_match(query, text) else {
                    continue;
                };
                assert!(found.positions.windows(2).all(|pair| pair[0] < pair[1]));
                for position in found.positions {
                    assert!(text.is_char_boundary(position), "{query:?} in {text:?}");
                }
            }
        }
    }
}
