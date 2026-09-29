//! One open note: its text and syntax tree, kept in step with the text view
//! so each keystroke reparses only the blocks it touched.

use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use editor_core::link_card::meta::card_from_html;
use editor_core::link_card::{card_replacement, url_on_line};
use editor_core::render::folds::Folds;
use editor_core::render::{RenderInput, reveal_settings};
use editor_core::syntax::{self, Edit, NodeKind, SyntaxTree, WikiInfo};
use editor_core::table::Table;
use editor_prose::{Length, Purpose, sentence_lengths, units};

use crate::display::{DisplayState, SharedDisplay};
use crate::edits::{self, CommandInput, CommandOutcome, TextReplacement};
use crate::offsets::{TextRange, Utf16Offsets};
use crate::plan::{NotePlan, note_plan};

#[derive(uniffi::Object)]
pub struct NoteDocument {
    parsed: Mutex<ParsedText>,
    display: SharedDisplay,
}

pub(crate) struct ParsedText {
    pub(crate) text: String,
    pub(crate) tree: SyntaxTree,
    pub(crate) offsets: Utf16Offsets,
    /// The table shown as its Markdown source until the cursor leaves it.
    source_table: Option<Range<usize>>,
    /// Headings and callouts folded in this view.
    pub(crate) folds: Folds,
}

impl ParsedText {
    fn new(text: String) -> Self {
        Self {
            tree: syntax::parse(&text),
            offsets: Utf16Offsets::new(&text),
            text,
            source_table: None,
            folds: Folds::default(),
        }
    }

    fn update(&mut self, text: String) {
        let Some(edit) = changed_span(&self.text, &text) else {
            return;
        };
        self.tree.edit(&text, &edit);
        self.folds.map(&edit);
        self.offsets = Utf16Offsets::new(&text);
        self.text = text;
        self.source_table = None;
    }

    fn table_at(&self, offset: usize) -> Option<Range<usize>> {
        Table::at(&self.text, &self.tree, offset).map(|table| table.range)
    }
}

/// A heading in a note's outline.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct OutlineHeading {
    pub level: u8,
    pub title: String,
    /// The heading's whole line.
    pub range: TextRange,
}

/// How long a sentence reads, for its tint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SentenceLength {
    Short,
    Medium,
    Long,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SentenceTint {
    pub range: TextRange,
    pub length: SentenceLength,
}

#[uniffi::export]
impl NoteDocument {
    /// A note drawn with the built-in settings.
    #[uniffi::constructor]
    pub fn new(text: String) -> Arc<Self> {
        Self::with_display(text, SharedDisplay::new(DisplayState::default()))
    }

    pub fn text(&self) -> String {
        self.lock().text.clone()
    }

    /// Takes the text view's whole text after any change (typing,
    /// autocorrect, dictation, paste or undo) and reparses what differs.
    pub fn update(&self, text: String) {
        self.lock().update(text);
    }

    /// What every line should look like with `selection` (in UTF-16
    /// offsets; an empty range is the cursor), as the desktop app plans it.
    pub fn plan(&self, selection: TextRange) -> NotePlan {
        let mut parsed = self.lock();
        let selected = parsed.offsets.byte_range(selection);
        let cursor_left_table = parsed
            .source_table
            .as_ref()
            .is_some_and(|table| !(table.start <= selected.end && selected.end <= table.end));
        if cursor_left_table {
            parsed.source_table = None;
        }
        let mut settings = reveal_settings(&self.display.lock().symbols);
        settings.source_table = parsed.source_table.as_ref().map(|table| table.start);
        let selections = [selected];
        let mut plan = editor_core::render::plan(&RenderInput {
            text: &parsed.text,
            tree: &parsed.tree,
            selections: &selections,
            settings: &settings,
        });
        parsed
            .folds
            .apply(&mut plan.lines, &parsed.tree, &selections);
        note_plan(&plan, &parsed.offsets)
    }

    /// Runs the editing command `id` on `selection`.
    pub fn run_command(&self, id: String, selection: TextRange) -> CommandOutcome {
        let mut parsed = self.lock();
        if id == "table.edit-as-markdown" {
            return show_table_source(&mut parsed, selection);
        }
        let input = CommandInput {
            text: &parsed.text,
            tree: &parsed.tree,
            offsets: &parsed.offsets,
            selection: edits::selection(&parsed.offsets, selection),
        };
        edits::run(&id, &input)
    }

    /// Whether `id` is a command [`NoteDocument::run_command`] handles.
    pub fn handles(&self, id: String) -> bool {
        id == "table.edit-as-markdown" || edits::is_note_command(&id)
    }

    /// Every heading, in order.
    pub fn outline(&self) -> Vec<OutlineHeading> {
        let parsed = self.lock();
        parsed
            .tree
            .preorder()
            .into_iter()
            .map(|id| parsed.tree.node(id))
            .filter_map(|node| match node.kind {
                NodeKind::Heading { level, .. } => Some(OutlineHeading {
                    level,
                    title: plain_text(&parsed.text, &parsed.tree, node.range.clone()),
                    range: parsed.offsets.range(&node.range),
                }),
                _ => None,
            })
            .collect()
    }

    /// Where the link at `offset` goes, as written: a web address, a note
    /// with an optional `#heading`, or only `#heading` in this note.
    pub fn link_at(&self, offset: u32) -> Option<String> {
        let parsed = self.lock();
        let at = parsed.offsets.byte(offset);
        parsed
            .tree
            .path_at(at)
            .into_iter()
            .rev()
            .find_map(|id| link_target(&parsed.tree.node(id).kind))
    }

    /// The web address alone on the cursor's line, which can become a card.
    pub fn card_address(&self, offset: u32) -> Option<String> {
        let parsed = self.lock();
        url_on_line(&parsed.text, parsed.offsets.byte(offset))
    }

    /// The edit that turns the line holding `url` into a card, from the
    /// page's HTML.
    pub fn make_card(&self, url: String, html: String) -> CommandOutcome {
        let parsed = self.lock();
        let card = card_from_html(&url, &html);
        let Some((range, markdown)) = card_replacement(&parsed.text, &url, &card) else {
            return CommandOutcome::Notice {
                message: "The address isn't on its own line any more.".to_owned(),
            };
        };
        let caret = range.start + markdown.len();
        let mut new_text = parsed.text.clone();
        new_text.replace_range(range.clone(), &markdown);
        let selection = Utf16Offsets::new(&new_text).range(&(caret..caret));
        CommandOutcome::Edit {
            replacements: vec![TextReplacement {
                range: parsed.offsets.range(&range),
                text: markdown,
            }],
            selection,
        }
    }

    /// Each sentence's length, while sentence-length highlighting is on.
    pub fn sentence_tints(&self) -> Vec<SentenceTint> {
        let Some(thresholds) = self.display.lock().sentence_lengths else {
            return Vec::new();
        };
        let parsed = self.lock();
        units(&parsed.tree, 0..parsed.text.len(), Purpose::Rhythm)
            .iter()
            .flat_map(|unit| sentence_lengths(&parsed.text, unit, thresholds))
            .map(|(range, length)| SentenceTint {
                range: parsed.offsets.range(&range),
                length: sentence_length(length),
            })
            .collect()
    }
}

impl NoteDocument {
    pub(crate) fn with_display(text: String, display: SharedDisplay) -> Arc<Self> {
        Arc::new(Self {
            parsed: Mutex::new(ParsedText::new(text)),
            display,
        })
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, ParsedText> {
        self.parsed.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn show_table_source(parsed: &mut ParsedText, selection: TextRange) -> CommandOutcome {
    let head = parsed.offsets.byte(selection.end);
    match parsed.table_at(head) {
        Some(table) => {
            parsed.source_table = Some(table);
            CommandOutcome::Redraw
        }
        None => CommandOutcome::Notice {
            message: "Put the cursor in a table first.".to_owned(),
        },
    }
}

fn sentence_length(length: Length) -> SentenceLength {
    match length {
        Length::Short => SentenceLength::Short,
        Length::Medium => SentenceLength::Medium,
        Length::Long => SentenceLength::Long,
    }
}

/// The text in `range` as it reads, without the markup of any node in it,
/// so `## Two *parts*` is "Two parts".
fn plain_text(text: &str, tree: &SyntaxTree, range: Range<usize>) -> String {
    let mut markup: Vec<Range<usize>> = tree
        .nodes_overlapping(range.clone())
        .into_iter()
        .flat_map(|id| tree.node(id).markup.iter().map(|mark| mark.range.clone()))
        .filter(|mark| range.start <= mark.start && mark.end <= range.end)
        .collect();
    markup.sort_by_key(|mark| mark.start);
    let mut plain = String::new();
    let mut at = range.start;
    for mark in markup {
        if mark.start > at {
            plain.push_str(&text[at..mark.start]);
        }
        at = at.max(mark.end);
    }
    plain.push_str(&text[at.min(range.end)..range.end]);
    plain.trim().to_owned()
}

fn link_target(kind: &NodeKind) -> Option<String> {
    match kind {
        NodeKind::Link(info) => Some(info.destination.clone()),
        NodeKind::WikiLink(info) => Some(wiki_target(info)),
        _ => None,
    }
}

fn wiki_target(info: &WikiInfo) -> String {
    match &info.subpath {
        Some(subpath) => format!("{}#{subpath}", info.target),
        None => info.target.clone(),
    }
}

/// The one edit that turns `old` into `new`: everything between their
/// common start and common end. `None` when they're the same.
fn changed_span(old: &str, new: &str) -> Option<Edit> {
    if old == new {
        return None;
    }
    let prefix = floor_boundary(old, common_prefix(old.as_bytes(), new.as_bytes()));
    let suffix_limit = old.len().min(new.len()) - prefix;
    let suffix = common_suffix(old.as_bytes(), new.as_bytes()).min(suffix_limit);
    let old_end = ceil_boundary(old, old.len() - suffix);
    let suffix = old.len() - old_end;
    Some(Edit {
        old: Range {
            start: prefix,
            end: old_end,
        },
        new_len: new.len() - suffix - prefix,
    })
}

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

fn common_suffix(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

fn floor_boundary(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn ceil_boundary(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(old: &str, edit: &Edit, new: &str) -> String {
        let inserted = &new[edit.old.start..edit.old.start + edit.new_len];
        format!(
            "{}{inserted}{}",
            &old[..edit.old.start],
            &old[edit.old.end..]
        )
    }

    #[test]
    fn the_changed_span_rebuilds_the_new_text() {
        let cases = [
            ("abc", "abXc"),
            ("aaa", "aaaa"),
            ("hello world", "hello"),
            ("é", "è"),
            ("x𝜋y", "x𝜎y"),
            ("", "new"),
            ("gone", ""),
        ];
        for (old, new) in cases {
            let edit = changed_span(old, new).unwrap();
            assert_eq!(apply(old, &edit, new), new, "{old:?} → {new:?}");
        }
    }

    #[test]
    fn the_same_text_is_no_edit() {
        assert_eq!(changed_span("same", "same"), None);
    }

    #[test]
    fn updating_parses_as_a_fresh_document_would() {
        let document = NoteDocument::new("# Title\n\nSome *text*\n".into());
        document.update("# Title\n\nSome *text* and **more**\n\n- item\n".into());
        let fresh = NoteDocument::new(document.text());
        let at_end = TextRange { start: 0, end: 0 };
        assert_eq!(document.plan(at_end), fresh.plan(at_end));
    }

    #[test]
    fn the_outline_lists_headings_with_their_titles() {
        let document = NoteDocument::new("# One\n\ntext\n\n## Two *parts*\n".into());
        let outline: Vec<(u8, String)> = document
            .outline()
            .into_iter()
            .map(|heading| (heading.level, heading.title))
            .collect();
        assert_eq!(outline, [(1, "One".into()), (2, "Two parts".into())]);
    }

    #[test]
    fn links_under_the_cursor_name_their_target() {
        let document = NoteDocument::new("see [[Waves#Speed|waves]] and [x](https://a.org)".into());
        assert_eq!(document.link_at(8), Some("Waves#Speed".into()));
        assert_eq!(document.link_at(33), Some("https://a.org".into()));
        assert_eq!(document.link_at(1), None);
    }

    #[test]
    fn a_table_shows_its_source_until_the_cursor_leaves() {
        let text = "| a | b |\n| - | - |\n| 1 | 2 |\n\nafter";
        let document = NoteDocument::new(text.into());
        let in_table = TextRange { start: 2, end: 2 };
        let outcome = document.run_command("table.edit-as-markdown".into(), in_table);
        assert_eq!(outcome, CommandOutcome::Redraw);
        assert!(document.plan(in_table).lines[0].table_row.is_none());
        let after = TextRange { start: 32, end: 32 };
        assert!(document.plan(after).lines[0].table_row.is_some());
    }
}
