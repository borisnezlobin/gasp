use std::borrow::Cow;
use std::ops::Range;

use similar::{Algorithm, DiffTag, capture_diff_slices};

use crate::conflict::{ConflictHunk, Segment};

/// The result of a three-way line merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineMerge {
    /// Every change merged; this is the merged text.
    Clean(String),
    /// Some changes overlap. Clean text and conflicting hunks, in file order.
    Conflicted(Vec<Segment>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    ThisDevice,
    OtherDevice,
}

/// One contiguous change a side made to the base, in line numbers.
#[derive(Debug, Clone)]
struct Region {
    side: Side,
    base: Range<usize>,
    lines: Range<usize>,
}

/// Regions that overlap each other, merged as a unit.
#[derive(Debug)]
struct Chunk {
    base: Range<usize>,
    regions: Vec<Region>,
}

/// Merges `this_device` and `other_device`, both edited from `base`, line by line.
///
/// Changes to different lines merge automatically. When both sides only
/// insert lines at the same place, both insertions are kept, this device's
/// first, with the lines they start or end with in common kept once.
/// Changes to the same lines conflict unless identical.
///
/// A missing newline at the end of a file doesn't count as a change to its
/// last line, so two devices appending to a note saved without one still
/// merge cleanly.
pub fn merge_lines(base: &str, this_device: &str, other_device: &str) -> LineMerge {
    let endings = OpenEndings::of(base, this_device, other_device);
    let (base, this_device, other_device) = (
        close_last_line(base),
        close_last_line(this_device),
        close_last_line(other_device),
    );
    let texts = SideTexts {
        base: split_lines(&base),
        this_device: split_lines(&this_device),
        other_device: split_lines(&other_device),
    };
    let mut regions = changed_regions(&texts.base, &texts.this_device, Side::ThisDevice);
    regions.extend(changed_regions(
        &texts.base,
        &texts.other_device,
        Side::OtherDevice,
    ));
    regions.sort_by_key(|region| (region.base.start, region.base.end));
    let chunks = group_overlapping(regions);
    let mut segments = texts.assemble(&chunks);
    endings.reopen_last_line(&mut segments);
    match segments.as_slice() {
        [] => LineMerge::Clean(String::new()),
        [Segment::Clean(text)] => LineMerge::Clean(text.clone()),
        _ => LineMerge::Conflicted(segments),
    }
}

/// Whether each version's last line lacks a newline.
struct OpenEndings {
    base: bool,
    this_device: bool,
    other_device: bool,
}

impl OpenEndings {
    fn of(base: &str, this_device: &str, other_device: &str) -> Self {
        Self {
            base: ends_open(base),
            this_device: ends_open(this_device),
            other_device: ends_open(other_device),
        }
    }

    /// Whether the merged text ends open: a side that changed the ending wins.
    fn merged(&self) -> bool {
        if self.this_device == self.base {
            self.other_device
        } else {
            self.this_device
        }
    }

    /// Takes back the newlines [`close_last_line`] added.
    fn reopen_last_line(&self, segments: &mut [Segment]) {
        match segments.last_mut() {
            Some(Segment::Clean(text)) => drop_added_newline(text, self.merged()),
            Some(Segment::Conflict(hunk)) => {
                drop_added_newline(&mut hunk.base, self.base);
                drop_added_newline(&mut hunk.this_device, self.this_device);
                drop_added_newline(&mut hunk.other_device, self.other_device);
            }
            None => {}
        }
    }
}

fn ends_open(text: &str) -> bool {
    !text.is_empty() && !text.ends_with('\n')
}

fn close_last_line(text: &str) -> Cow<'_, str> {
    if ends_open(text) {
        Cow::Owned(format!("{text}\n"))
    } else {
        Cow::Borrowed(text)
    }
}

fn drop_added_newline(text: &mut String, was_open: bool) {
    if was_open && text.ends_with('\n') {
        text.pop();
    }
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn changed_regions(base: &[&str], side_lines: &[&str], side: Side) -> Vec<Region> {
    let mut regions: Vec<Region> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, base, side_lines) {
        let (tag, base_range, side_range) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        match regions.last_mut() {
            Some(last)
                if last.base.end == base_range.start && last.lines.end == side_range.start =>
            {
                last.base.end = base_range.end;
                last.lines.end = side_range.end;
            }
            _ => regions.push(Region {
                side,
                base: base_range,
                lines: side_range,
            }),
        }
    }
    regions
}

fn group_overlapping(sorted: Vec<Region>) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    for region in sorted {
        match chunks.last_mut() {
            Some(chunk) if overlaps(&chunk.base, &region.base) => {
                chunk.base.end = chunk.base.end.max(region.base.end);
                chunk.regions.push(region);
            }
            _ => chunks.push(Chunk {
                base: region.base.clone(),
                regions: vec![region],
            }),
        }
    }
    chunks
}

/// Ranges overlap when they share a line, or when both insert at the same point.
fn overlaps(chunk: &Range<usize>, region: &Range<usize>) -> bool {
    let same_insertion_point = chunk.is_empty() && region.is_empty() && chunk.start == region.start;
    region.start < chunk.end || same_insertion_point
}

struct SideTexts<'a> {
    base: Vec<&'a str>,
    this_device: Vec<&'a str>,
    other_device: Vec<&'a str>,
}

impl SideTexts<'_> {
    fn assemble(&self, chunks: &[Chunk]) -> Vec<Segment> {
        let mut output = SegmentBuilder::default();
        let mut base_cursor = 0;
        let mut this_cursor = 0;
        let mut other_cursor = 0;
        for chunk in chunks {
            let unchanged = chunk.base.start - base_cursor;
            output.push_clean(&self.base[base_cursor..chunk.base.start]);
            this_cursor += unchanged;
            other_cursor += unchanged;
            let this_lines = self.side_range(chunk, Side::ThisDevice, this_cursor);
            let other_lines = self.side_range(chunk, Side::OtherDevice, other_cursor);
            self.merge_chunk(&mut output, chunk, this_lines.clone(), other_lines.clone());
            base_cursor = chunk.base.end;
            this_cursor = this_lines.end;
            other_cursor = other_lines.end;
        }
        output.push_clean(&self.base[base_cursor..]);
        output.finish()
    }

    /// The lines of `side` that replace the chunk's base lines.
    fn side_range(&self, chunk: &Chunk, side: Side, cursor: usize) -> Range<usize> {
        let mut own = chunk.regions.iter().filter(|region| region.side == side);
        let Some(first) = own.next() else {
            return cursor..cursor + chunk.base.len();
        };
        let last = own.next_back().unwrap_or(first);
        let start = first.lines.start - (first.base.start - chunk.base.start);
        let end = last.lines.end + (chunk.base.end - last.base.end);
        start..end
    }

    fn lines_of(&self, side: Side) -> &[&str] {
        match side {
            Side::ThisDevice => &self.this_device,
            Side::OtherDevice => &self.other_device,
        }
    }

    fn merge_chunk(
        &self,
        output: &mut SegmentBuilder,
        chunk: &Chunk,
        this_lines: Range<usize>,
        other_lines: Range<usize>,
    ) {
        let this_text = &self.this_device[this_lines.clone()];
        let other_text = &self.other_device[other_lines.clone()];
        if let Some(side) = chunk.only_side() {
            let lines = match side {
                Side::ThisDevice => this_lines,
                Side::OtherDevice => other_lines,
            };
            output.push_clean(&self.lines_of(side)[lines]);
        } else if chunk.base.is_empty() {
            push_both_insertions(output, this_text, other_text);
        } else if this_text == other_text {
            output.push_clean(this_text);
        } else {
            output.push_conflict(ConflictHunk {
                base: self.base[chunk.base.clone()].concat(),
                this_device: this_text.concat(),
                other_device: other_text.concat(),
                base_lines: chunk.base.clone(),
                this_device_lines: this_lines,
                other_device_lines: other_lines,
            });
        }
    }
}

impl Chunk {
    /// The side that made every change in the chunk, if only one did.
    fn only_side(&self) -> Option<Side> {
        let first = self.regions[0].side;
        self.regions
            .iter()
            .all(|region| region.side == first)
            .then_some(first)
    }
}

/// Both sides inserted lines at the same place and changed nothing there:
/// this device's lines, then the other device's, with the lines both start
/// or end with kept once. Identical insertions come out once.
fn push_both_insertions(output: &mut SegmentBuilder, this_lines: &[&str], other_lines: &[&str]) {
    let leading = shared_leading_lines(this_lines, other_lines);
    let (this_rest, other_rest) = (&this_lines[leading..], &other_lines[leading..]);
    let trailing = shared_trailing_lines(this_rest, other_rest);
    output.push_clean(&this_lines[..leading]);
    output.push_clean(&this_rest[..this_rest.len() - trailing]);
    output.push_clean(&other_rest[..other_rest.len() - trailing]);
    output.push_clean(&this_rest[this_rest.len() - trailing..]);
}

fn shared_leading_lines(a: &[&str], b: &[&str]) -> usize {
    a.iter().zip(b).take_while(|(a, b)| a == b).count()
}

fn shared_trailing_lines(a: &[&str], b: &[&str]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
}

#[derive(Default)]
struct SegmentBuilder {
    segments: Vec<Segment>,
}

impl SegmentBuilder {
    fn push_clean(&mut self, lines: &[&str]) {
        if lines.is_empty() {
            return;
        }
        if let Some(Segment::Clean(text)) = self.segments.last_mut() {
            text.push_str(&lines.concat());
            return;
        }
        self.segments.push(Segment::Clean(lines.concat()));
    }

    fn push_conflict(&mut self, hunk: ConflictHunk) {
        self.segments.push(Segment::Conflict(hunk));
    }

    fn finish(self) -> Vec<Segment> {
        self.segments
    }
}

/// Merges two versions of a file that share no history, such as a note a
/// folder had before it became a clone and the repository's copy of it.
///
/// Lines both versions have are kept once, and lines only one of them has
/// are kept where they stand, since nothing says the other side removed
/// them. A place where the two read differently, each with lines the other
/// lacks between the same shared lines, is a conflict for a person.
pub fn merge_unrelated(this_device: &str, other_device: &str) -> LineMerge {
    // With no base to say which side changed the ending, this device's wins.
    let endings = OpenEndings::of(other_device, this_device, other_device);
    let (this_device, other_device) = (close_last_line(this_device), close_last_line(other_device));
    let sides = UnrelatedSides {
        this_device: split_lines(&this_device),
        other_device: split_lines(&other_device),
    };
    let mut segments = sides.segments();
    endings.reopen_last_line(&mut segments);
    match segments.as_slice() {
        [] => LineMerge::Clean(String::new()),
        [Segment::Clean(text)] => LineMerge::Clean(text.clone()),
        _ => LineMerge::Conflicted(segments),
    }
}

/// The lines of two versions with no common ancestor.
struct UnrelatedSides<'a> {
    this_device: Vec<&'a str>,
    other_device: Vec<&'a str>,
}

impl UnrelatedSides<'_> {
    fn segments(&self) -> Vec<Segment> {
        let mut output = SegmentBuilder::default();
        let mut this_changed = 0..0;
        let mut other_changed = 0..0;
        for op in capture_diff_slices(Algorithm::Myers, &self.this_device, &self.other_device) {
            let (tag, this_range, other_range) = op.as_tag_tuple();
            if tag != DiffTag::Equal {
                this_changed.end = this_range.end;
                other_changed.end = other_range.end;
                continue;
            }
            self.push_difference(&mut output, &this_changed, &other_changed);
            output.push_clean(&self.this_device[this_range.clone()]);
            this_changed = this_range.end..this_range.end;
            other_changed = other_range.end..other_range.end;
        }
        self.push_difference(&mut output, &this_changed, &other_changed);
        output.finish()
    }

    /// Lines only one side has are kept; lines on both sides at one place
    /// are a conflict.
    fn push_difference(
        &self,
        output: &mut SegmentBuilder,
        this_lines: &Range<usize>,
        other_lines: &Range<usize>,
    ) {
        let this_text = &self.this_device[this_lines.clone()];
        let other_text = &self.other_device[other_lines.clone()];
        if this_text.is_empty() || other_text.is_empty() {
            output.push_clean(this_text);
            output.push_clean(other_text);
            return;
        }
        output.push_conflict(ConflictHunk {
            base: String::new(),
            this_device: this_text.concat(),
            other_device: other_text.concat(),
            base_lines: 0..0,
            this_device_lines: this_lines.clone(),
            other_device_lines: other_lines.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_versions_keep_every_line_and_clash_where_they_differ() {
        let this_device = "# Plan\nold line\nshared\nonly here\n";
        let other_device = "# Plan\nnew line\nshared\nadded there\nend";
        let LineMerge::Conflicted(segments) = merge_unrelated(this_device, other_device) else {
            panic!("the differing line is a conflict");
        };
        let hunks: Vec<&ConflictHunk> = segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Conflict(hunk) => Some(hunk),
                Segment::Clean(_) => None,
            })
            .collect();
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].this_device, "old line\n");
        assert_eq!(hunks[0].other_device, "new line\n");
        assert_eq!(hunks[1].this_device, "only here\n");
        assert_eq!(hunks[1].other_device, "added there\nend");
    }

    #[test]
    fn unrelated_versions_with_one_sided_lines_merge_cleanly() {
        assert_eq!(
            merge_unrelated("a\nb\n", "a\nb\nc\n"),
            LineMerge::Clean("a\nb\nc\n".to_owned())
        );
        assert_eq!(
            merge_unrelated("a\nlocal\nb", "a\nb"),
            LineMerge::Clean("a\nlocal\nb".to_owned())
        );
        assert_eq!(
            merge_unrelated("same\n", "same"),
            LineMerge::Clean("same\n".to_owned())
        );
    }

    const BASE: &str = "one\ntwo\nthree\nfour\nfive\n";

    fn conflicts(merge: &LineMerge) -> Vec<&ConflictHunk> {
        match merge {
            LineMerge::Clean(_) => Vec::new(),
            LineMerge::Conflicted(segments) => segments
                .iter()
                .filter_map(|segment| match segment {
                    Segment::Conflict(hunk) => Some(hunk),
                    Segment::Clean(_) => None,
                })
                .collect(),
        }
    }

    #[test]
    fn unchanged_sides_give_the_base() {
        assert_eq!(merge_lines(BASE, BASE, BASE), LineMerge::Clean(BASE.into()));
    }

    #[test]
    fn empty_everything_is_clean_and_empty() {
        assert_eq!(merge_lines("", "", ""), LineMerge::Clean(String::new()));
    }

    #[test]
    fn edits_to_different_lines_merge() {
        let this = "ONE\ntwo\nthree\nfour\nfive\n";
        let other = "one\ntwo\nthree\nfour\nFIVE\n";
        assert_eq!(
            merge_lines(BASE, this, other),
            LineMerge::Clean("ONE\ntwo\nthree\nfour\nFIVE\n".into())
        );
    }

    #[test]
    fn edits_to_adjacent_lines_merge() {
        let this = "one\nTWO\nthree\nfour\nfive\n";
        let other = "one\ntwo\nTHREE\nfour\nfive\n";
        assert_eq!(
            merge_lines(BASE, this, other),
            LineMerge::Clean("one\nTWO\nTHREE\nfour\nfive\n".into())
        );
    }

    #[test]
    fn insertion_and_deletion_elsewhere_merge() {
        let this = "zero\none\ntwo\nthree\nfour\nfive\n";
        let other = "one\ntwo\nthree\nfive\n";
        assert_eq!(
            merge_lines(BASE, this, other),
            LineMerge::Clean("zero\none\ntwo\nthree\nfive\n".into())
        );
    }

    #[test]
    fn identical_edits_merge_once() {
        let both = "one\n2\nthree\nfour\nfive\n";
        assert_eq!(merge_lines(BASE, both, both), LineMerge::Clean(both.into()));
    }

    #[test]
    fn same_line_edits_conflict_with_correct_ranges() {
        let this = "one\ntwo\nthree (laptop)\nfour\nfive\n";
        let other = "one\ntwo\nthree (phone)\nextra\nfour\nfive\n";
        let merge = merge_lines(BASE, this, other);
        let hunks = conflicts(&merge);
        assert_eq!(hunks.len(), 1);
        let hunk = hunks[0];
        assert_eq!(hunk.base, "three\n");
        assert_eq!(hunk.this_device, "three (laptop)\n");
        assert_eq!(hunk.other_device, "three (phone)\nextra\n");
        assert_eq!(hunk.base_lines, 2..3);
        assert_eq!(hunk.this_device_lines, 2..3);
        assert_eq!(hunk.other_device_lines, 2..4);
    }

    #[test]
    fn line_ranges_account_for_earlier_clean_changes() {
        let this = "zero\none\ntwo\nthree\nfour\nFIVE-a\n";
        let other = "one\nfour\nFIVE-b\n";
        let merge = merge_lines(BASE, this, other);
        let hunks = conflicts(&merge);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].base_lines, 4..5);
        assert_eq!(hunks[0].this_device_lines, 5..6);
        assert_eq!(hunks[0].other_device_lines, 2..3);
    }

    #[test]
    fn appends_at_the_end_keep_both_with_this_device_first() {
        let this = format!("{BASE}from laptop\n");
        let other = format!("{BASE}from phone\nand more\n");
        assert_eq!(
            merge_lines(BASE, &this, &other),
            LineMerge::Clean(format!("{BASE}from laptop\nfrom phone\nand more\n"))
        );
    }

    #[test]
    fn appends_to_a_note_without_a_final_newline_keep_both() {
        let base = "# Lemma\nproof";
        let merge = merge_lines(base, "# Lemma\nproof\nlaptop", "# Lemma\nproof\nphone");
        assert_eq!(
            merge,
            LineMerge::Clean("# Lemma\nproof\nlaptop\nphone".into())
        );
    }

    #[test]
    fn a_side_that_adds_the_final_newline_keeps_it() {
        let merge = merge_lines("a", "a\nlaptop\n", "a\nphone");
        assert_eq!(merge, LineMerge::Clean("a\nlaptop\nphone\n".into()));
    }

    #[test]
    fn insertions_at_the_same_point_mid_file_keep_both() {
        let this = "one\ntwo\nlaptop 1\nlaptop 2\nthree\nfour\nfive\n";
        let other = "one\ntwo\nphone\nthree\nfour\nfive\n";
        assert_eq!(
            merge_lines(BASE, this, other),
            LineMerge::Clean("one\ntwo\nlaptop 1\nlaptop 2\nphone\nthree\nfour\nfive\n".into())
        );
    }

    #[test]
    fn identical_insertions_are_kept_once() {
        let both = "one\ntwo\nsame\nthree\nfour\nfive\n";
        assert_eq!(merge_lines(BASE, both, both), LineMerge::Clean(both.into()));
    }

    #[test]
    fn lines_both_insertions_share_at_either_end_are_kept_once() {
        let this = format!("{BASE}## Log\nlaptop\n---\n");
        let other = format!("{BASE}## Log\nphone\n---\n");
        assert_eq!(
            merge_lines(BASE, &this, &other),
            LineMerge::Clean(format!("{BASE}## Log\nlaptop\nphone\n---\n"))
        );
    }

    #[test]
    fn an_insertion_inside_lines_the_other_side_rewrote_conflicts() {
        let this = "one\ntwo\ninserted\nthree\nfour\nfive\n";
        let other = "one\nTWO AND THREE\nfour\nfive\n";
        let merge = merge_lines(BASE, this, other);
        let hunks = conflicts(&merge);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].base, "two\nthree\n");
        assert_eq!(hunks[0].this_device, "two\ninserted\nthree\n");
        assert_eq!(hunks[0].other_device, "TWO AND THREE\n");
    }

    #[test]
    fn an_insertion_inside_lines_the_other_side_deleted_conflicts() {
        let this = "one\ntwo\ninserted\nthree\nfour\nfive\n";
        let other = "one\nfour\nfive\n";
        assert_eq!(conflicts(&merge_lines(BASE, this, other)).len(), 1);
    }

    #[test]
    fn rewriting_the_last_line_on_both_sides_still_conflicts() {
        let merge = merge_lines("a\nlast", "a\nlast, laptop", "a\nlast, phone");
        let hunks = conflicts(&merge);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].base, "last");
        assert_eq!(hunks[0].this_device, "last, laptop");
        assert_eq!(hunks[0].other_device, "last, phone");
    }

    #[test]
    fn two_new_files_with_the_same_name_keep_both() {
        assert_eq!(
            merge_lines("", "x\n", "x\n"),
            LineMerge::Clean("x\n".into())
        );
        assert_eq!(
            merge_lines("", "# Day\nlaptop\n", "# Day\nphone\n"),
            LineMerge::Clean("# Day\nlaptop\nphone\n".into())
        );
    }

    #[test]
    fn two_separate_conflicts_keep_clean_text_between() {
        let this = "ONE-a\ntwo\nthree\nfour\nFIVE-a\n";
        let other = "ONE-b\ntwo\nthree\nfour\nFIVE-b\n";
        let LineMerge::Conflicted(segments) = merge_lines(BASE, this, other) else {
            panic!("expected conflicts");
        };
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[1], Segment::Clean("two\nthree\nfour\n".into()));
    }
}
