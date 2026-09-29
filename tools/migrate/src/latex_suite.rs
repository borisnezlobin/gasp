//! Converts Latex Suite's snippets into the readable snippet format.
//!
//! Option letters follow Latex Suite's documentation: `t` text, `m` math, `M` block
//! math, `n` inline math, `c`/`C` code (block/inline; both become `code`), `A` instant
//! (otherwise Tab), `r` regex, `v` visual (on selection), `w` word boundary (whole
//! word), `U` skip-undo (no equivalent, ignored). `T` (inside `\text{}`) has no
//! equivalent and is sent to review.

use std::collections::BTreeMap;

use gasp_snippets::{
    CaptureRef, Expansion, ExpansionPart, Fire, GREEK_NAMES, InputContext, NEWLINE_GLYPH,
    NamedPattern, Options, SPACE_GLYPH, STOP_GLYPH, SYMBOL_NAMES, Scope, Snippet, SnippetFile,
    StopMark, TAB_GLYPH, Trigger, TriggerPart, parse_line,
};

use crate::js::{self, JsValue, Spanned};
use crate::regex_convert::{GroupValue, ReadableRegex, Variant, to_readable};

/// How Latex Suite itself classes a snippet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Plain,
    Regex,
    Function,
}

/// What became of one Latex Suite snippet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Converted to readable snippets (more than one when an alternation was split).
    Readable(Vec<Snippet>),
    /// Kept as a `regex:` snippet; the string says why it isn't readable.
    RegexForm(Snippet, String),
    /// Not migrated; the string is the reason.
    Review(String),
}

/// One source snippet and its outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Converted {
    pub line: usize,
    pub trigger: String,
    pub kind: SourceKind,
    pub description: Option<String>,
    pub headings: Vec<String>,
    pub outcome: Outcome,
    pub note: Option<String>,
}

/// The migrated file and what happened to every snippet.
#[derive(Clone, Debug)]
pub struct LatexSuiteMigration {
    pub converted: Vec<Converted>,
    /// Snippets left commented out in the source (`// {trigger: ...}`), not migrated.
    pub commented_out: usize,
    pub file: SnippetFile,
}

impl LatexSuiteMigration {
    pub fn count_kind(&self, kind: SourceKind) -> usize {
        self.converted.iter().filter(|c| c.kind == kind).count()
    }

    pub fn readable(&self) -> impl Iterator<Item = &Converted> {
        self.converted
            .iter()
            .filter(|c| matches!(c.outcome, Outcome::Readable(_)))
    }

    pub fn regex_form(&self) -> impl Iterator<Item = &Converted> {
        self.converted
            .iter()
            .filter(|c| matches!(c.outcome, Outcome::RegexForm(..)))
    }

    pub fn review(&self) -> impl Iterator<Item = &Converted> {
        self.converted
            .iter()
            .filter(|c| matches!(c.outcome, Outcome::Review(_)))
    }
}

/// Migrates the contents of `plugins/obsidian-latex-suite.json`.
pub fn migrate(settings_json: &str) -> Result<LatexSuiteMigration, String> {
    let settings: serde_json::Value = serde_json::from_str(settings_json)
        .map_err(|error| format!("obsidian-latex-suite.json isn't valid JSON: {error}"))?;
    let source = settings
        .get("snippets")
        .and_then(|value| value.as_str())
        .ok_or("obsidian-latex-suite.json has no `snippets` text")?;
    let variables = snippet_variables(&settings);
    let parsed = js::parse(source).map_err(|error| format!("Latex Suite snippets, {error}"))?;
    let JsValue::Array(items) = parsed.value else {
        return Err("Latex Suite snippets aren't an array".to_string());
    };
    let converted: Vec<Converted> = items
        .iter()
        .map(|item| convert_item(item, &variables))
        .collect();
    let commented_out = items
        .iter()
        .flat_map(|item| &item.comments)
        .filter(|comment| comment.contains("trigger:"))
        .count();
    let file = build_file(&converted);
    Ok(LatexSuiteMigration {
        converted,
        commented_out,
        file,
    })
}

fn snippet_variables(settings: &serde_json::Value) -> BTreeMap<String, String> {
    let text = settings
        .get("snippetVariables")
        .and_then(|value| value.as_str())
        .unwrap_or("{}");
    let parsed: BTreeMap<String, String> = serde_json::from_str(text).unwrap_or_default();
    parsed
        .into_iter()
        .map(|(key, value)| {
            let name = key
                .trim_start_matches("${")
                .trim_end_matches('}')
                .to_string();
            (name, value)
        })
        .collect()
}

/// A Latex Suite snippet object, before conversion.
struct RawSnippet {
    trigger: RawTrigger,
    replacement: RawReplacement,
    options: String,
    priority: i32,
    description: Option<String>,
}

enum RawTrigger {
    Text(String),
    Regex { source: String, flags: String },
}

enum RawReplacement {
    Text(String),
    Function,
}

impl RawSnippet {
    fn kind(&self) -> SourceKind {
        if matches!(self.replacement, RawReplacement::Function) {
            SourceKind::Function
        } else if matches!(self.trigger, RawTrigger::Regex { .. }) || self.options.contains('r') {
            SourceKind::Regex
        } else {
            SourceKind::Plain
        }
    }

    fn trigger_text(&self) -> String {
        match &self.trigger {
            RawTrigger::Text(text) => text.clone(),
            RawTrigger::Regex { source, flags } => format!("/{source}/{flags}"),
        }
    }
}

fn read_raw(item: &Spanned) -> Result<RawSnippet, String> {
    let trigger = match item.get("trigger").map(|v| &v.value) {
        Some(JsValue::String(text)) => RawTrigger::Text(text.clone()),
        Some(JsValue::Regex { source, flags }) => RawTrigger::Regex {
            source: source.clone(),
            flags: flags.clone(),
        },
        _ => return Err("has no trigger".to_string()),
    };
    let replacement = match item.get("replacement").map(|v| &v.value) {
        Some(JsValue::String(text)) => RawReplacement::Text(text.clone()),
        Some(JsValue::Function(_)) => RawReplacement::Function,
        _ => return Err("has no replacement".to_string()),
    };
    Ok(RawSnippet {
        trigger,
        replacement,
        options: string_field(item, "options").unwrap_or_default(),
        priority: match item.get("priority").map(|v| &v.value) {
            Some(JsValue::Number(number)) => *number as i32,
            _ => 0,
        },
        description: string_field(item, "description"),
    })
}

fn string_field(item: &Spanned, key: &str) -> Option<String> {
    match item.get(key).map(|v| &v.value) {
        Some(JsValue::String(text)) => Some(text.clone()),
        _ => None,
    }
}

fn convert_item(item: &Spanned, variables: &BTreeMap<String, String>) -> Converted {
    let headings = item
        .comments
        .iter()
        .filter(|comment| is_heading(comment))
        .cloned()
        .collect();
    let raw = match read_raw(item) {
        Ok(raw) => raw,
        Err(reason) => {
            return Converted {
                line: item.line,
                trigger: String::new(),
                kind: SourceKind::Plain,
                description: None,
                headings,
                outcome: Outcome::Review(format!("the snippet {reason}")),
                note: None,
            };
        }
    };
    Converted {
        line: item.line,
        trigger: raw.trigger_text(),
        kind: raw.kind(),
        description: raw.description.clone(),
        headings,
        outcome: convert(&raw, variables),
        note: plain_trigger_note(&raw),
    }
}

/// Comment lines worth keeping as section headings, like `// Greek letters`.
fn is_heading(comment: &str) -> bool {
    !comment.is_empty()
        && comment.chars().count() <= 48
        && !comment.ends_with('.')
        && !comment.contains(['(', '{', ':'])
}

fn plain_trigger_note(raw: &RawSnippet) -> Option<String> {
    let RawTrigger::Text(text) = &raw.trigger else {
        return None;
    };
    let looks_like_regex = ["[^", "\\d", "(?"].iter().any(|m| text.contains(m));
    (looks_like_regex && raw.kind() == SourceKind::Plain).then(|| {
        format!(
            "`{text}` looks like a regex but has no `r` option, so Latex Suite matched it as plain text; it was migrated as plain text too"
        )
    })
}

fn convert(raw: &RawSnippet, variables: &BTreeMap<String, String>) -> Outcome {
    let RawReplacement::Text(replacement) = &raw.replacement else {
        return Outcome::Review("the replacement is a JavaScript function".to_string());
    };
    let converted = LatexOptions::parse(&raw.options).and_then(|options| {
        let pieces = parse_replacement(replacement, raw.kind() == SourceKind::Regex)?;
        match (&raw.trigger, raw.kind()) {
            (RawTrigger::Text(text), SourceKind::Plain) => {
                plain(text, &pieces, &options, raw.priority).map(|s| Outcome::Readable(vec![s]))
            }
            _ => Ok(regex_outcome(raw, &pieces, &options, variables)),
        }
    });
    converted.unwrap_or_else(Outcome::Review)
}

/// Latex Suite's option letters, decoded.
#[derive(Default)]
struct LatexOptions {
    scopes: Vec<Scope>,
    instant: bool,
    whole_word: bool,
}

impl LatexOptions {
    fn parse(letters: &str) -> Result<LatexOptions, String> {
        let mut options = LatexOptions::default();
        for letter in letters.chars() {
            options.add(letter)?;
        }
        let both_math = [Scope::BlockMath, Scope::InlineMath]
            .iter()
            .all(|scope| options.scopes.contains(scope));
        if both_math {
            options
                .scopes
                .retain(|scope| !matches!(scope, Scope::BlockMath | Scope::InlineMath));
            options.push_scope(Scope::Context(InputContext::Math));
        }
        Ok(options)
    }

    fn add(&mut self, letter: char) -> Result<(), String> {
        match letter {
            't' => self.push_scope(Scope::Context(InputContext::Text)),
            'm' => self.push_scope(Scope::Context(InputContext::Math)),
            'M' => self.push_scope(Scope::BlockMath),
            'n' => self.push_scope(Scope::InlineMath),
            'c' | 'C' => self.push_scope(Scope::Context(InputContext::Code)),
            'A' => self.instant = true,
            'w' => self.whole_word = true,
            'r' | 'v' | 'U' => {}
            'T' => return Err("option `T` (only inside \\text{}) has no equivalent".to_string()),
            other => return Err(format!("option `{other}` isn't a Latex Suite option")),
        }
        Ok(())
    }

    fn push_scope(&mut self, scope: Scope) {
        if !self.scopes.contains(&scope) {
            self.scopes.push(scope);
        }
    }

    fn to_options(&self, priority: i32) -> Options {
        Options {
            scopes: self.scopes.clone(),
            fire: if self.instant {
                Fire::Instant
            } else {
                Fire::OnTab
            },
            whole_word: self.whole_word,
            priority,
            ..Options::default()
        }
    }
}

/// A piece of a Latex Suite replacement string.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(String),
    Stop(u32, Option<String>),
    /// `[[k]]`, 0-based.
    Capture(usize),
    Visual,
}

fn parse_replacement(text: &str, regex: bool) -> Result<Vec<Piece>, String> {
    if text.contains([STOP_GLYPH, SPACE_GLYPH, NEWLINE_GLYPH, TAB_GLYPH]) {
        return Err("the replacement uses a character the readable format reserves".to_string());
    }
    let mut pieces = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (piece, length) = next_piece(rest, regex)?;
        match (pieces.last_mut(), piece) {
            (Some(Piece::Text(last)), Piece::Text(more)) => last.push_str(&more),
            (_, piece) => pieces.push(piece),
        }
        rest = &rest[length..];
    }
    Ok(pieces)
}

fn next_piece(rest: &str, regex: bool) -> Result<(Piece, usize), String> {
    if let Some(after) = rest.strip_prefix("${VISUAL}") {
        return Ok((Piece::Visual, rest.len() - after.len()));
    }
    if let Some(found) = numbered_stop(rest) {
        return Ok(found);
    }
    if let Some(found) = placeholder_stop(rest)? {
        return Ok(found);
    }
    if regex && let Some(found) = capture(rest) {
        return Ok(found);
    }
    let c = rest.chars().next().unwrap_or_default();
    Ok((Piece::Text(c.to_string()), c.len_utf8()))
}

fn leading_digits(text: &str) -> &str {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    &text[..end]
}

fn numbered_stop(rest: &str) -> Option<(Piece, usize)> {
    let digits = leading_digits(rest.strip_prefix('$')?);
    let number = digits.parse().ok()?;
    Some((Piece::Stop(number, None), 1 + digits.len()))
}

fn placeholder_stop(rest: &str) -> Result<Option<(Piece, usize)>, String> {
    let Some(after) = rest.strip_prefix("${") else {
        return Ok(None);
    };
    let digits = leading_digits(after);
    let Some(body) = after[digits.len()..].strip_prefix(':') else {
        return Ok(None);
    };
    let Ok(number) = digits.parse() else {
        return Ok(None);
    };
    let mut depth = 1usize;
    for (index, c) in body.char_indices() {
        depth = match c {
            '{' => depth + 1,
            '}' => depth - 1,
            _ => depth,
        };
        if depth == 0 {
            let placeholder = Some(body[..index].to_string()).filter(|p| !p.is_empty());
            let length = rest.len() - body.len() + index + 1;
            return Ok(Some((Piece::Stop(number, placeholder), length)));
        }
    }
    Err("a `${n:…}` placeholder is missing its `}`".to_string())
}

fn capture(rest: &str) -> Option<(Piece, usize)> {
    let digits = leading_digits(rest.strip_prefix("[[")?);
    let number = digits.parse().ok()?;
    rest[2 + digits.len()..]
        .starts_with("]]")
        .then_some((Piece::Capture(number), 4 + digits.len()))
}

/// Builds an expansion, resolving `[[k]]` with `capture`.
fn expansion(
    pieces: &[Piece],
    capture: &dyn Fn(usize) -> Result<Vec<ExpansionPart>, String>,
) -> Result<Expansion, String> {
    let numbers: Vec<u32> = pieces
        .iter()
        .filter_map(|piece| match piece {
            Piece::Stop(number, _) => Some(*number),
            _ => None,
        })
        .collect();
    let plain_order = numbers.iter().enumerate().all(|(i, n)| *n as usize == i);
    let mut parts = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Text(text) => parts.push(ExpansionPart::Text(text.clone())),
            Piece::Stop(number, placeholder) => parts.push(ExpansionPart::Stop(StopMark {
                number: (!plain_order).then_some(number + 1),
                placeholder: placeholder.clone(),
            })),
            Piece::Capture(index) => parts.extend(capture(*index)?),
            Piece::Visual => parts.push(ExpansionPart::Selection),
        }
    }
    Ok(Expansion {
        parts: merge_text(parts),
    })
}

fn merge_text(parts: Vec<ExpansionPart>) -> Vec<ExpansionPart> {
    let mut merged: Vec<ExpansionPart> = Vec::with_capacity(parts.len());
    for part in parts {
        match (merged.last_mut(), part) {
            (Some(ExpansionPart::Text(last)), ExpansionPart::Text(text)) => last.push_str(&text),
            (_, part) => merged.push(part),
        }
    }
    merged
}

fn no_captures(_: usize) -> Result<Vec<ExpansionPart>, String> {
    Err("the replacement refers to a capture the trigger doesn't have".to_string())
}

fn plain(
    trigger: &str,
    pieces: &[Piece],
    options: &LatexOptions,
    priority: i32,
) -> Result<Snippet, String> {
    if trigger.is_empty() || trigger.contains([STOP_GLYPH, SPACE_GLYPH, NEWLINE_GLYPH, TAB_GLYPH]) {
        return Err("the trigger can't be written in the readable format".to_string());
    }
    let mut snippet_options = options.to_options(priority);
    if pieces.contains(&Piece::Visual) {
        if trigger.chars().count() != 1 {
            return Err("a visual snippet with a longer trigger needs a key binding".to_string());
        }
        snippet_options.on_selection = true;
        snippet_options.fire = Fire::Instant;
    }
    let snippet = Snippet {
        trigger: Trigger::Pattern(vec![TriggerPart::Text(trigger.to_string())]),
        expansion: expansion(pieces, &no_captures)?,
        options: snippet_options,
    };
    check_round_trip(snippet)
}

/// Makes sure the snippet survives being written and read back.
fn check_round_trip(snippet: Snippet) -> Result<Snippet, String> {
    let line = snippet.to_string();
    match parse_line(&line, 1) {
        Ok(parsed) if parsed == snippet => Ok(snippet),
        Ok(_) => Err(format!("`{line}` reads back differently")),
        Err(error) => Err(format!("`{line}` doesn't parse: {}", error.message)),
    }
}

fn regex_outcome(
    raw: &RawSnippet,
    pieces: &[Piece],
    options: &LatexOptions,
    variables: &BTreeMap<String, String>,
) -> Outcome {
    let (source, flags) = match &raw.trigger {
        RawTrigger::Regex { source, flags } => (source.as_str(), flags.as_str()),
        RawTrigger::Text(text) => (text.as_str(), ""),
    };
    if pieces.contains(&Piece::Visual) {
        return Outcome::Review("a regex visual snippet has no equivalent".to_string());
    }
    let resolve = |name: &str| variable_pattern(name, variables);
    let not_readable = if flags.is_empty() {
        to_readable(source, &resolve)
            .and_then(|readable| readable_snippets(&readable, pieces, options, raw.priority))
    } else {
        Err(format!("uses the regex flags `{flags}`"))
    };
    let reason = match not_readable {
        Ok(snippets) => return Outcome::Readable(snippets),
        Err(reason) => reason,
    };
    match regex_form(source, flags, pieces, options, raw.priority, variables) {
        Ok(snippet) => Outcome::RegexForm(snippet, reason),
        Err(problem) => Outcome::Review(problem),
    }
}

/// `${GREEK}` and `${SYMBOL}` become named patterns when they hold the default lists.
fn variable_pattern(name: &str, variables: &BTreeMap<String, String>) -> Option<NamedPattern> {
    let (pattern, default) = match name {
        "GREEK" => (NamedPattern::Greek, GREEK_NAMES),
        "SYMBOL" => (NamedPattern::Symbol, SYMBOL_NAMES),
        _ => return None,
    };
    let value = variables.get(name).map_or(default, String::as_str);
    (value == default).then_some(pattern)
}

fn readable_snippets(
    readable: &ReadableRegex,
    pieces: &[Piece],
    options: &LatexOptions,
    priority: i32,
) -> Result<Vec<Snippet>, String> {
    readable
        .variants
        .iter()
        .map(|variant| readable_snippet(readable, variant, pieces, options, priority))
        .collect()
}

fn readable_snippet(
    readable: &ReadableRegex,
    variant: &Variant,
    pieces: &[Piece],
    options: &LatexOptions,
    priority: i32,
) -> Result<Snippet, String> {
    let pieces = without_dropped_group(variant, pieces)?;
    let capture = |index: usize| match variant.groups.get(index) {
        Some(GroupValue::Parts(parts)) => Ok(parts.clone()),
        Some(GroupValue::Dropped) => {
            Err("the replacement moves the character before the trigger".to_string())
        }
        None => no_captures(index),
    };
    let mut snippet_options = options.to_options(priority);
    snippet_options.whole_word |= readable.whole_word;
    snippet_options.after_space = readable.after_space;
    if readable.not_after_backslash {
        snippet_options.not_after = Some("\\".to_string());
    }
    check_round_trip(Snippet {
        trigger: Trigger::Pattern(variant.trigger.clone()),
        expansion: expansion(&pieces, &capture)?,
        options: snippet_options,
    })
}

/// A dropped `([^\\])` group must be restored first in the replacement; that copy goes.
fn without_dropped_group(variant: &Variant, pieces: &[Piece]) -> Result<Vec<Piece>, String> {
    if variant.groups.first() != Some(&GroupValue::Dropped) {
        return Ok(pieces.to_vec());
    }
    match pieces.first() {
        Some(Piece::Capture(0)) => Ok(pieces[1..].to_vec()),
        _ => Err("the replacement doesn't start with the character before the trigger".to_string()),
    }
}

fn regex_form(
    source: &str,
    flags: &str,
    pieces: &[Piece],
    options: &LatexOptions,
    priority: i32,
    variables: &BTreeMap<String, String>,
) -> Result<Snippet, String> {
    let mut regex = flag_prefix(flags)?;
    regex.push_str(&rust_regex_source(source, variables));
    let capture = |index: usize| Ok(vec![ExpansionPart::Capture(CaptureRef::Group(index + 1))]);
    let snippet = Snippet {
        trigger: Trigger::Regex(regex),
        expansion: expansion(pieces, &capture)?,
        options: options.to_options(priority),
    };
    check_round_trip(snippet)
}

fn flag_prefix(flags: &str) -> Result<String, String> {
    let mut inline = String::new();
    for flag in flags.chars() {
        match flag {
            'i' | 'm' | 's' => inline.push(flag),
            'u' | 'g' | 'y' | 'd' => {}
            other => return Err(format!("the regex flag `{other}` has no equivalent")),
        }
    }
    Ok(if inline.is_empty() {
        String::new()
    } else {
        format!("(?{inline})")
    })
}

/// Rewrites a JavaScript regex for the Rust engine: `${GREEK}`/`${SYMBOL}` become named
/// patterns, other variables are inlined, and literal braces are escaped.
fn rust_regex_source(source: &str, variables: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = source;
    while let Some(start) = rest.find("${") {
        let Some(length) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&escape_literal_braces(&rest[..start]));
        let name = &rest[start + 2..start + length];
        out.push_str(&variable_text(name, variables));
        rest = &rest[start + length + 1..];
    }
    out.push_str(&escape_literal_braces(rest));
    out
}

fn variable_text(name: &str, variables: &BTreeMap<String, String>) -> String {
    if let Some(pattern) = variable_pattern(name, variables) {
        return format!("{{{}}}", pattern.name());
    }
    variables
        .get(name)
        .cloned()
        .unwrap_or_else(|| escape_literal_braces(&format!("${{{name}}}")))
}

/// Escapes `{` and `}` that aren't part of a `{n}`, `{n,}` or `{n,m}` repetition.
fn escape_literal_braces(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let length = match c {
            '\\' => 2,
            '{' => repetition_length(&chars[index..]).unwrap_or(0),
            _ => 1,
        };
        if length == 0 || (c == '}') {
            out.push('\\');
            out.push(c);
            index += 1;
            continue;
        }
        out.extend(&chars[index..(index + length).min(chars.len())]);
        index += length;
    }
    out
}

fn repetition_length(chars: &[char]) -> Option<usize> {
    let close = chars.iter().position(|c| *c == '}')?;
    let inner: String = chars[1..close].iter().collect();
    let mut bounds = inner.splitn(2, ',');
    let low = bounds.next()?;
    let high = bounds.next().unwrap_or("0");
    let numeric = |text: &str| text.chars().all(|c| c.is_ascii_digit());
    (!low.is_empty() && numeric(low) && numeric(high)).then_some(close + 1)
}

fn build_file(converted: &[Converted]) -> SnippetFile {
    let mut file = SnippetFile::default();
    file.push_comment("Migrated from Latex Suite by gasp-migrate.");
    file.push_comment("Snippets that couldn't be migrated are listed at the end.");
    let mut heading: Option<&String> = None;
    for item in converted {
        heading = item.headings.last().or(heading);
        let snippets = match &item.outcome {
            Outcome::Readable(snippets) => snippets.clone(),
            Outcome::RegexForm(snippet, _) => vec![snippet.clone()],
            Outcome::Review(_) => continue,
        };
        if let Some(title) = heading.take() {
            file.push_blank();
            file.push_comment(title.clone());
        }
        if let Some(description) = &item.description {
            file.push_comment(description.clone());
        }
        snippets.into_iter().for_each(|s| file.push_snippet(s));
    }
    push_review_section(&mut file, converted);
    file
}

fn push_review_section(file: &mut SnippetFile, converted: &[Converted]) {
    let review: Vec<&Converted> = converted
        .iter()
        .filter(|c| matches!(c.outcome, Outcome::Review(_)))
        .collect();
    if review.is_empty() {
        return;
    }
    file.push_blank();
    file.push_comment("Not migrated, for review:");
    for item in review {
        if let Outcome::Review(reason) = &item.outcome {
            file.push_comment(format!(
                "  `{}` (Latex Suite snippets, line {}): {reason}",
                item.trigger.replace('\n', " "),
                item.line
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(snippets: &str) -> String {
        serde_json::json!({ "snippets": snippets }).to_string()
    }

    fn outcome(snippet: &str) -> Outcome {
        let migration = migrate(&settings(&format!("[{snippet}]"))).unwrap();
        migration.converted[0].outcome.clone()
    }

    fn lines(outcome: &Outcome) -> Vec<String> {
        match outcome {
            Outcome::Readable(snippets) => snippets.iter().map(|s| s.to_string()).collect(),
            Outcome::RegexForm(snippet, _) => vec![snippet.to_string()],
            Outcome::Review(reason) => vec![format!("review: {reason}")],
        }
    }

    #[test]
    fn plain_snippets_and_option_letters() {
        assert_eq!(
            lines(&outcome(
                r#"{trigger: "mk", replacement: "$$0$", options: "tA"}"#
            )),
            vec!["mk → $●$  text, instant"]
        );
        assert_eq!(
            lines(&outcome(
                r#"{trigger: "dm", replacement: "$$\n$0\n$$", options: "tAw"}"#
            )),
            vec!["dm → $$⏎●⏎$$  text, instant, whole word"]
        );
        assert_eq!(
            lines(&outcome(
                r#"{trigger: "pmat", replacement: "\\begin{pmatrix}$0\\end{pmatrix}", options: "nA"}"#
            )),
            vec!["pmat → \\begin{pmatrix}●\\end{pmatrix}  inline math, instant"]
        );
        assert_eq!(
            lines(&outcome(
                r#"{trigger: "x", replacement: "y", options: "MnA"}"#
            )),
            vec!["x → y  math, instant"]
        );
    }

    #[test]
    fn mirrored_and_placeholder_stops_are_numbered() {
        assert_eq!(
            lines(&outcome(
                r#"{trigger: "beg", replacement: "\\begin{$0}\n$1\n\\end{$0}", options: "mA"}"#
            )),
            vec!["beg → \\begin{●1}⏎●2⏎\\end{●1}  math, instant"]
        );
        assert_eq!(
            lines(&outcome(
                r#"{trigger: "\\sum", replacement: "\\sum_{${0:i}=${1:1}}^{${2:N}} $3", options: "m"}"#
            )),
            vec!["\\sum → \\sum_{●{i}=●{1}}^{●{N}} ●  math"]
        );
    }

    #[test]
    fn whole_word_regex_becomes_options() {
        assert_eq!(
            lines(&outcome(
                r#"{trigger: /(?<![\\A-Za-z])forall $/, replacement: "\\forall ", options: "rmA"}"#
            )),
            vec!["forall → \\forall␣  math, instant, whole word, after space"]
        );
    }

    #[test]
    fn captures_become_named_patterns() {
        assert_eq!(
            lines(&outcome(
                r#"{trigger: /([A-Za-z])(\d)/, replacement: "[[0]]_{[[1]]}", options: "rmA", priority: -1}"#
            )),
            vec!["{letter}{digit} → {letter}_{{digit}}  math, instant, priority -1"]
        );
        assert_eq!(
            lines(&outcome(
                r#"{trigger: "([^\\\\])(${GREEK})", replacement: "[[0]]\\[[1]]", options: "rmA"}"#
            )),
            vec!["{greek} → \\{greek}  math, instant, not after \\"]
        );
    }

    #[test]
    fn unreadable_regex_keeps_regex_form() {
        let result = outcome(
            r#"{trigger: /\\(sin|cos)([A-Za-gi-z])/, replacement: "\\[[0]] [[1]]", options: "rmA"}"#,
        );
        assert_eq!(
            lines(&result),
            vec!["regex:\\\\(sin|cos)([A-Za-gi-z]) → \\{group1} {group2}  math, instant"]
        );
    }

    #[test]
    fn javascript_braces_are_escaped_in_regex_form() {
        assert_eq!(
            escape_literal_braces(r"\\hat{(a)}x{2}"),
            r"\\hat\{(a)\}x{2}"
        );
        assert_eq!(escape_literal_braces(r"\{a"), r"\{a");
    }

    #[test]
    fn functions_and_unknown_options_go_to_review() {
        assert!(matches!(
            outcome(r#"{trigger: /iden(\d)/, replacement: (m) => m[1], options: "mA"}"#),
            Outcome::Review(reason) if reason.contains("function")
        ));
        assert!(matches!(
            outcome(r#"{trigger: "x", replacement: "y", options: "mTA"}"#),
            Outcome::Review(reason) if reason.contains("`T`")
        ));
    }

    #[test]
    fn visual_snippets_run_on_selection() {
        assert_eq!(
            lines(&outcome(
                r#"{trigger: "U", replacement: "\\underbrace{ ${VISUAL} }_{ $0 }", options: "mA"}"#
            )),
            vec!["U → \\underbrace{ {selection} }_{ ● }  math, instant, on selection"]
        );
    }
}
