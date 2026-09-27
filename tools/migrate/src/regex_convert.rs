//! Turns simple Latex Suite trigger regexes into readable triggers.
//!
//! Handled: a leading `(?<![\\A-Za-z])` (whole word), a leading `[^\\]` (not after a
//! backslash), letter and digit classes, `${GREEK}`-style variables, one alternation
//! group (split into one snippet per alternative), a trailing space (after space) and
//! a final `$`. Anything else is reported with a reason.

use editor_snippets::{CaptureRef, ExpansionPart, NamedPattern, TriggerPart};

/// What a capture group of the original regex becomes in the readable snippet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupValue {
    Parts(Vec<ExpansionPart>),
    /// The `([^\\])` group, which becomes `not after \` and is dropped from the expansion.
    Dropped,
}

/// One readable trigger; an alternation yields several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub trigger: Vec<TriggerPart>,
    /// Indexed by capture group number minus one.
    pub groups: Vec<GroupValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadableRegex {
    pub variants: Vec<Variant>,
    pub whole_word: bool,
    pub not_after_backslash: bool,
    pub after_space: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    Lit(char),
    Pattern(NamedPattern),
    NotBackslash,
    WholeWord,
    Open,
    Close,
    Alt,
    End,
}

const FIXED_TOKENS: [(&str, Token); 7] = [
    ("(?<![\\\\A-Za-z])", Token::WholeWord),
    ("(?<![A-Za-z\\\\])", Token::WholeWord),
    ("[^\\\\]", Token::NotBackslash),
    ("[A-Za-z]", Token::Pattern(NamedPattern::Letter)),
    ("[a-zA-Z]", Token::Pattern(NamedPattern::Letter)),
    ("[0-9]", Token::Pattern(NamedPattern::Digit)),
    ("\\d", Token::Pattern(NamedPattern::Digit)),
];

/// Converts a regex source, resolving `${NAME}` variables with `variable`.
pub fn to_readable(
    source: &str,
    variable: &dyn Fn(&str) -> Option<NamedPattern>,
) -> Result<ReadableRegex, String> {
    let tokens = tokenize(source, variable)?;
    let items = parse_items(&tokens)?;
    build(items)
}

fn tokenize(
    source: &str,
    variable: &dyn Fn(&str) -> Option<NamedPattern>,
) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut rest = source;
    while let Some(c) = rest.chars().next() {
        let (token, length) = next_token(rest, c, variable)?;
        tokens.push(token);
        rest = &rest[length..];
    }
    Ok(tokens)
}

fn next_token(
    rest: &str,
    c: char,
    variable: &dyn Fn(&str) -> Option<NamedPattern>,
) -> Result<(Token, usize), String> {
    if let Some((text, token)) = FIXED_TOKENS.iter().find(|(text, _)| rest.starts_with(text)) {
        return Ok((*token, text.len()));
    }
    if let Some(after) = rest.strip_prefix("${") {
        let name = after.split('}').next().unwrap_or_default();
        let pattern = variable(name)
            .ok_or_else(|| format!("uses ${{{name}}}, which has no named pattern"))?;
        return Ok((Token::Pattern(pattern), name.len() + 3));
    }
    if c == '\\' {
        return escaped(rest);
    }
    single_char_token(rest, c).map(|token| (token, c.len_utf8()))
}

fn escaped(rest: &str) -> Result<(Token, usize), String> {
    let next = rest[1..].chars().next().ok_or("ends with a backslash")?;
    if next.is_ascii_punctuation() {
        Ok((Token::Lit(next), 1 + next.len_utf8()))
    } else {
        Err(format!("uses the escape `\\{next}`"))
    }
}

/// Regex syntax the readable format can't express, and how to say so.
const UNSUPPORTED: [(char, &str); 6] = [
    ('^', "uses the anchor `^` inside the pattern"),
    ('[', "uses a character class"),
    ('.', "uses `.`"),
    ('*', "uses the repetition `*`"),
    ('+', "uses the repetition `+`"),
    ('?', "uses the repetition `?`"),
];

fn single_char_token(rest: &str, c: char) -> Result<Token, String> {
    if let Some((_, reason)) = UNSUPPORTED.iter().find(|(special, _)| *special == c) {
        return Err(reason.to_string());
    }
    let next = rest[c.len_utf8()..].chars().next();
    match c {
        '(' if next == Some('?') => Err("uses a lookaround or a special group".to_string()),
        '(' => Ok(Token::Open),
        ')' => Ok(Token::Close),
        '|' => Ok(Token::Alt),
        '$' if next.is_none() => Ok(Token::End),
        '$' => Err("uses the anchor `$` inside the pattern".to_string()),
        '{' if next.is_some_and(|n| n.is_ascii_digit()) => {
            Err("uses a counted repetition".to_string())
        }
        other => Ok(Token::Lit(other)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Token(Token),
    Group(Vec<Vec<Token>>),
}

fn parse_items(tokens: &[Token]) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index] {
            Token::Open => {
                let (group, next) = parse_group(tokens, index + 1)?;
                items.push(Item::Group(group));
                index = next;
            }
            Token::Close => return Err("has an unmatched `)`".to_string()),
            Token::Alt => return Err("uses `|` outside a group".to_string()),
            Token::End => index += 1,
            token => {
                items.push(Item::Token(token));
                index += 1;
            }
        }
    }
    Ok(items)
}

fn parse_group(tokens: &[Token], start: usize) -> Result<(Vec<Vec<Token>>, usize), String> {
    let mut alternatives = vec![Vec::new()];
    for (offset, token) in tokens[start..].iter().enumerate() {
        match token {
            Token::Close => return Ok((alternatives, start + offset + 1)),
            Token::Open => return Err("nests groups".to_string()),
            Token::Alt => alternatives.push(Vec::new()),
            Token::WholeWord | Token::End => return Err("has an anchor inside a group".to_string()),
            other => {
                if let Some(last) = alternatives.last_mut() {
                    last.push(*other);
                }
            }
        }
    }
    Err("has an unclosed `(`".to_string())
}

/// How a capture group reads once converted.
enum GroupKind {
    Patterns(Vec<NamedPattern>),
    Alternation(Vec<Alternative>),
}

#[derive(Clone)]
enum Alternative {
    Text(String),
    Pattern(NamedPattern),
}

struct Prefix {
    whole_word: bool,
    not_after_backslash: bool,
    dropped_group: bool,
}

fn build(mut items: Vec<Item>) -> Result<ReadableRegex, String> {
    let prefix = take_prefix(&mut items);
    let after_space = items.len() > 1 && items.last() == Some(&Item::Token(Token::Lit(' ')));
    if after_space {
        items.pop();
    }
    let kinds = classify_groups(&items)?;
    let alternatives = kinds
        .iter()
        .filter_map(|kind| match kind {
            GroupKind::Alternation(alternatives) if alternatives.len() > 1 => Some(alternatives),
            _ => None,
        })
        .collect::<Vec<_>>();
    if alternatives.len() > 1 {
        return Err("has more than one alternation".to_string());
    }
    let variant_count = alternatives.first().map_or(1, |list| list.len());
    let variants = (0..variant_count)
        .map(|choice| build_variant(&items, &kinds, choice, prefix.dropped_group))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ReadableRegex {
        variants,
        whole_word: prefix.whole_word,
        not_after_backslash: prefix.not_after_backslash,
        after_space,
    })
}

fn take_prefix(items: &mut Vec<Item>) -> Prefix {
    let whole_word = items.first() == Some(&Item::Token(Token::WholeWord));
    if whole_word {
        items.remove(0);
    }
    let (not_after_backslash, dropped_group) = match items.first() {
        Some(Item::Token(Token::NotBackslash)) => (true, false),
        Some(Item::Group(alternatives)) if alternatives == &vec![vec![Token::NotBackslash]] => {
            (true, true)
        }
        _ => (false, false),
    };
    if not_after_backslash {
        items.remove(0);
    }
    Prefix {
        whole_word,
        not_after_backslash,
        dropped_group,
    }
}

fn classify_groups(items: &[Item]) -> Result<Vec<GroupKind>, String> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Group(alternatives) => Some(classify_group(alternatives)),
            Item::Token(Token::WholeWord | Token::NotBackslash) => {
                Some(Err("has a lookbehind in the middle".to_string()))
            }
            Item::Token(_) => None,
        })
        .collect()
}

fn classify_group(alternatives: &[Vec<Token>]) -> Result<GroupKind, String> {
    if let [only] = alternatives {
        let patterns: Option<Vec<NamedPattern>> = only
            .iter()
            .map(|token| match token {
                Token::Pattern(pattern) => Some(*pattern),
                _ => None,
            })
            .collect();
        if let Some(patterns) = patterns.filter(|list| !list.is_empty()) {
            return Ok(GroupKind::Patterns(patterns));
        }
    }
    alternatives
        .iter()
        .map(|tokens| alternative(tokens))
        .collect::<Option<Vec<_>>>()
        .map(GroupKind::Alternation)
        .ok_or_else(|| "has a group that mixes text and patterns".to_string())
}

fn alternative(tokens: &[Token]) -> Option<Alternative> {
    if let [Token::Pattern(pattern)] = tokens {
        return Some(Alternative::Pattern(*pattern));
    }
    let text: Option<String> = tokens
        .iter()
        .map(|token| match token {
            Token::Lit(c) => Some(*c),
            _ => None,
        })
        .collect();
    text.filter(|text| !text.is_empty()).map(Alternative::Text)
}

/// Builds the readable trigger for one choice of the alternation.
struct VariantBuilder {
    trigger: Vec<TriggerPart>,
    groups: Vec<GroupValue>,
}

impl VariantBuilder {
    fn push_char(&mut self, c: char) {
        match self.trigger.last_mut() {
            Some(TriggerPart::Text(text)) => text.push(c),
            _ => self.trigger.push(TriggerPart::Text(c.to_string())),
        }
    }

    fn push_text(&mut self, text: &str) {
        text.chars().for_each(|c| self.push_char(c));
    }

    fn push_pattern(&mut self, pattern: NamedPattern) -> ExpansionPart {
        let occurrence = self
            .trigger
            .iter()
            .filter(|part| **part == TriggerPart::Named(pattern))
            .count()
            + 1;
        self.trigger.push(TriggerPart::Named(pattern));
        ExpansionPart::Capture(CaptureRef::Named {
            pattern,
            occurrence,
        })
    }

    fn push_group(&mut self, kind: &GroupKind, choice: usize) {
        let parts = match kind {
            GroupKind::Patterns(patterns) => patterns
                .iter()
                .map(|pattern| self.push_pattern(*pattern))
                .collect(),
            GroupKind::Alternation(alternatives) => {
                match &alternatives[choice.min(alternatives.len() - 1)] {
                    Alternative::Text(text) => {
                        self.push_text(text);
                        vec![ExpansionPart::Text(text.clone())]
                    }
                    Alternative::Pattern(pattern) => vec![self.push_pattern(*pattern)],
                }
            }
        };
        self.groups.push(GroupValue::Parts(parts));
    }
}

fn build_variant(
    items: &[Item],
    kinds: &[GroupKind],
    choice: usize,
    dropped_group: bool,
) -> Result<Variant, String> {
    let mut builder = VariantBuilder {
        trigger: Vec::new(),
        groups: Vec::new(),
    };
    if dropped_group {
        builder.groups.push(GroupValue::Dropped);
    }
    let mut kinds = kinds.iter();
    for item in items {
        match item {
            Item::Token(Token::Lit(c)) => builder.push_char(*c),
            Item::Token(Token::Pattern(pattern)) => {
                builder.push_pattern(*pattern);
            }
            Item::Group(_) => {
                let kind = kinds.next().ok_or("has an unexpected group")?;
                let choice = if matches!(kind, GroupKind::Alternation(list) if list.len() > 1) {
                    choice
                } else {
                    0
                };
                builder.push_group(kind, choice);
            }
            Item::Token(_) => return Err("has an unexpected token".to_string()),
        }
    }
    if builder.trigger.is_empty() {
        return Err("matches nothing readable".to_string());
    }
    Ok(Variant {
        trigger: builder.trigger,
        groups: builder.groups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variables(name: &str) -> Option<NamedPattern> {
        match name {
            "GREEK" => Some(NamedPattern::Greek),
            "SYMBOL" => Some(NamedPattern::Symbol),
            _ => None,
        }
    }

    fn convert(source: &str) -> Result<ReadableRegex, String> {
        to_readable(source, &variables)
    }

    fn text(value: &str) -> TriggerPart {
        TriggerPart::Text(value.to_string())
    }

    #[test]
    fn whole_word_followed_by_space() {
        let readable = convert(r"(?<![\\A-Za-z])forall $").unwrap();
        assert!(readable.whole_word && readable.after_space);
        assert_eq!(readable.variants[0].trigger, vec![text("forall")]);
    }

    #[test]
    fn letter_and_digit_captures() {
        let readable = convert(r"([A-Za-z])(\d)").unwrap();
        let variant = &readable.variants[0];
        assert_eq!(
            variant.trigger,
            vec![
                TriggerPart::Named(NamedPattern::Letter),
                TriggerPart::Named(NamedPattern::Digit)
            ]
        );
        assert_eq!(variant.groups.len(), 2);
    }

    #[test]
    fn two_digit_group_references_both_digits() {
        let readable = convert(r"([A-Za-z])_(\d\d)").unwrap();
        let GroupValue::Parts(parts) = &readable.variants[0].groups[1] else {
            panic!("expected parts");
        };
        assert_eq!(
            parts[1],
            ExpansionPart::Capture(CaptureRef::Named {
                pattern: NamedPattern::Digit,
                occurrence: 2
            })
        );
    }

    #[test]
    fn not_after_backslash_and_alternation_split() {
        let readable = convert(r"([^\\])(exp|log|ln)").unwrap();
        assert!(readable.not_after_backslash);
        assert_eq!(readable.variants.len(), 3);
        assert_eq!(readable.variants[2].trigger, vec![text("ln")]);
        assert_eq!(readable.variants[0].groups[0], GroupValue::Dropped);
    }

    #[test]
    fn variables_become_named_patterns() {
        let readable = convert(r"\\(${GREEK}|${SYMBOL}) sr").unwrap();
        assert_eq!(readable.variants.len(), 2);
        assert!(!readable.after_space);
        assert_eq!(
            readable.variants[1].trigger,
            vec![
                text("\\"),
                TriggerPart::Named(NamedPattern::Symbol),
                text(" sr")
            ]
        );
    }

    #[test]
    fn literal_braces_are_text() {
        let readable = convert(r"\\hat{([A-Za-z])}(\d)").unwrap();
        assert_eq!(readable.variants[0].trigger[0], text("\\hat{"));
    }

    #[test]
    fn unsupported_regex_gives_a_reason() {
        assert_eq!(
            convert(r"\\(sin|cos)([A-Za-gi-z])").unwrap_err(),
            "uses a character class"
        );
        assert!(
            convert(r"${MORE_SYMBOLS}")
                .unwrap_err()
                .contains("MORE_SYMBOLS")
        );
        assert!(
            convert(r"(a|b)(c|d)")
                .unwrap_err()
                .contains("more than one")
        );
        assert!(convert(r"a\s").unwrap_err().contains("\\s"));
    }
}
