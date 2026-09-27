//! Formatting toggles. With a selection they wrap or unwrap it. With a caret
//! inside a word they act on the word. On an empty spot they insert the pair
//! with the caret between.

use std::ops::Range;

use crate::document::{Document, Selection, SelectionRange};
use crate::pipeline::RangePlan;
use crate::transaction::{TextEdit, Transaction};

use super::command_transaction;

/// A formatting command and the markers it toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Underline,
    Code,
    Strikethrough,
    Highlight,
    InlineMath,
    Comment,
}

/// (format, command id, opening marker, closing marker)
const FORMATS: [(Format, &str, &str, &str); 8] = [
    (Format::Bold, "format.bold", "**", "**"),
    (Format::Italic, "format.italic", "*", "*"),
    (Format::Underline, "format.underline", "<u>", "</u>"),
    (Format::Code, "format.code", "`", "`"),
    (Format::Strikethrough, "format.strikethrough", "~~", "~~"),
    (Format::Highlight, "format.highlight", "==", "=="),
    (Format::InlineMath, "format.math-inline", "$", "$"),
    (Format::Comment, "format.comment", "%%", "%%"),
];

impl Format {
    pub const ALL: [Format; 8] = [
        Format::Bold,
        Format::Italic,
        Format::Underline,
        Format::Code,
        Format::Strikethrough,
        Format::Highlight,
        Format::InlineMath,
        Format::Comment,
    ];

    fn entry(self) -> (&'static str, &'static str, &'static str) {
        let (_, id, open, close) = FORMATS[self as usize];
        (id, open, close)
    }

    /// The command id, such as `format.bold`.
    pub fn command_id(self) -> &'static str {
        self.entry().0
    }

    pub fn from_command_id(id: &str) -> Option<Format> {
        FORMATS
            .iter()
            .find(|(_, command, ..)| *command == id)
            .map(|(format, ..)| *format)
    }
}

struct Markers {
    open: &'static str,
    close: &'static str,
}

/// Toggles `format` at every selected range.
pub fn toggle_format(
    doc: &Document,
    selection: &Selection,
    format: Format,
    timestamp_ms: u64,
) -> Transaction {
    let (id, open, close) = format.entry();
    let markers = Markers { open, close };
    let plans = selection
        .ranges()
        .iter()
        .map(|range| plan_toggle(doc, *range, &markers))
        .collect();
    command_transaction(doc, selection, plans, id, timestamp_ms)
}

fn plan_toggle(doc: &Document, range: SelectionRange, markers: &Markers) -> RangePlan {
    let target = if range.is_empty() {
        word_at(doc, range.from())
    } else {
        range.range()
    };
    if let Some(plan) = unwrap_inner(doc, &target, markers) {
        return plan;
    }
    if let Some(plan) = unwrap_outer(doc, &target, markers) {
        return plan;
    }
    wrap(doc, target, markers)
}

/// The markers are part of the target, as in a selected `**word**`.
fn unwrap_inner(doc: &Document, target: &Range<usize>, markers: &Markers) -> Option<RangePlan> {
    let text = doc.slice(target.clone());
    let inner = text
        .strip_prefix(markers.open)?
        .strip_suffix(markers.close)?;
    if !is_exact_marker(
        &text,
        markers.open.len(),
        text.len() - markers.close.len(),
        markers,
    ) {
        return None;
    }
    Some(select_all(TextEdit::new(target.clone(), inner)))
}

/// The markers sit just outside the target, as in `**|word|**`.
fn unwrap_outer(doc: &Document, target: &Range<usize>, markers: &Markers) -> Option<RangePlan> {
    let start = target.start.checked_sub(markers.open.len())?;
    let end = target.end + markers.close.len();
    if end > doc.len() || !doc.is_char_boundary(start) || !doc.is_char_boundary(end) {
        return None;
    }
    let text = doc.slice(start..end);
    let wrapped = text.starts_with(markers.open) && text.ends_with(markers.close);
    let exact = is_exact_marker(
        &doc.slice(0..end),
        start + markers.open.len(),
        target.end,
        markers,
    );
    if !(wrapped && exact) {
        return None;
    }
    let inner = doc.slice(target.clone());
    Some(select_all(TextEdit::new(start..end, inner)))
}

/// A single-character marker must not be half of a doubled one, so italic
/// `*` does not unwrap bold `**`.
fn is_exact_marker(text: &str, inner_start: usize, inner_end: usize, markers: &Markers) -> bool {
    if markers.open.len() != 1 || markers.open != markers.close {
        return true;
    }
    let marker = markers.open;
    let open_start = inner_start - 1;
    let doubled_open = text[..open_start].ends_with(marker) && !text[..open_start].ends_with("**");
    let doubled_close = text[inner_end + 1..].starts_with(marker);
    !(doubled_open || doubled_close)
}

fn wrap(doc: &Document, target: Range<usize>, markers: &Markers) -> RangePlan {
    let inner = doc.slice(target.clone());
    let text = format!("{}{inner}{}", markers.open, markers.close);
    let start = markers.open.len();
    let end = start + inner.len();
    let (anchor, head) = if inner.is_empty() {
        (start, start)
    } else {
        (start, end)
    };
    RangePlan {
        edit: TextEdit::new(target, text),
        anchor,
        head,
    }
}

fn select_all(edit: TextEdit) -> RangePlan {
    let len = edit.insert.len();
    RangePlan {
        edit,
        anchor: 0,
        head: len,
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '\''
}

/// The word around `offset`, or an empty range at it.
fn word_at(doc: &Document, offset: usize) -> Range<usize> {
    let mut start = offset;
    while let Some(ch) = doc.char_before(start).filter(|ch| is_word_char(*ch)) {
        start -= ch.len_utf8();
    }
    let mut end = offset;
    while let Some(ch) = doc.char_after(end).filter(|ch| is_word_char(*ch)) {
        end += ch.len_utf8();
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::EditorState;
    use crate::transaction::Origin;

    /// Applies `format` to `text`, where `|` marks a caret and `[`…`]` a
    /// selection, and returns the result in the same notation.
    fn toggle(text: &str, format: Format) -> String {
        let (plain, selection) = parse_marked(text);
        let doc = Document::from(plain.as_str());
        let transaction = toggle_format(&doc, &selection, format, 0);
        assert_eq!(transaction.origin(), &Origin::command(format.command_id()));
        let mut state = EditorState::new(doc);
        state.apply(transaction).unwrap();
        show(&state)
    }

    fn parse_marked(text: &str) -> (String, Selection) {
        if let Some(caret) = text.find('|') {
            return (text.replacen('|', "", 1), Selection::cursor(caret));
        }
        let start = text.find('[').unwrap();
        let end = text.find(']').unwrap() - 1;
        let plain = text.replacen('[', "", 1).replacen(']', "", 1);
        (plain, Selection::single(SelectionRange::new(start, end)))
    }

    fn show(state: &EditorState) -> String {
        let mut text = state.doc().slice(0..state.doc().len());
        let range = state.selection().primary();
        if range.is_empty() {
            text.insert(range.from(), '|');
        } else {
            text.insert(range.to(), ']');
            text.insert(range.from(), '[');
        }
        text
    }

    #[test]
    fn wraps_and_unwraps_a_selection() {
        assert_eq!(toggle("a [big] dog", Format::Bold), "a **[big]** dog");
        assert_eq!(toggle("a **[big]** dog", Format::Bold), "a [big] dog");
        assert_eq!(toggle("a [**big**] dog", Format::Bold), "a [big] dog");
    }

    #[test]
    fn acts_on_the_word_under_the_caret() {
        assert_eq!(toggle("a b|ig dog", Format::Italic), "a *[big]* dog");
        assert_eq!(toggle("a *b|ig* dog", Format::Italic), "a [big] dog");
    }

    #[test]
    fn inserts_an_empty_pair_on_an_empty_spot() {
        assert_eq!(toggle("a | dog", Format::Highlight), "a ==|== dog");
        assert_eq!(toggle("|", Format::InlineMath), "$|$");
    }

    #[test]
    fn italic_does_not_unwrap_bold() {
        assert_eq!(toggle("**[big]**", Format::Italic), "***[big]***");
        assert_eq!(toggle("***[big]***", Format::Italic), "**[big]**");
    }

    #[test]
    fn underline_uses_html_tags() {
        assert_eq!(toggle("[word]", Format::Underline), "<u>[word]</u>");
        assert_eq!(toggle("<u>[word]</u>", Format::Underline), "[word]");
    }

    #[test]
    fn every_format_round_trips() {
        for format in Format::ALL {
            let wrapped = toggle("x [word] y", format);
            assert_eq!(toggle(&wrapped, format), "x [word] y", "{format:?}");
            assert_eq!(Format::from_command_id(format.command_id()), Some(format));
        }
    }

    #[test]
    fn works_with_several_carets() {
        let doc = Document::from("one two");
        let selection = Selection::new(
            vec![SelectionRange::cursor(1), SelectionRange::cursor(5)],
            0,
        );
        let transaction = toggle_format(&doc, &selection, Format::Code, 0);
        let mut state = EditorState::new(doc);
        state.apply(transaction).unwrap();
        assert_eq!(state.doc().slice(0..state.doc().len()), "`one` `two`");
    }
}
