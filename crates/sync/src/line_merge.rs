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
/// Changes to different lines merge automatically. Changes to the same
/// lines, or two insertions at the same place, conflict unless identical.
pub fn merge_lines(base: &str, this_device: &str, other_device: &str) -> LineMerge {
    let texts = SideTexts {
        base: split_lines(base),
        this_device: split_lines(this_device),
        other_device: split_lines(other_device),
    };
    let mut regions = changed_regions(&texts.base, &texts.this_device, Side::ThisDevice);
    regions.extend(changed_regions(
        &texts.base,
        &texts.other_device,
        Side::OtherDevice,
    ));
    regions.sort_by_key(|region| (region.base.start, region.base.end));
    let chunks = group_overlapping(regions);
    let segments = texts.assemble(&chunks);
    match segments.as_slice() {
        [] => LineMerge::Clean(String::new()),
        [Segment::Clean(text)] => LineMerge::Clean(text.clone()),
        _ => LineMerge::Conflicted(segments),
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
        let first_side = chunk.regions[0].side;
        let one_side_only = chunk.regions.iter().all(|region| region.side == first_side);
        if one_side_only {
            let lines = match first_side {
                Side::ThisDevice => this_lines,
                Side::OtherDevice => other_lines,
            };
            output.push_clean(&self.lines_of(first_side)[lines]);
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn insertions_at_the_same_point_conflict() {
        let merge = merge_lines("a\n", "a\nfrom laptop\n", "a\nfrom phone\n");
        let hunks = conflicts(&merge);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].base, "");
        assert_eq!(hunks[0].base_lines, 1..1);
    }

    #[test]
    fn add_add_with_no_base_conflicts_unless_equal() {
        assert_eq!(
            merge_lines("", "x\n", "x\n"),
            LineMerge::Clean("x\n".into())
        );
        assert_eq!(conflicts(&merge_lines("", "x\n", "y\n")).len(), 1);
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
