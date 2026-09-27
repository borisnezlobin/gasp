//! Named patterns such as `{letter}` and `{greek}` that a trigger can use instead of regex.

/// Greek letter names, matching Latex Suite's default `${GREEK}` variable.
pub const GREEK_NAMES: &str = "alpha|beta|gamma|Gamma|delta|Delta|epsilon|varepsilon|zeta|eta|theta|vartheta|Theta|iota|kappa|lambda|Lambda|mu|nu|xi|omicron|pi|rho|varrho|sigma|Sigma|tau|upsilon|Upsilon|phi|varphi|Phi|chi|psi|omega|Omega";

/// Symbol command names, matching Latex Suite's default `${SYMBOL}` variable.
pub const SYMBOL_NAMES: &str = "parallel|perp|partial|nabla|hbar|ell|infty|oplus|ominus|otimes|oslash|square|star|dagger|vee|wedge|subseteq|subset|supseteq|supset|emptyset|exists|nexists|forall|implies|impliedby|iff|setminus|neg|lor|land|bigcup|bigcap|cdot|times|simeq|approx";

/// A named pattern usable in a trigger and referable in the expansion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NamedPattern {
    /// One ASCII letter.
    Letter,
    /// One ASCII digit.
    Digit,
    /// A Greek letter name such as `alpha` or `Omega`.
    Greek,
    /// A symbol command name such as `nabla` or `forall`.
    Symbol,
    /// One or more letters.
    Word,
}

const PATTERN_NAMES: [(NamedPattern, &str); 5] = [
    (NamedPattern::Letter, "letter"),
    (NamedPattern::Digit, "digit"),
    (NamedPattern::Greek, "greek"),
    (NamedPattern::Symbol, "symbol"),
    (NamedPattern::Word, "word"),
];

impl NamedPattern {
    /// The name written between braces, such as `letter`.
    pub fn name(self) -> &'static str {
        PATTERN_NAMES
            .iter()
            .find(|(pattern, _)| *pattern == self)
            .map_or("letter", |(_, name)| name)
    }

    pub fn from_name(name: &str) -> Option<NamedPattern> {
        PATTERN_NAMES
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(pattern, _)| *pattern)
    }

    /// The regex this pattern stands for, without a capture group.
    pub fn regex(self) -> String {
        match self {
            NamedPattern::Letter => "[A-Za-z]".to_string(),
            NamedPattern::Digit => "[0-9]".to_string(),
            NamedPattern::Greek => format!("(?:{GREEK_NAMES})"),
            NamedPattern::Symbol => format!("(?:{SYMBOL_NAMES})"),
            NamedPattern::Word => "\\p{L}+".to_string(),
        }
    }

    /// Splits `letter2` into the `Letter` pattern and occurrence 2; `letter` is occurrence 1.
    pub fn parse_reference(text: &str) -> Option<(NamedPattern, usize, bool)> {
        let digits_start = text
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(text.len());
        let pattern = NamedPattern::from_name(&text[..digits_start])?;
        let digits = &text[digits_start..];
        if digits.is_empty() {
            return Some((pattern, 1, false));
        }
        let occurrence = digits.parse::<usize>().ok().filter(|n| *n > 0)?;
        Some((pattern, occurrence, true))
    }
}

/// Replaces every `{name}` of a known pattern in a raw regex with its regex.
pub fn expand_patterns_in_regex(source: &str) -> String {
    let mut out = source.to_string();
    for (pattern, name) in PATTERN_NAMES {
        out = out.replace(&format!("{{{name}}}"), &pattern.regex());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_carry_an_optional_occurrence() {
        assert_eq!(
            NamedPattern::parse_reference("letter"),
            Some((NamedPattern::Letter, 1, false))
        );
        assert_eq!(
            NamedPattern::parse_reference("digit2"),
            Some((NamedPattern::Digit, 2, true))
        );
        assert_eq!(NamedPattern::parse_reference("digit0"), None);
        assert_eq!(NamedPattern::parse_reference("colour"), None);
    }

    #[test]
    fn patterns_expand_inside_raw_regex() {
        let expanded = expand_patterns_in_regex("\\\\({greek}) sr");
        assert!(expanded.starts_with("\\\\((?:alpha|beta"));
        assert!(expanded.ends_with(")) sr"));
    }
}
