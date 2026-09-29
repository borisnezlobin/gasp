//! Sentence lengths across a whole note, remembered per unit. Asking again
//! after an edit only segments the paragraphs whose text changed; the rest
//! cost a hash and a comparison.

use std::collections::HashMap;
use std::hash::Hasher;
use std::ops::Range;

use gasp_core::syntax::SyntaxTree;

use crate::markdown::{Purpose, Unit, sentence_lengths, units};
use crate::projection::Piece;
use crate::segment::{Length, Thresholds};

/// Entries kept beyond the ones the last call used, before the unused go.
const SPARE_ENTRIES: usize = 1024;

/// One unit's sentences, counted from the unit's start.
struct CachedUnit {
    text: Box<str>,
    /// The unit's pieces, counted from its start.
    pieces: Vec<Piece>,
    thresholds: Thresholds,
    lengths: Vec<(Range<usize>, Length)>,
    last_used: u64,
}

impl CachedUnit {
    fn matches(&self, text: &str, unit: &Unit, thresholds: Thresholds) -> bool {
        let start = unit.range.start;
        self.thresholds == thresholds
            && *self.text == *text
            && self.pieces.len() == unit.pieces.len()
            && self.pieces.iter().zip(&unit.pieces).all(|(cached, piece)| {
                cached.kind == piece.kind
                    && cached.range == (piece.range.start - start..piece.range.end - start)
            })
    }
}

/// Remembers each unit's sentence lengths by its text, so that finding
/// every sentence in a note on each keystroke only segments what changed.
#[derive(Default)]
pub struct SentenceLengthCache {
    /// Units by a hash of their text; a bucket holds the rare units whose
    /// hashes collide or whose text reads differently in another context.
    buckets: HashMap<u64, Vec<CachedUnit>>,
    len: usize,
    calls: u64,
}

impl SentenceLengthCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Each sentence of the prose overlapping `within`, as ranges of the
    /// note with their lengths: what [`sentence_lengths`] gives for every
    /// unit [`units`] finds for [`Purpose::Rhythm`].
    pub fn sentence_lengths(
        &mut self,
        source: &str,
        tree: &SyntaxTree,
        within: Range<usize>,
        thresholds: Thresholds,
    ) -> Vec<(Range<usize>, Length)> {
        self.calls += 1;
        let mut found = Vec::new();
        let mut used = 0;
        for unit in units(tree, within, Purpose::Rhythm) {
            let start = unit.range.start;
            let lengths = self.unit_lengths(source, &unit, thresholds);
            found.extend(
                lengths
                    .iter()
                    .map(|(range, length)| (range.start + start..range.end + start, *length)),
            );
            used += 1;
        }
        if self.len > used + SPARE_ENTRIES {
            self.forget_unused();
        }
        found
    }

    /// The unit's sentences, counted from its start, from the cache or
    /// segmented now.
    fn unit_lengths(
        &mut self,
        source: &str,
        unit: &Unit,
        thresholds: Thresholds,
    ) -> &[(Range<usize>, Length)] {
        let text = &source[unit.range.clone()];
        let calls = self.calls;
        let bucket = self.buckets.entry(text_hash(text)).or_default();
        let at = match bucket
            .iter()
            .position(|cached| cached.matches(text, unit, thresholds))
        {
            Some(at) => at,
            None => {
                bucket.push(segmented(source, unit, thresholds));
                self.len += 1;
                bucket.len() - 1
            }
        };
        bucket[at].last_used = calls;
        &bucket[at].lengths
    }

    /// Drops the units the last call didn't use.
    fn forget_unused(&mut self) {
        let calls = self.calls;
        self.buckets.retain(|_, bucket| {
            bucket.retain(|cached| cached.last_used == calls);
            !bucket.is_empty()
        });
        self.len = self.buckets.values().map(Vec::len).sum();
    }
}

fn segmented(source: &str, unit: &Unit, thresholds: Thresholds) -> CachedUnit {
    let start = unit.range.start;
    let relative = |range: &Range<usize>| range.start - start..range.end - start;
    CachedUnit {
        text: source[unit.range.clone()].into(),
        pieces: unit
            .pieces
            .iter()
            .map(|piece| Piece::new(relative(&piece.range), piece.kind))
            .collect(),
        thresholds,
        lengths: sentence_lengths(source, unit, thresholds)
            .into_iter()
            .map(|(range, length)| (relative(&range), length))
            .collect(),
        last_used: 0,
    }
}

/// A quick hash of a unit's text. Hits are compared in full, so a
/// collision only costs a lookup.
fn text_hash(text: &str) -> u64 {
    let mut hasher = QuickHasher::default();
    hasher.write(text.as_bytes());
    hasher.finish()
}

/// A multiply-rotate hash that reads eight bytes at a time.
#[derive(Default)]
struct QuickHasher(u64);

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl QuickHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for QuickHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word = [0; 8];
            word.copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
        let mut last = [0; 8];
        last[..chunks.remainder().len()].copy_from_slice(chunks.remainder());
        self.add(u64::from_le_bytes(last));
        self.add(bytes.len() as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use gasp_core::syntax::parse;

    use super::*;

    fn uncached(text: &str, thresholds: Thresholds) -> Vec<(Range<usize>, Length)> {
        let tree = parse(text);
        units(&tree, 0..text.len(), Purpose::Rhythm)
            .iter()
            .flat_map(|unit| sentence_lengths(text, unit, thresholds))
            .collect()
    }

    #[test]
    fn cached_lengths_match_segmenting_afresh_through_edits() {
        let mut cache = SentenceLengthCache::new();
        let thresholds = Thresholds::default();
        let mut text = String::from(
            "# Title\n\nOne short one. Then a much longer sentence that goes on and on for \
             quite a few words indeed, past eighteen of them.\n\n- An item. Two.\n\n> Quoted.\n",
        );
        for typed in ["", " More.", "x", " And so on."] {
            text.insert_str(20, typed);
            let tree = parse(&text);
            let cached = cache.sentence_lengths(&text, &tree, 0..text.len(), thresholds);
            assert_eq!(cached, uncached(&text, thresholds));
        }
        let strict = Thresholds {
            short_below: 3,
            long_above: 5,
        };
        let tree = parse(&text);
        let cached = cache.sentence_lengths(&text, &tree, 0..text.len(), strict);
        assert_eq!(cached, uncached(&text, strict));
    }

    #[test]
    fn the_same_text_in_a_different_place_is_reused() {
        let mut cache = SentenceLengthCache::new();
        let text = "Same words here.\n\nSame words here.\n";
        let tree = parse(text);
        let found = cache.sentence_lengths(text, &tree, 0..text.len(), Thresholds::default());
        assert_eq!(found, uncached(text, Thresholds::default()));
        assert_eq!(cache.len, 1);
    }

    #[test]
    fn units_the_note_no_longer_has_are_forgotten() {
        let mut cache = SentenceLengthCache::new();
        let many: String = (0..SPARE_ENTRIES + 10)
            .map(|at| format!("Paragraph {at}.\n\n"))
            .collect();
        let tree = parse(&many);
        cache.sentence_lengths(&many, &tree, 0..many.len(), Thresholds::default());
        let short = "Just one.\n";
        let tree = parse(short);
        cache.sentence_lengths(short, &tree, 0..short.len(), Thresholds::default());
        assert_eq!(cache.len, 1);
    }
}
