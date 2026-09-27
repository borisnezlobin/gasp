//! The replacements table: Smart Typography and Symbols Prettifier merged into one list.

use std::fmt;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::context::InputContext;

/// Characters after which a quote opens rather than closes (besides whitespace).
const OPENING_CONTEXT: &str = "{[(<'\"‘“";

/// When a replacement fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplacementFire {
    /// As soon as the last character of `from` is typed.
    #[default]
    Instant,
    /// When a space is typed right after `from`. The space is kept.
    AfterSpace,
}

/// One entry of the table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Replacement {
    pub from: String,
    /// The replacement, or the opening quote when `closing` is set.
    pub to: String,
    /// For quotes: the closing form. The opening form is used after whitespace, an
    /// opening bracket or another quote, and at the start of the text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closing: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub fire: ReplacementFire,
    /// `from` must not follow a letter or digit.
    #[serde(default, skip_serializing_if = "is_false")]
    pub word_start: bool,
    /// Where the entry may fire. Unset means everywhere except code, math and frontmatter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<InputContext>>,
}

fn enabled_by_default() -> bool {
    true
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Replacement {
    /// An enabled, instant entry with the default contexts.
    pub fn new(from: &str, to: &str, group: &str) -> Replacement {
        Replacement {
            from: from.to_string(),
            to: to.to_string(),
            closing: None,
            group: group.to_string(),
            enabled: true,
            fire: ReplacementFire::Instant,
            word_start: false,
            contexts: None,
        }
    }

    /// Whether the entry may fire in this context.
    pub fn allows(&self, context: InputContext) -> bool {
        match &self.contexts {
            Some(contexts) => contexts.contains(&context),
            None => !matches!(
                context,
                InputContext::Code | InputContext::Math | InputContext::Frontmatter
            ),
        }
    }

    fn overlaps(&self, other: &Replacement) -> bool {
        InputContext::ALL
            .iter()
            .any(|context| self.allows(*context) && other.allows(*context))
    }

    fn word_start_ok(&self, before_from: &str) -> bool {
        !self.word_start
            || !before_from
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
    }

    fn text_after(&self, before_from: &str) -> String {
        let Some(closing) = &self.closing else {
            return self.to.clone();
        };
        let opens = before_from.chars().next_back().is_none_or(|previous| {
            previous.is_whitespace()
                || OPENING_CONTEXT.contains(previous)
                || self.to.contains(previous)
        });
        if opens {
            self.to.clone()
        } else {
            closing.clone()
        }
    }

    /// The range `from` covers in `before`, if this keystroke completes it.
    fn matched_range(&self, before: &str, typed: char) -> Option<Range<usize>> {
        let body = match self.fire {
            ReplacementFire::Instant if self.from.ends_with(typed) => before,
            ReplacementFire::AfterSpace if typed == ' ' => before.strip_suffix(' ')?,
            _ => return None,
        };
        let start = body.strip_suffix(self.from.as_str())?.len();
        self.word_start_ok(&body[..start])
            .then_some(start..body.len())
    }
}

/// The edit a replacement makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacementEdit {
    /// Byte range in the text before the cursor.
    pub replace: Range<usize>,
    pub text: String,
    /// Index of the entry that fired.
    pub entry: usize,
}

/// A problem reading a replacements file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacementsError {
    pub message: String,
}

impl fmt::Display for ReplacementsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ReplacementsError {}

/// The whole table, in priority order for equal-length matches.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacements {
    #[serde(default, rename = "replacement")]
    pub entries: Vec<Replacement>,
}

impl Replacements {
    pub fn new(entries: Vec<Replacement>) -> Replacements {
        Replacements { entries }
    }

    /// Reads a `replacements.toml` file made of `[[replacement]]` tables.
    pub fn from_toml(text: &str) -> Result<Replacements, ReplacementsError> {
        toml::from_str(text).map_err(|error| ReplacementsError {
            message: error.to_string(),
        })
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    /// The replacement for the character just typed, which `before` already ends with.
    /// The longest matching `from` wins; ties go to the earlier entry.
    pub fn find(
        &self,
        before: &str,
        typed: char,
        context: InputContext,
    ) -> Option<ReplacementEdit> {
        if !before.ends_with(typed) {
            return None;
        }
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.enabled && entry.allows(context))
            .filter_map(|(index, entry)| {
                let range = entry.matched_range(before, typed)?;
                Some((index, entry, range))
            })
            .max_by_key(|(index, entry, _)| (entry.from.len(), std::cmp::Reverse(*index)))
            .map(|(index, entry, range)| ReplacementEdit {
                text: entry.text_after(&before[..range.start]),
                replace: range,
                entry: index,
            })
    }

    /// Pairs `(shadowed, by)` of enabled entries where an instant entry always fires
    /// while the other's `from` is still being typed, so the other can never fire.
    pub fn shadowed(&self) -> Vec<(usize, usize)> {
        let mut pairs = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if let Some(by) = self.shadowing_entry(index, entry) {
                pairs.push((index, by));
            }
        }
        pairs
    }

    fn shadowing_entry(&self, index: usize, entry: &Replacement) -> Option<usize> {
        if !entry.enabled {
            return None;
        }
        let prefixes: Vec<&str> = entry
            .from
            .char_indices()
            .map(|(at, c)| &entry.from[..at + c.len_utf8()])
            .collect();
        let limit = match entry.fire {
            ReplacementFire::Instant => prefixes.len().saturating_sub(1),
            ReplacementFire::AfterSpace => prefixes.len(),
        };
        prefixes[..limit].iter().find_map(|prefix| {
            self.entries
                .iter()
                .enumerate()
                .position(|(other_index, other)| {
                    other_index != index
                        && other.enabled
                        && other.fire == ReplacementFire::Instant
                        && other.closing.is_none()
                        && other.overlaps(entry)
                        && prefix.ends_with(other.from.as_str())
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Replacements {
        let mut quote = Replacement::new("\"", "“", "Quotes");
        quote.closing = Some("”".to_string());
        let mut single = Replacement::new("'", "‘", "Quotes");
        single.closing = Some("’".to_string());
        let mut with = Replacement::new("w/", "with", "Words");
        with.fire = ReplacementFire::AfterSpace;
        let mut without = Replacement::new("w/o", "without", "Words");
        without.fire = ReplacementFire::AfterSpace;
        let mut disabled = Replacement::new("(1)", "1️⃣", "Numbers");
        disabled.enabled = false;
        disabled.fire = ReplacementFire::AfterSpace;
        let mut half = Replacement::new("1/2", "½", "Fractions");
        half.word_start = true;
        let mut equil = Replacement::new("\\equil", "\\rightleftharpoons", "misc");
        equil.fire = ReplacementFire::AfterSpace;
        equil.contexts = Some(vec![InputContext::Math]);
        let mut double_arrow = Replacement::new("<->", "↔", "Arrows");
        double_arrow.fire = ReplacementFire::AfterSpace;
        Replacements::new(vec![
            Replacement::new("--", "—", "Dashes"),
            Replacement::new("—-", "---", "Dashes"),
            Replacement::new("...", "…", "Ellipsis"),
            Replacement::new("->", "→", "Arrows"),
            Replacement::new("<-", "←", "Arrows"),
            Replacement::new(">=", "≥", "Comparisons"),
            quote,
            single,
            with,
            without,
            disabled,
            half,
            equil,
            double_arrow,
        ])
    }

    fn apply(before: &str, context: InputContext) -> String {
        let typed = before.chars().last().unwrap();
        match table().find(before, typed, context) {
            Some(edit) => {
                let mut out = before.to_string();
                out.replace_range(edit.replace, &edit.text);
                out
            }
            None => before.to_string(),
        }
    }

    #[test]
    fn dashes_ellipsis_arrows_comparisons() {
        assert_eq!(apply("a--", InputContext::Text), "a—");
        assert_eq!(apply("a—-", InputContext::Text), "a---");
        assert_eq!(apply("wait...", InputContext::Text), "wait…");
        assert_eq!(apply("a ->", InputContext::Text), "a →");
        assert_eq!(apply("a <-", InputContext::Text), "a ←");
        assert_eq!(apply("x >=", InputContext::Text), "x ≥");
    }

    #[test]
    fn quotes_open_or_close_by_the_previous_character() {
        assert_eq!(apply("\"", InputContext::Text), "“");
        assert_eq!(apply("say \"", InputContext::Text), "say “");
        assert_eq!(apply("(\"", InputContext::Text), "(“");
        assert_eq!(apply("“hi\"", InputContext::Text), "“hi”");
        assert_eq!(apply("don'", InputContext::Text), "don’");
        assert_eq!(apply("“'", InputContext::Text), "“‘");
    }

    #[test]
    fn never_in_code_or_math_unless_configured() {
        assert_eq!(apply("a--", InputContext::Code), "a--");
        assert_eq!(apply("a--", InputContext::Math), "a--");
        assert_eq!(apply("a--", InputContext::Frontmatter), "a--");
        assert_eq!(
            apply("\\equil ", InputContext::Math),
            "\\rightleftharpoons "
        );
        assert_eq!(apply("\\equil ", InputContext::Text), "\\equil ");
    }

    #[test]
    fn after_space_entries_prefer_the_longest() {
        assert_eq!(apply("go w/ ", InputContext::Text), "go with ");
        assert_eq!(apply("go w/o ", InputContext::Text), "go without ");
        assert_eq!(apply("go w/o", InputContext::Text), "go w/o");
    }

    #[test]
    fn disabled_entries_stay_off() {
        assert_eq!(apply("(1) ", InputContext::Text), "(1) ");
    }

    #[test]
    fn word_start_needs_a_boundary() {
        assert_eq!(apply("take 1/2", InputContext::Text), "take ½");
        assert_eq!(apply("11/2", InputContext::Text), "11/2");
    }

    #[test]
    fn reports_entries_an_instant_entry_shadows() {
        let table = table();
        let shadowed = table.shadowed();
        let arrow = table.entries.iter().position(|e| e.from == "<->").unwrap();
        let left = table.entries.iter().position(|e| e.from == "<-").unwrap();
        assert_eq!(shadowed, vec![(arrow, left)]);
    }

    #[test]
    fn toml_round_trip() {
        let table = table();
        let text = table.to_toml();
        assert!(text.contains("[[replacement]]"));
        assert!(text.contains("fire = \"after-space\""));
        assert_eq!(Replacements::from_toml(&text).unwrap(), table);
    }

    #[test]
    fn toml_errors_are_reported() {
        let error = Replacements::from_toml("[[replacement]]\nfrom = 1\n").unwrap_err();
        assert!(error.message.contains("from"), "{}", error.message);
    }
}
