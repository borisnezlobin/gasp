//! The first two layers of the grammar checker: mechanical checks and
//! spelling, both from Harper.
//!
//! Harper's mechanical rules (repeated words, stray spaces, a or an) are
//! deterministic and rarely wrong, so only those run; its style and
//! grammar rules flag too much that's fine. Spelling skips any word the
//! vault uses in several notes, lone letters, and words that are names
//! more often than typos: acronyms, words with capitals or digits inside
//! (macOS, GPT4), and capitalised words inside a sentence.
//!
//! A [`Checker`] is slow to build (Harper compiles every rule), so build
//! one per thread and keep it.

use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use harper_core::linting::{LintGroup, LintGroupConfig, LintKind, Suggestion};
use harper_core::spell::FstDictionary;
use harper_core::{Dialect, Document, TokenStringExt};

use crate::markdown::Unit;
use crate::projection::Projection;
use crate::spelling::Speller;

/// What kind of problem a flag is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlagKind {
    Spelling,
    /// Spacing, repeated words, articles and the like.
    Mechanical,
}

/// One problem in a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flag {
    /// Where it is in the note.
    pub range: Range<usize>,
    pub kind: FlagKind,
    /// The rule that found it, as Harper names it.
    pub rule: String,
    /// A plain sentence saying what's wrong.
    pub message: String,
    /// Text that could replace the range, best first. Empty text removes it.
    pub replacements: Vec<String>,
}

impl Flag {
    /// The flag moved by `delta` bytes, as when its paragraph moved.
    pub fn shifted(&self, delta: isize) -> Flag {
        let shift = |at: usize| at.saturating_add_signed(delta);
        Flag {
            range: shift(self.range.start)..shift(self.range.end),
            ..self.clone()
        }
    }
}

/// The rules that run, with the message each flag shows. `None` keeps
/// Harper's own message, which names the right word.
const RULES: &[(&str, Option<&str>)] = &[
    (
        "SpellCheck",
        Some("This word isn’t in the dictionary or in your other notes."),
    ),
    (
        "RepeatedWords",
        Some("The same word appears twice in a row."),
    ),
    ("Spaces", Some("There’s more than one space here.")),
    (
        "NoFrenchSpaces",
        Some("There’s more than one space after this full stop."),
    ),
    ("AnA", None),
    (
        "CorrectNumberSuffix",
        Some("The ending doesn’t match the number."),
    ),
    ("EllipsisLength", Some("An ellipsis has three dots.")),
];

/// Suggestions a flag offers at most.
const MAX_REPLACEMENTS: usize = 3;

/// Which English the dictionary checks against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum English {
    #[default]
    American,
    British,
    Canadian,
    Australian,
}

impl English {
    fn dialect(self) -> Dialect {
        match self {
            English::American => Dialect::American,
            English::British => Dialect::British,
            English::Canadian => Dialect::Canadian,
            English::Australian => Dialect::Australian,
        }
    }
}

/// What a checker checks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckOptions {
    pub spelling: bool,
    pub english: English,
}

/// The rule that finds misspellings.
const SPELL_CHECK: &str = "SpellCheck";

/// Runs the mechanical and spelling checks on units of prose.
pub struct Checker {
    /// Harper's mechanical rules. Spelling runs apart from them, through
    /// `speller`.
    group: LintGroup,
    speller: Speller,
    /// Lower-cased words the vault uses in several notes.
    known: Arc<HashSet<String>>,
    /// Lower-cased phrases the writer dismissed.
    ignored: Arc<HashSet<String>>,
    options: CheckOptions,
}

impl Checker {
    pub fn new(options: CheckOptions) -> Checker {
        let dictionary = FstDictionary::curated();
        let dialect = options.english.dialect();
        let mut group = LintGroup::new_curated(dictionary.clone(), dialect);
        // A rule missing from the config is off, and a small config is
        // quick to hash, which Harper does for every chunk it checks.
        group.config = LintGroupConfig::default();
        for (rule, _) in RULES.iter().filter(|(rule, _)| *rule != SPELL_CHECK) {
            group.config.set_rule_enabled(*rule, true);
        }
        Checker {
            group,
            speller: Speller::new(dictionary, dialect),
            known: Arc::default(),
            ignored: Arc::default(),
            options,
        }
    }

    pub fn options(&self) -> &CheckOptions {
        &self.options
    }

    /// Words (lower-cased) never flagged as misspelled.
    pub fn set_known_words(&mut self, known: Arc<HashSet<String>>) {
        self.known = known;
    }

    /// Phrases (lower-cased) never flagged.
    pub fn set_ignored(&mut self, ignored: Arc<HashSet<String>>) {
        self.ignored = ignored;
    }

    /// The flags in one unit of `source`.
    pub fn check(&mut self, source: &str, unit: &Unit) -> Vec<Flag> {
        let projection = unit.project(source);
        let text = projection.text();
        let document = Document::new_plain_english(text, self.speller.dictionary());
        let chars = CharOffsets::new(text);
        // Flags go in by rule name, as Harper orders its rules, so the
        // first of two flags on the same range is the one Harper puts first.
        let mut by_rule: BTreeMap<String, Vec<Flag>> = BTreeMap::new();
        for (rule, lints) in self.group.organized_lints(&document) {
            let flags = lints
                .iter()
                .filter_map(|lint| {
                    let range = chars.bytes(lint.span.start..lint.span.end);
                    self.flag(&rule, lint, range, source, &projection)
                })
                .collect();
            by_rule.insert(rule, flags);
        }
        if self.options.spelling {
            let misspellings = self.misspellings(&document, &chars, source, &projection);
            by_rule.insert(SPELL_CHECK.to_owned(), misspellings);
        }
        let mut flags: Vec<Flag> = by_rule.into_values().flatten().collect();
        flags.sort_by_key(|flag| (flag.range.start, flag.range.end));
        flags.dedup_by(|a, b| a.range == b.range);
        flags
    }

    /// The flag for one lint, unless it shouldn't show.
    fn flag(
        &self,
        rule: &str,
        lint: &harper_core::linting::Lint,
        range: Range<usize>,
        source: &str,
        projection: &Projection,
    ) -> Option<Flag> {
        let source_range = self.shown_range(rule, range, projection)?;
        let original = &source[source_range.clone()];
        Some(Flag {
            range: source_range,
            kind: match lint.lint_kind == LintKind::Spelling {
                true => FlagKind::Spelling,
                false => FlagKind::Mechanical,
            },
            rule: rule.to_owned(),
            message: message_for(rule, &lint.message),
            replacements: replacements(original, &lint.suggestions),
        })
    }

    /// The words Harper's dictionary doesn't have that are worth flagging,
    /// with suggestions looked up only for those.
    fn misspellings(
        &mut self,
        document: &Document,
        chars: &CharOffsets,
        source: &str,
        projection: &Projection,
    ) -> Vec<Flag> {
        let mut flags = Vec::new();
        for word in document.iter_words() {
            let spelled = document.get_span_content(&word.span);
            if self.speller.knows(word, spelled) {
                continue;
            }
            let range = chars.bytes(word.span.start..word.span.end);
            let Some(source_range) = self.shown_range(SPELL_CHECK, range, projection) else {
                continue;
            };
            let original = &source[source_range.clone()];
            let suggestions = self.speller.suggestions(spelled);
            flags.push(Flag {
                range: source_range,
                kind: FlagKind::Spelling,
                rule: SPELL_CHECK.to_owned(),
                message: message_for(SPELL_CHECK, ""),
                replacements: replacements(original, &suggestions),
            });
        }
        flags
    }

    /// Where in the note a lint from `rule` at `range` of the projected
    /// text shows, unless it shouldn't.
    fn shown_range(
        &self,
        rule: &str,
        range: Range<usize>,
        projection: &Projection,
    ) -> Option<Range<usize>> {
        // Markup, code or a line break inside the range means the lint saw
        // something the note doesn't say.
        if range.is_empty()
            || !projection.is_verbatim(range.clone())
            || projection.near_atom(range.clone())
        {
            return None;
        }
        let text = projection.text();
        let word = &text[range.clone()];
        let wanted = match rule {
            SPELL_CHECK => self.is_misspelling(text, range.clone()),
            "RepeatedWords" => !repeats_an_initial(word),
            "AnA" => word_follows(text, range.end),
            _ => true,
        };
        if !wanted || self.ignored.contains(&word.to_lowercase()) {
            return None;
        }
        Some(projection.source_range(range))
    }

    /// Whether the word at `range` of `text` is worth flagging: not one
    /// the vault uses, not a lone letter, and not a name. A capitalised
    /// word inside a sentence is taken for a name.
    fn is_misspelling(&self, text: &str, range: Range<usize>) -> bool {
        let word = text[range.clone()].trim_end_matches('.');
        let stem = word
            .strip_suffix("'s")
            .or_else(|| word.strip_suffix("’s"))
            .unwrap_or(word);
        let letters = stem.chars().filter(|c| c.is_alphabetic()).count();
        let capitalised = stem.chars().next().is_some_and(char::is_uppercase);
        !(letters < 2
            || self.known.contains(&stem.to_lowercase())
            || looks_like_a_name(stem)
            || (capitalised && !starts_sentence(text, range.start)))
    }
}

/// Whether the text before `at` ends a sentence, or there is none.
fn starts_sentence(text: &str, at: usize) -> bool {
    let before = text[..at]
        .trim_end()
        .trim_end_matches(['"', '\'', '“', '‘', '(', '[']);
    before.is_empty() || before.ends_with(['.', '!', '?', ':', '…'])
}

/// "A. A. Castellan": initials repeat without being a mistake.
fn repeats_an_initial(words: &str) -> bool {
    words
        .split_whitespace()
        .all(|word| word.trim_end_matches('.').chars().count() == 1)
}

/// Whether a word, rather than a dash or a symbol, comes after `at`.
fn word_follows(text: &str, at: usize) -> bool {
    text[at..]
        .trim_start()
        .chars()
        .next()
        .is_some_and(char::is_alphanumeric)
}

/// Acronyms and words with capitals or digits after the first letter.
fn looks_like_a_name(word: &str) -> bool {
    word.chars()
        .skip(1)
        .any(|c| c.is_uppercase() || c.is_ascii_digit())
}

fn message_for(rule: &str, harper: &str) -> String {
    let ours = RULES
        .iter()
        .find(|(name, _)| *name == rule)
        .and_then(|(_, message)| *message);
    match ours {
        Some(message) => message.to_owned(),
        None => curl_backticks(harper),
    }
}

/// Harper quotes words in backticks; the card shows curly quotes.
fn curl_backticks(message: &str) -> String {
    let mut open = true;
    message
        .chars()
        .map(|c| match c {
            '`' => {
                open = !open;
                if open { '”' } else { '“' }
            }
            other => other,
        })
        .collect()
}

/// The replacement text for each suggestion, for the flagged `original`.
fn replacements(original: &str, suggestions: &[Suggestion]) -> Vec<String> {
    let mut texts: Vec<String> = Vec::new();
    for suggestion in suggestions {
        let text = match suggestion {
            Suggestion::ReplaceWith(chars) => chars.iter().collect(),
            Suggestion::InsertAfter(chars) => format!("{original}{}", String::from_iter(chars)),
            Suggestion::Remove => String::new(),
        };
        let text = match_case(original, text);
        if text != original && !texts.contains(&text) {
            texts.push(text);
        }
    }
    // Harper ranks by closeness, which puts "approach's" ahead of
    // "approaches" for "approachs". An apostrophe the writer didn't type
    // is rarely what they meant.
    texts.sort_by_key(|text| text.contains(['\'', '’']) && !original.contains(['\'', '’']));
    texts.truncate(MAX_REPLACEMENTS);
    texts
}

/// `replacement` in the case of `original`: "A" becomes "An", not "AN".
fn match_case(original: &str, replacement: String) -> String {
    let mut chars = original.chars();
    let capitalised = chars.next().is_some_and(char::is_uppercase);
    if !capitalised || !chars.all(char::is_lowercase) {
        return replacement;
    }
    let mut out = replacement.chars();
    match out.next() {
        Some(first) => first
            .to_uppercase()
            .chain(out.flat_map(char::to_lowercase))
            .collect(),
        None => replacement,
    }
}

/// Byte offsets of each character, since Harper counts characters.
struct CharOffsets(Vec<usize>);

impl CharOffsets {
    fn new(text: &str) -> CharOffsets {
        let mut offsets: Vec<usize> = text.char_indices().map(|(at, _)| at).collect();
        offsets.push(text.len());
        CharOffsets(offsets)
    }

    fn bytes(&self, chars: Range<usize>) -> Range<usize> {
        let at = |index: usize| self.0[index.min(self.0.len() - 1)];
        at(chars.start)..at(chars.end)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::OnceLock;

    use gasp_core::syntax::parse;

    use super::*;
    use crate::markdown::{Purpose, units};

    /// One checker for every test: building one compiles every rule.
    fn checker() -> std::sync::MutexGuard<'static, Checker> {
        static CHECKER: OnceLock<Mutex<Checker>> = OnceLock::new();
        let checker = CHECKER.get_or_init(|| {
            Mutex::new(Checker::new(CheckOptions {
                spelling: true,
                english: English::American,
            }))
        });
        let mut guard = checker.lock().unwrap_or_else(|e| e.into_inner());
        guard.set_known_words(Arc::default());
        guard.set_ignored(Arc::default());
        guard
    }

    fn flags(checker: &mut Checker, text: &str) -> Vec<(String, String)> {
        let tree = parse(text);
        units(&tree, 0..text.len(), Purpose::Grammar)
            .iter()
            .flat_map(|unit| checker.check(text, unit))
            .map(|flag| (text[flag.range].to_owned(), flag.rule))
            .collect()
    }

    #[test]
    fn mechanical_problems_and_misspellings_are_flagged() {
        let mut checker = checker();
        let found = flags(
            &mut checker,
            "I saw the the cat.  It was an cat with a whisker.\n\nThis is a mispeled word.",
        );
        assert!(
            found.contains(&("the the".into(), "RepeatedWords".into())),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|(_, rule)| rule == "NoFrenchSpaces" || rule == "Spaces"),
            "{found:?}"
        );
        assert!(found.contains(&("an".into(), "AnA".into())), "{found:?}");
        assert!(
            found.contains(&("mispeled".into(), "SpellCheck".into())),
            "{found:?}"
        );
    }

    #[test]
    fn code_math_links_html_and_quotes_are_skipped() {
        let mut checker = checker();
        let text = "Run `teh cmd` and $\\alpha xyzzq$ via [wrnog](https://qwzx.io) or [[Notte]].\n\n\
                    > Quotted text is someone elses.\n\n\
                    <div>\nhtmml blok\n</div>\n\n\
                    ```\nfn mian() {}\n```\n\n\
                    Fine words <span>spann</span> here.";
        assert_eq!(flags(&mut checker, text), Vec::<(String, String)>::new());
    }

    #[test]
    fn markup_inside_a_flag_drops_it() {
        let mut checker = checker();
        // The two spaces meet around a hidden comment, not in the text.
        assert_eq!(
            flags(&mut checker, "Words %%aside%% more words."),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn vault_words_names_and_ignored_phrases_pass() {
        let mut checker = checker();
        let text = "Nezlobin wrote GitSync and GPT4 notes about teh thing.";
        let before = flags(&mut checker, text);
        assert!(
            before.iter().any(|(word, _)| word == "Nezlobin"),
            "{before:?}"
        );
        assert!(
            !before
                .iter()
                .any(|(word, _)| word == "GitSync" || word == "GPT4")
        );
        checker.set_known_words(Arc::new(HashSet::from(["nezlobin".to_owned()])));
        checker.set_ignored(Arc::new(HashSet::from(["teh".to_owned()])));
        assert_eq!(flags(&mut checker, text), Vec::<(String, String)>::new());
    }

    #[test]
    fn flags_offer_replacements() {
        let mut checker = checker();
        let tree = parse("It was the the end.");
        let unit = &units(&tree, 0..19, Purpose::Grammar)[0];
        let flag = checker.check("It was the the end.", unit).remove(0);
        assert_eq!(flag.kind, FlagKind::Mechanical);
        assert_eq!(flag.replacements, ["the"]);
        assert!(!flag.message.is_empty());
    }
}
