//! Conversions between byte offsets and the UTF-16 offsets that platform
//! input methods use.

use std::ops::Range;

use gasp_core::document::Document;

pub fn offset_to_utf16(doc: &Document, offset: usize) -> usize {
    let rope = doc.rope();
    let offset = doc.floor_char_boundary(offset.min(doc.len()));
    rope.char_to_utf16_cu(rope.byte_to_char(offset))
}

pub fn offset_from_utf16(doc: &Document, offset_utf16: usize) -> usize {
    let rope = doc.rope();
    let offset_utf16 = offset_utf16.min(rope.len_utf16_cu());
    rope.char_to_byte(rope.utf16_cu_to_char(offset_utf16))
}

pub fn range_to_utf16(doc: &Document, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(doc, range.start)..offset_to_utf16(doc, range.end)
}

pub fn range_from_utf16(doc: &Document, range_utf16: &Range<usize>) -> Range<usize> {
    offset_from_utf16(doc, range_utf16.start)..offset_from_utf16(doc, range_utf16.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_offsets_are_unchanged() {
        let doc = Document::from("hello\nworld");
        assert_eq!(offset_to_utf16(&doc, 7), 7);
        assert_eq!(offset_from_utf16(&doc, 7), 7);
    }

    #[test]
    fn japanese_and_emoji_round_trip() {
        let doc = Document::from("あい😀x");
        // あ and い are 3 bytes and 1 UTF-16 unit; 😀 is 4 bytes and 2 units.
        assert_eq!(offset_to_utf16(&doc, 6), 2);
        assert_eq!(offset_to_utf16(&doc, 10), 4);
        assert_eq!(offset_from_utf16(&doc, 4), 10);
        assert_eq!(range_from_utf16(&doc, &(1..4)), 3..10);
        assert_eq!(range_to_utf16(&doc, &(3..10)), 1..4);
    }

    #[test]
    fn out_of_range_offsets_clamp_to_the_end() {
        let doc = Document::from("abc");
        assert_eq!(offset_from_utf16(&doc, 99), 3);
        assert_eq!(offset_to_utf16(&doc, 99), 3);
    }
}
