//! Where the reader was in each note, kept in `.gasp/device.toml`
//! (which never syncs) so a note opens where it was left after a relaunch:
//! the cursor, the line at the top of the view and the folded headings.

use editor_config::device::{NotePosition, move_positions, remember_position};
use editor_config::loader::CONFIG_DIR;
use editor_config::store::save;

use crate::document::NoteDocument;
use crate::vault::{VaultError, VaultFolder};

/// Where the reader was in a note, in UTF-16 offsets of its text now.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ReadingPosition {
    pub cursor: u32,
    /// Where the line at the top of the view starts.
    pub top: u32,
    /// The folded headings' lines.
    pub folded_lines: Vec<u32>,
}

#[uniffi::export]
impl VaultFolder {
    /// Where the reader was in the note at `path`, if this device has
    /// shown it before.
    pub fn reading_position(
        &self,
        path: String,
        document: std::sync::Arc<NoteDocument>,
    ) -> Option<ReadingPosition> {
        let saved = self
            .config()
            .device
            .positions
            .iter()
            .find(|position| position.path == path)
            .cloned()?;
        let parsed = document.lock();
        let clamp = |byte: usize| parsed.offsets.utf16(byte.min(parsed.text.len()));
        Some(ReadingPosition {
            cursor: clamp(saved.cursor),
            top: clamp(saved.top),
            folded_lines: saved.folds.iter().map(|&line| line as u32).collect(),
        })
    }

    /// Remembers where the reader is in the note at `path`, as the newest.
    pub fn save_reading_position(
        &self,
        path: String,
        document: std::sync::Arc<NoteDocument>,
        position: ReadingPosition,
    ) -> Result<(), VaultError> {
        let entry = {
            let parsed = document.lock();
            NotePosition {
                cursor: parsed.offsets.byte(position.cursor),
                top: parsed.offsets.byte(position.top),
                folds: position
                    .folded_lines
                    .iter()
                    .map(|&line| line as usize)
                    .collect(),
                path,
            }
        };
        let mut config = self.config();
        remember_position(&mut config.device.positions, entry);
        let file = self.root.join(CONFIG_DIR).join("device.toml");
        Ok(save(&file, &config.device.to_toml())?)
    }

    /// Follows a note, or a folder of notes, that was renamed or moved.
    pub fn move_reading_position(&self, from: String, to: String) {
        move_positions(&mut self.config().device.positions, &from, &to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests::vault_with;

    #[test]
    fn a_note_opens_where_it_was_left_after_a_relaunch() {
        let (dir, vault) = vault_with(&[("A.md", "é line\n# Head\nbody\n")]);
        let text = vault.read_note("A.md".into()).unwrap();
        let document = vault.document(text.clone());
        let position = ReadingPosition {
            cursor: 9,
            top: 7,
            folded_lines: vec![1],
        };
        vault
            .save_reading_position("A.md".into(), document, position.clone())
            .unwrap();
        let reopened = VaultFolder::open(dir.path().to_string_lossy().into_owned()).unwrap();
        let document = reopened.document(text);
        assert_eq!(
            reopened.reading_position("A.md".into(), document.clone()),
            Some(position)
        );
        assert_eq!(reopened.reading_position("B.md".into(), document), None);
    }
}
