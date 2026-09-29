//! The words a vault uses. A word that turns up in several notes, such as
//! a name or a piece of jargon, is one the writer means, so spelling
//! leaves it alone. A typo repeated inside one note doesn't count.

use std::collections::{HashMap, HashSet};

/// Notes a word must appear in before it counts as known.
pub const NOTES_TO_LEARN: usize = 3;

/// Lower-cased words that appear in at least `min_notes` of `notes`.
pub fn learn<'a>(notes: impl IntoIterator<Item = &'a str>, min_notes: usize) -> HashSet<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for note in notes {
        for word in note_words(note) {
            *counts.entry(word).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count >= min_notes)
        .map(|(word, _)| word)
        .collect()
}

/// Where a vault keeps the phrases its writer dismissed, from its root.
/// It syncs like the rest of the config, so a dismissal holds everywhere.
pub const IGNORED_FILE: &str = concat!(editor_config::config_dir!(), "/prose/ignored.txt");

/// The dismissed phrases in the ignore file's text, lower-cased.
pub fn parse_ignored(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_lowercase)
        .collect()
}

/// The ignore file's text for `ignored`, sorted, one phrase a line.
pub fn ignored_file_text(ignored: &HashSet<String>) -> String {
    let mut phrases: Vec<&str> = ignored.iter().map(String::as_str).collect();
    phrases.sort_unstable();
    let mut text = String::from("# Phrases the grammar checker leaves alone, one per line.\n");
    for phrase in phrases {
        text.push_str(phrase);
        text.push('\n');
    }
    text
}

/// Each distinct lower-cased word of a note once.
fn note_words(text: &str) -> HashSet<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’'))
        .map(|word| word.trim_matches(['\'', '’']))
        .map(|word| {
            word.strip_suffix("'s")
                .or_else(|| word.strip_suffix("’s"))
                .unwrap_or(word)
        })
        .filter(|word| word.chars().count() > 1 && word.chars().any(char::is_alphabetic))
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_in_several_notes_are_learned() {
        let notes = [
            "Nezlobin wrote about Typst.",
            "More from nezlobin on typst and a tpyo tpyo tpyo.",
            "Nezlobin’s third note mentions Typst.",
        ];
        let known = learn(notes, 3);
        assert!(known.contains("typst"));
        assert!(known.contains("nezlobin"));
        // A typo repeated in one note isn't learned.
        assert!(!known.contains("tpyo"));
        assert!(!learn(notes, 4).contains("typst"));
    }

    #[test]
    fn ignored_phrases_round_trip_through_the_file() {
        let phrases = HashSet::from(["teh".to_owned(), "the the".to_owned()]);
        let text = ignored_file_text(&phrases);
        assert!(text.ends_with("teh\nthe the\n"), "{text}");
        assert_eq!(parse_ignored(&text), phrases);
    }
}
