//! Sentence segmentation and word counts.
//!
//! Boundaries start from Unicode sentence boundaries (UAX #29) and then
//! drop the ones that broke Musical Text:
//!
//! - em and en dashes never end a sentence;
//! - an ellipsis ends one only when a capitalised word follows it;
//! - abbreviations, acronyms and initials (e.g., i.e., U.S., Dr., St., J.)
//!   don't end one.
//!
//! UAX #29 already leaves decimals, version numbers, times and URLs alone,
//! since no space follows their dots, and keeps a closing quote or
//! bracket after the period with its sentence.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// One sentence: its byte range in the text, without surrounding
/// whitespace, and how many words it has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentence {
    pub range: Range<usize>,
    pub words: usize,
}

/// How long a sentence reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Length {
    Short,
    Medium,
    Long,
}

/// Where short ends and long begins, in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Thresholds {
    /// Sentences with fewer words than this are short.
    pub short_below: usize,
    /// Sentences with more words than this are long.
    pub long_above: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            short_below: 7,
            long_above: 18,
        }
    }
}

impl Thresholds {
    pub fn classify(&self, words: usize) -> Length {
        if words < self.short_below {
            Length::Short
        } else if words > self.long_above {
            Length::Long
        } else {
            Length::Medium
        }
    }
}

/// Abbreviations that never end a sentence, lower-cased with their dot.
/// Words that are also ordinary words ("no.", "in.") and ones that often
/// do end a sentence ("etc.") are left out.
const ABBREVIATIONS: &[&str] = &[
    "al.", "approx.", "ca.", "cf.", "ch.", "dept.", "dr.", "eq.", "fig.", "figs.", "jr.", "mr.",
    "mrs.", "ms.", "mt.", "pp.", "prof.", "resp.", "sec.", "sr.", "st.", "vol.", "vs.", "viz.",
];

const DASHES: [char; 2] = ['—', '–'];

/// Closing quotes and brackets that may follow a sentence's last word.
fn is_closer(c: char) -> bool {
    matches!(c, '"' | '\'' | ')' | ']' | '}' | '”' | '’' | '»' | '›')
}

/// Opening quotes and brackets that may come before a word.
fn is_opener(c: char) -> bool {
    matches!(c, '"' | '\'' | '(' | '[' | '{' | '“' | '‘' | '«' | '‹')
}

/// The sentences of `text`, which should be one paragraph with its line
/// breaks already turned into spaces. Stretches with no words, such as a
/// lone symbol, aren't sentences.
pub fn sentences(text: &str) -> Vec<Sentence> {
    let mut starts = vec![0];
    starts.extend(boundaries(text));
    starts.push(text.len());
    let mut found = Vec::new();
    let mut from = 0;
    for &end in &starts[1..] {
        // A stretch with no words, such as a leading "…", joins the next.
        if let Some(sentence) = sentence_in(text, from..end) {
            found.push(sentence);
            from = end;
        }
    }
    found
}

fn sentence_in(text: &str, range: Range<usize>) -> Option<Sentence> {
    let slice = &text[range.clone()];
    let start = range.start + (slice.len() - slice.trim_start().len());
    let end = range.start + slice.trim_end().len();
    let words = word_count(&text[start..end.max(start)]);
    (words > 0).then_some(Sentence {
        range: start..end,
        words,
    })
}

/// Words in `text`: runs between spaces and dashes that hold a letter or
/// a digit, so "well-known" is one word and "then—now" is two.
pub fn word_count(text: &str) -> usize {
    text.split(|c: char| c.is_whitespace() || DASHES.contains(&c))
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
}

/// Where each sentence after the first starts.
fn boundaries(text: &str) -> Vec<usize> {
    let mut cuts = unicode_boundaries(text);
    cuts.extend(ellipsis_cuts(text));
    cuts.sort_unstable();
    cuts.dedup();
    cuts.retain(|&at| ends_sentence(&text[..at], &text[at..]));
    cuts
}

/// Line and paragraph separators, after which UAX #29 always breaks.
const SEPARATORS: [char; 5] = ['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}'];

/// Where UAX #29 starts each sentence after the first.
///
/// Its rules only break after a full stop, question or exclamation mark
/// (and the closing quotes and spaces after one), or after a separator.
/// So rather than classifying every character, only the stretches around
/// those marks are segmented, each with the context the rules look at.
fn unicode_boundaries(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    if text.contains(SEPARATORS) {
        push_segment_starts(text, 0..text.len(), &mut starts);
        return starts;
    }
    // Each stretch runs from the ASCII character before a possible
    // terminator (the rules look one letter back, past accents) to the
    // first ASCII letter after it (they look ahead to the next letter).
    // Stretches that touch are segmented together.
    let mut window: Option<Range<usize>> = None;
    for (at, _) in text.match_indices(may_end_sentence) {
        let next = ascii_before(text, at)..past_next_ascii_letter(text, at);
        window = match window {
            Some(open) if open.end >= next.start => Some(open.start..open.end.max(next.end)),
            Some(done) => {
                push_segment_starts(text, done, &mut starts);
                Some(next)
            }
            None => Some(next),
        };
    }
    if let Some(done) = window {
        push_segment_starts(text, done, &mut starts);
    }
    starts
}

/// Adds the sentence starts UAX #29 finds in `window` of `text`, past its
/// start, to `starts`.
fn push_segment_starts(text: &str, window: Range<usize>, starts: &mut Vec<usize>) {
    starts.extend(
        text[window.clone()]
            .split_sentence_bound_indices()
            .map(|(at, _)| window.start + at)
            .filter(|&at| at > window.start),
    );
}

/// A character that might end a sentence. Every sentence terminator
/// outside ASCII counts, and so, to be safe, does anything else outside it.
fn may_end_sentence(c: char) -> bool {
    matches!(c, '.' | '!' | '?') || !c.is_ascii()
}

/// Where the nearest ASCII character before `at` starts, or 0.
fn ascii_before(text: &str, at: usize) -> usize {
    text[..at].rfind(|c: char| c.is_ascii()).unwrap_or(0)
}

/// Just past the first ASCII letter after the character at `at`, or the
/// end of the text.
fn past_next_ascii_letter(text: &str, at: usize) -> usize {
    let after = at + text[at..].chars().next().map_or(0, char::len_utf8);
    text[after..]
        .find(|c: char| c.is_ascii_alphabetic())
        .map_or(text.len(), |offset| after + offset + 1)
}

/// UAX #29 doesn't treat "…" as a full stop, so these are the places
/// after one where a sentence might start.
fn ellipsis_cuts(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.match_indices('…').filter_map(|(at, ellipsis)| {
        let after = at + ellipsis.len();
        let rest = &text[after..];
        let trimmed = rest.trim_start();
        let gap = rest.len() - trimmed.len();
        (gap > 0 && !trimmed.is_empty()).then_some(after + gap)
    })
}

/// Whether a candidate boundary between `before` and `after` really ends
/// a sentence.
fn ends_sentence(before: &str, after: &str) -> bool {
    let before = before.trim_end();
    let after = after.trim_start();
    if after.starts_with(DASHES) || before.ends_with(DASHES) {
        return false;
    }
    // “Why?” she asked: a quoted question goes on in lower case.
    if before.ends_with(is_closer) && starts_lower_case(after) {
        return false;
    }
    let last = last_word(before);
    if ends_with_ellipsis(last) {
        return starts_capitalised(after);
    }
    !(is_abbreviation(last) || is_acronym(last) || is_initial(last))
}

/// The last word before a boundary, without brackets or quotes around it.
fn last_word(before: &str) -> &str {
    let word = before
        .rsplit(char::is_whitespace)
        .next()
        .unwrap_or_default();
    word.trim_end_matches(is_closer)
        .trim_start_matches(is_opener)
}

fn ends_with_ellipsis(word: &str) -> bool {
    word.ends_with('…') || word.ends_with("...")
}

fn starts_capitalised(after: &str) -> bool {
    after
        .trim_start_matches(is_opener)
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
}

fn starts_lower_case(after: &str) -> bool {
    after.chars().next().is_some_and(char::is_lowercase)
}

fn is_abbreviation(word: &str) -> bool {
    if word.is_ascii() {
        return ABBREVIATIONS
            .iter()
            .any(|abbreviation| abbreviation.eq_ignore_ascii_case(word));
    }
    let lower = word.to_lowercase();
    ABBREVIATIONS.contains(&lower.as_str())
}

/// Letters with a dot after each short run, such as U.S., e.g. or Ph.D.
fn is_acronym(word: &str) -> bool {
    let Some(body) = word.strip_suffix('.') else {
        return false;
    };
    let parts: Vec<&str> = body.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|part| (1..=2).contains(&part.len()) && part.chars().all(char::is_alphabetic))
}

/// A single capital and a dot, as in "J. R. R. Tolkien".
fn is_initial(word: &str) -> bool {
    let mut chars = word.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some('.'), None) if letter.is_uppercase()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str) -> Vec<&str> {
        sentences(text)
            .into_iter()
            .map(|sentence| &text[sentence.range])
            .collect()
    }

    #[test]
    fn plain_sentences_split_at_full_stops() {
        assert_eq!(
            split("It rained. We stayed in! Did you?"),
            ["It rained.", "We stayed in!", "Did you?"]
        );
    }

    #[test]
    fn dashes_never_end_a_sentence() {
        assert_eq!(
            split("He paused — then left. She stayed."),
            ["He paused — then left.", "She stayed."]
        );
        assert_eq!(
            split("Wait!—he said it twice."),
            ["Wait!—he said it twice."]
        );
        assert_eq!(split("Run! – or don't."), ["Run! – or don't."]);
    }

    #[test]
    fn an_ellipsis_ends_a_sentence_only_before_a_capital() {
        assert_eq!(
            split("I wonder… maybe not. Then again… Who knows?"),
            ["I wonder… maybe not.", "Then again…", "Who knows?"]
        );
        assert_eq!(
            split("Well... fine. So... It works."),
            ["Well... fine.", "So...", "It works."]
        );
        assert_eq!(split("Wait… 5 more minutes."), ["Wait… 5 more minutes."]);
    }

    #[test]
    fn abbreviations_acronyms_and_initials_stay_inside() {
        assert_eq!(
            split(
                "Ask Dr. Smith about it, e.g. Tomorrow works. The U.S. Army is big. \
                 J. R. R. Tolkien wrote it. See Fig. 3 and St. Paul, i.e. Minnesota."
            ),
            [
                "Ask Dr. Smith about it, e.g. Tomorrow works.",
                "The U.S. Army is big.",
                "J. R. R. Tolkien wrote it.",
                "See Fig. 3 and St. Paul, i.e. Minnesota."
            ]
        );
        assert!(is_acronym("Ph.D."));
        assert!(!is_acronym("today."));
    }

    #[test]
    fn numbers_times_and_urls_are_left_alone() {
        assert_eq!(
            split(
                "Version 1.2.3 shipped at 10:30 with pi at 3.14 on example.com/a.b today. Next one."
            ),
            [
                "Version 1.2.3 shipped at 10:30 with pi at 3.14 on example.com/a.b today.",
                "Next one."
            ]
        );
        assert_eq!(
            split("It costs $4.50. Cheap."),
            ["It costs $4.50.", "Cheap."]
        );
    }

    #[test]
    fn closing_quotes_and_brackets_stay_with_their_sentence() {
        assert_eq!(
            split("He said, “Stop.” Then he left. (It was late.) We slept."),
            [
                "He said, “Stop.”",
                "Then he left.",
                "(It was late.)",
                "We slept."
            ]
        );
        assert_eq!(
            split("She asked \"why?\" and waited."),
            ["She asked \"why?\" and waited."]
        );
    }

    #[test]
    fn words_count_between_spaces_and_dashes() {
        assert_eq!(word_count("A well-known fact—mostly."), 4);
        assert_eq!(word_count("It's 3.14 — roughly!"), 3);
        assert_eq!(word_count(" … — ! "), 0);
    }

    #[test]
    fn stretches_without_words_are_not_sentences() {
        assert_eq!(split("… Right."), ["… Right."]);
        assert!(sentences("  ").is_empty());
    }

    /// Random text from pieces that exercise every rule of UAX #29.
    fn random_text(state: &mut u64, pieces: usize) -> String {
        const PIECES: &[&str] = &[
            "a", "b", "Z", "Q", "5", " ", " ", "  ", ".", ".", "!", "?", ",", ";", ":", "'", "\"",
            "(", ")", "[", "]", "“", "”", "‘", "’", "…", "—", "–", "é", "e\u{301}", "\u{200d}",
            "Ω", "ω", "。", "؟", "ß", "\u{2024}", "\u{fe52}", "‼", "U.S.", "e.g.", "Dr.", "word",
            "Word", "\t", "\u{a0}", "»", "«", "-", "%", "3.14",
        ];
        let mut text = String::new();
        for _ in 0..pieces {
            *state ^= *state << 13;
            *state ^= *state >> 7;
            *state ^= *state << 17;
            text.push_str(PIECES[(*state % PIECES.len() as u64) as usize]);
        }
        text
    }

    #[test]
    fn segmenting_around_terminators_matches_segmenting_everything() {
        let mut state = 0x9e37_79b9_7f4a_7c15;
        let mut found = 0;
        for round in 0..20_000 {
            let text = random_text(&mut state, 1 + round % 40);
            let mut everything = Vec::new();
            push_segment_starts(&text, 0..text.len(), &mut everything);
            assert_eq!(unicode_boundaries(&text), everything, "{text:?}");
            found += everything.len();
        }
        assert!(found > 20_000, "only {found} boundaries");
    }

    #[test]
    fn thresholds_classify_by_word_count() {
        let thresholds = Thresholds::default();
        assert_eq!(thresholds.classify(6), Length::Short);
        assert_eq!(thresholds.classify(7), Length::Medium);
        assert_eq!(thresholds.classify(18), Length::Medium);
        assert_eq!(thresholds.classify(19), Length::Long);
    }
}
