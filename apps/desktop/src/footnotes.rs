//! Footnotes as you write, carrying over Footnotes Plus: problems
//! (missing, duplicate, unused and empty footnotes, and `^[1]` typos) are
//! underlined a moment after typing pauses, and once it has paused for a
//! while numbered footnotes renumber and `^[1]` typos become `[^1]`.
//!
//! Both passes read a copy of the text on a background thread and apply
//! only if the text hasn't changed since. Each automatic fix is its own
//! undo step, and undoing it isn't fought: the text it replaced is
//! remembered and left alone.

use std::ops::Range;
use std::time::Duration;

use gasp_core::document::Selection;
use gasp_core::footnotes::{
    AUTO_RENUMBER_DEBOUNCE_MS, AutoRenumber, FootnoteEdit, FootnoteProblem, FootnoteProblemKind,
    FootnoteSettings, apply_renumber, fix_inline_typos, fix_typos_message, highlight_problems,
    parse_footnotes, tidy_message,
};
use gasp_core::transaction::{ChangeSet, Origin, TextEdit, Transaction};
use gpui::{AppContext, Context, Task};

use crate::editor::{EditorView, HighlightKind};

/// How long typing pauses before problems are underlined.
pub const LINT_DELAY: Duration = Duration::from_millis(250);

/// How long typing pauses before footnotes are tidied.
pub const TIDY_DELAY: Duration = Duration::from_millis(AUTO_RENUMBER_DEBOUNCE_MS);

/// The command automatic fixes are recorded as, for undo.
pub const AUTO_TIDY_COMMAND: &str = "footnote.auto-tidy";

/// The editor's footnote state.
#[derive(Default)]
pub struct FootnoteChecks {
    problems: Vec<FootnoteProblem>,
    lint: Option<Task<()>>,
    tidy: Option<Task<()>>,
    auto: AutoRenumber,
    /// The text an automatic typo fix replaced, so an undo back to it
    /// isn't fixed again.
    typo_guard: Option<String>,
    settings: FootnoteSettings,
}

/// What an idle pass found to do.
struct Tidy {
    edits: Vec<FootnoteEdit>,
    cursor: usize,
}

impl EditorView {
    /// The footnote problems found when typing last paused.
    pub fn footnote_problems(&self) -> &[FootnoteProblem] {
        &self.footnotes.problems
    }

    /// Whether footnotes renumber by themselves once typing pauses.
    pub(crate) fn set_footnote_renumbering(&mut self, on: bool) {
        self.footnotes.settings.auto_renumber_on_edit = on;
        if !on {
            self.footnotes.tidy = None;
        }
    }

    /// The problem covering `offset`, if any.
    pub(crate) fn footnote_problem_at(&self, offset: usize) -> Option<&FootnoteProblem> {
        self.footnotes
            .problems
            .iter()
            .find(|problem| problem.range.start <= offset && offset < problem.range.end)
    }

    /// Underlines problems in a note just opened.
    pub(crate) fn check_footnotes_soon(&mut self, cx: &mut Context<Self>) {
        if !self.read_only {
            self.footnotes.lint = Some(self.lint_after(Duration::ZERO, cx));
        }
    }

    /// Restarts both timers; runs after every edit.
    pub(crate) fn schedule_footnote_checks(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.footnotes.lint = Some(self.lint_after(LINT_DELAY, cx));
        self.footnotes.tidy = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(TIDY_DELAY).await;
            view.update(cx, |view, cx| view.tidy_when_idle(cx)).ok();
        }));
    }

    fn lint_after(&self, delay: Duration, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |view, cx| {
            cx.background_executor().timer(delay).await;
            let Ok(text) = view.update(cx, |view, _| view.text()) else {
                return;
            };
            let found = cx
                .background_spawn(async move { lint(&text).map(|found| (text, found)) })
                .await;
            view.update(cx, |view, cx| view.show_problems(found, cx))
                .ok();
        })
    }

    /// Underlines what the background pass found, if the text is still
    /// the text it read. `None` means the note has no footnotes at all.
    fn show_problems(
        &mut self,
        found: Option<(String, Vec<FootnoteProblem>)>,
        cx: &mut Context<Self>,
    ) {
        let mut problems = match found {
            Some((text, problems)) if self.doc().len() == text.len() && self.text() == text => {
                problems
            }
            Some(_) => return,
            None => Vec::new(),
        };
        // A definition being written is empty only until the next key.
        let cursor_line = self.doc().line_of_offset(self.cursor());
        problems.retain(|problem| {
            problem.kind != FootnoteProblemKind::Empty
                || self.doc().line_of_offset(problem.range.start) != cursor_line
        });
        if problems.is_empty() && self.footnotes.problems.is_empty() {
            return;
        }
        let ranges: Vec<Range<usize>> = problems.iter().map(|p| p.range.clone()).collect();
        self.footnotes.problems = problems;
        self.set_highlights(HighlightKind::FootnoteProblem, ranges, cx);
    }

    /// Converts typos away from the cursor, else renumbers, as its own
    /// undo step. The work runs in the background on a copy of the text.
    fn tidy_when_idle(&mut self, cx: &mut Context<Self>) {
        if !self.footnotes.settings.auto_renumber_on_edit {
            return;
        }
        let text = self.text();
        let cursor = self.cursor();
        let mut auto = std::mem::take(&mut self.footnotes.auto);
        let guard = self.footnotes.typo_guard.take();
        let plan = cx.background_spawn(async move {
            let tidy = plan_idle_tidy(&text, cursor, &mut auto, guard.as_deref());
            (text, auto, tidy)
        });
        self.footnotes.tidy = Some(cx.spawn(async move |view, cx| {
            let (text, auto, tidy) = plan.await;
            view.update(cx, |view, cx| {
                view.footnotes.auto = auto;
                let Some((tidy, fixed_typos)) = tidy else {
                    return;
                };
                if view.text() != text {
                    return;
                }
                if fixed_typos {
                    view.footnotes.typo_guard = Some(text);
                }
                view.apply_footnote_edits(tidy, AUTO_TIDY_COMMAND, cx);
            })
            .ok();
        }));
    }

    fn apply_footnote_edits(&mut self, tidy: Tidy, command: &str, cx: &mut Context<Self>) {
        let edits = tidy
            .edits
            .into_iter()
            .map(|edit| TextEdit::new(edit.range, edit.insert))
            .collect();
        let Ok(changes) = ChangeSet::new(edits) else {
            return;
        };
        let transaction = Transaction::new(changes, Origin::command(command), self.now_ms())
            .with_selection(Selection::cursor(tidy.cursor));
        self.apply_transaction(transaction, cx);
    }

    /// `footnote.tidy`: renumbers now, whatever the cursor is doing.
    pub(crate) fn tidy_footnotes(&mut self, cx: &mut Context<Self>) {
        let outcome = apply_renumber(&self.text(), self.cursor(), false);
        // Notices have no surface yet; the plugin showed this one as a toast.
        eprintln!("{}", tidy_message(&outcome.result, outcome.applied));
        if outcome.applied {
            let tidy = Tidy {
                edits: outcome.result.edits,
                cursor: outcome.cursor,
            };
            self.apply_footnote_edits(tidy, "footnote.tidy", cx);
        }
    }

    /// `footnote.fix-typos`: every `^[1]` becomes `[^1]`, then renumbers.
    pub(crate) fn fix_footnote_typos(&mut self, cx: &mut Context<Self>) {
        let fix = fix_inline_typos(&self.text(), self.cursor());
        eprintln!(
            "{}",
            fix_typos_message(fix.as_ref().map_or(0, |fix| fix.fixed))
        );
        if let Some(fix) = fix {
            let tidy = Tidy {
                edits: fix.edits,
                cursor: fix.cursor,
            };
            self.apply_footnote_edits(tidy, "footnote.fix-typos", cx);
        }
    }
}

/// The problems to underline, or `None` when the text has no footnote
/// syntax, which most notes don't.
fn lint(text: &str) -> Option<Vec<FootnoteProblem>> {
    let has_footnotes = text.contains("[^") || text.contains("^[");
    has_footnotes.then(|| highlight_problems(text))
}

/// What an idle pass does: fix typos the cursor isn't touching (and
/// renumber with them), or renumber alone. The flag says whether typos
/// were fixed. A typo fix is skipped when the text is the one the last
/// fix replaced, since that means the fix was undone.
fn plan_idle_tidy(
    text: &str,
    cursor: usize,
    auto: &mut AutoRenumber,
    typo_guard: Option<&str>,
) -> Option<(Tidy, bool)> {
    if !text.contains("[^") && !text.contains("^[") {
        return None;
    }
    let settings = FootnoteSettings::default();
    let typos = parse_footnotes(text).inline_typos;
    let typing_one = typos
        .iter()
        .any(|typo| typo.range.start <= cursor && cursor <= typo.range.end);
    if !typos.is_empty() && !typing_one && typo_guard != Some(text) {
        let fix = fix_inline_typos(text, cursor)?;
        let tidy = Tidy {
            edits: fix.edits,
            cursor: fix.cursor,
        };
        return Some((tidy, true));
    }
    let (edits, cursor) = auto.on_idle(text, cursor, &settings)?;
    Some((Tidy { edits, cursor }, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gasp_core::footnotes::apply_edits;

    fn idle(text: &str, cursor: usize) -> Option<String> {
        let mut auto = AutoRenumber::new();
        plan_idle_tidy(text, cursor, &mut auto, None)
            .map(|(tidy, _)| apply_edits(text, &tidy.edits))
    }

    #[test]
    fn notes_without_footnotes_cost_nothing() {
        assert_eq!(lint("plain text [link](x)"), None);
        assert_eq!(idle("plain text", 0), None);
    }

    #[test]
    fn out_of_order_footnotes_renumber() {
        let text = "B[^2] then A[^1].\n\n[^1]: one\n[^2]: two\n";
        let tidied = idle(text, 0).unwrap();
        assert_eq!(tidied, "B[^1] then A[^2].\n\n[^1]: two\n[^2]: one\n");
    }

    #[test]
    fn typos_away_from_the_cursor_are_fixed() {
        let text = "Claim^[1].\n\n[^1]: Source.\n";
        assert_eq!(idle(text, 0).unwrap(), "Claim[^1].\n\n[^1]: Source.\n");
        assert_eq!(idle(text, 8), None, "still typing it");
    }

    #[test]
    fn an_undone_typo_fix_stays_undone() {
        let text = "Claim^[1].\n\n[^1]: Source.\n";
        let mut auto = AutoRenumber::new();
        assert!(plan_idle_tidy(text, 0, &mut auto, Some(text)).is_none());
    }

    #[test]
    fn problems_are_found_for_highlighting() {
        let problems = lint("A[^1] and B[^2].\n\n[^1]: one\n[^3]:\n").unwrap();
        let kinds: Vec<&str> = problems.iter().map(|p| p.kind.id()).collect();
        assert_eq!(kinds, ["dangling-ref", "orphan-def"]);
    }
}
