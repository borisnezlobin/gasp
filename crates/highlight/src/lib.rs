//! Syntax colours for fenced code blocks, from the fence's language.
//!
//! Highlighting uses syntect with two-face's grammars. Each stretch of
//! code is named by what it is (a comment, a string, a keyword…), and the
//! theme's `color.code.*` tokens colour it, so code follows the theme
//! rather than a syntax theme of its own. The grammars take a moment to
//! load, so [`load_syntaxes`] belongs off the main thread.

use std::ops::Range;
use std::sync::OnceLock;

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

/// What a stretch of code is, which picks its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodeKind {
    Comment,
    String,
    Number,
    Constant,
    Keyword,
    Function,
    Type,
}

impl CodeKind {
    pub const ALL: [CodeKind; 7] = [
        CodeKind::Comment,
        CodeKind::String,
        CodeKind::Number,
        CodeKind::Constant,
        CodeKind::Keyword,
        CodeKind::Function,
        CodeKind::Type,
    ];

    /// The theme token for the colour, such as `color.code.keyword`.
    pub fn token(self) -> &'static str {
        const TOKENS: [&str; 7] = [
            "color.code.comment",
            "color.code.string",
            "color.code.number",
            "color.code.constant",
            "color.code.keyword",
            "color.code.function",
            "color.code.type",
        ];
        TOKENS[self as usize]
    }
}

/// Byte ranges of one line, relative to its start, and what they are.
/// Text outside every range is plain.
pub type Spans = Vec<(Range<usize>, CodeKind)>;

/// Scope prefixes and what they mean, checked from the innermost scope
/// out; the first match wins, and `None` keeps text plain.
const SCOPE_KINDS: [(&str, Option<CodeKind>); 24] = [
    ("comment", Some(CodeKind::Comment)),
    ("punctuation.definition.comment", Some(CodeKind::Comment)),
    ("string", Some(CodeKind::String)),
    ("punctuation.definition.string", Some(CodeKind::String)),
    ("constant.numeric", Some(CodeKind::Number)),
    ("constant.character.escape", Some(CodeKind::Constant)),
    ("constant", Some(CodeKind::Constant)),
    ("variable.language", Some(CodeKind::Constant)),
    ("support.constant", Some(CodeKind::Constant)),
    ("keyword.operator", None),
    ("keyword", Some(CodeKind::Keyword)),
    ("storage", Some(CodeKind::Keyword)),
    ("entity.name.tag", Some(CodeKind::Keyword)),
    ("entity.name.function", Some(CodeKind::Function)),
    ("support.function", Some(CodeKind::Function)),
    ("variable.function", Some(CodeKind::Function)),
    ("meta.function-call.identifier", Some(CodeKind::Function)),
    ("entity.name.type", Some(CodeKind::Type)),
    ("entity.name.class", Some(CodeKind::Type)),
    ("entity.name.struct", Some(CodeKind::Type)),
    ("entity.other.inherited-class", Some(CodeKind::Type)),
    ("support.type", Some(CodeKind::Type)),
    ("support.class", Some(CodeKind::Type)),
    ("entity.other.attribute-name", Some(CodeKind::Constant)),
];

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static SCOPES: OnceLock<Vec<(Scope, Option<CodeKind>)>> = OnceLock::new();

/// The grammars, loading them on first use. This takes a while, so call
/// it off the main thread.
pub fn load_syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

/// The grammars if they've loaded, without loading them.
pub fn loaded_syntaxes() -> Option<&'static SyntaxSet> {
    SYNTAXES.get()
}

fn scopes() -> &'static [(Scope, Option<CodeKind>)] {
    SCOPES.get_or_init(|| {
        SCOPE_KINDS
            .iter()
            .filter_map(|(prefix, kind)| Some((Scope::new(prefix).ok()?, *kind)))
            .collect()
    })
}

/// What the innermost meaningful scope on `stack` makes the text.
fn kind_of(stack: &[Scope]) -> Option<CodeKind> {
    let table = scopes();
    stack.iter().rev().find_map(|scope| {
        table
            .iter()
            .find(|(prefix, _)| prefix.is_prefix_of(*scope))
            .map(|(_, kind)| *kind)
    })?
}

/// Highlights one line, carrying the parser's state and scope stack on to
/// the next.
pub fn highlight_line(
    text: &str,
    state: &mut ParseState,
    stack: &mut ScopeStack,
    syntaxes: &SyntaxSet,
) -> Spans {
    // The grammars expect each line to end with its line break.
    let with_newline = format!("{text}\n");
    let ops = state
        .parse_line(&with_newline, syntaxes)
        .unwrap_or_default();
    let mut spans = Spans::new();
    let mut start = 0;
    for (at, op) in &ops {
        let end = (*at).min(text.len());
        push_span(&mut spans, start..end, stack);
        start = start.max(end);
        stack.apply(op).ok();
    }
    push_span(&mut spans, start..text.len(), stack);
    spans
}

/// Adds `range` with the kind `stack` gives it, joining it to the last
/// span when they touch and match.
fn push_span(spans: &mut Spans, range: Range<usize>, stack: &ScopeStack) {
    if range.is_empty() {
        return;
    }
    let Some(kind) = kind_of(stack.as_slice()) else {
        return;
    };
    match spans.last_mut() {
        Some((last, last_kind)) if *last_kind == kind && last.end == range.start => {
            last.end = range.end
        }
        _ => spans.push((range, kind)),
    }
}

/// Every line of a block of code in `language`, highlighted from the top;
/// `None` for a language with no grammar.
pub fn highlight_block<'a>(
    language: &str,
    lines: impl IntoIterator<Item = &'a str>,
) -> Option<Vec<Spans>> {
    let syntaxes = load_syntaxes();
    let syntax = syntaxes.find_syntax_by_token(language)?;
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    Some(
        lines
            .into_iter()
            .map(|line| highlight_line(line, &mut state, &mut stack, syntaxes))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(language: &str, lines: &[&str]) -> Vec<Vec<(String, CodeKind)>> {
        highlight_block(language, lines.iter().copied())
            .unwrap()
            .into_iter()
            .zip(lines)
            .map(|(spans, line)| {
                spans
                    .into_iter()
                    .map(|(range, kind)| (line[range].to_owned(), kind))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn rust_keywords_strings_and_comments() {
        let lines = kinds(
            "rust",
            &["fn main() {", "    let s = \"hi\"; // greet", "}"],
        );
        assert!(lines[0].contains(&("fn".to_owned(), CodeKind::Keyword)));
        assert!(lines[1].contains(&("\"hi\"".to_owned(), CodeKind::String)));
        assert!(lines[1].contains(&("// greet".to_owned(), CodeKind::Comment)));
    }

    #[test]
    fn unknown_languages_have_no_colours() {
        assert!(highlight_block("no-such-language", ["x"]).is_none());
        assert_eq!(CodeKind::Keyword.token(), "color.code.keyword");
    }
}
