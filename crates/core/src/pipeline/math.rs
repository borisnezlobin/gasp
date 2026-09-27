//! Latex Suite's typing helpers for math, besides snippets:
//!
//! - **Auto-fraction.** `/` after a term makes `\frac{term}{}` with the
//!   cursor in the denominator, and Tab leaves it.
//! - **Matrix shortcuts.** Inside `pmatrix`, `cases`, `align` and the like,
//!   Tab adds ` & ` and Enter ends the row with ` \\`.
//! - **Tab-out.** Tab jumps past the next closing bracket, and at the end
//!   of the math, out of it.
//! - **Enlarged brackets.** After a snippet expands, brackets around a
//!   `\sum`, `\int`, `\frac` or similar become `\left(` and `\right)`
//!   ([`enlarge_brackets`], which the snippet step calls).

use crate::document::Selection;
use crate::transaction::{ChangeSet, Origin, TextEdit, Transaction};

use super::apply::{RangePlan, plan_transaction};
use super::{EditRequest, MathSpan, PipelineStep, StepContext, StepOutcome};

/// Characters that end the term an auto-fraction takes as its numerator,
/// besides whitespace and opening brackets: Latex Suite's breaking
/// characters as the owner has them.
const FRACTION_BREAKS: &[u8] = b"+-=<>";

/// Groups an auto-fraction stays out of, by what opens them: exponents,
/// and units in `\pu{…}`.
const FRACTION_EXCLUDED: [&str; 2] = ["^", "\\pu"];

/// Environments whose rows and columns Tab and Enter fill in.
const MATRIX_ENVIRONMENTS: [&str; 9] = [
    "pmatrix", "cases", "align", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix", "array", "matrix",
];

/// Commands inside a bracket pair that enlarge it.
const ENLARGE_TRIGGERS: [&str; 6] = ["\\sum", "\\int", "\\frac", "\\prod", "\\bigcup", "\\bigcap"];

/// Brackets that can be enlarged, with their closing halves.
const ENLARGE_PAIRS: [(&str, &str); 8] = [
    ("(", ")"),
    ("[", "]"),
    ("\\{", "\\}"),
    ("\\langle", "\\rangle"),
    ("\\lvert", "\\rvert"),
    ("\\lVert", "\\rVert"),
    ("\\lceil", "\\rceil"),
    ("\\lfloor", "\\rfloor"),
];

/// Commands that already size the bracket after them.
const SIZED: [&str; 5] = ["\\left", "\\big", "\\Big", "\\bigg", "\\Bigg"];

/// Which helpers are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathOptions {
    pub auto_fraction: bool,
    pub matrix_shortcuts: bool,
    pub tab_out: bool,
}

impl Default for MathOptions {
    fn default() -> Self {
        MathOptions {
            auto_fraction: true,
            matrix_shortcuts: true,
            tab_out: true,
        }
    }
}

/// The math helpers as a pipeline step. It acts only with a single caret
/// inside math whose bounds are known.
#[derive(Clone, Copy, Debug, Default)]
pub struct MathStep {
    pub options: MathOptions,
}

impl MathStep {
    pub fn new(options: MathOptions) -> Self {
        MathStep { options }
    }

    fn plan(&self, request: &EditRequest, cx: &StepContext<'_>) -> Option<StepOutcome> {
        let [range] = cx.selection.ranges() else {
            return None;
        };
        let math = cx.math?;
        let caret = range.head;
        if !range.is_empty() || caret < math.inner.start || caret > math.inner.end {
            return None;
        }
        match request {
            EditRequest::InsertText(text) if text == "/" && self.options.auto_fraction => {
                auto_fraction(cx, math, caret)
            }
            EditRequest::Tab => self.tab(cx, math, caret),
            EditRequest::Newline if self.in_matrix(cx, math, caret) => {
                let indent = line_indent(cx, caret);
                Some(insert(cx, caret, &format!(" \\\\\n{indent}")))
            }
            _ => None,
        }
    }

    fn tab(&self, cx: &StepContext<'_>, math: &MathSpan, caret: usize) -> Option<StepOutcome> {
        if self.in_matrix(cx, math, caret) {
            return Some(insert(cx, caret, " & "));
        }
        if self.options.tab_out {
            return tab_out(cx, math, caret);
        }
        None
    }

    fn in_matrix(&self, cx: &StepContext<'_>, math: &MathSpan, caret: usize) -> bool {
        self.options.matrix_shortcuts && in_matrix(cx, math, caret)
    }
}

impl PipelineStep for MathStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        match self.plan(&request, cx) {
            Some(outcome) => outcome,
            None => StepOutcome::Continue(request),
        }
    }
}

fn insert(cx: &StepContext<'_>, caret: usize, text: &str) -> StepOutcome {
    StepOutcome::Emit(plan_transaction(
        cx,
        vec![RangePlan::replace(caret..caret, text)],
    ))
}

fn line_indent(cx: &StepContext<'_>, caret: usize) -> String {
    let line = cx.doc.line_text(cx.doc.line_of_offset(caret));
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

// ---- Auto-fraction ----

fn auto_fraction(cx: &StepContext<'_>, math: &MathSpan, caret: usize) -> Option<StepOutcome> {
    let before = cx.doc.slice(math.inner.start..caret);
    if in_excluded_group(&before) {
        return None;
    }
    let start = numerator_start(&before)?;
    if start == before.len() {
        return None;
    }
    let numerator = strip_outer_parens(&before[start..]);
    let text = format!("\\frac{{{numerator}}}{{}}");
    let from = math.inner.start + start;
    let denominator = from + text.len() - 1;
    let after = from + text.len();
    let plan = RangePlan::replace_with_caret(from..caret, &text, text.len() - 1);
    let transaction = plan_transaction(cx, vec![plan]);
    let stops = vec![vec![denominator..denominator], vec![after..after]];
    Some(StepOutcome::EmitWithStops(transaction, stops))
}

/// Where the term before the caret starts: after the nearest space,
/// opening bracket or operator outside any brackets. `None` when a
/// closing bracket has no opening one.
fn numerator_start(before: &str) -> Option<usize> {
    let bytes = before.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let c = bytes[i - 1];
        if matches!(c, b')' | b']' | b'}') {
            i = matching_open(bytes, i - 1)?;
            continue;
        }
        if c.is_ascii_whitespace()
            || matches!(c, b'$' | b'(' | b'[' | b'{')
            || FRACTION_BREAKS.contains(&c)
        {
            return Some(i);
        }
        i -= 1;
    }
    Some(0)
}

/// The index of the bracket that opens the one closing at `close`.
fn matching_open(bytes: &[u8], close: usize) -> Option<usize> {
    let (open, shut) = match bytes[close] {
        b')' => (b'(', b')'),
        b']' => (b'[', b']'),
        _ => (b'{', b'}'),
    };
    let mut depth = 0usize;
    for i in (0..=close).rev() {
        if bytes[i] == shut {
            depth += 1;
        } else if bytes[i] == open {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// `(a+b)` as a numerator loses its parentheses, which the fraction bar
/// replaces; `(a)+(b)` keeps them.
fn strip_outer_parens(term: &str) -> &str {
    let bytes = term.as_bytes();
    let wrapped = bytes.len() >= 2
        && bytes[0] == b'('
        && bytes[bytes.len() - 1] == b')'
        && matching_open(bytes, bytes.len() - 1) == Some(0);
    if wrapped {
        &term[1..term.len() - 1]
    } else {
        term
    }
}

/// Whether the caret is inside a `^{…}` or `\pu{…}` group.
fn in_excluded_group(before: &str) -> bool {
    open_braces(before).into_iter().any(|at| {
        FRACTION_EXCLUDED
            .iter()
            .any(|opener| before[..at].ends_with(opener))
    })
}

/// Offsets of the `{` still open at the end of `text`.
fn open_braces(text: &str) -> Vec<usize> {
    let mut open = Vec::new();
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        match c {
            '{' if !escaped => open.push(at),
            '}' if !escaped => {
                open.pop();
            }
            _ => {}
        }
        escaped = c == '\\' && !escaped;
    }
    open
}

// ---- Matrix shortcuts ----

/// Whether the innermost environment open at the caret is one whose cells
/// Tab and Enter fill in.
fn in_matrix(cx: &StepContext<'_>, math: &MathSpan, caret: usize) -> bool {
    let before = cx.doc.slice(math.inner.start..caret);
    innermost_environment(&before)
        .is_some_and(|name| MATRIX_ENVIRONMENTS.contains(&name.trim_end_matches('*')))
}

/// The name of the last `\begin{…}` in `text` that no `\end{…}` closes.
fn innermost_environment(text: &str) -> Option<&str> {
    let mut open: Vec<&str> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('\\') {
        rest = &rest[at + 1..];
        if let Some(name) = environment_name(rest, "begin{") {
            open.push(name);
        } else if environment_name(rest, "end{").is_some() {
            open.pop();
        }
    }
    open.pop()
}

fn environment_name<'t>(text: &'t str, command: &str) -> Option<&'t str> {
    let after = text.strip_prefix(command)?;
    after.find('}').map(|end| &after[..end])
}

// ---- Tab-out ----

/// Past the next closing bracket before the end of the math, or out of
/// the math when only whitespace is left.
fn tab_out(cx: &StepContext<'_>, math: &MathSpan, caret: usize) -> Option<StepOutcome> {
    let rest = cx.doc.slice(caret..math.inner.end);
    if let Some(past) = past_next_closer(&rest) {
        return Some(move_to(cx, caret + past));
    }
    if !rest.trim().is_empty() {
        return None;
    }
    let end = math.outer.end;
    if !math.block || cx.doc.char_after(end) == Some('\n') {
        let past = if math.block { end + 1 } else { end };
        return Some(move_to(cx, past));
    }
    // Past a block's closing `$$` onto a new last line.
    let plan = RangePlan::replace_with_caret(end..end, "\n", 1);
    Some(StepOutcome::Emit(plan_transaction(cx, vec![plan])))
}

/// How far past the next `}`, `)`, `]`, `>`, `|` or `\rangle` the caret
/// goes.
fn past_next_closer(rest: &str) -> Option<usize> {
    rest.char_indices().find_map(|(at, c)| {
        if matches!(c, '}' | ')' | ']' | '>' | '|') {
            return Some(at + 1);
        }
        rest[at..]
            .starts_with("\\rangle")
            .then_some(at + "\\rangle".len())
    })
}

fn move_to(cx: &StepContext<'_>, offset: usize) -> StepOutcome {
    StepOutcome::Emit(Transaction::select(
        Selection::cursor(offset),
        Origin::Input,
        cx.timestamp_ms,
    ))
}

// ---- Enlarged brackets ----

/// Edits that turn each bracket pair in `text` whose contents hold a
/// `\sum`, `\int`, `\frac` or similar into `\left…\right`, skipping pairs
/// that already have a size. Offsets are in `text`, in order.
pub fn enlarge_brackets(text: &str) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let pair = ENLARGE_PAIRS
            .iter()
            .find(|(open, _)| text[i..].starts_with(open));
        let Some((open, close)) = pair else {
            i += text[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        if let Some(j) = matching_close(text, i, open, close) {
            let sized = SIZED.iter().any(|size| text[..i].ends_with(size));
            let contents = &text[i + open.len()..j];
            if !sized && ENLARGE_TRIGGERS.iter().any(|t| contents.contains(t)) {
                edits.push(TextEdit::insert(i, "\\left"));
                edits.push(TextEdit::insert(j, "\\right"));
            }
        }
        i += open.len();
    }
    edits.sort_by_key(|edit| edit.range.start);
    edits
}

/// Where the `close` matching the `open` at `start` begins.
fn matching_close(text: &str, start: usize, open: &str, close: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = start;
    while i < text.len() {
        if text[i..].starts_with(open) {
            depth += 1;
            i += open.len();
        } else if text[i..].starts_with(close) {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
            i += close.len();
        } else {
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    None
}

/// [`enlarge_brackets`] for the math source `region`, which starts at
/// `region_start` in the document, as a change to the document.
pub(crate) fn enlarge_in(region: &str, region_start: usize) -> Option<ChangeSet> {
    let edits: Vec<TextEdit> = enlarge_brackets(region)
        .into_iter()
        .map(|edit| TextEdit::insert(region_start + edit.range.start, edit.insert))
        .collect();
    if edits.is_empty() {
        return None;
    }
    ChangeSet::new(edits).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enlarged(text: &str) -> String {
        let mut out = text.to_string();
        for edit in enlarge_brackets(text).into_iter().rev() {
            out.replace_range(edit.range, &edit.insert);
        }
        out
    }

    fn term(before: &str) -> &str {
        &before[numerator_start(before).unwrap()..]
    }

    #[test]
    fn brackets_around_big_operators_enlarge() {
        assert_eq!(enlarged("(\\frac{a}{b})"), "\\left(\\frac{a}{b}\\right)");
        assert_eq!(enlarged("2(x+1)"), "2(x+1)");
        assert_eq!(enlarged("\\left(\\sum x\\right)"), "\\left(\\sum x\\right)");
        assert_eq!(
            enlarged("[a (\\int f) b]"),
            "\\left[a \\left(\\int f\\right) b\\right]"
        );
        assert_eq!(enlarged("\\{ \\sum_i \\}"), "\\left\\{ \\sum_i \\right\\}");
    }

    #[test]
    fn numerators_take_the_term_before_the_slash() {
        assert_eq!(term("a + x^2"), "x^2");
        assert_eq!(term("y=\\sin(x)"), "\\sin(x)");
        assert_eq!(term("(a+b)"), "(a+b)");
        assert_eq!(term("e^{i\\pi}"), "e^{i\\pi}");
        assert_eq!(term("a "), "");
        assert!(numerator_start("a)").is_none());
        assert_eq!(strip_outer_parens("(a+b)"), "a+b");
        assert_eq!(strip_outer_parens("(a)+(b)"), "(a)+(b)");
    }

    #[test]
    fn exponents_and_units_are_left_alone() {
        assert!(in_excluded_group("x^{a"));
        assert!(in_excluded_group("\\pu{3 m"));
        assert!(!in_excluded_group("x^{a} b"));
        assert!(!in_excluded_group("\\frac{a"));
    }

    #[test]
    fn the_innermost_open_environment_counts() {
        assert_eq!(
            innermost_environment("\\begin{pmatrix} a & b"),
            Some("pmatrix")
        );
        assert_eq!(
            innermost_environment("\\begin{align} \\begin{cases} x \\end{cases} y"),
            Some("align")
        );
        assert_eq!(innermost_environment("\\begin{cases}\\end{cases}"), None);
    }

    #[test]
    fn tab_out_passes_the_next_closer() {
        assert_eq!(past_next_closer("b}{c}"), Some(2));
        assert_eq!(past_next_closer(" \\rangle x"), Some(8));
        assert_eq!(past_next_closer("abc"), None);
    }
}
