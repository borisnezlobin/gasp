use std::ops::Range;
use std::path::PathBuf;

use crate::error::{SyncError, SyncResult};

pub(crate) const THIS_DEVICE_MARKER: &str = "<<<<<<< this device\n";
pub(crate) const SEPARATOR_MARKER: &str = "=======\n";
pub(crate) const OTHER_DEVICE_MARKER: &str = ">>>>>>> other device\n";

/// One place where both devices changed the same lines differently.
///
/// Line ranges are zero-based line numbers in each version of the file.
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
}
