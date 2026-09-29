//! The notes a sync left waiting for a person, as the phone's resolver
//! shows them, and settling one with the same resolutions the desktop uses.

use std::path::Path;

use editor_sync::phase::file_label;
use editor_sync::{ConflictHunk, ConflictedFile, Resolution, Segment};

use crate::sync::{SyncOutcome, VaultSync, slash_path};
use crate::vault::VaultError;

/// Lines of unchanged text shown above each place, for orientation, as on
/// the desktop.
const CONTEXT_LINES: usize = 2;

/// One place in a note where both devices changed the same lines.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ConflictPlace {
    /// The last lines both versions share above it.
    pub context: String,
    pub this_device: String,
    pub other_device: String,
}

/// A note waiting for a person.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ConflictNote {
    pub path: String,
    pub title: String,
    /// The note as shown. Resolving refuses if it has changed since.
    pub version: String,
    pub places: Vec<ConflictPlace>,
}

/// What to keep in one place.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PlaceChoice {
    ThisDevice,
    OtherDevice,
    /// This device's lines, then the other device's.
    Both,
    /// Text the person wrote.
    Edited {
        text: String,
    },
}

fn note(file: &ConflictedFile) -> ConflictNote {
    let mut places = Vec::new();
    let mut previous_clean: Option<&str> = None;
    for segment in &file.segments {
        match segment {
            Segment::Clean(text) => previous_clean = Some(text),
            Segment::Conflict(hunk) => places.push(ConflictPlace {
                context: previous_clean.take().map(last_lines).unwrap_or_default(),
                this_device: hunk.this_device.clone(),
                other_device: hunk.other_device.clone(),
            }),
        }
    }
    ConflictNote {
        path: slash_path(&file.path),
        title: file_label(&file.path),
        version: file.marked_text().text,
        places,
    }
}

fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end_matches('\n').lines().collect();
    let start = lines.len().saturating_sub(CONTEXT_LINES);
    lines[start..].join("\n")
}

fn resolution(choice: PlaceChoice, hunk: &ConflictHunk) -> Resolution {
    match choice {
        PlaceChoice::ThisDevice => Resolution::ThisDevice,
        PlaceChoice::OtherDevice => Resolution::OtherDevice,
        PlaceChoice::Both => Resolution::Both,
        PlaceChoice::Edited { text } => Resolution::Merged(ending_like(text, hunk)),
    }
}

/// Edited text ends its last line when the lines it replaces did, so the
/// text after the place stays on its own line.
fn ending_like(mut text: String, hunk: &ConflictHunk) -> String {
    let lines_ended = hunk.this_device.ends_with('\n') || hunk.other_device.ends_with('\n');
    if lines_ended && !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn refused(message: &str) -> VaultError {
    VaultError::Refused {
        message: message.to_owned(),
    }
}

#[uniffi::export]
impl VaultSync {
    /// Every note waiting for a person, read from disk.
    pub fn conflicts(&self) -> Vec<ConflictNote> {
        let guard = self.open_clone();
        let Some(clone) = guard.as_ref() else {
            return Vec::new();
        };
        let files = clone.vault.conflicts().unwrap_or_default();
        files.iter().map(note).collect()
    }

    /// Settles the note at `path` with one choice per place, then syncs it.
    /// `version` is the note as it was shown; if it has changed since,
    /// nothing is written.
    pub fn resolve(
        &self,
        path: String,
        version: String,
        choices: Vec<PlaceChoice>,
    ) -> Result<SyncOutcome, VaultError> {
        self.write_resolution(&path, &version, choices)?;
        self.conflicts_resolved();
        Ok(self.drive())
    }
}

impl VaultSync {
    fn write_resolution(
        &self,
        path: &str,
        version: &str,
        choices: Vec<PlaceChoice>,
    ) -> Result<(), VaultError> {
        let guard = self.open_clone();
        let clone = guard
            .as_ref()
            .ok_or_else(|| refused("Sync isn't set up for this vault."))?;
        let waiting = clone
            .vault
            .conflicts()
            .map_err(|error| refused(&error.to_string()))?;
        let file = waiting
            .into_iter()
            .find(|file| file.path == Path::new(path))
            .ok_or_else(|| refused("This note isn't waiting any more."))?;
        if file.marked_text().text != version {
            return Err(refused(
                "This note changed since it was shown. Look at it again.",
            ));
        }
        if choices.len() != file.hunk_count() {
            return Err(refused("Pick what to keep in every place first."));
        }
        let resolutions: Vec<Resolution> = choices
            .into_iter()
            .zip(file.hunks())
            .map(|(choice, hunk)| resolution(choice, hunk))
            .collect();
        clone
            .vault
            .resolve(&file, &resolutions)
            .map_err(|error| refused(&error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(this_device: &str, other_device: &str) -> ConflictHunk {
        ConflictHunk {
            base: String::new(),
            this_device: this_device.into(),
            other_device: other_device.into(),
            base_lines: 0..0,
            this_device_lines: 0..1,
            other_device_lines: 0..1,
        }
    }

    #[test]
    fn places_carry_two_lines_of_context_and_the_version_shown() {
        let file = ConflictedFile {
            path: "Daily/Today.md".into(),
            segments: vec![
                Segment::Clean("one\ntwo\nthree\n".into()),
                Segment::Conflict(hunk("phone\n", "laptop\n")),
            ],
        };
        let note = note(&file);
        assert_eq!(note.title, "Today");
        assert_eq!(note.path, "Daily/Today.md");
        assert_eq!(note.places[0].context, "two\nthree");
        assert_eq!(note.places[0].this_device, "phone\n");
        assert_eq!(note.version, file.marked_text().text);
    }

    #[test]
    fn edited_text_keeps_its_line_ending() {
        let place = hunk("phone\n", "laptop\n");
        let edited = resolution(
            PlaceChoice::Edited {
                text: "both of us".into(),
            },
            &place,
        );
        assert_eq!(edited, Resolution::Merged("both of us\n".into()));
        let kept = resolution(PlaceChoice::Both, &place);
        assert_eq!(kept, Resolution::Both);
    }
}
