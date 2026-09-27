//! Writes snippets back in the readable format, so parse → format round-trips.

use std::fmt;

use crate::snippet::{
    CaptureRef, Expansion, ExpansionPart, Fire, NEWLINE_GLYPH, Options, REGEX_PREFIX, SPACE_GLYPH,
    STOP_GLYPH, Scope, Snippet, StopMark, TAB_GLYPH, Trigger, TriggerPart,
};

impl fmt::Display for Snippet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → {}  {}",
            format_trigger(&self.trigger),
            format_expansion(self),
            format_options(&self.options)
        )
    }
}

/// The trigger column, such as `{letter}{digit}` or `regex:\\(sin|cos)`.
pub fn format_trigger(trigger: &Trigger) -> String {
    match trigger {
        Trigger::Regex(source) => format!("{REGEX_PREFIX}{}", encode_all_whitespace(source)),
        Trigger::Pattern(parts) => parts.iter().map(format_trigger_part).collect(),
    }
}

fn format_trigger_part(part: &TriggerPart) -> String {
    match part {
        TriggerPart::Text(text) => encode_all_whitespace(text),
        TriggerPart::Named(pattern) => format!("{{{}}}", pattern.name()),
    }
}

/// The expansion column.
pub fn format_expansion(snippet: &Snippet) -> String {
    let raw = expansion_with_raw_spaces(&snippet.expansion, &snippet.trigger);
    mark_ambiguous_spaces(&raw)
}

fn expansion_with_raw_spaces(expansion: &Expansion, trigger: &Trigger) -> String {
    let mut out = String::new();
    let parts = &expansion.parts;
    for (index, part) in parts.iter().enumerate() {
        match part {
            ExpansionPart::Text(text) => out.push_str(&encode_line_breaks(text)),
            ExpansionPart::Stop(stop) => out.push_str(&format_stop(stop, parts.get(index + 1))),
            ExpansionPart::Capture(capture) => out.push_str(&format_capture(capture, trigger)),
            ExpansionPart::Selection => out.push_str("{selection}"),
        }
    }
    out
}

fn format_stop(stop: &StopMark, next: Option<&ExpansionPart>) -> String {
    let mut out = STOP_GLYPH.to_string();
    if let Some(number) = stop.number {
        out.push_str(&number.to_string());
    }
    match &stop.placeholder {
        Some(placeholder) => out.push_str(&format!("{{{}}}", encode_line_breaks(placeholder))),
        None if next_needs_separator(next) => out.push_str("{}"),
        None => {}
    }
    out
}

/// A stop followed by a digit or `{` would read as a number or a placeholder.
fn next_needs_separator(next: Option<&ExpansionPart>) -> bool {
    let Some(ExpansionPart::Text(text)) = next else {
        return false;
    };
    text.starts_with(|c: char| c.is_ascii_digit() || c == '{')
}

fn format_capture(capture: &CaptureRef, trigger: &Trigger) -> String {
    match capture {
        CaptureRef::Group(number) => format!("{{group{number}}}"),
        CaptureRef::Named {
            pattern,
            occurrence,
        } if trigger.pattern_count(*pattern) > 1 => format!("{{{}{occurrence}}}", pattern.name()),
        CaptureRef::Named { pattern, .. } => format!("{{{}}}", pattern.name()),
    }
}

/// The options column, such as `math, instant, whole word`.
pub fn format_options(options: &Options) -> String {
    let mut words: Vec<String> = Vec::new();
    if options.scopes.is_empty() {
        words.push("anywhere".to_string());
    }
    words.extend(options.scopes.iter().map(|scope| scope_word(*scope)));
    if options.fire == Fire::Instant {
        words.push("instant".to_string());
    }
    push_if(&mut words, options.whole_word, "whole word");
    if let Some(chars) = &options.not_after {
        words.push(format!("not after {}", encode_all_whitespace(chars)));
    }
    push_if(&mut words, options.after_space, "after space");
    push_if(&mut words, options.on_selection, "on selection");
    if options.priority != 0 {
        words.push(format!("priority {}", options.priority));
    }
    words.join(", ")
}

fn push_if(words: &mut Vec<String>, condition: bool, word: &str) {
    if condition {
        words.push(word.to_string());
    }
}

fn scope_word(scope: Scope) -> String {
    match scope {
        Scope::Context(context) => context.name().to_string(),
        Scope::InlineMath => "inline math".to_string(),
        Scope::BlockMath => "block math".to_string(),
    }
}

fn encode_line_breaks(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' => NEWLINE_GLYPH,
            '\t' => TAB_GLYPH,
            other => other,
        })
        .collect()
}

fn encode_all_whitespace(text: &str) -> String {
    encode_line_breaks(text).replace(' ', &SPACE_GLYPH.to_string())
}

/// Writes a space as ␣ where a bare one would be trimmed or read as the options gap:
/// at either end, and in runs of two or more.
fn mark_ambiguous_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let last = chars.len().saturating_sub(1);
    let is_space = |i: usize| chars.get(i) == Some(&' ');
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let ambiguous = i == 0 || i == last || is_space(i + 1) || (i > 0 && is_space(i - 1));
            if c == ' ' && ambiguous {
                SPACE_GLYPH
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::parse::parse_snippet;

    fn round_trip(line: &str) {
        let snippet = parse_snippet(line).unwrap();
        let written = snippet.to_string();
        assert_eq!(written, line, "formatting changed the line");
        assert_eq!(parse_snippet(&written).unwrap(), snippet);
    }

    #[test]
    fn canonical_lines_round_trip() {
        round_trip("mk → $●$  anywhere, instant");
        round_trip("forall → \\forall␣  math, instant, whole word, after space");
        round_trip("{letter}{digit} → {letter}_{digit}  math, instant, priority -1");
        round_trip(
            "pa{letter}{letter} → \\frac{ \\partial {letter1} }{ \\partial {letter2} }␣  math",
        );
        round_trip("dm → $$⏎●⏎$$  text, instant, whole word");
        round_trip("beg → \\begin{●1}⏎●2⏎\\end{●1}  math, instant");
        round_trip("tayl → ●1{f}(●2{x} + ●3{h})●4  math, instant");
        round_trip("exp → \\exp  math, instant, not after \\");
        round_trip("U → \\underbrace{ {selection} }_{ ● }  math, instant, on selection");
        round_trip("regex:\\\\({greek})([A-Za-z]) → \\{group1} {group2}  math, instant");
        round_trip("e\\xi␣sts → \\exists  math, instant, priority 1");
        round_trip("pmat → \\begin{pmatrix}●\\end{pmatrix}  inline math, instant");
        round_trip("x → a␣␣b  anywhere");
    }

    #[test]
    fn stops_before_digits_and_braces_get_a_separator() {
        round_trip("q → ●{}2●{}{x}  anywhere");
    }

    #[test]
    fn padded_lines_normalise() {
        let snippet = parse_snippet("mk     → $●$        anywhere,instant").unwrap();
        assert_eq!(snippet.to_string(), "mk → $●$  anywhere, instant");
    }
}
