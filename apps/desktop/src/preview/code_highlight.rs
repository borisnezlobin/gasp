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

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::{Range, RangeInclusive};
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
#[derive(Clone)]
pub struct HighlightedLine {
    hash: u64,
    state: ParseState,
    stack: ScopeStack,
    spans: LineSpans,
}

/// A block's highlighted lines, from its first line down.
struct BlockHighlight {
    language: String,
    lines: Vec<HighlightedLine>,
    used: bool,
    /// How many leading lines were checked against the text in
    /// `generation`, so a frame checks each line once.
    checked: usize,
    generation: u64,
}

/// Blocks kept between frames before unused ones are dropped.
const MAX_BLOCKS: usize = 64;

/// Lines of a block highlighted during layout at most; a longer stretch,
/// or a language whose grammar hasn't run yet (its patterns compile on
/// first use), is highlighted in the background instead.
const SYNC_LINES: usize = 24;

/// Bytes in the longest line highlighted during layout. Parsing costs
/// grow with the line, and code lines are rarely this long.
const SYNC_LINE_LENGTH: usize = 120;

/// A block to highlight in the background, from line `from` down.
pub struct HighlightJob {
    id: u64,
    language: String,
    from: usize,
    /// The highlighted line before `from`, whose parser state the job
    /// continues from.
    seed: Option<HighlightedLine>,
    /// The block's lines as last highlighted, from `from` down. Once the
    /// parser state after a line matches what it was and the lines below
    /// are unchanged, the rest is kept as it was.
    known: Vec<HighlightedLine>,
    /// The text of the block's lines from `from` down.
    lines: Vec<String>,
}

/// The finished job: the block's lines from `from` down.
pub struct HighlightedBlock {
    id: u64,
    language: String,
    from: usize,
    seed: Option<HighlightedLine>,
    lines: Vec<HighlightedLine>,
}

impl HighlightJob {
    /// Highlights the block from its first changed line down, stopping
    /// where the rest is known to be unchanged. Slow the first time a
    /// language runs, so call it off the main thread.
    pub fn run(self) -> HighlightedBlock {
        let syntaxes = load_syntaxes();
        let mut lines: Vec<HighlightedLine> = Vec::with_capacity(self.lines.len());
        if let Some(syntax) = syntaxes.find_syntax_by_token(&self.language) {
            for (at, text) in self.lines.iter().enumerate() {
                let previous = lines.last().or(self.seed.as_ref());
                let line = highlight_after(previous, syntax, syntaxes, text, hash_of(text));
                let converged = self.converges_after(at, &line);
                lines.push(line);
                if converged {
                    lines.extend_from_slice(&self.known[at + 1..]);
                    break;
                }
            }
        }
        HighlightedBlock {
            id: self.id,
            language: self.language,
            from: self.from,
            seed: self.seed,
            lines,
        }
    }

    /// Whether the lines after `at` stay as last highlighted: the parser
    /// leaves `at` as it did, and none of the lines below changed.
    fn converges_after(&self, at: usize, line: &HighlightedLine) -> bool {
        let Some(old) = self.known.get(at) else {
            return false;
        };
        let same_state = old.state == line.state && old.stack == line.stack;
        same_state
            && self.known.len() == self.lines.len()
            && self.known[at + 1..]
                .iter()
                .zip(&self.lines[at + 1..])
                .all(|(known, text)| known.hash == hash_of(text))
    }
}

/// The highlighted lines of the code blocks an editor has drawn.
#[derive(Default)]
pub struct CodeHighlighter {
    /// Blocks by where their code starts, kept current through edits.
    blocks: HashMap<usize, BlockHighlight>,
    /// Bumped every frame and every edit; see [`BlockHighlight::checked`].
    generation: u64,
    /// Languages whose grammar has run once, so its patterns are compiled.
    warmed: HashSet<String>,
    jobs: Vec<HighlightJob>,
    /// Jobs running, by id, with where their block's code starts now.
    pending: HashMap<u64, usize>,
    next_job: u64,
    /// A code block was drawn before the grammars loaded.
    wants_syntaxes: bool,
    loading: bool,
}

/// What [`CodeHighlighter::line`] needs to know about a block.
pub struct BlockLines<'a, F: Fn(usize) -> &'a str> {
    pub language: &'a str,
    /// Where the block's code starts in the note.
    pub start: usize,
    /// Line `at` of the block's code.
    pub line: F,
    pub count: usize,
}

impl CodeHighlighter {
    /// Forgets blocks no longer on screen once too many are kept.
    pub fn begin_frame(&mut self) {
        self.generation += 1;
        if self.blocks.len() > MAX_BLOCKS {
            self.blocks.retain(|_, block| block.used);
        }
        for block in self.blocks.values_mut() {
            block.used = false;
        }
    }

    /// The note's bytes `old` became `new_len` bytes: lines must be
    /// checked against the text again, and blocks below move.
    pub fn text_changed(&mut self, old: Range<usize>, new_len: usize) {
        self.generation += 1;
        let blocks = std::mem::take(&mut self.blocks);
        self.blocks = blocks
            .into_iter()
            .filter_map(|(start, block)| Some((moved_start(start, &old, new_len)?, block)))
            .collect();
        self.pending
            .retain(|_, start| match moved_start(*start, &old, new_len) {
                Some(moved) => {
                    *start = moved;
                    true
                }
                None => false,
            });
    }

    /// Whether the grammars should be loaded, which the caller does in
    /// the background before redrawing. True once per load.
    pub fn take_load_request(&mut self) -> bool {
        let wanted = self.wants_syntaxes && !self.loading && SYNTAXES.get().is_none();
        self.loading |= wanted;
        wanted
    }

    /// Blocks to highlight in the background before redrawing.
    pub fn take_jobs(&mut self) -> Vec<HighlightJob> {
        std::mem::take(&mut self.jobs)
    }

    /// Keeps a block highlighted in the background.
    pub fn finish_job(&mut self, done: HighlightedBlock) {
        self.warmed.insert(done.language.clone());
        let Some(start) = self.pending.remove(&done.id) else {
            return;
        };
        if done.from > 0 {
            self.splice(start, done);
            return;
        }
        self.blocks.insert(
            start,
            BlockHighlight {
                language: done.language,
                lines: done.lines,
                used: true,
                checked: 0,
                generation: 0,
            },
        );
    }

    /// The spans of line `index` of a code block. `None` for a language
    /// without a grammar, and while the grammars load or the block is
    /// highlighted in the background.
    pub fn line<'a, F: Fn(usize) -> &'a str>(
        &mut self,
        block: &BlockLines<'a, F>,
        index: usize,
    ) -> Option<LineSpans> {
        let Some(syntaxes) = SYNTAXES.get() else {
            self.wants_syntaxes = true;
            return None;
        };
        let syntax = syntaxes.find_syntax_by_token(block.language)?;
        let key = block.start;
        let cached = self
            .blocks
            .get(&key)
            .filter(|cached| cached.language == block.language)
            .map_or(0, |cached| cached.lines.len());
        let too_long = index + 1 > cached + SYNC_LINES;
        if too_long || !self.warmed.contains(block.language) {
            self.queue(key, block, 0);
            return None;
        }
        let generation = self.generation;
        let cache = self.blocks.entry(key).or_insert_with(|| BlockHighlight {
            language: block.language.to_owned(),
            lines: Vec::new(),
            used: true,
            checked: 0,
            generation,
        });
        cache.used = true;
        if cache.language != block.language {
            cache.language = block.language.to_owned();
            cache.lines.clear();
        }
        if cache.generation != generation {
            cache.generation = generation;
            cache.checked = 0;
        }
        let checked = cache.checked;
        match refresh_lines(cache, block, checked..=index, syntax, syntaxes) {
            Refreshed::Current => {
                cache.checked = cache.checked.max(index + 1);
                Some(cache.lines[index].spans.clone())
            }
            Refreshed::LongLine(at) => {
                // Keep what the line looked like until the background
                // catches up.
                let stale = cache.lines.get(index).map(|line| line.spans.clone());
                self.queue(key, block, at);
                stale
            }
        }
    }

    /// Puts lines highlighted from `done.from` down in place of the
    /// block's, when the line above them is still what the job started
    /// from; otherwise the next frame asks again.
    fn splice(&mut self, start: usize, done: HighlightedBlock) {
        let Some(block) = self.blocks.get_mut(&start) else {
            return;
        };
        let above = block.lines.get(done.from - 1);
        let seed_holds = above.zip(done.seed.as_ref()).is_some_and(|(above, seed)| {
            above.hash == seed.hash && above.state == seed.state && above.stack == seed.stack
        });
        if block.language != done.language || !seed_holds {
            return;
        }
        block.lines.truncate(done.from);
        block.lines.extend(done.lines);
        block.used = true;
        block.generation = 0;
    }

    /// Queues the block for the background from line `from` down, which
    /// must follow lines current in the cache.
    fn queue<'a, F: Fn(usize) -> &'a str>(
        &mut self,
        key: usize,
        block: &BlockLines<'a, F>,
        from: usize,
    ) {
        if self.pending.values().any(|start| *start == key) {
            return;
        }
        let cached = self
            .blocks
            .get(&key)
            .map_or(&[][..], |cached| &cached.lines);
        let from = if from > cached.len() { 0 } else { from };
        let id = self.next_job;
        self.next_job += 1;
        self.pending.insert(id, key);
        self.jobs.push(HighlightJob {
            id,
            language: block.language.to_owned(),
            from,
            seed: from.checked_sub(1).map(|above| cached[above].clone()),
            known: cached[from..].to_vec(),
            lines: (from..block.count)
                .map(|at| (block.line)(at).to_owned())
                .collect(),
        });
    }
}

/// Where a block's code that started at `start` starts after bytes `old`
/// became `new_len` bytes, or `None` when the edit swallowed its start.
fn moved_start(start: usize, old: &Range<usize>, new_len: usize) -> Option<usize> {
    if old.start >= start {
        Some(start)
    } else if old.end <= start {
        Some(start - old.len() + new_len)
    } else {
        None
    }
}

/// Whether [`refresh_lines`] brought every line up to date.
enum Refreshed {
    Current,
    /// A changed line, the one at this index, is too long to highlight
    /// during layout.
    LongLine(usize),
}

/// Checks `lines` of the block against the cache, highlighting changed
/// ones, and stops at a changed line longer than [`SYNC_LINE_LENGTH`].
fn refresh_lines<'a, F: Fn(usize) -> &'a str>(
    cache: &mut BlockHighlight,
    block: &BlockLines<'a, F>,
    lines: RangeInclusive<usize>,
    syntax: &SyntaxReference,
    syntaxes: &SyntaxSet,
) -> Refreshed {
    for at in lines {
        let text = (block.line)(at);
        let hash = hash_of(&text);
        if cache.lines.get(at).is_some_and(|line| line.hash == hash) {
            continue;
        }
        if text.len() > SYNC_LINE_LENGTH {
            return Refreshed::LongLine(at);
        }
        let previous = at.checked_sub(1).map(|p| &cache.lines[p]);
        let fresh = highlight_after(previous, syntax, syntaxes, text, hash);
        replace_line(&mut cache.lines, at, fresh);
    }
    Refreshed::Current
}

/// Puts a freshly highlighted line at `at`. When it leaves the parser
/// where the old line did, the lines after it still hold; otherwise they
/// go, to be highlighted again.
fn replace_line(lines: &mut Vec<HighlightedLine>, at: usize, fresh: HighlightedLine) {
    let converged = lines
        .get(at)
        .is_some_and(|old| old.state == fresh.state && old.stack == fresh.stack);
    if !converged {
        lines.truncate(at);
    }
    match lines.get_mut(at) {
        Some(slot) => *slot = fresh,
        None => lines.push(fresh),
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
    let last = source.line_of(content.end.saturating_sub(1).max(content.start));
    let index = plan.line.checked_sub(first)?;
    let block = BlockLines {
        language: &language,
        start: content.start,
        line: |at| source.line_text(first + at),
        count: last + 1 - first,
    };
    code.line(&block, index)
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

    #[test]
    fn blocks_move_with_edits_above_them() {
        assert_eq!(moved_start(100, &(10..12), 5), Some(103));
        assert_eq!(
            moved_start(100, &(100..100), 5),
            Some(100),
            "typing at its start"
        );
        assert_eq!(moved_start(100, &(120..130), 0), Some(100));
        assert_eq!(moved_start(100, &(90..110), 0), None);
    }

    /// Highlights `lines` as one block, running background jobs in place.
    fn highlight(code: &mut CodeHighlighter, language: &str, lines: &[&str]) -> Vec<LineSpans> {
        load_syntaxes();
        let block = BlockLines {
            language,
            start: 0,
            line: |at: usize| lines[at],
            count: lines.len(),
        };
        (0..lines.len())
            .map(|index| {
                code.line(&block, index).unwrap_or_else(|| {
                    for job in code.take_jobs() {
                        code.finish_job(job.run());
                    }
                    code.line(&block, index)
                        .expect("highlighted in the background")
                })
            })
            .collect()
    }

    fn kinds(language: &str, lines: &[&str]) -> Vec<Vec<(String, CodeKind)>> {
        let spans = highlight(&mut CodeHighlighter::default(), language, lines);
        spans
            .iter()
            .zip(lines)
            .map(|(spans, line)| {
                spans
                    .iter()
                    .map(|(range, kind)| (line[range.clone()].to_owned(), *kind))
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
        let block = BlockLines {
            language: "no-such-language",
            start: 0,
            line: |_| "x",
            count: 1,
        };
        assert!(code.line(&block, 0).is_none());
        assert!(code.take_jobs().is_empty());
    }

    #[test]
    fn an_edit_rehighlights_the_changed_line_and_what_it_affects() {
        let mut code = CodeHighlighter::default();
        highlight(&mut code, "python", &["a = 1", "b = 2", "c = 3"]);
        code.text_changed(0..0, 0);
        let edited = highlight(&mut code, "python", &["a = 1", "b = \"2\"", "c = 3"]);
        assert!(edited[1].iter().any(|(_, kind)| *kind == CodeKind::String));
        assert!(
            code.take_jobs().is_empty(),
            "a warm language edits in place"
        );
        code.text_changed(0..0, 0);
        let opened = highlight(&mut code, "python", &["a = 1", "b = \"\"\"2", "c = 3"]);
        assert_eq!(opened[2].len(), 1, "an open string reaches the next line");
        assert_eq!(opened[2][0].1, CodeKind::String);
    }

    #[test]
    fn a_cold_language_or_a_long_stretch_goes_to_the_background() {
        load_syntaxes();
        let mut code = CodeHighlighter::default();
        let lines: Vec<String> = (0..100).map(|n| format!("let x{n} = {n};")).collect();
        let block = BlockLines {
            language: "js",
            start: 0,
            line: |at: usize| lines[at].as_str(),
            count: lines.len(),
        };
        assert!(code.line(&block, 0).is_none(), "js hasn't run yet");
        let jobs = code.take_jobs();
        assert_eq!(jobs.len(), 1);
        code.finish_job(jobs.into_iter().next().unwrap().run());
        assert!(code.line(&block, 99).is_some(), "the whole block came back");
        let other = BlockLines {
            language: "js",
            start: 5000,
            line: |at: usize| {
                if at == 0 {
                    "// other"
                } else {
                    lines[at].as_str()
                }
            },
            count: lines.len(),
        };
        assert!(
            code.line(&other, 90).is_none(),
            "90 lines is too many to do now"
        );
        assert!(code.line(&other, 3).is_some(), "a few lines are fine");
    }

    #[test]
    fn a_long_changed_line_keeps_its_old_colours_until_the_background_is_done() {
        let mut code = CodeHighlighter::default();
        let short = ["x = 1  # note"];
        let before = highlight(&mut code, "python", &short);
        let long = format!("x = 1  # note{}", " and more".repeat(20));
        code.text_changed(0..0, 0);
        let block = BlockLines {
            language: "python",
            start: 0,
            line: |_: usize| long.as_str(),
            count: 1,
        };
        assert_eq!(code.line(&block, 0), Some(before[0].clone()));
        let jobs = code.take_jobs();
        assert_eq!(jobs.len(), 1);
        code.finish_job(jobs.into_iter().next().unwrap().run());
        let after = code.line(&block, 0).unwrap();
        assert_eq!(
            after.last().unwrap().0.end,
            long.len(),
            "the comment reaches the end"
        );
    }

    #[test]
    fn a_long_line_is_highlighted_from_where_it_changed() {
        let mut code = CodeHighlighter::default();
        let mut lines: Vec<String> = (0..30).map(|n| format!("x{n} = {n}  # line {n}")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        highlight(&mut code, "python", &refs);
        lines[10] = format!("y = 'a string'  # {}", "and more ".repeat(20));
        code.text_changed(0..0, 0);
        let block = BlockLines {
            language: "python",
            start: 0,
            line: |at: usize| lines[at].as_str(),
            count: lines.len(),
        };
        for at in 0..lines.len() {
            code.line(&block, at);
        }
        let jobs = code.take_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].from, 10, "the lines above stay as they are");
        let done = jobs.into_iter().next().unwrap().run();
        assert_eq!(done.lines.len(), 20);
        code.finish_job(done);
        let spliced: Vec<Option<LineSpans>> =
            (0..lines.len()).map(|at| code.line(&block, at)).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let fresh = highlight(&mut CodeHighlighter::default(), "python", &refs);
        let fresh: Vec<Option<LineSpans>> = fresh.into_iter().map(Some).collect();
        assert_eq!(spliced, fresh);
    }

    #[test]
    fn nothing_before_the_grammars_load_but_a_request() {
        let mut code = CodeHighlighter::default();
        if SYNTAXES.get().is_none() {
            let block = BlockLines {
                language: "rust",
                start: 0,
                line: |_| "fn x() {}",
                count: 1,
            };
            assert!(code.line(&block, 0).is_none());
            assert!(code.take_load_request());
            assert!(!code.take_load_request(), "asked once");
        }
    }
}
