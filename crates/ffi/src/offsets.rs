//! The core counts text in UTF-8 bytes and UIKit counts it in UTF-16 code
//! units, so every offset crossing the bridge is converted here.

use std::ops::Range;

/// A range of text in UTF-16 code units, as `NSRange` counts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

/// Converts between byte offsets and UTF-16 offsets in one text.
pub(crate) struct Utf16Offsets {
    /// The UTF-16 offset of every byte offset, the text's end included. A
    /// byte inside a character maps to where that character starts.
    utf16_at_byte: Vec<u32>,
}

impl Utf16Offsets {
    pub(crate) fn new(text: &str) -> Self {
        let mut utf16_at_byte = Vec::with_capacity(text.len() + 1);
        let mut utf16 = 0;
        for character in text.chars() {
            utf16_at_byte.extend(std::iter::repeat_n(utf16, character.len_utf8()));
            utf16 += character.len_utf16() as u32;
        }
        utf16_at_byte.push(utf16);
        Self { utf16_at_byte }
    }

    fn text_len(&self) -> usize {
        self.utf16_at_byte.len() - 1
    }

    pub(crate) fn utf16(&self, byte: usize) -> u32 {
        self.utf16_at_byte[byte.min(self.text_len())]
    }

    /// The byte offset of the character at or after `utf16`.
    pub(crate) fn byte(&self, utf16: u32) -> usize {
        self.utf16_at_byte
            .partition_point(|&offset| offset < utf16)
            .min(self.text_len())
    }

    pub(crate) fn range(&self, bytes: &Range<usize>) -> TextRange {
        TextRange {
            start: self.utf16(bytes.start),
            end: self.utf16(bytes.end),
        }
    }

    pub(crate) fn ranges(&self, bytes: &[Range<usize>]) -> Vec<TextRange> {
        bytes.iter().map(|range| self.range(range)).collect()
    }

    pub(crate) fn byte_range(&self, range: TextRange) -> Range<usize> {
        self.byte(range.start)..self.byte(range.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_offsets_are_the_same_in_both_counts() {
        let offsets = Utf16Offsets::new("plain");
        assert_eq!(offsets.utf16(3), 3);
        assert_eq!(offsets.byte(3), 3);
    }

    #[test]
    fn wide_characters_convert_both_ways() {
        let text = "é𝜋x";
        let offsets = Utf16Offsets::new(text);
        let x_byte = text.find('x').unwrap();
        assert_eq!(offsets.utf16(x_byte), 3);
        assert_eq!(offsets.byte(3), x_byte);
        assert_eq!(offsets.byte(1), 2);
        assert_eq!(offsets.utf16(text.len()), 4);
    }

    #[test]
    fn offsets_past_the_end_clamp_to_it() {
        let offsets = Utf16Offsets::new("ab");
        assert_eq!(offsets.byte(40), 2);
        assert_eq!(offsets.utf16(40), 2);
    }

    #[test]
    fn an_offset_inside_a_surrogate_pair_moves_to_the_next_character() {
        let text = "𝜋x";
        let offsets = Utf16Offsets::new(text);
        assert_eq!(offsets.byte(1), text.find('x').unwrap());
    }
}
