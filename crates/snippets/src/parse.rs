//! Parses one line of the readable format: `trigger → expansion  options`.

use std::fmt;

use crate::context::InputContext;
use crate::pattern::{NamedPattern, expand_patterns_in_regex};
use crate::snippet::{
    ARROW, CaptureRef, Expansion, ExpansionPart, Fire, NEWLINE_GLYPH, Options, REGEX_PREFIX,
    SPACE_GLYPH, STOP_GLYPH, Scope, Snippet, StopMark, TAB_GLYPH, Trigger, TriggerPart,
};

/// A problem in a snippet line. Lines and columns are 1-based; columns count characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// Parses a single snippet written on one line.
pub fn parse_snippet(text: &str) -> Result<Snippet, ParseError> {
    parse_line(text, 1)
}

/// Parses one snippet line, reporting errors against `line`.
pub fn parse_line(text: &str, line: usize) -> Result<Snippet, ParseError> {
    let chars: Vec<char> = text.trim_end().chars().collect();
    let parser = LineParser {
        chars: &chars,
        line,
    };
    parser.parse()
}

struct LineParser<'a> {
    chars: &'a [char],
    line: usize,
}

/// Character ranges of the three sections of a line.
struct Sections {
    trigger: (usize, usize),
    expansion: (usize, usize),
    options: (usize, usize),
}

impl LineParser<'_> {
    fn error(&self, index: usize, message: impl Into<String>) -> ParseError {
        ParseError {
            line: self.line,
            column: index + 1,
            message: message.into(),
        }
    }

    fn parse(&self) -> Result<Snippet, ParseError> {
        let sections = self.split()?;
        let trigger = self.parse_trigger(sections.trigger)?;
        let options = self.parse_options(sections.options)?;
        self.check_selection(&trigger, &options, sections.trigger.0)?;
        let expansion = self.parse_expansion(sections.expansion, &trigger, &options)?;
        Ok(Snippet {
            trigger,
            expansion,
            options,
        })
    }

    fn split(&self) -> Result<Sections, ParseError> {
        let chars = self.chars;
        let start = chars.iter().take_while(|c| c.is_whitespace()).count();
        let trigger_end = (start..chars.len())
            .find(|&i| chars[i] == ' ')
            .ok_or_else(|| self.error(chars.len(), "expected ` → ` after the trigger"))?;
        let arrow = (trigger_end..chars.len())
            .find(|&i| chars[i] != ' ')
            .unwrap_or(chars.len());
        if chars.get(arrow) != Some(&ARROW) {
            return Err(self.error(
                arrow,
                "expected `→` after the trigger (write a space inside a trigger as ␣)",
            ));
        }
        let rest = arrow + 2;
        if arrow + 1 < chars.len() && chars[arrow + 1] != ' ' {
            return Err(self.error(arrow + 1, "put a space after `→`"));
        }
        let rest = rest.min(chars.len());
        let split = (rest..chars.len().saturating_sub(1))
            .find(|&i| chars[i] == ' ' && chars[i + 1] == ' ')
            .unwrap_or(chars.len());
        Ok(Sections {
            trigger: (start, trigger_end),
            expansion: (rest, split),
            options: (split, chars.len()),
        })
    }

    fn text(&self, range: (usize, usize)) -> String {
        self.chars[range.0..range.1].iter().collect()
    }

    fn parse_trigger(&self, range: (usize, usize)) -> Result<Trigger, ParseError> {
        let text = self.text(range);
        if let Some(regex) = text.strip_prefix(REGEX_PREFIX) {
            return self.parse_regex_trigger(regex, range.0);
        }
        let mut parts = Vec::new();
        let mut index = range.0;
        while index < range.1 {
            index = self.trigger_part(index, range.1, &mut parts)?;
        }
        if parts.is_empty() {
            return Err(self.error(range.0, "the trigger is empty"));
        }
        Ok(Trigger::Pattern(merge_trigger_text(parts)))
    }

    fn parse_regex_trigger(&self, regex: &str, start: usize) -> Result<Trigger, ParseError> {
        let source = decode_glyphs(regex);
        if source.is_empty() {
            return Err(self.error(start, "`regex:` needs a regex after it"));
        }
        if let Err(problem) = fancy_regex::Regex::new(&expand_patterns_in_regex(&source)) {
            return Err(self.error(start, format!("the regex doesn't compile: {problem}")));
        }
        Ok(Trigger::Regex(source))
    }

    /// Reads one trigger part starting at `index` and returns where the next one starts.
    fn trigger_part(
        &self,
        index: usize,
        end: usize,
        parts: &mut Vec<TriggerPart>,
    ) -> Result<usize, ParseError> {
        let c = self.chars[index];
        if c == STOP_GLYPH {
            return Err(self.error(index, "a trigger can't contain a tab stop"));
        }
        if let Some((name, next)) = self.brace_name(index, end)
            && let Some((pattern, _, numbered)) = NamedPattern::parse_reference(&name)
        {
            if numbered {
                return Err(self.error(
                    index,
                    format!(
                        "write `{{{}}}` in the trigger; number it only in the expansion",
                        pattern.name()
                    ),
                ));
            }
            parts.push(TriggerPart::Named(pattern));
            return Ok(next);
        }
        parts.push(TriggerPart::Text(decode_char(c).to_string()));
        Ok(index + 1)
    }

    /// If `{name}` starts at `index`, returns the name and the index after `}`.
    fn brace_name(&self, index: usize, end: usize) -> Option<(String, usize)> {
        if self.chars[index] != '{' {
            return None;
        }
        let close = (index + 1..end).find(|&i| self.chars[i] == '}')?;
        Some((self.text((index + 1, close)), close + 1))
    }

    fn parse_options(&self, range: (usize, usize)) -> Result<Options, ParseError> {
        let mut options = OptionsBuilder::default();
        let mut start = range.0;
        while start < range.1 {
            let end = (start..range.1)
                .find(|&i| self.chars[i] == ',')
                .unwrap_or(range.1);
            let raw = self.text((start, end));
            let leading = raw.chars().take_while(|c| c.is_whitespace()).count();
            let word = raw.trim();
            if word.is_empty() {
                return Err(self.error(start + leading, "empty option"));
            }
            options
                .add(word)
                .map_err(|message| self.error(start + leading, message))?;
            start = end + 1;
        }
        Ok(options.options)
    }

    fn check_selection(
        &self,
        trigger: &Trigger,
        options: &Options,
        at: usize,
    ) -> Result<(), ParseError> {
        let single_char = trigger.literal().is_some_and(|t| t.chars().count() == 1);
        if options.on_selection && !single_char {
            return Err(self.error(
                at,
                "an `on selection` snippet needs a one-character trigger",
            ));
        }
        Ok(())
    }

    fn parse_expansion(
        &self,
        range: (usize, usize),
        trigger: &Trigger,
        options: &Options,
    ) -> Result<Expansion, ParseError> {
        let mut parts = Vec::new();
        let mut stops = StopCheck::default();
        let mut index = range.0;
        while index < range.1 {
            index = self.expansion_part(index, range.1, trigger, options, &mut parts)?;
            if let Some(ExpansionPart::Stop(stop)) = parts.last() {
                stops
                    .record(stop, index)
                    .map_err(|message| self.error(stops.last_at, message))?;
            }
        }
        Ok(Expansion {
            parts: merge_expansion_text(parts),
        })
    }

    fn expansion_part(
        &self,
        index: usize,
        end: usize,
        trigger: &Trigger,
        options: &Options,
        parts: &mut Vec<ExpansionPart>,
    ) -> Result<usize, ParseError> {
        let c = self.chars[index];
        if c == STOP_GLYPH {
            let (stop, next) = self.stop(index, end)?;
            parts.push(ExpansionPart::Stop(stop));
            return Ok(next);
        }
        if let Some((name, next)) = self.brace_name(index, end)
            && let Some(part) = self.reference(&name, index, trigger, options)?
        {
            parts.push(part);
            return Ok(next);
        }
        parts.push(ExpansionPart::Text(decode_char(c).to_string()));
        Ok(index + 1)
    }

    fn stop(&self, index: usize, end: usize) -> Result<(StopMark, usize), ParseError> {
        let digits_end = (index + 1..end)
            .find(|&i| !self.chars[i].is_ascii_digit())
            .unwrap_or(end);
        let number = if digits_end > index + 1 {
            let digits = self.text((index + 1, digits_end));
            Some(
                digits
                    .parse::<u32>()
                    .map_err(|_| self.error(index + 1, "tab stop number is too large"))?,
            )
        } else {
            None
        };
        if self.chars.get(digits_end) != Some(&'{') || digits_end >= end {
            return Ok((
                StopMark {
                    number,
                    placeholder: None,
                },
                digits_end,
            ));
        }
        let close = self.matching_brace(digits_end, end)?;
        let raw = self.text((digits_end + 1, close));
        if raw.contains(STOP_GLYPH) {
            return Err(self.error(digits_end + 1, "a placeholder can't contain a tab stop"));
        }
        let placeholder = Some(decode_glyphs(&raw)).filter(|text| !text.is_empty());
        Ok((
            StopMark {
                number,
                placeholder,
            },
            close + 1,
        ))
    }

    fn matching_brace(&self, open: usize, end: usize) -> Result<usize, ParseError> {
        let mut depth = 0usize;
        for i in open..end {
            match self.chars[i] {
                '{' => depth += 1,
                '}' if depth == 1 => return Ok(i),
                '}' => depth -= 1,
                _ => {}
            }
        }
        Err(self.error(open, "this tab stop placeholder is missing its `}`"))
    }

    /// Resolves `{name}` in an expansion. `None` means it is literal text.
    fn reference(
        &self,
        name: &str,
        index: usize,
        trigger: &Trigger,
        options: &Options,
    ) -> Result<Option<ExpansionPart>, ParseError> {
        if name == "selection" && options.on_selection {
            return Ok(Some(ExpansionPart::Selection));
        }
        if let Trigger::Regex(_) = trigger {
            return Ok(group_reference(name).map(ExpansionPart::Capture));
        }
        let Some((pattern, occurrence, _)) = NamedPattern::parse_reference(name) else {
            return Ok(None);
        };
        let count = trigger.pattern_count(pattern);
        if count == 0 {
            return Ok(None);
        }
        if occurrence > count {
            return Err(self.error(
                index,
                format!(
                    "the trigger has {count} `{{{}}}`, so `{{{name}}}` refers to nothing",
                    pattern.name()
                ),
            ));
        }
        Ok(Some(ExpansionPart::Capture(CaptureRef::Named {
            pattern,
            occurrence,
        })))
    }
}

fn group_reference(name: &str) -> Option<CaptureRef> {
    let number = name.strip_prefix("group")?.parse::<usize>().ok()?;
    (number > 0).then_some(CaptureRef::Group(number))
}

/// Tracks tab stops so plain and numbered stops aren't mixed.
#[derive(Default)]
struct StopCheck {
    plain: usize,
    numbered: usize,
    finals: usize,
    last_at: usize,
}

impl StopCheck {
    fn record(&mut self, stop: &StopMark, next: usize) -> Result<(), String> {
        self.last_at = next.saturating_sub(1);
        match stop.number {
            None => self.plain += 1,
            Some(0) => self.finals += 1,
            Some(_) => self.numbered += 1,
        }
        if self.plain > 0 && self.numbered > 0 {
            return Err("don't mix plain `●` with numbered tab stops like `●1`".to_string());
        }
        if self.finals > 1 {
            return Err("there can be only one final tab stop `●0`".to_string());
        }
        Ok(())
    }
}

#[derive(Default)]
struct OptionsBuilder {
    options: Options,
    anywhere: bool,
    fire_set: bool,
}

const OPTION_WORDS: &str = "anywhere, text, math, code, link, frontmatter, table, html, comment, \
inline math, block math, instant, on tab, whole word, after space, not after <characters>, \
on selection, priority <number>, off";

impl OptionsBuilder {
    fn add(&mut self, word: &str) -> Result<(), String> {
        if let Some(scope) = scope_from_word(word) {
            return self.add_scope(scope);
        }
        match word {
            "anywhere" => self.set_anywhere(),
            "instant" => self.set_fire(Fire::Instant),
            "on tab" => self.set_fire(Fire::OnTab),
            "whole word" => set_flag(&mut self.options.whole_word),
            "after space" => set_flag(&mut self.options.after_space),
            "on selection" => set_flag(&mut self.options.on_selection),
            "off" => set_flag(&mut self.options.off),
            _ => self.add_with_argument(word),
        }
    }

    fn add_with_argument(&mut self, word: &str) -> Result<(), String> {
        if let Some(chars) = word.strip_prefix("not after ") {
            let chars = decode_glyphs(chars.trim());
            if chars.is_empty() {
                return Err("`not after` needs the characters to avoid".to_string());
            }
            self.options.not_after = Some(chars);
            return Ok(());
        }
        if let Some(number) = word.strip_prefix("priority ") {
            self.options.priority = number
                .trim()
                .parse()
                .map_err(|_| format!("`{}` isn't a whole number", number.trim()))?;
            return Ok(());
        }
        Err(format!(
            "unknown option `{word}`; options are: {OPTION_WORDS}"
        ))
    }

    fn add_scope(&mut self, scope: Scope) -> Result<(), String> {
        if self.anywhere {
            return Err("`anywhere` can't be combined with a context".to_string());
        }
        if !self.options.scopes.contains(&scope) {
            self.options.scopes.push(scope);
        }
        Ok(())
    }

    fn set_anywhere(&mut self) -> Result<(), String> {
        if !self.options.scopes.is_empty() {
            return Err("`anywhere` can't be combined with a context".to_string());
        }
        self.anywhere = true;
        Ok(())
    }

    fn set_fire(&mut self, fire: Fire) -> Result<(), String> {
        if self.fire_set && self.options.fire != fire {
            return Err("choose either `instant` or `on tab`".to_string());
        }
        self.fire_set = true;
        self.options.fire = fire;
        Ok(())
    }
}

fn set_flag(flag: &mut bool) -> Result<(), String> {
    *flag = true;
    Ok(())
}

fn scope_from_word(word: &str) -> Option<Scope> {
    match word {
        "inline math" => Some(Scope::InlineMath),
        "block math" => Some(Scope::BlockMath),
        _ => InputContext::from_name(word).map(Scope::Context),
    }
}

fn decode_char(c: char) -> char {
    match c {
        SPACE_GLYPH => ' ',
        NEWLINE_GLYPH => '\n',
        TAB_GLYPH => '\t',
        other => other,
    }
}

/// Turns ␣, ⏎ and ⇥ into a space, a line break and a tab.
pub fn decode_glyphs(text: &str) -> String {
    text.chars().map(decode_char).collect()
}

fn merge_trigger_text(parts: Vec<TriggerPart>) -> Vec<TriggerPart> {
    let mut merged: Vec<TriggerPart> = Vec::with_capacity(parts.len());
    for part in parts {
        match (merged.last_mut(), part) {
            (Some(TriggerPart::Text(last)), TriggerPart::Text(text)) => last.push_str(&text),
            (_, part) => merged.push(part),
        }
    }
    merged
}

fn merge_expansion_text(parts: Vec<ExpansionPart>) -> Vec<ExpansionPart> {
    let mut merged: Vec<ExpansionPart> = Vec::with_capacity(parts.len());
    for part in parts {
        match (merged.last_mut(), part) {
            (Some(ExpansionPart::Text(last)), ExpansionPart::Text(text)) => last.push_str(&text),
            (_, part) => merged.push(part),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> ExpansionPart {
        ExpansionPart::Text(value.to_string())
    }

    fn stop() -> ExpansionPart {
        ExpansionPart::Stop(StopMark::default())
    }

    #[test]
    fn parses_the_plan_examples() {
        let mk = parse_snippet("mk          → $●$                     anywhere, instant").unwrap();
        assert_eq!(
            mk.trigger,
            Trigger::Pattern(vec![TriggerPart::Text("mk".into())])
        );
        assert_eq!(mk.expansion.parts, vec![text("$"), stop(), text("$")]);
        assert_eq!(mk.options.fire, Fire::Instant);
        assert!(mk.options.scopes.is_empty());

        let forall =
            parse_snippet("forall      → \\forall␣  math, instant, whole word, after space")
                .unwrap();
        assert_eq!(forall.expansion.parts, vec![text("\\forall ")]);
        assert!(forall.options.whole_word && forall.options.after_space);
        assert_eq!(
            forall.options.scopes,
            vec![Scope::Context(InputContext::Math)]
        );
    }

    #[test]
    fn named_patterns_are_captured_and_referenced() {
        let snippet = parse_snippet("{letter}{digit} → {letter}_{digit}    math, instant").unwrap();
        assert_eq!(
            snippet.trigger,
            Trigger::Pattern(vec![
                TriggerPart::Named(NamedPattern::Letter),
                TriggerPart::Named(NamedPattern::Digit)
            ])
        );
        assert_eq!(
            snippet.expansion.parts[1],
            ExpansionPart::Text("_".to_string())
        );
    }

    #[test]
    fn repeated_patterns_are_numbered_in_the_expansion() {
        let snippet = parse_snippet("pa{letter}{letter} → d{letter2}/d{letter1}  math").unwrap();
        assert_eq!(
            snippet.expansion.parts[1],
            ExpansionPart::Capture(CaptureRef::Named {
                pattern: NamedPattern::Letter,
                occurrence: 2
            })
        );
        let error = parse_snippet("pa{letter} → {letter2}  math").unwrap_err();
        assert_eq!(error.column, 14);
        assert!(error.message.contains("refers to nothing"));
    }

    #[test]
    fn braces_that_are_not_patterns_stay_literal() {
        let snippet = parse_snippet("reals → \\mathbb{R}  math").unwrap();
        assert_eq!(snippet.expansion.parts, vec![text("\\mathbb{R}")]);
        let hat = parse_snippet("{letter}hat → \\hat{{letter}}  math, instant").unwrap();
        assert_eq!(hat.expansion.parts.len(), 3);
    }

    #[test]
    fn numbered_stops_and_placeholders() {
        let snippet = parse_snippet("beg → \\begin{●1}⏎●2{x}⏎\\end{●1}●0  math").unwrap();
        let stops: Vec<_> = snippet
            .expansion
            .parts
            .iter()
            .filter_map(|part| match part {
                ExpansionPart::Stop(stop) => Some(stop.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(stops.len(), 4);
        assert_eq!(stops[1].placeholder.as_deref(), Some("x"));
        assert_eq!(stops[3].number, Some(0));
    }

    #[test]
    fn placeholders_may_nest_braces() {
        let snippet = parse_snippet("lim → \\lim_{●{n \\to {\\infty}}}  math").unwrap();
        let ExpansionPart::Stop(stop) = &snippet.expansion.parts[1] else {
            panic!("expected a stop");
        };
        assert_eq!(stop.placeholder.as_deref(), Some("n \\to {\\infty}"));
    }

    #[test]
    fn errors_point_at_the_problem() {
        let error = parse_snippet("a b → c").unwrap_err();
        assert_eq!((error.line, error.column), (1, 3));

        let error = parse_snippet("ab → c  math, sometimes").unwrap_err();
        assert_eq!(error.column, 15);
        assert!(error.message.starts_with("unknown option `sometimes`"));

        let error = parse_snippet("ab → ●1 ●  math").unwrap_err();
        assert!(error.message.contains("don't mix"));

        let error = parse_snippet("ab → c  anywhere, math").unwrap_err();
        assert_eq!(error.column, 19);

        let error = parse_snippet("ab").unwrap_err();
        assert!(error.message.contains("expected ` → `"));

        let error = parse_snippet("ab →c").unwrap_err();
        assert_eq!(error.column, 5);

        let error = parse_snippet("regex:( → c").unwrap_err();
        assert!(error.message.contains("doesn't compile"));

        let error = parse_snippet("ab → c  instant, on tab").unwrap_err();
        assert!(error.message.contains("either"));

        let error = parse_snippet("ab → {selection}  on selection").unwrap_err();
        assert!(error.message.contains("one-character"));
    }

    #[test]
    fn options_with_arguments() {
        let snippet = parse_snippet("exp → \\exp  math, not after \\, priority -2").unwrap();
        assert_eq!(snippet.options.not_after.as_deref(), Some("\\"));
        assert_eq!(snippet.options.priority, -2);
    }

    #[test]
    fn regex_triggers_refer_to_groups() {
        let snippet = parse_snippet("regex:([a-z])␣x → {group1}^{group2}  math").unwrap();
        assert_eq!(snippet.trigger, Trigger::Regex("([a-z]) x".to_string()));
        assert_eq!(
            snippet.expansion.parts[0],
            ExpansionPart::Capture(CaptureRef::Group(1))
        );
    }

    #[test]
    fn off_keeps_a_snippet_but_switches_it_off() {
        let snippet = parse_snippet("mk → $●$  text, instant, off").unwrap();
        assert!(snippet.options.off);
        assert_eq!(snippet.to_string(), "mk → $●$  text, instant, off");
    }

    #[test]
    fn empty_expansion_and_missing_options() {
        let snippet = parse_snippet("zz →   math").unwrap();
        assert!(snippet.expansion.parts.is_empty());
        let snippet = parse_snippet("zz → y").unwrap();
        assert_eq!(snippet.options, Options::default());
    }
}
