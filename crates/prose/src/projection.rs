//! A paragraph as a reader sees it: the note's text with markup taken
//! out, line breaks turned into spaces, and code, math and similar spans
//! standing in as single words. Segmentation and checking run on this
//! text, and every range found in it maps back to the note.

use std::ops::Range;

/// What a piece of the note becomes in the projected text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PieceKind {
    /// Copied as it is.
    Text,
    /// One opaque word, such as inline code or math. Nothing inside it
    /// is segmented or checked.
    Atom,
    /// A single space, such as a line break inside a paragraph.
    Space,
}

/// A range of the note and what it becomes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Piece {
    pub range: Range<usize>,
    pub kind: PieceKind,
}

impl Piece {
    pub fn new(range: Range<usize>, kind: PieceKind) -> Piece {
        Piece { range, kind }
    }
}

/// What an atom reads as: a short word that is spelled right and carries
/// no punctuation.
const ATOM: &str = "x";

/// One piece placed in the projected text.
#[derive(Clone, Debug)]
struct Span {
    /// Where it starts in the projected text.
    at: usize,
    len: usize,
    source: Range<usize>,
    kind: PieceKind,
}

impl Span {
    fn end(&self) -> usize {
        self.at + self.len
    }
}

/// The projected text of one unit and the way back to the note.
#[derive(Clone, Debug)]
pub struct Projection {
    text: String,
    spans: Vec<Span>,
}

impl Projection {
    /// Projects `pieces` of `source`, which must be in order and not
    /// overlap. Whatever lies between pieces, such as markup, is left out.
    pub fn new(source: &str, pieces: &[Piece]) -> Projection {
        let mut text = String::new();
        let mut spans = Vec::with_capacity(pieces.len());
        for piece in pieces {
            let at = text.len();
            match piece.kind {
                PieceKind::Text => text.push_str(&source[piece.range.clone()]),
                PieceKind::Atom => text.push_str(ATOM),
                PieceKind::Space => text.push(' '),
            }
            spans.push(Span {
                at,
                len: text.len() - at,
                source: piece.range.clone(),
                kind: piece.kind,
            });
        }
        Projection { text, spans }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The note's range for a range of the projected text. An end inside
    /// an atom or space takes the whole of it.
    pub fn source_range(&self, range: Range<usize>) -> Range<usize> {
        let start = self.source_start(range.start);
        let end = self.source_end(range.end).max(start);
        start..end
    }

    fn source_start(&self, at: usize) -> usize {
        let index = self.spans.partition_point(|span| span.end() <= at);
        match self.spans.get(index) {
            Some(span) if span.kind == PieceKind::Text => span.source.start + (at - span.at),
            Some(span) => span.source.start,
            None => self.spans.last().map_or(0, |span| span.source.end),
        }
    }

    fn source_end(&self, at: usize) -> usize {
        let index = self.spans.partition_point(|span| span.end() < at);
        match self.spans.get(index) {
            Some(span) if span.kind == PieceKind::Text => span.source.start + (at - span.at),
            Some(span) if at > span.at => span.source.end,
            Some(span) => span.source.start,
            None => self.spans.last().map_or(0, |span| span.source.end),
        }
    }

    /// Whether a range of the projected text is exactly what the note
    /// says there: plain text, with no markup, atom or line break inside.
    pub fn is_verbatim(&self, range: Range<usize>) -> bool {
        let first = self.spans.partition_point(|span| span.end() <= range.start);
        let mut expected: Option<usize> = None;
        for span in self.spans[first..]
            .iter()
            .take_while(|span| span.at < range.end.max(range.start + 1))
        {
            if span.kind != PieceKind::Text {
                return false;
            }
            if expected.is_some_and(|end| end != span.source.start) {
                return false;
            }
            expected = Some(span.source.end);
        }
        true
    }

    /// Whether an atom lies inside `range` or right beside it, across
    /// spaces at most.
    pub fn near_atom(&self, range: Range<usize>) -> bool {
        let before = self.text[..range.start].trim_end().len();
        let after =
            range.end + (self.text[range.end..].len() - self.text[range.end..].trim_start().len());
        self.spans
            .iter()
            .any(|span| span.kind == PieceKind::Atom && span.at <= after && span.end() >= before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(source: &str, pieces: &[(Range<usize>, PieceKind)]) -> Projection {
        let pieces: Vec<Piece> = pieces
            .iter()
            .map(|(range, kind)| Piece::new(range.clone(), *kind))
            .collect();
        Projection::new(source, &pieces)
    }

    #[test]
    fn markup_is_left_out_and_atoms_read_as_words() {
        let source = "Some **bold** and `code`.\n> more";
        let projection = project(
            source,
            &[
                (0..5, PieceKind::Text),
                (7..11, PieceKind::Text),
                (13..18, PieceKind::Text),
                (18..24, PieceKind::Atom),
                (24..25, PieceKind::Text),
                (25..26, PieceKind::Space),
                (28..32, PieceKind::Text),
            ],
        );
        assert_eq!(projection.text(), "Some bold and x. more");
        // "bold" maps back inside the markup.
        assert_eq!(projection.source_range(5..9), 7..11);
        // A range ending on the atom takes all of it.
        assert_eq!(projection.source_range(10..15), 14..24);
        assert_eq!(projection.source_range(14..21), 18..32);
        assert!(projection.is_verbatim(0..4));
        assert!(!projection.is_verbatim(0..9));
        assert!(!projection.is_verbatim(14..15));
        assert!(projection.near_atom(10..13));
        assert!(!projection.near_atom(0..4));
    }
}
