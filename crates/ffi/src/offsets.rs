//! The core counts text in UTF-8 bytes and UIKit counts it in UTF-16 code
//! units, so every offset crossing the bridge is converted here.

use std::ops::Range;

use gasp_core::syntax::Edit;

/// A range of text in UTF-16 code units, as `NSRange` counts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

/// A character that isn't ASCII, where the two counts drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WideChar {
    byte: usize,
    /// The UTF-16 offset where it starts.
    utf16: u32,
    bytes: u8,
    units: u8,
}

impl WideChar {
    fn byte_end(&self) -> usize {
        self.byte + usize::from(self.bytes)
    }

    fn utf16_end(&self) -> u32 {
        self.utf16 + u32::from(self.units)
    }
}

/// The text is cut into blocks of this many bytes, each knowing its first
/// wide character, so a lookup only searches its own block.
const BLOCK: usize = 256;

/// Converts between byte offsets and UTF-16 offsets in one text. Between
/// two characters that aren't ASCII the counts move together, so only
/// those characters are kept, and a lookup finds the nearest one before.
pub(crate) struct Utf16Offsets {
    wide: Vec<WideChar>,
    /// For each block, how many wide characters start before it, with
    /// one more entry for the end.
    wide_before_block: Vec<u32>,
    text_len: usize,
}

impl Utf16Offsets {
    pub(crate) fn new(text: &str) -> Self {
        let mut wide = Vec::new();
        push_wide_chars(text, 0, 0, &mut wide);
        let mut offsets = Self {
            wide,
            wide_before_block: Vec::new(),
            text_len: text.len(),
        };
        offsets.index_blocks();
        offsets
    }

    fn index_blocks(&mut self) {
        let blocks = self.text_len / BLOCK + 2;
        self.wide_before_block.clear();
        self.wide_before_block.reserve(blocks);
        let mut counted = 0;
        for block in 0..blocks {
            let start = block * BLOCK;
            while counted < self.wide.len() && self.wide[counted].byte < start {
                counted += 1;
            }
            self.wide_before_block.push(counted as u32);
        }
    }

    /// The offsets of `new_text`, which is this one's text with `edit`
    /// applied: characters before the edit stay, those after it move,
    /// and only the inserted text is scanned.
    pub(crate) fn edited(&mut self, new_text: &str, edit: &Edit) {
        let kept = self.wide.partition_point(|wide| wide.byte < edit.old.start);
        let after = self.wide.partition_point(|wide| wide.byte < edit.old.end);
        let new_end = edit.old.start + edit.new_len;
        let start_utf16 = self.utf16(edit.old.start);
        let old_end_utf16 = self.utf16(edit.old.end);
        let mut inserted = Vec::new();
        let end_utf16 = push_wide_chars(
            &new_text[edit.old.start..new_end],
            edit.old.start,
            start_utf16,
            &mut inserted,
        );
        let byte_shift = edit.new_len as isize - edit.old.len() as isize;
        let utf16_shift = i64::from(end_utf16) - i64::from(old_end_utf16);
        for wide in &mut self.wide[after..] {
            wide.byte = (wide.byte as isize + byte_shift) as usize;
            wide.utf16 = (i64::from(wide.utf16) + utf16_shift) as u32;
        }
        self.wide.splice(kept..after, inserted);
        self.text_len = new_text.len();
        self.index_blocks();
    }

    /// How many wide characters start at or before `byte`, which is at
    /// most the text's length.
    fn wide_before_byte(&self, byte: usize) -> usize {
        let block = byte / BLOCK;
        let first = self.wide_before_block[block] as usize;
        let end = self.wide_before_block[block + 1] as usize;
        first + self.wide[first..end].partition_point(|wide| wide.byte <= byte)
    }

    pub(crate) fn utf16(&self, byte: usize) -> u32 {
        let byte = byte.min(self.text_len);
        let found = self.wide_before_byte(byte);
        let Some(wide) = found.checked_sub(1).map(|index| self.wide[index]) else {
            return byte as u32;
        };
        if byte < wide.byte_end() {
            return wide.utf16;
        }
        wide.utf16_end() + (byte - wide.byte_end()) as u32
    }

    /// The byte offset of the character at or after `utf16`.
    pub(crate) fn byte(&self, utf16: u32) -> usize {
        let ended = self.wide.partition_point(|wide| wide.utf16_end() <= utf16);
        if let Some(wide) = self.wide.get(ended)
            && wide.utf16 < utf16
        {
            return wide.byte_end();
        }
        let (byte, units) = match ended.checked_sub(1).map(|index| self.wide[index]) {
            Some(wide) => (wide.byte_end(), wide.utf16_end()),
            None => (0, 0),
        };
        (byte + (utf16 - units) as usize).min(self.text_len)
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

/// Records the characters of `text` that aren't ASCII, where `text`
/// starts at `byte` and `utf16` in the whole text. Answers the UTF-16
/// offset of `text`'s end.
fn push_wide_chars(text: &str, byte: usize, utf16: u32, wide: &mut Vec<WideChar>) -> u32 {
    let mut units_before = utf16;
    let mut ascii_from = 0;
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        at += bytes[at..]
            .iter()
            .take_while(|byte| byte.is_ascii())
            .count();
        let Some(character) = text[at..].chars().next() else {
            break;
        };
        units_before += (at - ascii_from) as u32;
        let (bytes_long, units) = (character.len_utf8(), character.len_utf16());
        wide.push(WideChar {
            byte: byte + at,
            utf16: units_before,
            bytes: bytes_long as u8,
            units: units as u8,
        });
        units_before += units as u32;
        at += bytes_long;
        ascii_from = at;
    }
    units_before + (bytes.len() - ascii_from) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plain table of every byte's UTF-16 offset that the compact
    /// form must agree with.
    fn utf16_at_every_byte(text: &str) -> Vec<u32> {
        let mut table = Vec::with_capacity(text.len() + 1);
        let mut utf16 = 0;
        for character in text.chars() {
            table.extend(std::iter::repeat_n(utf16, character.len_utf8()));
            utf16 += character.len_utf16() as u32;
        }
        table.push(utf16);
        table
    }

    fn assert_matches_table(offsets: &Utf16Offsets, text: &str) {
        let table = utf16_at_every_byte(text);
        for byte in 0..=text.len() + 2 {
            let expected = table[byte.min(text.len())];
            assert_eq!(offsets.utf16(byte), expected, "utf16({byte}) in {text:?}");
        }
        for byte in (0..=text.len()).rev() {
            assert_eq!(offsets.utf16(byte), table[byte], "backwards, {text:?}");
        }
        let last = *table.last().unwrap();
        for utf16 in 0..=last + 2 {
            let expected = table
                .partition_point(|&offset| offset < utf16)
                .min(text.len());
            assert_eq!(offsets.byte(utf16), expected, "byte({utf16}) in {text:?}");
        }
    }

    const TEXTS: [&str; 8] = [
        "",
        "plain",
        "é𝜋x",
        "𝜋",
        "a “quoted” — line\nwith é and 𝜋𝜎 and 😀!",
        "ééé",
        "xx𝜋",
        "𝜋xx",
    ];

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

    #[test]
    fn every_offset_agrees_with_a_full_table() {
        for text in TEXTS {
            assert_matches_table(&Utf16Offsets::new(text), text);
        }
    }

    #[test]
    fn long_texts_agree_across_blocks_and_edits() {
        let piece = "plain words and “curly quotes” then 𝜋, é and 😀 again\n";
        let text = piece.repeat(40) + &"x".repeat(700) + &"é".repeat(300);
        assert_matches_table(&Utf16Offsets::new(&text), &text);
        let edits = [
            (5, 5, "𝜋"),
            (300, 900, ""),
            (1000, 1004, "ab\né"),
            (0, 0, "😀"),
        ];
        let (mut current, mut offsets) = (text.clone(), Utf16Offsets::new(&text));
        for (start, end, insert) in edits {
            let start = (start..).find(|&at| current.is_char_boundary(at)).unwrap();
            let end = (end.max(start)..)
                .find(|&at| current.is_char_boundary(at))
                .unwrap();
            current.replace_range(start..end, insert);
            let edit = Edit {
                old: start..end,
                new_len: insert.len(),
            };
            offsets.edited(&current, &edit);
            assert_matches_table(&offsets, &current);
        }
    }

    #[test]
    fn an_edited_index_matches_a_fresh_one() {
        let inserts = ["", "x", "é", "𝜋 and “q”", "\n😀\n", "abc"];
        for text in TEXTS {
            let boundaries: Vec<usize> = (0..=text.len())
                .filter(|&at| text.is_char_boundary(at))
                .collect();
            for &start in &boundaries {
                for &end in boundaries.iter().filter(|&&end| end >= start) {
                    for insert in inserts {
                        let mut new_text = text.to_owned();
                        new_text.replace_range(start..end, insert);
                        let mut offsets = Utf16Offsets::new(text);
                        let edit = Edit {
                            old: start..end,
                            new_len: insert.len(),
                        };
                        offsets.edited(&new_text, &edit);
                        assert_eq!(offsets.wide, Utf16Offsets::new(&new_text).wide);
                        assert_matches_table(&offsets, &new_text);
                    }
                }
            }
        }
    }
}
