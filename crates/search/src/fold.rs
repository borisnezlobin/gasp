//! Folding text for matching: lowercased and stripped of diacritics, with
//! a way back from offsets in the folded text to the original's.

use std::ops::Range;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Text lowercased and stripped of diacritics, with a map back to the
/// original byte offsets.
#[derive(Clone, Debug, Default)]
pub struct Folded {
    pub text: String,
    /// Where the folded text and the original part ways.
    offsets: OffsetMap,
    /// Whether every character folded to the same number of bytes.
    pub same_offsets: bool,
}

impl Folded {
    /// Every non-overlapping match of the folded `needle`, as ranges in the
    /// original text.
    pub fn find_all(&self, needle: &str) -> Vec<Range<usize>> {
        if needle.is_empty() {
            return Vec::new();
        }
        self.text
            .match_indices(needle)
            .map(|(start, found)| self.offsets.original(start..start + found.len()))
            .collect()
    }

    /// The folded text, and the map from its offsets back to the
    /// original's.
    pub(crate) fn into_parts(self) -> (String, OffsetMap) {
        (self.text, self.offsets)
    }

    fn push_folded(&mut self, start: usize, ch: char) {
        let before = self.text.len();
        let mut chars = 0;
        let lowered = ch
            .to_lowercase()
            .nfd()
            .filter(|mark| !is_combining_mark(*mark));
        for out in lowered {
            self.text.push(out);
            chars += 1;
        }
        let length = self.text.len() - before;
        self.same_offsets &= length == ch.len_utf8();
        if chars != 1 || length != ch.len_utf8() {
            self.offsets.0.push(Detour {
                folded: before..self.text.len(),
                original: start..start + ch.len_utf8(),
            });
        }
    }
}

/// Lowercases and strips combining marks, so "Émile" matches "emile".
pub fn fold(text: &str) -> Folded {
    let mut folded = Folded {
        text: String::with_capacity(text.len()),
        offsets: OffsetMap::default(),
        same_offsets: true,
    };
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let ascii = rest.bytes().take_while(u8::is_ascii).count();
        if ascii > 0 {
            let start = folded.text.len();
            folded.text.push_str(&rest[..ascii]);
            folded.text[start..].make_ascii_lowercase();
            at += ascii;
            continue;
        }
        let ch = rest.chars().next().unwrap_or_default();
        folded.push_folded(at, ch);
        at += ch.len_utf8();
    }
    folded
}

/// A character that didn't fold to one character of its own length, with
/// the bytes it takes in the folded text and in the original.
#[derive(Clone, Debug)]
struct Detour {
    folded: Range<usize>,
    original: Range<usize>,
}

/// Every [`Detour`], in order. Between two of them, each character folded
/// to one character of the same length, so offsets there differ from the
/// original's by what the detours before them added or took away.
#[derive(Clone, Debug, Default)]
pub(crate) struct OffsetMap(Vec<Detour>);

impl OffsetMap {
    /// The original range of whole characters a folded match covers.
    pub(crate) fn original(&self, folded: Range<usize>) -> Range<usize> {
        let last = folded.end - 1;
        self.original_char(folded.start).start..self.original_char(last).end
    }

    /// The original character that folded byte `at` came from, given that
    /// `at` is the first or last byte of a folded character.
    fn original_char(&self, at: usize) -> Range<usize> {
        let after = self.0.partition_point(|detour| detour.folded.start <= at);
        let Some(detour) = after.checked_sub(1).map(|index| &self.0[index]) else {
            return at..at + 1;
        };
        if detour.folded.contains(&at) {
            return detour.original.clone();
        }
        let original = at - detour.folded.end + detour.original.end;
        original..original + 1
    }
}
