//! Spelling, done the way Harper's `SpellCheck` rule does it but in two
//! steps. Harper finds suggestions for every word missing from its
//! dictionary and the checker then drops most of those words as names or
//! vault words. Here the words are found first and suggestions are only
//! looked up for the ones that will be flagged, with the same results.
//!
//! The fuzzy search behind suggestions keeps an automaton per thread, and
//! the one for words that need three edits or more takes about a second
//! and 70 MB to build. So every search runs on one thread of its own and
//! the automata are built once, whichever threads check prose.

use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, OnceLock};

use harper_core::linting::Suggestion;
use harper_core::spell::{Dictionary, FstDictionary, suggest_correct_spelling};
use harper_core::{CharString, CharStringExt, Dialect, Token, TokenKind};

/// Suggestions kept before the cache starts over, as many as Harper keeps.
const CACHE_LIMIT: usize = 10_000;
/// Suggestions a misspelling offers at most, as Harper offers.
const MAX_SUGGESTIONS: usize = 3;
/// Dictionary words the fuzzy search considers for each edit distance.
const SEARCH_LIMIT: usize = 200;

pub(crate) struct Speller {
    dictionary: Arc<FstDictionary>,
    dialect: Dialect,
    suggestions: HashMap<CharString, Vec<CharString>>,
}

impl Speller {
    pub(crate) fn new(dictionary: Arc<FstDictionary>, dialect: Dialect) -> Speller {
        Speller {
            dictionary,
            dialect,
            suggestions: HashMap::new(),
        }
    }

    pub(crate) fn dictionary(&self) -> &FstDictionary {
        &self.dictionary
    }

    /// Whether Harper takes the word `token` spells, `chars`, as correct.
    pub(crate) fn knows(&self, token: &Token, chars: &[char]) -> bool {
        let TokenKind::Word(Some(metadata)) = &token.kind else {
            return false;
        };
        metadata.dialects.is_dialect_enabled(self.dialect)
            && (self.dictionary.contains_exact_word(chars)
                || self.dictionary.contains_exact_word(&chars.to_lower()))
    }

    /// Replacements for a misspelled word, best first, in its case.
    pub(crate) fn suggestions(&mut self, word: &[char]) -> Vec<Suggestion> {
        let mut possibilities = match self.suggestions.get(word) {
            Some(cached) => cached.clone(),
            None => {
                let found = self.search(word);
                if self.suggestions.len() >= CACHE_LIMIT {
                    self.suggestions.clear();
                }
                self.suggestions.insert(word.into(), found.clone());
                found
            }
        };
        if word.first().is_some_and(|first| first.is_uppercase()) {
            possibilities.iter_mut().for_each(capitalise);
        }
        possibilities
            .iter()
            .map(|possibility| Suggestion::ReplaceWith(possibility.to_vec()))
            .collect()
    }

    fn search(&self, word: &[char]) -> Vec<CharString> {
        let search = Search {
            word: word.into(),
            dictionary: self.dictionary.clone(),
            dialect: self.dialect,
        };
        search_on_the_search_thread(search)
    }
}

/// One word to find suggestions for.
struct Search {
    word: CharString,
    dictionary: Arc<FstDictionary>,
    dialect: Dialect,
}

impl Search {
    /// The closest dictionary words in the dialect, searching further until
    /// something turns up.
    fn run(&self) -> Vec<CharString> {
        for distance in 2..5 {
            let found: Vec<CharString> = suggest_correct_spelling(
                &self.word,
                SEARCH_LIMIT,
                distance,
                self.dictionary.as_ref(),
            )
            .into_iter()
            .filter(|candidate| self.in_dialect(candidate))
            .map(CharString::from)
            .take(MAX_SUGGESTIONS)
            .collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    fn in_dialect(&self, word: &[char]) -> bool {
        self.dictionary
            .get_word_metadata(word)
            .is_some_and(|metadata| metadata.dialects.is_dialect_enabled(self.dialect))
    }
}

type Request = (Search, Sender<Vec<CharString>>);

/// Runs `search` on the one search thread, or here if that thread can't
/// be started or has gone.
fn search_on_the_search_thread(search: Search) -> Vec<CharString> {
    static SEARCHER: OnceLock<Option<Sender<Request>>> = OnceLock::new();
    let Some(searcher) = SEARCHER.get_or_init(spawn_searcher) else {
        return search.run();
    };
    let (reply, answer) = mpsc::channel();
    match searcher.send((search, reply)) {
        Ok(()) => answer.recv().unwrap_or_default(),
        Err(returned) => returned.0.0.run(),
    }
}

fn spawn_searcher() -> Option<Sender<Request>> {
    let (sender, requests) = mpsc::channel::<Request>();
    std::thread::Builder::new()
        .name("spelling suggestions".into())
        .spawn(move || {
            for (search, reply) in requests {
                reply.send(search.run()).ok();
            }
        })
        .ok()?;
    Some(sender)
}

/// Capitalises a suggestion for a capitalised misspelling, unless it has
/// capitals of its own after the first letter (like macOS).
fn capitalise(word: &mut CharString) {
    if word.iter().skip(1).any(|c| c.is_uppercase()) {
        return;
    }
    if let Some(first) = word.first_mut() {
        *first = first.to_uppercase().next().unwrap_or(*first);
    }
}
