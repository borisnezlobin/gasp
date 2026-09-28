//! Places text and widgets into rows, soft-wrapping at the column width.
//!
//! Text is shaped once per chunk (a stretch of visible source set at one
//! size). A chunk that fits is used as shaped; one that doesn't is broken
//! after whitespace, or anywhere when a single word is too long, and each
//! row shows a slice of it. GPUI caches shaped lines across frames, so
//! unchanged text costs a lookup — and a long chunk is shaped in segments
//! of a few words, so typing in a long paragraph reshapes only the
//! segment being typed in rather than the whole paragraph.

use std::ops::Range;

use gpui::{Pixels, ShapedLine, SharedString, TextRun, WindowTextSystem, px};

use crate::line_layout::{Background, Hit, Piece, PieceContent, RowKind, TextPiece, VisualRow};

/// How far a piece reaches above and below the row's baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extent {
    pub ascent: Pixels,
    pub descent: Pixels,
}

impl Extent {
    /// Text centred in `line_height`, as CSS half-leading does.
    pub fn of_text(shaped: &ShapedLine, line_height: Pixels) -> Self {
        let (ascent, descent) = (shaped.ascent, shaped.descent.abs());
        let ascent = (line_height - ascent - descent) / 2. + ascent;
        Self {
            ascent,
            descent: line_height - ascent,
        }
    }

    /// A box that sits on the baseline.
    pub fn on_baseline(height: Pixels) -> Self {
        Self {
            ascent: height,
            descent: px(0.),
        }
    }

    fn max(self, other: Extent) -> Extent {
        Extent {
            ascent: self.ascent.max(other.ascent),
            descent: self.descent.max(other.descent),
        }
    }
}

/// GPUI's text shaper.
pub struct Shaper<'a> {
    pub text_system: &'a WindowTextSystem,
}

impl Shaper<'_> {
    pub fn shape(&self, text: &str, font_size: Pixels, runs: &[TextRun]) -> ShapedLine {
        let _phase = crate::keytrace::span("layout-lines: of which shape");
        self.text_system
            .shape_line(SharedString::from(text.to_owned()), font_size, runs, None)
    }
}

/// A stretch of visible text at one size, with its runs.
#[derive(Clone, Debug)]
pub struct Chunk {
    /// Line-relative source range.
    pub range: Range<usize>,
    /// The source text, with tabs as spaces so shaping keeps byte offsets.
    pub text: String,
    pub font_size: Pixels,
    pub line_height: Pixels,
    /// How far the text's baseline sits above the row's, as for `<sup>`;
    /// negative is below.
    pub baseline_shift: Pixels,
    pub runs: Vec<TextRun>,
    /// Fills behind parts of the text, painted by the editor rather than
    /// as run backgrounds.
    pub backgrounds: Vec<Background>,
}

/// Chunks longer than this are shaped in segments.
const SEGMENT_BYTES: usize = 64;
/// A segment ends at the first word after this many bytes that the word
/// hash picks, and at the first word after [`SEGMENT_MAX`] regardless.
const SEGMENT_MIN: usize = 32;
const SEGMENT_MAX: usize = 160;
/// One word in this many is picked to end a segment.
const SEGMENT_ODDS: u32 = 4;

impl Chunk {
    /// The chunk as segments, each ending after whitespace, so a row can
    /// wrap between any two of them just as it would inside the whole.
    fn segments(&self) -> Vec<Chunk> {
        if self.text.len() <= SEGMENT_BYTES {
            return vec![self.clone()];
        }
        let cuts = segment_cuts(&self.text);
        let mut runs = self.runs.iter().cloned().peekable();
        let mut carried: Option<TextRun> = None;
        cuts.windows(2)
            .map(|cut| self.segment(cut[0]..cut[1], &mut runs, &mut carried))
            .collect()
    }

    /// The segment over `range`, taking its runs from `runs` and splitting
    /// the run that crosses its end.
    fn segment(
        &self,
        range: Range<usize>,
        runs: &mut impl Iterator<Item = TextRun>,
        carried: &mut Option<TextRun>,
    ) -> Chunk {
        let mut own = Vec::new();
        let mut filled = 0;
        while filled < range.len() {
            let Some(mut run) = carried.take().or_else(|| runs.next()) else {
                break;
            };
            let room = range.len() - filled;
            if run.len > room {
                *carried = Some(TextRun {
                    len: run.len - room,
                    ..run.clone()
                });
                run.len = room;
            }
            filled += run.len;
            own.push(run);
        }
        let backgrounds = self
            .backgrounds
            .iter()
            .filter(|background| {
                background.range.start < range.end && background.range.end > range.start
            })
            .map(|background| Background {
                range: background.range.start.max(range.start) - range.start
                    ..background.range.end.min(range.end) - range.start,
                ..background.clone()
            })
            .collect();
        Chunk {
            range: self.range.start + range.start..self.range.start + range.end,
            text: self.text[range].to_owned(),
            font_size: self.font_size,
            line_height: self.line_height,
            baseline_shift: self.baseline_shift,
            runs: own,
            backgrounds,
        }
    }
}

/// Where a long text's segments start and end. A cut depends only on the
/// words just before it, not on its offset, so typing moves the cuts near
/// the cursor and leaves the rest of the paragraph's segments, and their
/// shaping, as they were.
fn segment_cuts(text: &str) -> Vec<usize> {
    let mut cuts = vec![0];
    let mut word_start = 0;
    for at in break_points(text) {
        if at == text.len() {
            break;
        }
        let since = at - cuts[cuts.len() - 1];
        let picked = word_hash(&text[word_start..at]).is_multiple_of(SEGMENT_ODDS);
        word_start = at;
        if since >= SEGMENT_MAX || since >= SEGMENT_MIN && picked {
            cuts.push(at);
        }
    }
    cuts.push(text.len());
    cuts
}

/// FNV-1a: cheap, and the same on every run.
fn word_hash(word: &str) -> u32 {
    word.bytes().fold(0x811c_9dc5, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

/// Builds the rows of one line.
pub struct RowBuilder {
    limit: Pixels,
    hang: Pixels,
    x: Pixels,
    y: Pixels,
    strut: Extent,
    /// The caret's reach above and below the baseline: the font's own
    /// ascent and descent, not the line's leading, so a tall line height
    /// doesn't stretch the caret past the letters.
    caret: Extent,
    pending: Vec<(Piece, Extent)>,
    rows: Vec<VisualRow>,
}

impl RowBuilder {
    /// Rows start at `left`, wrap at `limit` and are at least `strut` tall.
    pub fn new(left: Pixels, limit: Pixels, top: Pixels, strut: Extent) -> Self {
        Self {
            limit: limit.max(left + px(1.)),
            hang: left,
            x: left,
            y: top,
            strut,
            caret: strut,
            pending: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// Draws the caret `caret` tall about the baseline instead of the
    /// strut's full height.
    pub fn with_caret(mut self, caret: Extent) -> Self {
        self.caret = Extent {
            ascent: caret.ascent.min(self.strut.ascent),
            descent: caret.descent.min(self.strut.descent),
        };
        self
    }

    pub fn x(&self) -> Pixels {
        self.x
    }

    pub fn limit(&self) -> Pixels {
        self.limit
    }

    pub fn has_content(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Wrapped rows start at the current x from now on, so a list item's
    /// text lines up under its first word.
    pub fn hang_here(&mut self) {
        if self.x < self.limit - px(1.) {
            self.hang = self.x;
        }
    }

    /// Adds space, as for leading indentation.
    pub fn advance(&mut self, width: Pixels) {
        self.x += width;
    }

    fn fits(&self, width: Pixels) -> bool {
        self.x + width <= self.limit
    }

    /// Places a piece that can't be split, wrapping first if it doesn't fit.
    pub fn push_atomic(&mut self, mut piece: Piece, extent: Extent) {
        if !self.fits(piece.width) && self.has_content() {
            self.break_row();
        }
        piece.x = self.x;
        self.x += piece.width;
        self.pending.push((piece, extent));
    }

    /// Places a piece at its own x, without advancing.
    pub fn push_overlapping(&mut self, piece: Piece, extent: Extent) {
        self.pending.push((piece, extent));
    }

    /// Ends the current row, even an empty one.
    pub fn break_row(&mut self) {
        let row = self.finish_row(RowKind::Text);
        self.y = row.bottom();
        self.rows.push(row);
        self.x = self.hang;
    }

    /// Adds a finished block or below row after the current rows.
    pub fn push_row(&mut self, mut row: VisualRow) {
        if self.has_content() {
            self.break_row();
        }
        row.top = self.y;
        self.y = row.bottom();
        self.rows.push(row);
        self.x = self.hang;
    }

    /// The rows, ending the last text row if it has anything in it (or
    /// if there are no rows at all).
    pub fn finish(mut self) -> Vec<VisualRow> {
        if self.has_content() || self.rows.is_empty() {
            self.break_row();
        }
        self.rows
    }

    fn finish_row(&mut self, kind: RowKind) -> VisualRow {
        let extent = self
            .pending
            .iter()
            .fold(self.strut, |extent, (_, piece)| extent.max(*piece));
        let pieces = std::mem::take(&mut self.pending)
            .into_iter()
            .map(|(mut piece, piece_extent)| {
                piece.top = extent.ascent - piece_extent.ascent;
                piece
            })
            .collect();
        VisualRow {
            kind,
            top: self.y,
            height: extent.ascent + extent.descent,
            range: 0..0,
            soft_end: 0,
            caret_top: extent.ascent - self.caret.ascent,
            caret_height: self.caret.ascent + self.caret.descent,
            left: self.hang,
            pieces,
        }
    }

    /// Places a chunk of text, wrapping it across rows as needed. Each
    /// segment of the chunk is shaped once and each row shows a slice of
    /// one.
    pub fn push_chunk(&mut self, chunk: &Chunk, shaper: &Shaper<'_>) {
        for segment in chunk.segments() {
            self.push_segment(&segment, shaper);
        }
    }

    fn push_segment(&mut self, chunk: &Chunk, shaper: &Shaper<'_>) {
        let shaped = shaper.shape(&chunk.text, chunk.font_size, &chunk.runs);
        let whole =
            TextPiece::whole(shaped, chunk.line_height).with_backgrounds(chunk.backgrounds.clone());
        if self.fits(whole.shaped.width) {
            return self.place_text(chunk, whole);
        }
        let positions = GlyphPositions::new(&whole.shaped);
        let breaks = break_points(&chunk.text);
        let mut start = 0;
        while start < chunk.text.len() {
            start = self.place_next_part(chunk, start, &whole, &positions, &breaks);
        }
    }

    /// Places as much of the chunk from `start` as fits on this row and
    /// returns where the rest begins.
    fn place_next_part(
        &mut self,
        chunk: &Chunk,
        start: usize,
        whole: &TextPiece,
        positions: &GlyphPositions,
        breaks: &[usize],
    ) -> usize {
        let text = &chunk.text;
        let base = positions.x_at(start);
        let room = self.limit - self.x;
        let width_to = |end: usize| positions.x_at(trim_end(text, start, end)) - base;
        let end = if positions.x_at(text.len()) - base <= room {
            text.len()
        } else {
            let word_end = breaks
                .iter()
                .copied()
                .filter(|&at| at > start)
                .take_while(|&at| width_to(at) <= room)
                .last();
            match word_end {
                Some(end) => end,
                None if self.has_content() => {
                    self.break_row();
                    return start;
                }
                None => character_fit(text, start, positions.index_before(base + room)),
            }
        };
        let slice = TextPiece {
            slice: start..end,
            slice_x: base,
            ..whole.clone()
        };
        self.place_text(chunk, slice);
        if end < text.len() {
            self.break_row();
        }
        end
    }

    fn place_text(&mut self, chunk: &Chunk, text: TextPiece) {
        let mut extent = Extent::of_text(&text.shaped, chunk.line_height);
        extent.ascent += chunk.baseline_shift;
        extent.descent -= chunk.baseline_shift;
        let start = chunk.range.start + text.slice.start;
        let width = text.x_for_index(text.slice.len());
        let piece = Piece {
            range: start..start + text.slice.len(),
            x: self.x,
            top: px(0.),
            width,
            height: chunk.line_height,
            content: PieceContent::Text(Box::new(text)),
            hit: Hit::Text,
        };
        self.x += piece.width;
        self.pending.push((piece, extent));
    }
}

/// Where each character's glyph starts, for finding x by byte index
/// without scanning the shaped runs every time.
struct GlyphPositions {
    starts: Vec<(usize, Pixels)>,
    width: Pixels,
}

impl GlyphPositions {
    fn new(shaped: &ShapedLine) -> Self {
        let mut starts: Vec<(usize, Pixels)> = shaped
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| (glyph.index, glyph.position.x))
            .collect();
        starts.sort_by_key(|(index, _)| *index);
        Self {
            starts,
            width: shaped.width,
        }
    }

    /// The x where the character at `index` starts.
    fn x_at(&self, index: usize) -> Pixels {
        let at = self.starts.partition_point(|(start, _)| *start < index);
        self.starts.get(at).map_or(self.width, |(_, x)| *x)
    }

    /// The last character that starts before `x`.
    fn index_before(&self, x: Pixels) -> usize {
        let at = self.starts.partition_point(|(_, start)| *start < x);
        self.starts
            .get(at.saturating_sub(1))
            .map_or(0, |(index, _)| *index)
    }
}

/// The width of the widest word in shaped `text`: the narrowest it can
/// wrap to without breaking inside a word.
pub fn widest_word(shaped: &ShapedLine, text: &str) -> Pixels {
    let positions = GlyphPositions::new(shaped);
    let mut start = 0;
    let mut widest = px(0.);
    for end in break_points(text) {
        let width = positions.x_at(trim_end(text, start, end)) - positions.x_at(start);
        widest = widest.max(width);
        start = end;
    }
    widest
}

/// Offsets where a new word starts after whitespace.
pub fn break_points(text: &str) -> Vec<usize> {
    let mut points = Vec::new();
    let mut after_space = false;
    for (index, character) in text.char_indices() {
        let space = character == ' ' || character == '\t';
        if after_space && !space {
            points.push(index);
        }
        after_space = space;
    }
    points.push(text.len());
    points
}

/// `end` moved back over trailing whitespace, but not before `start`.
fn trim_end(text: &str, start: usize, end: usize) -> usize {
    let trimmed = text[start..end].trim_end_matches([' ', '\t']).len();
    start + trimmed
}

/// Where to cut a word too long for the row: before the character that
/// overflows, but always after at least one character.
fn character_fit(text: &str, start: usize, overflowing: usize) -> usize {
    let first_end = text[start..]
        .chars()
        .next()
        .map_or(text.len(), |c| start + c.len_utf8());
    let mut end = overflowing.max(first_end).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end.max(first_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_follow_whitespace() {
        assert_eq!(break_points("ab cd  ef"), vec![3, 7, 9]);
        assert_eq!(break_points(" lead"), vec![1, 5]);
        assert_eq!(break_points(""), vec![0]);
    }

    #[test]
    fn segments_cut_where_words_start_and_hold_through_edits() {
        let text = "the quick brown fox jumps over the lazy dog and keeps running ".repeat(8);
        let cuts = segment_cuts(&text);
        assert_eq!((cuts[0], *cuts.last().unwrap()), (0, text.len()));
        for pair in cuts.windows(2) {
            assert!(pair[1] - pair[0] <= SEGMENT_MAX + 16, "{cuts:?}");
            assert!(pair[1] == text.len() || text[..pair[1]].ends_with(' '));
        }
        // Typing a word near the start leaves the later cuts where they
        // were in the text after it.
        let typed = format!("new {text}");
        let later: Vec<usize> = segment_cuts(&typed)
            .iter()
            .map(|cut| cut.saturating_sub(4))
            .collect();
        let kept = cuts
            .iter()
            .filter(|cut| **cut > 200 && later.contains(cut))
            .count();
        let total = cuts.iter().filter(|cut| **cut > 200).count();
        assert_eq!(kept, total, "{cuts:?} {later:?}");
    }

    #[test]
    fn trailing_spaces_hang() {
        assert_eq!(trim_end("ab cd ", 0, 3), 2);
        assert_eq!(trim_end("   ", 1, 3), 1);
    }

    #[test]
    fn long_words_break_after_at_least_one_character() {
        assert_eq!(character_fit("abcdef", 0, 3), 3);
        assert_eq!(character_fit("abcdef", 2, 2), 3);
        assert_eq!(character_fit("日本語", 0, 1), 3);
    }
}
