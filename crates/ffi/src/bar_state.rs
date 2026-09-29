//! How the keyboard bar's buttons look where the cursor is: toggles that
//! are on (bold while the cursor is in bold text) show pressed, and
//! commands that wouldn't change anything there show greyed out.

use gasp_core::commands::{ShiftAvailability, active_commands, shift_availability};

use crate::document::NoteDocument;
use crate::edits;
use crate::offsets::TextRange;

/// The state of the commands a bar shows, for one selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct CommandStates {
    /// Toggles that are on here.
    pub pressed: Vec<String>,
    /// Commands that would change nothing here.
    pub unavailable: Vec<String>,
}

#[uniffi::export]
impl NoteDocument {
    /// Which commands are on and which would do nothing with `selection`.
    pub fn command_states(&self, selection: TextRange) -> CommandStates {
        let parsed = self.lock();
        let selected = edits::selection(&parsed.offsets, selection);
        let cursor = parsed.offsets.byte(selection.end);
        let pressed = active_commands(&parsed.tree, cursor)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let shifts = shift_availability(&parsed.text, &parsed.tree, &selected);
        CommandStates {
            pressed,
            unavailable: unavailable_shifts(shifts),
        }
    }
}

fn unavailable_shifts(shifts: ShiftAvailability) -> Vec<String> {
    [
        ("edit.indent", shifts.indent),
        ("edit.outdent", shifts.outdent),
    ]
    .into_iter()
    .filter(|(_, available)| !available)
    .map(|(id, _)| id.to_owned())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(text: &str, marker: &str) -> CommandStates {
        let at = text[..text.find(marker).unwrap()].encode_utf16().count() as u32;
        NoteDocument::new(text.into()).command_states(TextRange { start: at, end: at })
    }

    #[test]
    fn bold_shows_pressed_inside_bold_text() {
        let text = "plain **bold** text";
        assert!(states(text, "plain").pressed.is_empty());
        assert_eq!(states(text, "old").pressed, ["format.bold"]);
    }

    #[test]
    fn indenting_greys_out_where_it_does_nothing() {
        let text = "A paragraph.\n\n- one\n- two\n\t- nested\n";
        assert_eq!(
            states(text, "paragraph").unavailable,
            ["edit.indent", "edit.outdent"]
        );
        assert_eq!(
            states(text, "one").unavailable,
            ["edit.indent", "edit.outdent"]
        );
        assert_eq!(states(text, "two").unavailable, ["edit.outdent"]);
        assert_eq!(states(text, "nested").unavailable, ["edit.indent"]);
    }
}
