//! Auto-pairing of brackets, quotes and `$`: typing an opener inserts its
//! closer, typing a closer steps over one that is already there, and
//! backspace between an empty pair deletes both.

use crate::document::{Document, SelectionRange};

use super::apply::{RangePlan, backspace_plan, plan_each};
use super::{EditRequest, InputContext, PipelineStep, StepContext, StepOutcome};

/// Pairs an opening character with its closing one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pair {
    pub open: char,
    pub close: char,
}

impl Pair {
    pub const fn new(open: char, close: char) -> Self {
        Self { open, close }
    }

    fn is_symmetric(self) -> bool {
        self.open == self.close
    }
}

/// The auto-pair step and its pair table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoPairStep {
    pub pairs: Vec<Pair>,
}

impl Default for AutoPairStep {
    fn default() -> Self {
        Self {
            pairs: vec![
                Pair::new('(', ')'),
                Pair::new('[', ']'),
                Pair::new('{', '}'),
                Pair::new('"', '"'),
                Pair::new('\'', '\''),
                Pair::new('`', '`'),
                Pair::new('$', '$'),
            ],
        }
    }
}

impl PipelineStep for AutoPairStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        match &request {
            EditRequest::InsertText(text) => match single_char(text) {
                Some(typed) if self.handles(typed) => {
                    StepOutcome::Emit(plan_each(cx, |range| self.plan_typed(cx, range, typed)))
                }
                _ => StepOutcome::Continue(request),
            },
            EditRequest::DeleteBackward if self.any_empty_pair(cx) => {
                StepOutcome::Emit(plan_each(cx, |range| self.plan_backspace(cx.doc, range)))
            }
            _ => StepOutcome::Continue(request),
        }
    }
}

fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}

impl AutoPairStep {
    fn handles(&self, typed: char) -> bool {
        self.pairs
            .iter()
            .any(|pair| pair.open == typed || pair.close == typed)
    }

    fn opening(&self, typed: char) -> Option<Pair> {
        self.pairs.iter().copied().find(|pair| pair.open == typed)
    }

    fn is_closer(&self, ch: char) -> bool {
        self.pairs.iter().any(|pair| pair.close == ch)
    }

    fn plan_typed(&self, cx: &StepContext<'_>, range: &SelectionRange, typed: char) -> RangePlan {
        let typed_len = typed.len_utf8();
        let next = cx.doc.char_after(range.head);
        if range.is_empty() && next == Some(typed) && self.is_closer(typed) {
            return RangePlan::move_caret(range.head + typed_len);
        }
        let Some(pair) = self.opening(typed) else {
            return RangePlan::replace(range.range(), &typed.to_string());
        };
        if !range.is_empty() {
            return wrap_selection(cx.doc, range, pair);
        }
        if self.should_pair(cx, range.head, pair) {
            let text = format!("{}{}", pair.open, pair.close);
            return RangePlan::replace_with_caret(range.range(), &text, typed_len);
        }
        RangePlan::replace(range.range(), &typed.to_string())
    }

    /// Pairs only where a closer can't be mistaken for part of a word: the
    /// next character must be a space, a closer or the end, and a quote or
    /// `$` must not follow a letter or digit (so `don't` and `5$` stay).
    fn should_pair(&self, cx: &StepContext<'_>, offset: usize, pair: Pair) -> bool {
        let next_is_free = cx
            .doc
            .char_after(offset)
            .is_none_or(|next| next.is_whitespace() || self.is_closer(next));
        if !next_is_free {
            return false;
        }
        if !pair.is_symmetric() {
            return true;
        }
        let closes_math = pair.open == '$' && cx.context == InputContext::Math;
        let after_word = cx
            .doc
            .char_before(offset)
            .is_some_and(char::is_alphanumeric);
        !closes_math && !after_word
    }

    fn empty_pair_at(&self, doc: &Document, range: &SelectionRange) -> Option<Pair> {
        if !range.is_empty() {
            return None;
        }
        let before = doc.char_before(range.head)?;
        let after = doc.char_after(range.head)?;
        self.pairs
            .iter()
            .copied()
            .find(|pair| pair.open == before && pair.close == after)
    }

    fn any_empty_pair(&self, cx: &StepContext<'_>) -> bool {
        cx.selection
            .ranges()
            .iter()
            .any(|range| self.empty_pair_at(cx.doc, range).is_some())
    }

    fn plan_backspace(&self, doc: &Document, range: &SelectionRange) -> RangePlan {
        match self.empty_pair_at(doc, range) {
            Some(pair) => {
                let start = range.head - pair.open.len_utf8();
                let end = range.head + pair.close.len_utf8();
                RangePlan::replace(start..end, "")
            }
            None => backspace_plan(doc, range),
        }
    }
}

/// Surrounds the selection with the pair and keeps the inner text selected.
fn wrap_selection(doc: &Document, range: &SelectionRange, pair: Pair) -> RangePlan {
    let inner = doc.slice(range.range());
    let text = format!("{}{inner}{}", pair.open, pair.close);
    let inner_start = pair.open.len_utf8();
    let inner_end = inner_start + inner.len();
    let (anchor, head) = if range.head < range.anchor {
        (inner_end, inner_start)
    } else {
        (inner_start, inner_end)
    };
    RangePlan {
        edit: crate::transaction::TextEdit::new(range.range(), text),
        anchor,
        head,
    }
}
