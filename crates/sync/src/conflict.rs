use std::ops::Range;
use std::path::PathBuf;

use crate::error::{SyncError, SyncResult};

pub(crate) const THIS_DEVICE_MARKER: &str = "<<<<<<< this device\n";
pub(crate) const SEPARATOR_MARKER: &str = "=======\n";
pub(crate) const OTHER_DEVICE_MARKER: &str = ">>>>>>> other device\n";

/// One place where both devices changed the same lines differently.
///
/// Line ranges are zero-based line numbers in each version of the file.
/// `base` and `base_lines` are empty when a hunk was read back from a
/// marked file that no longer lines up with the merge that wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictHunk {
    pub base: String,
    pub this_device: String,
    pub other_device: String,
    pub base_lines: Range<usize>,
    pub this_device_lines: Range<usize>,
    pub other_device_lines: Range<usize>,
}

/// A piece of a conflicted file: text both sides agree on, or a conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Clean(String),
    Conflict(ConflictHunk),
}

/// How to settle one hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    ThisDevice,
    OtherDevice,
    /// This device's text, then the other device's.
    Both,
    /// Text the person wrote by hand.
    Merged(String),
}

/// Byte ranges of one hunk inside [`MarkedText::text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkedHunk {
    /// From the start of the opening marker to the end of the closing marker.
    pub whole: Range<usize>,
    pub this_device: Range<usize>,
    pub other_device: Range<usize>,
}

/// A conflicted file written out with both versions between conflict markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkedText {
    pub text: String,
    pub hunks: Vec<MarkedHunk>,
}

/// A text file whose merge needs a person, as clean text and hunks in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictedFile {
    /// Path relative to the vault root.
    pub path: PathBuf,
    pub segments: Vec<Segment>,
}

impl ConflictedFile {
    pub fn hunks(&self) -> impl Iterator<Item = &ConflictHunk> {
        self.segments.iter().filter_map(|segment| match segment {
            Segment::Conflict(hunk) => Some(hunk),
            Segment::Clean(_) => None,
        })
    }

    pub fn hunk_count(&self) -> usize {
        self.hunks().count()
    }

    /// Reads text written by [`ConflictedFile::marked_text`], and perhaps
    /// edited since, back into clean text and hunks. Markers nested inside
    /// a hunk stay part of its text, and a hunk missing its closing marker
    /// counts as clean text. Markers don't show what the lines were before
    /// either device changed them, so each hunk's `base` comes from the
    /// matching hunk of `known`, and stays empty when the hunks no longer
    /// line up with it.
    pub fn from_marked_text(path: PathBuf, text: &str, known: Option<&ConflictedFile>) -> Self {
        let mut parser = MarkerParser::default();
        for line in text.split_inclusive('\n') {
            parser.push_line(line);
        }
        let mut file = ConflictedFile {
            path,
            segments: parser.finish(),
        };
        if let Some(known) = known.filter(|known| known.hunk_count() == file.hunk_count()) {
            file.take_bases_from(known);
        }
        file
    }

    fn take_bases_from(&mut self, known: &ConflictedFile) {
        let parsed = self
            .segments
            .iter_mut()
            .filter_map(|segment| match segment {
                Segment::Conflict(hunk) => Some(hunk),
                Segment::Clean(_) => None,
            });
        for (hunk, known) in parsed.zip(known.hunks()) {
            hunk.base.clone_from(&known.base);
            hunk.base_lines = known.base_lines.clone();
        }
    }

    /// The file with every hunk shown as `<<<<<<< this device` / `=======` / `>>>>>>> other device`.
    pub fn marked_text(&self) -> MarkedText {
        let mut text = String::new();
        let mut hunks = Vec::new();
        for segment in &self.segments {
            match segment {
                Segment::Clean(clean) => text.push_str(clean),
                Segment::Conflict(hunk) => hunks.push(push_marked_hunk(&mut text, hunk)),
            }
        }
        MarkedText { text, hunks }
    }

    /// The merged file text, with `resolutions[i]` applied to the i-th hunk.
    pub fn resolve(&self, resolutions: &[Resolution]) -> SyncResult<String> {
        if resolutions.len() != self.hunk_count() {
            return Err(SyncError::InvalidResolution(format!(
                "{} has {} hunk(s) but {} resolution(s) were given",
                self.path.display(),
                self.hunk_count(),
                resolutions.len()
            )));
        }
        let mut resolutions = resolutions.iter();
        let mut text = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Clean(clean) => text.push_str(clean),
                Segment::Conflict(hunk) => {
                    let resolution = resolutions.next().expect("counted above");
                    push_resolved(&mut text, hunk, resolution);
                }
            }
        }
        Ok(text)
    }
}

fn push_marked_hunk(text: &mut String, hunk: &ConflictHunk) -> MarkedHunk {
    let whole_start = text.len();
    text.push_str(THIS_DEVICE_MARKER);
    let this_device = push_block(text, &hunk.this_device);
    text.push_str(SEPARATOR_MARKER);
    let other_device = push_block(text, &hunk.other_device);
    text.push_str(OTHER_DEVICE_MARKER);
    MarkedHunk {
        whole: whole_start..text.len(),
        this_device,
        other_device,
    }
}

/// Pushes `block` and makes sure the next marker starts on its own line.
fn push_block(text: &mut String, block: &str) -> Range<usize> {
    let start = text.len();
    text.push_str(block);
    let range = start..text.len();
    end_line(text);
    range
}

fn end_line(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
}

fn push_resolved(text: &mut String, hunk: &ConflictHunk, resolution: &Resolution) {
    match resolution {
        Resolution::ThisDevice => text.push_str(&hunk.this_device),
        Resolution::OtherDevice => text.push_str(&hunk.other_device),
        Resolution::Merged(merged) => text.push_str(merged),
        Resolution::Both => {
            text.push_str(&hunk.this_device);
            if !hunk.other_device.is_empty() {
                end_line(text);
            }
            text.push_str(&hunk.other_device);
        }
    }
}

/// Where the marker parser is in the text.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Place {
    #[default]
    Outside,
    ThisDeviceSide,
    OtherDeviceSide,
}

/// Reads marked text a line at a time into segments.
#[derive(Default)]
struct MarkerParser {
    segments: Vec<Segment>,
    place: Place,
    clean: String,
    /// The open hunk's lines exactly as written, markers included.
    written: String,
    this_device: String,
    other_device: String,
    /// Markers opened inside the open hunk and not closed yet.
    nesting: usize,
    /// Lines so far in this device's version and in the other device's.
    this_device_line: usize,
    other_device_line: usize,
    hunk_start: (usize, usize),
}

impl MarkerParser {
    fn push_line(&mut self, line: &str) {
        match self.place {
            Place::Outside if line == THIS_DEVICE_MARKER => self.open_hunk(line),
            Place::Outside => {
                self.clean.push_str(line);
                self.this_device_line += 1;
                self.other_device_line += 1;
            }
            Place::ThisDeviceSide | Place::OtherDeviceSide => self.push_hunk_line(line),
        }
    }

    fn open_hunk(&mut self, marker: &str) {
        self.flush_clean();
        self.place = Place::ThisDeviceSide;
        self.written = marker.to_owned();
        self.nesting = 0;
        self.hunk_start = (self.this_device_line, self.other_device_line);
    }

    fn push_hunk_line(&mut self, line: &str) {
        self.written.push_str(line);
        let outermost = self.nesting == 0;
        match line {
            SEPARATOR_MARKER if outermost && self.place == Place::ThisDeviceSide => {
                self.place = Place::OtherDeviceSide;
                return;
            }
            OTHER_DEVICE_MARKER if outermost && self.place == Place::OtherDeviceSide => {
                return self.close_hunk();
            }
            THIS_DEVICE_MARKER => self.nesting += 1,
            OTHER_DEVICE_MARKER => self.nesting = self.nesting.saturating_sub(1),
            _ => {}
        }
        if self.place == Place::ThisDeviceSide {
            self.this_device.push_str(line);
            self.this_device_line += 1;
        } else {
            self.other_device.push_str(line);
            self.other_device_line += 1;
        }
    }

    fn close_hunk(&mut self) {
        let (this_start, other_start) = self.hunk_start;
        self.segments.push(Segment::Conflict(ConflictHunk {
            base: String::new(),
            this_device: std::mem::take(&mut self.this_device),
            other_device: std::mem::take(&mut self.other_device),
            base_lines: 0..0,
            this_device_lines: this_start..self.this_device_line,
            other_device_lines: other_start..self.other_device_line,
        }));
        self.written.clear();
        self.place = Place::Outside;
    }

    fn flush_clean(&mut self) {
        if !self.clean.is_empty() {
            self.segments
                .push(Segment::Clean(std::mem::take(&mut self.clean)));
        }
    }

    fn finish(mut self) -> Vec<Segment> {
        if self.place != Place::Outside {
            let unclosed = std::mem::take(&mut self.written);
            self.clean.push_str(&unclosed);
        }
        self.flush_clean();
        self.segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ConflictedFile {
        ConflictedFile {
            path: PathBuf::from("note.md"),
            segments: vec![
                Segment::Clean("# Title\n".into()),
                Segment::Conflict(ConflictHunk {
                    base: "b\n".into(),
                    this_device: "laptop\n".into(),
                    other_device: "phone".into(),
                    base_lines: 1..2,
                    this_device_lines: 1..2,
                    other_device_lines: 1..2,
                }),
            ],
        }
    }

    #[test]
    fn marked_text_shows_both_versions_with_ranges() {
        let marked = sample().marked_text();
        assert_eq!(
            marked.text,
            "# Title\n<<<<<<< this device\nlaptop\n=======\nphone\n>>>>>>> other device\n"
        );
        let hunk = &marked.hunks[0];
        assert_eq!(&marked.text[hunk.this_device.clone()], "laptop\n");
        assert_eq!(&marked.text[hunk.other_device.clone()], "phone");
        assert_eq!(hunk.whole.end, marked.text.len());
    }

    #[test]
    fn each_resolution_produces_expected_text() {
        let file = sample();
        let cases = [
            (Resolution::ThisDevice, "# Title\nlaptop\n"),
            (Resolution::OtherDevice, "# Title\nphone"),
            (Resolution::Both, "# Title\nlaptop\nphone"),
            (Resolution::Merged("mixed\n".into()), "# Title\nmixed\n"),
        ];
        for (resolution, expected) in cases {
            assert_eq!(file.resolve(&[resolution]).unwrap(), expected);
        }
    }

    #[test]
    fn wrong_number_of_resolutions_is_rejected() {
        assert!(matches!(
            sample().resolve(&[]),
            Err(SyncError::InvalidResolution(_))
        ));
    }

    #[test]
    fn marked_text_reads_back_into_the_same_file() {
        let file = sample();
        let marked = file.marked_text().text;
        let read = ConflictedFile::from_marked_text(file.path.clone(), &marked, Some(&file));
        let hunk = read.hunks().next().unwrap();
        assert_eq!(hunk.base, "b\n");
        assert_eq!(hunk.this_device, "laptop\n");
        assert_eq!(hunk.other_device, "phone\n");
        assert_eq!(hunk.this_device_lines, 1..2);
        assert_eq!(read.segments[0], Segment::Clean("# Title\n".into()));
    }

    #[test]
    fn edits_around_the_markers_are_kept_when_reading_back() {
        let file = sample();
        let edited = format!("{}added below\n", file.marked_text().text)
            .replace("laptop\n", "laptop, edited\n");
        let read = ConflictedFile::from_marked_text(file.path.clone(), &edited, Some(&file));
        assert_eq!(
            read.resolve(&[Resolution::ThisDevice]).unwrap(),
            "# Title\nlaptop, edited\nadded below\n"
        );
    }

    #[test]
    fn nested_markers_stay_inside_their_hunk() {
        let text = "top\n<<<<<<< this device\n<<<<<<< this device\na\n=======\nb\n>>>>>>> other device\n=======\nc\n>>>>>>> other device\n";
        let read = ConflictedFile::from_marked_text(PathBuf::from("n.md"), text, None);
        assert_eq!(read.hunk_count(), 1);
        let hunk = read.hunks().next().unwrap();
        assert_eq!(
            hunk.this_device,
            "<<<<<<< this device\na\n=======\nb\n>>>>>>> other device\n"
        );
        assert_eq!(hunk.other_device, "c\n");
        assert_eq!(hunk.base, "");
    }

    #[test]
    fn a_hunk_without_its_closing_marker_is_clean_text() {
        let text = "a\n<<<<<<< this device\nb\n=======\nc\n";
        let read = ConflictedFile::from_marked_text(PathBuf::from("n.md"), text, None);
        assert_eq!(read.hunk_count(), 0);
        assert_eq!(read.resolve(&[]).unwrap(), text);
    }
}
