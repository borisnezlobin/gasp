//! Tab stops left by a snippet: `\frac{●}{●}●` puts the cursor in the
//! numerator, and Tab moves it to the denominator and then past the
//! fraction. The stops follow the text as it's edited.

use std::ops::Range;

use crate::document::{Selection, SelectionRange};
use crate::transaction::{Assoc, ChangeSet, Origin, Transaction};

use super::{EditRequest, PipelineOutput, PipelineStep, StepContext, StepOutcome};

/// The stops of the snippets expanded most recently, in the order Tab
/// visits them, and the one the cursor is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStops {
    /// Each stop is one or more ranges: a stop that appears twice in a
    /// snippet, like the environment name in `\begin{●1}…\end{●1}`, is
    /// edited in both places at once.
    stops: Vec<Vec<Range<usize>>>,
    current: usize,
}

impl TabStops {
    /// Stops for a fresh expansion, with the cursor on the first. `None`
    /// when there is nowhere for Tab to go.
    pub fn new(stops: Vec<Vec<Range<usize>>>) -> Option<TabStops> {
        let stops: Vec<_> = stops.into_iter().filter(|s| !s.is_empty()).collect();
        (stops.len() > 1).then_some(TabStops { stops, current: 0 })
    }

    /// Stops for an expansion made while `outer`'s were still pending: the
    /// new ones come first, then the rest of the outer snippet's, so Tab
    /// finishes the inner snippet before going on.
    pub fn nest(inner: Vec<Vec<Range<usize>>>, outer: Option<&TabStops>) -> Option<TabStops> {
        let rest = outer
            .map(|outer| outer.stops[outer.current + 1..].to_vec())
            .unwrap_or_default();
        let mut stops: Vec<_> = inner.into_iter().filter(|s| !s.is_empty()).collect();
        if stops.is_empty() {
            return outer.cloned();
        }
        stops.extend(rest);
        TabStops::new(stops)
    }

    /// Follows a change to the text. A stop grows when text is typed at
    /// either end, so what's typed into an empty stop stays in it.
    pub fn map(&mut self, changes: &ChangeSet) {
        for range in self.stops.iter_mut().flatten() {
            let start = changes.map_offset(range.start, Assoc::Before);
            let end = changes.map_offset(range.end, Assoc::After);
            *range = start..end.max(start);
        }
    }

    /// Which stop the cursor is on, counting from 0.
    pub fn current(&self) -> usize {
        self.current
    }

    pub fn len(&self) -> usize {
        self.stops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }

    /// Whether the cursor is on the last stop, where the snippet is done.
    pub fn is_done(&self) -> bool {
        self.current + 1 >= self.stops.len()
    }

    /// The selection that puts the cursor on stop `index`, with every copy
    /// of a mirrored stop selected.
    pub fn selection(&self, index: usize) -> Option<Selection> {
        let ranges: Vec<SelectionRange> = self
            .stops
            .get(index)?
            .iter()
            .map(|range| SelectionRange::new(range.start, range.end))
            .collect();
        Some(Selection::new(ranges, 0))
    }

    /// Moves to stop `index`.
    pub fn go_to(&mut self, index: usize) {
        self.current = index.min(self.stops.len().saturating_sub(1));
    }

    /// Moves back to the previous stop, as Shift+Tab does, and returns the
    /// selection there. `None` on the first stop.
    pub fn back(&mut self) -> Option<Selection> {
        let previous = self.current.checked_sub(1)?;
        self.current = previous;
        self.selection(previous)
    }

    /// Whether `offset` is on the current stop, so Tab still means "next
    /// stop" there. Once the cursor has wandered off, Tab does its usual
    /// job again.
    pub fn holds(&self, offset: usize) -> bool {
        self.stops[self.current]
            .iter()
            .any(|range| range.start <= offset && offset <= range.end)
    }

    /// The stops still to come, which the editor marks so it's clear where
    /// Tab goes next.
    pub fn pending(&self) -> impl Iterator<Item = &Range<usize>> {
        self.stops[self.current + 1..].iter().flatten()
    }
}

/// Carries a snippet's tab stops through what the pipeline made of a
/// request, once its transactions are applied: they follow the edit, a
/// new expansion's stops go first, and Tab onto the last stop finishes
/// the snippet.
pub fn follow_stops(session: Option<TabStops>, output: &PipelineOutput) -> Option<TabStops> {
    let mut session = session;
    if let Some(stops) = session.as_mut() {
        for transaction in &output.transactions {
            stops.map(&transaction.changes);
        }
    }
    if let Some(new) = &output.stops {
        return TabStops::nest(new.clone(), session.as_ref());
    }
    let mut stops = session?;
    if output.step.as_deref() == Some(super::step_names::TAB_STOPS) {
        stops.go_to(stops.current() + 1);
        if stops.is_done() {
            return None;
        }
    }
    Some(stops)
}

/// Tab moves to the next stop while the cursor is on one.
#[derive(Clone, Copy, Debug, Default)]
pub struct TabStopStep;

impl PipelineStep for TabStopStep {
    fn run(&self, request: EditRequest, cx: &StepContext<'_>) -> StepOutcome {
        let next = cx
            .tab_stops
            .filter(|_| request == EditRequest::Tab)
            .filter(|stops| stops.holds(cx.selection.primary().head))
            .and_then(|stops| stops.selection(stops.current() + 1));
        match next {
            Some(selection) => StepOutcome::Emit(Transaction::select(
                selection,
                Origin::Input,
                cx.timestamp_ms,
            )),
            None => StepOutcome::Continue(request),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stops() -> TabStops {
        // \frac{}{} with stops in the numerator, the denominator and after.
        TabStops::new(vec![vec![6..6], vec![8..8], vec![9..9]]).unwrap()
    }

    #[test]
    fn a_single_stop_leaves_nowhere_to_go() {
        assert!(TabStops::new(vec![vec![1..1]]).is_none());
    }

    #[test]
    fn typing_in_a_stop_grows_it_and_moves_the_rest() {
        let mut stops = stops();
        stops.map(&ChangeSet::insert(6, "ab"));
        assert_eq!(stops.stops, vec![vec![6..8], vec![10..10], vec![11..11]]);
        assert!(stops.holds(8));
        assert!(!stops.holds(10));
    }

    #[test]
    fn nesting_puts_the_inner_stops_first() {
        let mut outer = stops();
        outer.map(&ChangeSet::insert(6, "\\sqrt{}"));
        let nested = TabStops::nest(vec![vec![12..12], vec![13..13]], Some(&outer)).unwrap();
        assert_eq!(
            nested.stops,
            vec![vec![12..12], vec![13..13], vec![15..15], vec![16..16]]
        );
    }

    #[test]
    fn pending_stops_are_the_ones_after_the_cursor() {
        let mut stops = stops();
        stops.go_to(1);
        assert_eq!(stops.pending().cloned().collect::<Vec<_>>(), vec![9..9]);
        assert!(!stops.is_done());
        assert!(stops.back().is_some());
        assert_eq!(stops.current(), 0);
        assert!(stops.back().is_none());
        stops.go_to(2);
        assert!(stops.is_done());
    }
}
