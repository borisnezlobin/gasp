//! The emoji and symbol picker's catalogue: every emoji by its GitHub
//! shortcodes, a hand-picked list of Unicode symbols a writer reaches for,
//! and the ASCII emoticons people type by name. It's built once, the first
//! time `:` is typed, and searched with the same fuzzy matcher as the
//! quick switcher.

use std::sync::OnceLock;

use crate::picker::fuzzy::{Candidate, Matcher, Query};

/// Symbols by the names people search for them with: LaTeX's command
/// names for maths, so `:rightarrow` and `:leq` work as they do in `$…$`,
/// and plain names with `_` between words for typography.
const SYMBOLS: &[(&str, &str)] = &[
    ("rightarrow", "→"),
    ("leftarrow", "←"),
    ("uparrow", "↑"),
    ("downarrow", "↓"),
    ("leftrightarrow", "↔"),
    ("implies", "⇒"),
    ("impliedby", "⇐"),
    ("iff", "⇔"),
    ("mapsto", "↦"),
    ("pm", "±"),
    ("times", "×"),
    ("div", "÷"),
    ("neq", "≠"),
    ("approx", "≈"),
    ("equiv", "≡"),
    ("leq", "≤"),
    ("geq", "≥"),
    ("ll", "≪"),
    ("gg", "≫"),
    ("infty", "∞"),
    ("sqrt", "√"),
    ("sum", "∑"),
    ("prod", "∏"),
    ("int", "∫"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("in", "∈"),
    ("notin", "∉"),
    ("subset", "⊂"),
    ("subseteq", "⊆"),
    ("cup", "∪"),
    ("cap", "∩"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("neg", "¬"),
    ("land", "∧"),
    ("lor", "∨"),
    ("emptyset", "∅"),
    ("propto", "∝"),
    ("therefore", "∴"),
    ("because", "∵"),
    ("cdot", "·"),
    ("prime", "′"),
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("pi", "π"),
    ("rho", "ρ"),
    ("sigma", "σ"),
    ("tau", "τ"),
    ("phi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("em_dash", "—"),
    ("en_dash", "–"),
    ("ellipsis", "…"),
    ("bullet", "•"),
    ("section", "§"),
    ("pilcrow", "¶"),
    ("dagger", "†"),
    ("double_dagger", "‡"),
    ("copyright", "©"),
    ("registered", "®"),
    ("trademark", "™"),
    ("degree", "°"),
    ("per_mille", "‰"),
    ("half", "½"),
    ("third", "⅓"),
    ("quarter", "¼"),
    ("three_quarters", "¾"),
    ("squared", "²"),
    ("cubed", "³"),
    ("euro", "€"),
    ("pound", "£"),
    ("yen", "¥"),
    ("cent", "¢"),
    ("bitcoin", "₿"),
    ("checkmark", "✓"),
    ("ballot_x", "✗"),
    ("star_outline", "☆"),
    ("guillemet_left", "«"),
    ("guillemet_right", "»"),
    ("non_breaking_space", "\u{a0}"),
];

/// ASCII and kaomoji emoticons, by name.
const EMOTICONS: &[(&str, &str)] = &[
    ("shrug", "¯\\_(ツ)_/¯"),
    ("tableflip", "(╯°□°)╯︵ ┻━┻"),
    ("unflip", "┬─┬ノ( º _ ºノ)"),
    ("lenny", "( ͡° ͜ʖ ͡°)"),
    ("disapproval", "ಠ_ಠ"),
    ("happy_face", ":)"),
    ("sad_face", ":("),
    ("wink_face", ";)"),
    ("grin_face", ":D"),
    ("tongue_face", ":P"),
    ("surprised_face", ":O"),
    ("heart_text", "<3"),
    ("meh", "-_-"),
    ("cheer", "\\o/"),
];

/// What an entry is, which decides how its row looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Emoji,
    Symbol,
    Emoticon,
}

/// One searchable name and what it inserts.
pub struct Entry {
    pub name: &'static str,
    pub glyph: &'static str,
    pub kind: EntryKind,
    candidate: Candidate,
}

/// A match: the entry and the matched byte offsets in its name.
pub struct Hit {
    pub entry: &'static Entry,
    pub positions: Vec<usize>,
}

fn entry(name: &'static str, glyph: &'static str, kind: EntryKind) -> Entry {
    Entry {
        name,
        glyph,
        kind,
        candidate: Candidate::new(name),
    }
}

/// Every entry, emoji first in Unicode's order (smileys, then people,
/// and so on), so ties go to the more common one.
pub fn catalogue() -> &'static [Entry] {
    static CATALOGUE: OnceLock<Vec<Entry>> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        let emoji = emojis::iter().flat_map(|emoji| {
            emoji
                .shortcodes()
                .map(move |code| entry(code, emoji.as_str(), EntryKind::Emoji))
        });
        let symbols = SYMBOLS
            .iter()
            .map(|(name, glyph)| entry(name, glyph, EntryKind::Symbol));
        let emoticons = EMOTICONS
            .iter()
            .map(|(name, glyph)| entry(name, glyph, EntryKind::Emoticon));
        emoji.chain(symbols).chain(emoticons).collect()
    })
}

/// Up to `limit` entries matching `query`, best first. A name that starts
/// with the query beats one that only contains it, and a shorter name
/// beats a longer one, so `:smi` offers 😄 before 😼. Each glyph shows
/// once, under its best-matching name.
pub fn search(query: &str, limit: usize) -> Vec<Hit> {
    let prepared = Query::new(query);
    let mut matcher = Matcher::new();
    let mut hits: Vec<(i32, usize, Vec<usize>)> = catalogue()
        .iter()
        .enumerate()
        .filter_map(|(at, entry)| {
            let found = matcher.score(&prepared, &entry.candidate)?;
            // Matching case breaks the tie between `:gamma` and `:Gamma`.
            let prefix = if entry.name.starts_with(query) {
                PREFIX_BONUS + 1
            } else if entry
                .name
                .get(..query.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(query))
            {
                PREFIX_BONUS
            } else {
                0
            };
            let score = found.score + prefix - entry.name.len() as i32;
            Some((score, at, found.positions))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let entries = catalogue();
    let mut seen = std::collections::HashSet::new();
    hits.into_iter()
        .filter(|(_, at, _)| seen.insert(entries[*at].glyph))
        .take(limit)
        .map(|(_, at, positions)| Hit {
            entry: &entries[at],
            positions,
        })
        .collect()
}

/// How much starting with the query counts for, against the matcher's
/// scores for gaps and word boundaries.
const PREFIX_BONUS: i32 = 1000;

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphs(query: &str) -> Vec<&'static str> {
        search(query, 5).iter().map(|hit| hit.entry.glyph).collect()
    }

    #[test]
    fn prefixes_rank_first() {
        assert_eq!(glyphs("smile")[0], "😄");
        assert_eq!(glyphs("heart")[0], "❤️");
        assert_eq!(glyphs("+1")[0], "👍");
    }

    #[test]
    fn symbols_and_emoticons_are_found_by_name() {
        assert_eq!(glyphs("shrug")[..2], ["🤷", "¯\\_(ツ)_/¯"]);
        assert_eq!(glyphs("rightarrow")[0], "→");
        assert_eq!(glyphs("leq")[0], "≤");
        assert_eq!(glyphs("em_d")[0], "—");
        assert_eq!(glyphs("lambda")[0], "λ");
    }

    #[test]
    fn fuzzy_matches_skip_letters() {
        let found = search("thmup", 3);
        assert_eq!(found[0].entry.glyph, "👍");
        assert_eq!(found[0].entry.name, "thumbsup");
    }

    #[test]
    fn each_glyph_shows_once() {
        let found = search("laugh", 50);
        let laughing = found.iter().filter(|hit| hit.entry.glyph == "😆").count();
        assert_eq!(laughing, 1);
    }

    #[test]
    fn searching_is_quick() {
        catalogue();
        let started = std::time::Instant::now();
        for query in ["s", "sm", "smi", "smil", "smile"] {
            search(query, 50);
        }
        let per_query = started.elapsed() / 5;
        // Debug builds are several times slower than release; this still
        // catches an accidental quadratic.
        assert!(per_query.as_millis() < 50, "{per_query:?} per query");
    }
}
