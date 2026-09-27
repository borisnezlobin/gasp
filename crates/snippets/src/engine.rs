//! Finds the snippet that fires for a keystroke and builds its expansion.

use std::cmp::Reverse;
use std::fmt;
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use fancy_regex::{Captures, Regex};

use crate::context::{InputContext, TriggerKey};
use crate::pattern::expand_patterns_in_regex;
use crate::snippet::{CaptureRef, ExpansionPart, Fire, Options, Snippet, Trigger, TriggerPart};

/// What the editor knows at the moment a snippet might fire.
#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    /// The text before the cursor (or before the selection). Usually the current line.
    /// For a typed character without a selection, it already ends with that character.
    pub before: &'a str,
    /// The selected text; empty when nothing is selected.
    pub selection: &'a str,
    /// The text after the cursor, used by `whole word` to look at the next character.
    pub after: &'a str,
    pub context: InputContext,
    /// Whether the math around the cursor is a `$$` block rather than inline `$`.
    pub block_math: bool,
    pub key: TriggerKey,
}

impl<'a> Request<'a> {
    /// A request with no selection and nothing after the cursor.
    pub fn new(before: &'a str, context: InputContext, key: TriggerKey) -> Request<'a> {
        Request {
            before,
            selection: "",
            after: "",
            context,
            block_math: false,
            key,
        }
    }
}

/// The edit a snippet makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetEdit {
    /// Byte range to replace, in `before` followed by `selection`.
    pub replace: Range<usize>,
    pub text: String,
    /// Tab stops in the order the cursor visits them; the final stop, if any, is last.
    pub stops: Vec<TabStop>,
    /// Index of the snippet that fired, in the list the engine was built from.
    pub snippet: usize,
}

impl SnippetEdit {
    /// Where the cursor goes right after expanding, as a byte range in `text`.
    pub fn first_selection(&self) -> Range<usize> {
        self.stops
            .first()
            .and_then(|stop| stop.ranges.first().cloned())
            .unwrap_or(self.text.len()..self.text.len())
    }
}

/// One tab stop; several ranges when the same number appears more than once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStop {
    /// 1 and up in visit order; 0 is the final stop.
    pub number: u32,
    /// Byte ranges in the inserted text, each covering its placeholder.
    pub ranges: Vec<Range<usize>>,
}

/// A snippet whose trigger can't be compiled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    pub index: usize,
    pub message: String,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "snippet {}: {}", self.index + 1, self.message)
    }
}

impl std::error::Error for CompileError {}

struct Compiled {
    snippet: Snippet,
    regex: Regex,
    /// What the text before the cursor must end with for the trigger to
    /// match, checked before the regex runs.
    ends_with: LastChar,
    /// How many characters at the end of the text the regex needs to see:
    /// the longest text the trigger matches, plus one for a lookbehind.
    reach: usize,
    /// For pattern triggers, capture group i + 1 holds the pattern at `named_groups[i]`.
    named_groups: Vec<(crate::pattern::NamedPattern, usize)>,
}

/// The last character a trigger can match. Most keystrokes rule out all but
/// a handful of snippets on this alone, which keeps typing cheap with
/// hundreds of snippets loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LastChar {
    Any,
    Exactly(char),
    Letter,
    Digit,
}

impl LastChar {
    fn of(snippet: &Snippet) -> LastChar {
        if snippet.options.after_space {
            return LastChar::Exactly(' ');
        }
        let Trigger::Pattern(parts) = &snippet.trigger else {
            return LastChar::Any;
        };
        match parts.last() {
            Some(TriggerPart::Text(text)) => text
                .chars()
                .next_back()
                .map_or(LastChar::Any, LastChar::Exactly),
            Some(TriggerPart::Named(crate::pattern::NamedPattern::Digit)) => LastChar::Digit,
            Some(TriggerPart::Named(_)) => LastChar::Letter,
            None => LastChar::Any,
        }
    }

    fn admits(self, last: Option<char>) -> bool {
        match (self, last) {
            (LastChar::Any, _) => true,
            (_, None) => false,
            (LastChar::Exactly(wanted), Some(last)) => wanted == last,
            (LastChar::Letter, Some(last)) => last.is_alphabetic(),
            (LastChar::Digit, Some(last)) => last.is_ascii_digit(),
        }
    }
}

/// How many characters at the end of the text before the cursor a trigger
/// with no length limit, such as a raw regex, is matched against. A long
/// line shouldn't make every keystroke slower.
const MATCH_WINDOW: usize = 128;

/// The longest text a trigger matches, in characters, plus one for a
/// lookbehind to see what comes before it.
fn trigger_reach(snippet: &Snippet) -> usize {
    let Trigger::Pattern(parts) = &snippet.trigger else {
        return MATCH_WINDOW;
    };
    let longest = parts.iter().map(|part| match part {
        TriggerPart::Text(text) => text.chars().count(),
        TriggerPart::Named(pattern) => pattern.max_len().unwrap_or(MATCH_WINDOW),
    });
    let space = usize::from(snippet.options.after_space);
    (longest.sum::<usize>() + space + 1).min(MATCH_WINDOW)
}

/// The last `chars` characters of `text` and where they start.
fn tail(text: &str, chars: usize) -> (usize, &str) {
    let start = text
        .char_indices()
        .rev()
        .nth(chars.saturating_sub(1))
        .map_or(0, |(at, _)| at);
    (start, &text[start..])
}

/// A compiled set of snippets.
pub struct SnippetEngine {
    /// Snippets still waiting to compile, for an engine made by
    /// [`SnippetEngine::lazy`].
    pending: Mutex<Option<Vec<Snippet>>>,
    compiled: OnceLock<Vec<Compiled>>,
    count: usize,
}

struct Candidate<'r> {
    index: usize,
    /// Where the text the regex saw starts in `before`.
    offset: usize,
    priority: i32,
    length: usize,
    captures: Captures<'r>,
}

impl SnippetEngine {
    pub fn new(snippets: Vec<Snippet>) -> Result<SnippetEngine, CompileError> {
        let compiled = snippets
            .into_iter()
            .enumerate()
            .map(|(index, snippet)| {
                compile(snippet).map_err(|message| CompileError { index, message })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let count = compiled.len();
        Ok(SnippetEngine {
            pending: Mutex::new(None),
            compiled: OnceLock::from(compiled),
            count,
        })
    }

    /// An engine that compiles its snippets the first time it's used, or
    /// on [`SnippetEngine::warm`]: compiling a few hundred takes several
    /// milliseconds. A snippet whose trigger doesn't compile is kept, so
    /// indices still match the list, but never fires; the parser already
    /// refuses raw regexes that don't compile.
    pub fn lazy(snippets: Vec<Snippet>) -> SnippetEngine {
        SnippetEngine {
            count: snippets.len(),
            pending: Mutex::new(Some(snippets)),
            compiled: OnceLock::new(),
        }
    }

    /// Compiles the snippets now, if they haven't been.
    pub fn warm(&self) {
        self.compiled();
    }

    fn compiled(&self) -> &[Compiled] {
        self.compiled.get_or_init(|| {
            let snippets = self
                .pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
                .unwrap_or_default();
            snippets
                .into_iter()
                .map(|snippet| compile(snippet.clone()).unwrap_or_else(|_| never_fires(snippet)))
                .collect()
        })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns the expansion for this keystroke, if a snippet fires.
    pub fn expand(&self, request: &Request) -> Option<SnippetEdit> {
        if !request.selection.is_empty() {
            return self.expand_selection(request);
        }
        let last = request.before.chars().next_back();
        let compiled = self.compiled();
        let best = compiled
            .iter()
            .enumerate()
            .filter(|(_, compiled)| compiled.ends_with.admits(last))
            .filter(|(_, compiled)| fires_for(&compiled.snippet.options, request))
            .filter_map(|(index, compiled)| candidate(index, compiled, request))
            .min_by_key(|candidate| {
                (
                    Reverse(candidate.priority),
                    Reverse(candidate.length),
                    candidate.index,
                )
            })?;
        let compiled = &compiled[best.index];
        let whole = best.captures.get(0)?;
        let text_parts = build(&compiled.snippet, &|capture| {
            capture_text(compiled, &best.captures, capture)
        });
        Some(finish(
            best.offset + whole.start()..best.offset + whole.end(),
            text_parts,
            best.index,
        ))
    }

    fn expand_selection(&self, request: &Request) -> Option<SnippetEdit> {
        let TriggerKey::Char(typed) = request.key else {
            return None;
        };
        let (index, compiled) = self.compiled().iter().enumerate().find(|(_, compiled)| {
            let options = &compiled.snippet.options;
            options.on_selection
                && !options.off
                && options.allows(request.context, request.block_math)
                && compiled.snippet.trigger.literal() == Some(typed.encode_utf8(&mut [0; 4]))
        })?;
        let parts = build(&compiled.snippet, &|_| String::new());
        let parts = parts
            .into_iter()
            .map(|part| match part {
                Piece::Selection => Piece::Text(request.selection.to_string()),
                other => other,
            })
            .collect();
        let start = request.before.len();
        Some(finish(start..start + request.selection.len(), parts, index))
    }
}

fn fires_for(options: &Options, request: &Request) -> bool {
    let key_matches = match request.key {
        TriggerKey::Tab => options.fire == Fire::OnTab,
        TriggerKey::Char(_) => options.fire == Fire::Instant,
    };
    key_matches
        && !options.on_selection
        && !options.off
        && options.allows(request.context, request.block_math)
}

fn candidate<'r>(
    index: usize,
    compiled: &Compiled,
    request: &Request<'r>,
) -> Option<Candidate<'r>> {
    let (offset, before) = tail(request.before, compiled.reach);
    let captures = compiled.regex.captures(before).ok()??;
    let whole = captures.get(0)?;
    if compiled.snippet.options.whole_word && next_is_letter(request.after) {
        return None;
    }
    Some(Candidate {
        index,
        offset,
        priority: compiled.snippet.options.priority,
        length: whole.end() - whole.start(),
        captures,
    })
}

fn next_is_letter(after: &str) -> bool {
    after.chars().next().is_some_and(char::is_alphabetic)
}

fn compile(snippet: Snippet) -> Result<Compiled, String> {
    let mut source = lookbehinds(&snippet.options);
    let mut named_groups = Vec::new();
    match &snippet.trigger {
        Trigger::Regex(raw) => {
            source.push_str(&format!("(?:{})", expand_patterns_in_regex(raw)));
        }
        Trigger::Pattern(parts) => {
            source.push_str(&pattern_source(parts, &mut named_groups));
        }
    }
    if snippet.options.after_space {
        source.push(' ');
    }
    source.push('$');
    let regex = Regex::new(&source).map_err(|error| error.to_string())?;
    Ok(Compiled {
        ends_with: LastChar::of(&snippet),
        reach: trigger_reach(&snippet),
        snippet,
        regex,
        named_groups,
    })
}

fn never_fires(mut snippet: Snippet) -> Compiled {
    snippet.options.off = true;
    Compiled {
        ends_with: LastChar::Any,
        reach: 0,
        snippet,
        regex: Regex::new("[^\\s\\S]").expect("the empty class compiles"),
        named_groups: Vec::new(),
    }
}

fn lookbehinds(options: &Options) -> String {
    let mut source = String::new();
    if options.whole_word {
        source.push_str("(?<![\\\\\\p{L}])");
    }
    if let Some(chars) = &options.not_after {
        let class: String = chars.chars().map(escape_class_char).collect();
        source.push_str(&format!("(?<![{class}])"));
    }
    source
}

fn escape_class_char(c: char) -> String {
    if c.is_ascii_punctuation() {
        format!("\\{c}")
    } else {
        c.to_string()
    }
}

fn pattern_source(
    parts: &[TriggerPart],
    named_groups: &mut Vec<(crate::pattern::NamedPattern, usize)>,
) -> String {
    let mut source = String::new();
    for part in parts {
        match part {
            TriggerPart::Text(text) => source.push_str(&fancy_regex::escape(text)),
            TriggerPart::Named(pattern) => {
                let occurrence = named_groups.iter().filter(|(p, _)| p == pattern).count() + 1;
                named_groups.push((*pattern, occurrence));
                source.push_str(&format!("({})", pattern.regex()));
            }
        }
    }
    source
}

fn capture_text(compiled: &Compiled, captures: &Captures, capture: &CaptureRef) -> String {
    let group = match capture {
        CaptureRef::Group(number) => Some(*number),
        CaptureRef::Named {
            pattern,
            occurrence,
        } => compiled
            .named_groups
            .iter()
            .position(|entry| *entry == (*pattern, *occurrence))
            .map(|position| position + 1),
    };
    group
        .and_then(|group| captures.get(group))
        .map_or_else(String::new, |found| found.as_str().to_string())
}

/// An expansion part with captures already filled in.
enum Piece {
    Text(String),
    Stop {
        number: Option<u32>,
        placeholder: String,
    },
    Selection,
}

fn build(snippet: &Snippet, capture: &dyn Fn(&CaptureRef) -> String) -> Vec<Piece> {
    snippet
        .expansion
        .parts
        .iter()
        .map(|part| match part {
            ExpansionPart::Text(text) => Piece::Text(text.clone()),
            ExpansionPart::Capture(reference) => Piece::Text(capture(reference)),
            ExpansionPart::Selection => Piece::Selection,
            ExpansionPart::Stop(stop) => Piece::Stop {
                number: stop.number,
                placeholder: stop.placeholder.clone().unwrap_or_default(),
            },
        })
        .collect()
}

fn finish(replace: Range<usize>, pieces: Vec<Piece>, snippet: usize) -> SnippetEdit {
    let mut text = String::new();
    let mut found: Vec<(u32, Range<usize>)> = Vec::new();
    let mut next_plain = 1;
    for piece in pieces {
        match piece {
            Piece::Text(value) => text.push_str(&value),
            Piece::Selection => {}
            Piece::Stop {
                number,
                placeholder,
            } => {
                let number = number.unwrap_or_else(|| {
                    next_plain += 1;
                    next_plain - 1
                });
                let start = text.len();
                text.push_str(&placeholder);
                found.push((number, start..text.len()));
            }
        }
    }
    SnippetEdit {
        replace,
        text,
        stops: group_stops(found),
        snippet,
    }
}

fn group_stops(found: Vec<(u32, Range<usize>)>) -> Vec<TabStop> {
    let mut stops: Vec<TabStop> = Vec::new();
    for (number, range) in found {
        match stops.iter_mut().find(|stop| stop.number == number) {
            Some(stop) => stop.ranges.push(range),
            None => stops.push(TabStop {
                number,
                ranges: vec![range],
            }),
        }
    }
    stops.sort_by_key(|stop| (stop.number == 0, stop.number));
    stops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::SnippetFile;

    const SNIPPETS: &str = "\
mk → $●$  text, instant
reals → \\mathbb{R}  math, instant
forall → \\forall␣  math, instant, whole word, after space
{letter}{digit} → {letter}_{digit}  math, instant, priority -1
// → \\frac{●}{●}●  math, instant
dot → \\dot{●}●  math, instant, priority -1
{letter}dot → \\dot{{letter}}  math, instant, priority -1
cdot → \\cdot  math, instant
{greek} → \\{greek}  math, instant, not after \\
beg → \\begin{●1}⏎●2⏎\\end{●1}  math, instant
sum → \\sum_{●{i}=●{1}}^{●{N}} ●0  math
pmat → \\begin{pmatrix}⏎●⏎\\end{pmatrix}  block math, instant
pmat → \\begin{pmatrix}●\\end{pmatrix}  inline math, instant
U → \\underbrace{ {selection} }_{ ● }  math, instant, on selection
regex:\\\\(sin|cos)([A-Za-gi-z]) → \\{group1}␣{group2}  math, instant
pa{letter}{letter} → \\frac{ \\partial {letter1} }{ \\partial {letter2} }␣  math
";

    fn engine() -> SnippetEngine {
        let file = SnippetFile::parse(SNIPPETS).unwrap();
        SnippetEngine::new(file.snippets().cloned().collect()).unwrap()
    }

    fn typed(before: &str, context: InputContext) -> Option<SnippetEdit> {
        let last = before.chars().last().unwrap();
        engine().expand(&Request::new(before, context, TriggerKey::Char(last)))
    }

    fn applied(before: &str, edit: &SnippetEdit) -> String {
        let mut out = before.to_string();
        out.replace_range(edit.replace.clone(), &edit.text);
        out
    }

    #[test]
    fn plain_instant_snippet() {
        let edit = typed("see mk", InputContext::Text).unwrap();
        assert_eq!(edit.replace, 4..6);
        assert_eq!(edit.text, "$$");
        assert_eq!(edit.first_selection(), 1..1);
    }

    #[test]
    fn contexts_are_respected() {
        assert!(typed("mk", InputContext::Math).is_none());
        assert!(typed("reals", InputContext::Text).is_none());
        assert!(typed("reals", InputContext::Math).is_some());
    }

    #[test]
    fn whole_word_after_space() {
        let edit = typed("x forall ", InputContext::Math).unwrap();
        assert_eq!(applied("x forall ", &edit), "x \\forall ");
        assert!(typed("\\forall ", InputContext::Math).is_none());
        assert!(typed("xforall ", InputContext::Math).is_none());
        assert!(typed("forall", InputContext::Math).is_none());
    }

    #[test]
    fn named_pattern_captures() {
        let edit = typed("x2", InputContext::Math).unwrap();
        assert_eq!(edit.text, "x_2");
        assert_eq!(edit.replace, 0..2);
    }

    #[test]
    fn priority_then_longest_match_wins() {
        assert_eq!(typed("adot", InputContext::Math).unwrap().text, "\\dot{a}");
        assert_eq!(typed("cdot", InputContext::Math).unwrap().text, "\\cdot");
        assert_eq!(typed("1 dot", InputContext::Math).unwrap().text, "\\dot{}");
    }

    #[test]
    fn greek_not_after_backslash() {
        let edit = typed("2beta", InputContext::Math).unwrap();
        assert_eq!(applied("2beta", &edit), "2\\beta");
        assert!(typed("\\alpha", InputContext::Math).is_none());
    }

    #[test]
    fn tab_stops_in_order_with_mirrors() {
        let edit = typed("//", InputContext::Math).unwrap();
        assert_eq!(edit.text, "\\frac{}{}");
        let numbers: Vec<u32> = edit.stops.iter().map(|stop| stop.number).collect();
        assert_eq!(numbers, vec![1, 2, 3]);
        assert_eq!(edit.stops[1].ranges, vec![8..8]);

        let edit = typed("beg", InputContext::Math).unwrap();
        assert_eq!(edit.stops[0].ranges.len(), 2);
    }

    #[test]
    fn placeholders_and_final_stop_on_tab() {
        let engine = engine();
        let request = Request::new("sum", InputContext::Math, TriggerKey::Tab);
        let edit = engine.expand(&request).unwrap();
        assert_eq!(edit.text, "\\sum_{i=1}^{N} ");
        assert_eq!(edit.first_selection(), 6..7);
        assert_eq!(edit.stops.last().unwrap().number, 0);
        let typed_request = Request::new("sum", InputContext::Math, TriggerKey::Char('m'));
        assert!(engine.expand(&typed_request).is_none());
    }

    #[test]
    fn block_and_inline_math_variants() {
        let engine = engine();
        let mut request = Request::new("pmat", InputContext::Math, TriggerKey::Char('t'));
        assert_eq!(
            engine.expand(&request).unwrap().text,
            "\\begin{pmatrix}\\end{pmatrix}"
        );
        request.block_math = true;
        assert_eq!(
            engine.expand(&request).unwrap().text,
            "\\begin{pmatrix}\n\n\\end{pmatrix}"
        );
    }

    #[test]
    fn selection_snippets_wrap_the_selection() {
        let engine = engine();
        let mut request = Request::new("a + ", InputContext::Math, TriggerKey::Char('U'));
        request.selection = "b + c";
        let edit = engine.expand(&request).unwrap();
        assert_eq!(edit.replace, 4..9);
        assert_eq!(edit.text, "\\underbrace{ b + c }_{  }");
        assert_eq!(edit.first_selection(), 23..23);
        request.key = TriggerKey::Char('x');
        assert!(engine.expand(&request).is_none());
    }

    #[test]
    fn raw_regex_groups() {
        let edit = typed("\\sinx", InputContext::Math).unwrap();
        assert_eq!(edit.text, "\\sin x");
        assert!(typed("\\sinh", InputContext::Math).is_none());
    }

    #[test]
    fn repeated_patterns_on_tab() {
        let request = Request::new("paxt", InputContext::Math, TriggerKey::Tab);
        let edit = engine().expand(&request).unwrap();
        assert_eq!(edit.text, "\\frac{ \\partial x }{ \\partial t } ");
    }

    #[test]
    fn whole_word_looks_at_the_next_character() {
        let file = SnippetFile::parse("dm → $$  text, instant, whole word\n").unwrap();
        let engine = SnippetEngine::new(file.snippets().cloned().collect()).unwrap();
        let mut request = Request::new("dm", InputContext::Text, TriggerKey::Char('m'));
        assert!(engine.expand(&request).is_some());
        request.after = "ore";
        assert!(engine.expand(&request).is_none());
    }
}
