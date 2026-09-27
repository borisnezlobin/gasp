//! Where word, line and note motions land, and the ranges that double and
//! triple clicks select.
//!
//! Word motions follow macOS text fields: moving left skips any spaces and
//! punctuation, then the word before them; moving right skips spaces and
//! punctuation, then the word after them. Windows and Linux use the same
//! stops, so a word motion behaves the same on every platform.

use std::ops::Range;

use crate::document::Document;

/// What a character counts as when finding word boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punctuation,
}

fn class_of(ch: char) -> CharClass {
    if ch.is_whitespace() {
        CharClass::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

fn is_word(ch: char) -> bool {
    class_of(ch) == CharClass::Word
}

/// Moves back over characters while `keep` holds.
fn skip_back(doc: &Document, mut offset: usize, keep: impl Fn(char) -> bool) -> usize {
    while let Some(ch) = doc.char_before(offset).filter(|ch| keep(*ch)) {
        offset -= ch.len_utf8();
    }
    offset
}

/// Moves forward over characters while `keep` holds.
fn skip_forward(doc: &Document, mut offset: usize, keep: impl Fn(char) -> bool) -> usize {
    while let Some(ch) = doc.char_after(offset).filter(|ch| keep(*ch)) {
        offset += ch.len_utf8();
    }
    offset
}

/// The start of the word before `offset`.
pub fn word_left(doc: &Document, offset: usize) -> usize {
    let past_gap = skip_back(doc, offset, |ch| !is_word(ch));
    skip_back(doc, past_gap, is_word)
}

/// The end of the word after `offset`.
pub fn word_right(doc: &Document, offset: usize) -> usize {
    let past_gap = skip_forward(doc, offset, |ch| !is_word(ch));
    skip_forward(doc, past_gap, is_word)
}

/// The start of `offset`'s line.
pub fn line_start(doc: &Document, offset: usize) -> usize {
    doc.line_start(doc.line_of_offset(offset))
}

/// The end of `offset`'s line, before its line break.
pub fn line_end(doc: &Document, offset: usize) -> usize {
    doc.line_end(doc.line_of_offset(offset))
}

/// The run a double click at `offset` selects: the word, space or
/// punctuation under the pointer. At a boundary the character after the
/// pointer decides, unless it is a line break.
pub fn word_at(doc: &Document, offset: usize) -> Range<usize> {
    let after = doc
        .char_after(offset)
        .filter(|ch| *ch != '\n' && *ch != '\r');
    let Some(ch) = after.or_else(|| doc.char_before(offset)) else {
        return offset..offset;
    };
    let class = class_of(ch);
    let same = |other: char| class_of(other) == class && other != '\n' && other != '\r';
    skip_back(doc, offset, same)..skip_forward(doc, offset, same)
}

/// The line a triple click at `offset` selects, including its line break so
/// that deleting it removes the whole line.
pub fn line_at(doc: &Document, offset: usize) -> Range<usize> {
    let line = doc.line_of_offset(offset);
    let next = line + 1;
    let end = if next < doc.line_count() {
        doc.line_start(next)
    } else {
        doc.len()
    };
    doc.line_start(line)..end
}

/// The range to delete for "delete to line start": back to the start of the
/// line, or the line break before it when the caret is already there.
pub fn to_line_start(doc: &Document, offset: usize) -> Range<usize> {
    let start = line_start(doc, offset);
    if start < offset {
        return start..offset;
    }
    doc.prev_char_boundary(offset)..offset
}

/// The range to delete for "delete to line end": up to the line break, or
/// the line break itself when the caret is already at the end.
pub fn to_line_end(doc: &Document, offset: usize) -> Range<usize> {
    let end = line_end(doc, offset);
    if end > offset {
        return offset..end;
    }
    offset..doc.next_char_boundary(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        Document::from(text)
    }

    #[test]
    fn word_left_skips_spaces_and_punctuation_then_the_word() {
        let text = "one, two  three";
        let d = doc(text);
        assert_eq!(word_left(&d, text.len()), 10);
        assert_eq!(word_left(&d, 10), 5);
        assert_eq!(word_left(&d, 5), 0);
        assert_eq!(word_left(&d, 0), 0);
    }

    #[test]
    fn word_right_skips_to_the_end_of_the_next_word() {
        let text = "one, two  three";
        let d = doc(text);
        assert_eq!(word_right(&d, 0), 3);
        assert_eq!(word_right(&d, 3), 8);
        assert_eq!(word_right(&d, 8), text.len());
        assert_eq!(word_right(&d, text.len()), text.len());
    }

    #[test]
    fn words_cross_line_breaks() {
        let d = doc("end.\nnext");
        assert_eq!(word_left(&d, 5), 0);
        assert_eq!(word_right(&d, 3), 9);
    }

    #[test]
    fn words_include_non_ascii_letters() {
        let text = "naïve café";
        let d = doc(text);
        assert_eq!(word_left(&d, text.len()), "naïve ".len());
        assert_eq!(word_right(&d, 0), "naïve".len());
    }

    #[test]
    fn double_click_selects_the_run_under_the_pointer() {
        let text = "say hello, world";
        let d = doc(text);
        assert_eq!(word_at(&d, 6), 4..9);
        assert_eq!(word_at(&d, 4), 4..9);
        assert_eq!(word_at(&d, 9), 9..10);
        assert_eq!(word_at(&d, 3), 3..4);
        assert_eq!(word_at(&d, text.len()), 11..16);
    }

    #[test]
    fn double_click_at_a_line_end_takes_the_word_before() {
        let d = doc("first\nsecond");
        assert_eq!(word_at(&d, 5), 0..5);
        assert_eq!(word_at(&doc(""), 0), 0..0);
    }

    #[test]
    fn triple_click_selects_the_line_and_its_break() {
        let d = doc("one\ntwo\nthree");
        assert_eq!(line_at(&d, 5), 4..8);
        assert_eq!(line_at(&d, 10), 8..13);
    }

    #[test]
    fn line_deletes_take_the_break_when_at_an_edge() {
        let d = doc("one\ntwo");
        assert_eq!(to_line_start(&d, 6), 4..6);
        assert_eq!(to_line_start(&d, 4), 3..4);
        assert_eq!(to_line_start(&d, 0), 0..0);
        assert_eq!(to_line_end(&d, 1), 1..3);
        assert_eq!(to_line_end(&d, 3), 3..4);
        assert_eq!(to_line_end(&d, 7), 7..7);
    }
}
