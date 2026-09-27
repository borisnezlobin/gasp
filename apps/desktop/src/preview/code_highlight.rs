//! Syntax colours for fenced code blocks, from the fence's language.
//!
//! Highlighting uses syntect with two-face's grammars, which Typst already
//! brings in. The grammars load once, on a background thread, the first
//! time a code block is drawn; until then code shows in one colour.
//!
//! Only lines that are laid out get highlighted, and each block keeps the
//! parser state after every line it has seen, keyed by the line's text.
//! An edit re-highlights from the changed line down to the last one on
//! screen, so typing in a long block costs one or two lines.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use editor_core::render::{LinePlan, LineStyle};
use editor_core::syntax::NodeKind;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

use super::source::Source;

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
pub type LineSpans = Arc<[(Range<usize>, CodeKind)]>;

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

/// One highlighted line and the parser state after it.
struct HighlightedLine {
    hash: u64,
    state: ParseState,
    stack: ScopeStack,
    spans: LineSpans,
}

/// A block's highlighted lines, from its first line down.
struct BlockHighlight {
    lines: Vec<HighlightedLine>,
    used: bool,
}

/// Blocks kept between frames before unused ones are dropped.
const MAX_BLOCKS: usize = 64;

/// The highlighted lines of the code blocks an editor has drawn.
#[derive(Default)]
pub struct CodeHighlighter {
    blocks: HashMap<u64, BlockHighlight>,
    /// A code block was drawn before the grammars loaded.
    wants_syntaxes: bool,
    loading: bool,
}

impl CodeHighlighter {
    /// Forgets blocks no longer on screen once too many are kept.
    pub fn begin_frame(&mut self) {
        if self.blocks.len() > MAX_BLOCKS {
            self.blocks.retain(|_, block| block.used);
        }
        for block in self.blocks.values_mut() {
            block.used = false;
        }
    }

    /// Whether the grammars should be loaded, which the caller does in
    /// the background before redrawing. True once per load.
    pub fn take_load_request(&mut self) -> bool {
        let wanted = self.wants_syntaxes && !self.loading && SYNTAXES.get().is_none();
        self.loading |= wanted;
        wanted
    }

    /// The spans of line `index` of a code block in `language`, whose
    /// lines `line(0..=index)` gives. `None` for a language without a
    /// grammar, or while the grammars load.
    pub fn line<'a>(
        &mut self,
        language: &str,
        line: impl Fn(usize) -> &'a str,
        index: usize,
    ) -> Option<LineSpans> {
        let Some(syntaxes) = SYNTAXES.get() else {
            self.wants_syntaxes = true;
            return None;
        };
        let syntax = syntaxes.find_syntax_by_token(language)?;
        let key = hash_of(&(language, line(0)));
        let block = self.blocks.entry(key).or_insert_with(|| BlockHighlight {
            lines: Vec::new(),
            used: true,
        });
        block.used = true;
        for at in 0..=index {
            let text = line(at);
            let hash = hash_of(&text);
            if block
                .lines
                .get(at)
                .is_some_and(|cached| cached.hash == hash)
            {
                continue;
            }
            block.lines.truncate(at);
            let highlighted = highlight_after(block.lines.last(), syntax, syntaxes, text, hash);
            block.lines.push(highlighted);
        }
        Some(block.lines[index].spans.clone())
    }
}

fn hash_of(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Highlights `text`, continuing from the state after `previous`.
fn highlight_after(
    previous: Option<&HighlightedLine>,
    syntax: &SyntaxReference,
    syntaxes: &SyntaxSet,
    text: &str,
    hash: u64,
) -> HighlightedLine {
    let (mut state, mut stack) = previous.map_or_else(
        || (ParseState::new(syntax), ScopeStack::new()),
        |line| (line.state.clone(), line.stack.clone()),
    );
    let spans = highlight_line(text, &mut state, &mut stack, syntaxes);
    HighlightedLine {
        hash,
        state,
        stack,
        spans,
    }
}

fn highlight_line(
    text: &str,
    state: &mut ParseState,
    stack: &mut ScopeStack,
    syntaxes: &SyntaxSet,
) -> LineSpans {
    // The grammars expect each line to end with its line break.
    let with_newline = format!("{text}\n");
    let ops = state
        .parse_line(&with_newline, syntaxes)
        .unwrap_or_default();
    let mut spans: Vec<(Range<usize>, CodeKind)> = Vec::new();
    let mut start = 0;
    for (at, op) in &ops {
        let end = (*at).min(text.len());
        push_span(&mut spans, start..end, stack);
        start = start.max(end);
        stack.apply(op).ok();
    }
    push_span(&mut spans, start..text.len(), stack);
    spans.into()
}

/// Adds `range` with the kind `stack` gives it, joining it to the last
/// span when they touch and match.
fn push_span(spans: &mut Vec<(Range<usize>, CodeKind)>, range: Range<usize>, stack: &ScopeStack) {
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

/// The spans for a planned line when it is code inside a fenced block
/// with a language.
pub fn spans_for_line(
    plan: &LinePlan,
    source: &Source,
    code: &mut CodeHighlighter,
) -> Option<LineSpans> {
    let in_code = plan
        .line_styles
        .iter()
        .any(|style| matches!(style, LineStyle::CodeBlock { .. }));
    if !in_code {
        return None;
    }
    let (language, content) = fenced_block_at(source, plan.range.start)?;
    if !content.contains(&plan.range.start) {
        return None;
    }
    let first = source.line_of(content.start);
    let index = plan.line.checked_sub(first)?;
    code.line(&language, |at| source.line_text(first + at), index)
}

/// The language and code range of the fenced block around `offset`.
fn fenced_block_at(source: &Source, offset: usize) -> Option<(String, Range<usize>)> {
    let tree = source.tree();
    tree.path_at(offset).into_iter().rev().find_map(|id| {
        let node = tree.node(id);
        let NodeKind::CodeBlock(info) = &node.kind else {
            return None;
        };
        let language = info.language.clone().filter(|_| info.fenced)?;
        let open = node.markup.first()?;
        let start = (open.range.end + 1).min(node.range.end);
        let end = node
            .markup
            .get(1)
            .map_or(node.range.end, |close| close.range.start);
        Some((language, start..end))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(language: &str, lines: &[&str]) -> Vec<Vec<(String, CodeKind)>> {
        load_syntaxes();
        let mut code = CodeHighlighter::default();
        (0..lines.len())
            .map(|index| {
                let spans = code.line(language, |at| lines[at], index).unwrap();
                spans
                    .iter()
                    .map(|(range, kind)| (lines[index][range.clone()].to_owned(), *kind))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn rust_keywords_strings_and_comments() {
        let found = kinds(
            "rust",
            &["fn main() {", "    let s = \"hi\"; // greet", "}"],
        );
        assert!(
            found[0].contains(&("fn".into(), CodeKind::Keyword)),
            "{found:?}"
        );
        assert!(
            found[0].contains(&("main".into(), CodeKind::Function)),
            "{found:?}"
        );
        assert!(
            found[1].contains(&("\"hi\"".into(), CodeKind::String)),
            "{found:?}"
        );
        assert!(
            found[1].contains(&("// greet".into(), CodeKind::Comment)),
            "{found:?}"
        );
    }

    #[test]
    fn state_carries_across_lines() {
        let found = kinds("python", &["x = \"\"\"doc", "still doc", "\"\"\""]);
        assert_eq!(found[1], [("still doc".to_owned(), CodeKind::String)]);
    }

    #[test]
    fn unknown_languages_stay_plain() {
        load_syntaxes();
        let mut code = CodeHighlighter::default();
        assert!(code.line("no-such-language", |_| "x", 0).is_none());
    }

    #[test]
    fn an_edit_rehighlights_from_the_changed_line() {
        load_syntaxes();
        let mut code = CodeHighlighter::default();
        let before = ["a = 1", "b = 2"];
        code.line("python", |at| before[at], 1).unwrap();
        let after = ["a = 1", "b = \"2\""];
        let spans = code.line("python", |at| after[at], 1).unwrap();
        assert!(spans.iter().any(|(_, kind)| *kind == CodeKind::String));
    }

    #[test]
    fn nothing_before_the_grammars_load_but_a_request() {
        let mut code = CodeHighlighter::default();
        if SYNTAXES.get().is_none() {
            assert!(code.line("rust", |_| "fn x() {}", 0).is_none());
            assert!(code.take_load_request());
            assert!(!code.take_load_request(), "asked once");
        }
    }
}
