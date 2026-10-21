//! Encrypted notes through the connector. Each note's header says whether
//! Claude may read it: never, only while it's unlocked in the app (which
//! then hands over the editor's text), or always, with a key the app kept
//! for Claude (see [`crate::claude_keys`]). Whatever the access, a write
//! never replaces an encrypted note with plain text: it goes through the
//! app, which seals on save, or is sealed here with the kept key.

use std::io::Read;
use std::path::Path;

use gasp_crypt::{ClaudeAccess, NoteOptions, Plain, is_encrypted, peek};
use gasp_vault::encryption::Encryption;

use crate::bridge::Request;
use crate::context::Context;
use crate::paths::VaultPath;
use crate::tool::ToolError;
use crate::tools::files::{Written, app_error, write_file};

/// How much of a file [`is_encrypted_file`] reads: the notice and the
/// marker fit well within it.
const HEADER_BYTES: u64 = 512;

/// A note whose file is encrypted.
pub struct EncryptedNote {
    raw: String,
    access: ClaudeAccess,
}

impl EncryptedNote {
    /// The note at `note`, if its file exists and is encrypted. A header
    /// that doesn't read counts as never letting Claude in.
    pub fn at(note: &VaultPath) -> Result<Option<EncryptedNote>, ToolError> {
        let raw = match std::fs::read_to_string(&note.absolute) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ToolError::io(&note.relative, &error)),
        };
        if !is_encrypted(&raw) {
            return Ok(None);
        }
        let access = peek(&raw).map_or(ClaudeAccess::Never, |header| header.options.claude);
        Ok(Some(EncryptedNote { raw, access }))
    }
}

/// An encrypted note's text, and where it came from: `"app"` for the
/// editor's, `"disk"` for the file's, opened here.
pub struct Opened {
    pub text: String,
    pub source: &'static str,
}

/// Whether the app may hand Claude the text of a note it has unlocked,
/// given the options the note was sealed with. The app answers
/// [`Request::Unlocked`] with this, reading the options from its
/// encryption session rather than the file, whose header could be edited.
pub fn app_may_share(options: NoteOptions) -> bool {
    options.claude != ClaudeAccess::Never
}

/// Whether the file at `path` is an encrypted note, reading only its start.
pub fn is_encrypted_file(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut start = Vec::new();
    if file.take(HEADER_BYTES).read_to_end(&mut start).is_err() {
        return false;
    }
    is_encrypted(&String::from_utf8_lossy(&start))
}

/// The text of the encrypted note at `note`, if its owner lets Claude
/// read it now.
pub fn read(
    context: &Context,
    note: &VaultPath,
    encrypted: &EncryptedNote,
) -> Result<Opened, ToolError> {
    if encrypted.access == ClaudeAccess::Never {
        return Err(not_shared(note));
    }
    if let Some(text) = text_in_app(context, note)? {
        return Ok(Opened {
            text,
            source: "app",
        });
    }
    if encrypted.access != ClaudeAccess::Always {
        return Err(locked_in_app(note));
    }
    let (_, plain) = open_with_claude_key(context, note, encrypted)?;
    Ok(Opened {
        text: plain.body.clone(),
        source: "disk",
    })
}

/// Changes the encrypted note at `note` with `change`, which sees its
/// text: through the app while it has the note unlocked, else sealed
/// with the key kept for Claude when its owner lets Claude in always.
pub fn change(
    context: &Context,
    note: &VaultPath,
    encrypted: &EncryptedNote,
    change: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Written, ToolError> {
    if encrypted.access == ClaudeAccess::Never {
        return Err(not_shared(note));
    }
    if let Some(old) = text_in_app(context, note)? {
        let new = change(&old).map_err(ToolError::new)?;
        let request = Request::Edit {
            path: note.relative.clone(),
            old,
            new,
        };
        context.app().call(request).map_err(app_error)?;
        return Ok(Written::InApp);
    }
    if encrypted.access != ClaudeAccess::Always {
        return Err(locked_in_app(note));
    }
    let (encryption, plain) = open_with_claude_key(context, note, encrypted)?;
    let new = change(&plain.body).map_err(ToolError::new)?;
    let sealed = encryption
        .seal(&note.relative, &new)
        .ok_or_else(|| key_doesnt_open(note))?;
    write_file(note, &sealed)?;
    Ok(Written::OnDisk)
}

fn text_in_app(context: &Context, note: &VaultPath) -> Result<Option<String>, ToolError> {
    context
        .app()
        .unlocked_text(&note.relative)
        .map_err(app_error)
}

/// Opens the note with the key kept for Claude, and checks the options it
/// was sealed with still let Claude in always.
fn open_with_claude_key(
    context: &Context,
    note: &VaultPath,
    encrypted: &EncryptedNote,
) -> Result<(Encryption, Plain), ToolError> {
    let mut encryption = Encryption::load(context.root());
    let key = encryption
        .key_id_for(&encrypted.raw)
        .and_then(|id| context.claude_keys().key(&id))
        .ok_or_else(|| no_key(note))?;
    let plain = encryption
        .unlock_note_with_key(&note.relative, &encrypted.raw, key)
        .map_err(|_| key_doesnt_open(note))?;
    let sealed_access = encryption
        .options(&note.relative)
        .map(|options| options.claude);
    if sealed_access != Some(ClaudeAccess::Always) {
        return Err(not_shared(note));
    }
    Ok((encryption, plain))
}

fn not_shared(note: &VaultPath) -> ToolError {
    ToolError::new(format!(
        "{} is encrypted and its owner hasn't let Claude read it.",
        note.relative
    ))
}

fn locked_in_app(note: &VaultPath) -> ToolError {
    ToolError::new(format!(
        "{} is encrypted, and Claude may read it only while it's unlocked in the app. \
         Ask its owner to unlock it, then try again.",
        note.relative
    ))
}

fn no_key(note: &VaultPath) -> ToolError {
    ToolError::new(format!(
        "{} is encrypted. Its owner lets Claude read it, but the key that opens it isn't \
         in this device's keychain.",
        note.relative
    ))
}

fn key_doesnt_open(note: &VaultPath) -> ToolError {
    ToolError::new(format!(
        "{} is encrypted, and the key kept for Claude doesn't open it, perhaps because its \
         password changed.",
        note.relative
    ))
}

#[cfg(test)]
mod tests;
