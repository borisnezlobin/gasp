//! Smart quotes: a typed `"` or `'` becomes a curly quote, opening after a
//! space, the start of a line, an opening bracket or another opening quote,
//! and closing everywhere else, so `it's` gets an apostrophe.
//!
//! The quote goes in straight first and is curled as a separate undo step,
//! so undo right after gives back the straight quote. Pasted text is curled
//! by [`curl_quotes`], which skips code, math, frontmatter, links and HTML.

use crate::document::{Document, SelectionRange};
use crate::syntax::SyntaxTree;
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

use super::apply::RangePlan;
use super::{EditRequest, InputContext, PipelineStep, StepContext, StepOutcome};

/// The command a curl is recorded as, apart from the typing before it.
pub const CURL_COMMAND: &str = "smart-quotes";

/// Characters after which a quote opens, besides whitespace.
const OPENS_AFTER: &str = "([{<‘“—–";

/// The contexts quotes curl in. Everything else (code, math, frontmatter,
/// links and HTML) keeps its straight quotes.
pub const CURLS_IN: [InputContext; 3] = [
    InputContext::Text,
    InputContext::Table,
    InputContext::Comment,
];

/// The curly form of `quote` after `previous`, or `None` when `quote`
/// isn't a straight quote.
pub fn curly_quote(quote: char, previous: Option<char>) -> Option<char> {
    let opens =
        previous.is_none_or(|previous| previous.is_whitespace() || OPENS_AFTER.contains(previous));
    match (quote, opens) {
        ('"', true) => Some('“'),
        ('"', false) => Some('”'),
        ('\'', true) => Some('‘'),
        ('\'', false) => Some('’'),
        _ => None,
    }
}

/// Curls the quotes typed at a single caret, and wraps a single selection
/// in curly quotes. Several carets are left to the steps after it.
#[derive(Clone, Copy, Debug, Default)]
pub struct SmartQuoteStep;

impl PipelineStep for SmartQuoteStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        let EditRequest::InsertText(text) = &request else {
            return StepOutcome::Continue(request);
        };
        let (Some(quote), [range]) = (single_quote(text), cx.selection.ranges()) else {
            return StepOutcome::Continue(request);
        };
        if in_unclosed_code_or_link(cx.doc, range.from()) {
            return StepOutcome::Continue(request);
        }
        if !range.is_empty() {
            return StepOutcome::Emit(wrap(cx, range, quote));
        }
        let at = range.head;
        let curly = curly_quote(quote, cx.doc.char_before(at)).expect("the quote is straight");
        let changes = ChangeSet::replace(at..at + quote.len_utf8(), curly.to_string());
        let curl = Transaction::new(changes, Origin::command(CURL_COMMAND), cx.timestamp_ms)
            .with_selection(crate::document::Selection::cursor(at + curly.len_utf8()));
        StepOutcome::EmitAfterTyping(curl)
    }
}

fn single_quote(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let quote = chars.next().filter(|c| matches!(c, '"' | '\''))?;
    chars.next().is_none().then_some(quote)
}

/// Whether `offset` follows an unclosed backtick or `[[` on its line: a
/// code span or wikilink still being typed, which the parser doesn't see
/// yet.
fn in_unclosed_code_or_link(doc: &Document, offset: usize) -> bool {
    let line = doc.line_start(doc.line_of_offset(offset));
    let before = doc.slice(line..offset);
    let open_code = before.matches('`').count() % 2 == 1;
    let open_link = before
        .rfind("[[")
        .is_some_and(|open| !before[open..].contains("]]"));
    open_code || open_link
}

/// Surrounds the selection with an opening and closing quote, keeping the
/// inner text selected.
fn wrap(cx: &StepContext<'_>, range: &SelectionRange, quote: char) -> Transaction {
    let (open, close) = match quote {
        '"' => ('“', '”'),
        _ => ('‘', '’'),
    };
    let inner = cx.doc.slice(range.range());
    let start = open.len_utf8();
    let end = start + inner.len();
    let (anchor, head) = if range.head < range.anchor {
        (end, start)
    } else {
        (start, end)
    };
    let plan = RangePlan {
        edit: TextEdit::new(range.range(), format!("{open}{inner}{close}")),
        anchor,
        head,
    };
    super::plan_transaction(cx, vec![plan])
}

/// Curls the straight quotes in `text`, which is about to go in after the
/// character `previous`. Quotes inside code, math, frontmatter, links or
/// HTML, as `text` parses on its own, stay straight.
pub fn curl_quotes(text: &str, previous: Option<char>) -> String {
    if !text.contains(['"', '\'']) {
        return text.to_owned();
    }
    let tree = crate::syntax::parse(text);
    let mut curled = String::with_capacity(text.len() + 8);
    let mut last = previous;
    for (at, ch) in text.char_indices() {
        let replacement = curly_quote(ch, last).filter(|_| curls_at(&tree, at));
        curled.push(replacement.unwrap_or(ch));
        last = Some(ch);
    }
    curled
}

/// Whether a quote at `offset` of the parsed text curls. The context is
/// read just after the quote, so a quote opening a code span or link
/// counts as inside it.
fn curls_at(tree: &SyntaxTree, offset: usize) -> bool {
    let inside = |at: usize| CURLS_IN.contains(&tree.context_at(at));
    inside(offset) && inside(offset + 1)
}
