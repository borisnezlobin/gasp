//! Jump to heading (`outline.jump-to-heading`, Mod+Shift+O): the note's
//! headings, indented by level, filtered by what you type.

use editor_core::syntax::{self, NodeKind, SyntaxTree};
use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    ParentElement, Render, SharedString, Subscription, Window, div, prelude::*,
};

use crate::picker::fuzzy::{Candidate, Matcher, Query};
use crate::picker::{Confirmed, Picker, PickerDelegate, highlighted_text};
use crate::theme::PickerTheme;

/// What the outline asks its owner to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutlineEvent {
    /// Move the cursor to this byte offset, the start of a heading.
    Jump(usize),
}

/// A heading in a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// 1 to 6.
    pub level: u8,
    /// The heading's text without its `#` or underline markers.
    pub title: String,
    /// The byte offset where the heading starts.
    pub offset: usize,
}

/// Every heading in `text`, in document order, including ones inside
/// quotes and callouts.
pub fn headings(text: &str) -> Vec<Heading> {
    headings_in(&syntax::parse(text), text)
}

/// Every heading in `text`, read from its already parsed `tree`.
pub fn headings_in(tree: &SyntaxTree, text: &str) -> Vec<Heading> {
    tree.preorder()
        .into_iter()
        .filter_map(|id| heading_at(tree, id, text))
        .collect()
}

fn heading_at(tree: &SyntaxTree, id: syntax::NodeId, text: &str) -> Option<Heading> {
    let node = tree.node(id);
    let NodeKind::Heading { level, .. } = node.kind else {
        return None;
    };
    let raw: String = node
        .content
        .iter()
        .filter_map(|range| text.get(range.clone()))
        .collect();
    let title = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(Heading {
        level,
        title,
        offset: node.range.start,
    })
}

#[derive(Clone, Debug)]
struct HeadingMatch {
    heading: usize,
    score: i32,
    positions: Vec<usize>,
}

/// The outline's items and matching.
pub struct OutlineDelegate {
    headings: Vec<Heading>,
    candidates: Vec<Candidate>,
    matches: Vec<HeadingMatch>,
    /// The heading the cursor is under.
    current: Option<usize>,
    top_level: u8,
    matcher: Matcher,
}

impl OutlineDelegate {
    /// The headings of `text`, with the one the byte offset `cursor` is
    /// under selected first.
    pub fn new(text: &str, cursor: usize) -> Self {
        let headings = headings(text);
        let current = headings
            .iter()
            .rposition(|heading| heading.offset <= cursor);
        Self {
            candidates: headings.iter().map(|h| Candidate::new(&h.title)).collect(),
            top_level: headings.iter().map(|h| h.level).min().unwrap_or(1),
            headings,
            matches: Vec::new(),
            current,
            matcher: Matcher::new(),
        }
    }

    pub fn headings(&self) -> &[Heading] {
        &self.headings
    }

    /// The heading shown in row `index`.
    pub fn heading_at(&self, index: usize) -> Option<&Heading> {
        self.headings.get(self.matches.get(index)?.heading)
    }
}

impl PickerDelegate for OutlineDelegate {
    type Event = OutlineEvent;

    fn placeholder(&self) -> SharedString {
        "Jump to a heading".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn update_matches(&mut self, query: &str) {
        let query = Query::new(query);
        let mut matches: Vec<HeadingMatch> = Vec::new();
        for (heading, candidate) in self.candidates.iter().enumerate() {
            if let Some(found) = self.matcher.score(&query, candidate) {
                matches.push(HeadingMatch {
                    heading,
                    score: found.score,
                    positions: found.positions,
                });
            }
        }
        matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.heading.cmp(&b.heading)));
        self.matches = matches;
    }

    fn default_selection(&self, query: &str) -> usize {
        if !query.trim().is_empty() {
            return 0;
        }
        self.current.unwrap_or(0)
    }

    fn render_match(&self, index: usize, _selected: bool, theme: &PickerTheme) -> AnyElement {
        let Some(found) = self.matches.get(index) else {
            return div().into_any_element();
        };
        let heading = &self.headings[found.heading];
        let depth = heading.level.saturating_sub(self.top_level);
        div()
            .flex()
            .w_full()
            .pl(theme.level_indent * f32::from(depth))
            .text_size(theme.row_font_size)
            .child(highlighted_text(heading.title.clone(), &found.positions, theme).grow())
            .into_any_element()
    }

    fn confirm(&mut self, index: usize) -> Option<OutlineEvent> {
        Some(OutlineEvent::Jump(self.heading_at(index)?.offset))
    }

    fn empty_message(&self, query: &str) -> SharedString {
        if self.headings.is_empty() {
            return "This note has no headings yet.".into();
        }
        format!("No headings match “{}”.", query.trim()).into()
    }
}

/// Jump to heading. Emits [`OutlineEvent`], then [`DismissEvent`].
pub struct OutlinePicker {
    picker: Entity<Picker<OutlineDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<OutlineEvent> for OutlinePicker {}
impl EventEmitter<DismissEvent> for OutlinePicker {}

impl Focusable for OutlinePicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl OutlinePicker {
    /// The headings of the note `text`, starting on the one above the byte
    /// offset `cursor`.
    pub fn new(text: &str, cursor: usize, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = OutlineDelegate::new(text, cursor);
        let picker = cx.new(|cx| Picker::new(delegate, window, cx));
        let subscriptions = vec![
            cx.subscribe(&picker, |_, _, event: &Confirmed<OutlineEvent>, cx| {
                cx.emit(event.0.clone());
                cx.emit(DismissEvent);
            }),
            cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| cx.emit(DismissEvent)),
        ];
        Self {
            picker,
            _subscriptions: subscriptions,
        }
    }

    pub fn picker(&self) -> &Entity<Picker<OutlineDelegate>> {
        &self.picker
    }
}

impl Render for OutlinePicker {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.picker.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ntitle: x\n---\n# Trip\n\nIntro\n\n## Packing *list*\n\ntext\n\nBudget\n---\n\n> ### Quoted\n\n```\n# not a heading\n```\n";

    #[test]
    fn finds_every_heading_with_its_level_and_offset() {
        let found = headings(NOTE);
        let summary: Vec<(u8, &str)> = found.iter().map(|h| (h.level, h.title.as_str())).collect();
        assert_eq!(
            summary,
            [
                (1, "Trip"),
                (2, "Packing *list*"),
                (2, "Budget"),
                (3, "Quoted")
            ]
        );
        assert_eq!(&NOTE[found[0].offset..found[0].offset + 6], "# Trip");
        assert!(NOTE[found[3].offset..].starts_with("### Quoted"));
    }

    #[test]
    fn starts_on_the_heading_above_the_cursor() {
        let cursor = NOTE.find("text").unwrap();
        let mut outline = OutlineDelegate::new(NOTE, cursor);
        outline.update_matches("");
        assert_eq!(outline.default_selection(""), 1);
        assert_eq!(outline.default_selection("bu"), 0);
    }

    #[test]
    fn filtering_ranks_the_best_heading_first() {
        let mut outline = OutlineDelegate::new(NOTE, 0);
        outline.update_matches("bud");
        assert_eq!(outline.heading_at(0).unwrap().title, "Budget");
        let offset = NOTE.find("Budget").unwrap();
        assert_eq!(outline.confirm(0), Some(OutlineEvent::Jump(offset)));
    }

    #[test]
    fn a_note_without_headings_says_so() {
        let mut outline = OutlineDelegate::new("just text", 0);
        outline.update_matches("");
        assert_eq!(outline.match_count(), 0);
        assert_eq!(outline.empty_message(""), "This note has no headings yet.");
    }
}
