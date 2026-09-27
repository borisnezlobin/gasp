//! Which note a link points at, from the vault index alone, so hovering
//! never walks the disk.

use crate::vault_index::VaultIndex;

/// A note link split into what it names and where in the note it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteLink {
    /// The note part, decoded and without `.md`: `Folder/Note`.
    pub note: String,
    /// The heading after `#`, if any.
    pub heading: Option<String>,
}

impl NoteLink {
    /// Reads a wikilink target or a Markdown link destination. Links to
    /// files that aren't notes, such as images and PDFs, give `None`.
    pub fn parse(link: &str) -> Option<NoteLink> {
        let decoded = percent_decode(link);
        let (note, heading) = match decoded.split_once('#') {
            Some((note, heading)) => (note, Some(heading.trim()).filter(|h| !h.is_empty())),
            None => (decoded.as_str(), None),
        };
        let note = note.trim().trim_start_matches("./");
        let note = note.strip_suffix(".md").unwrap_or(note);
        let file_name = note.rsplit('/').next().unwrap_or(note);
        if has_other_extension(file_name) {
            return None;
        }
        Some(NoteLink {
            note: note.to_owned(),
            heading: heading.map(str::to_owned),
        })
    }

    /// The name a missing note would be created with.
    pub fn name(&self) -> &str {
        self.note.rsplit('/').next().unwrap_or(&self.note)
    }

    /// The link the workspace follows to open or create this note.
    pub fn target(&self) -> String {
        match &self.heading {
            Some(heading) => format!("{}#{heading}", self.note),
            None => self.note.clone(),
        }
    }
}

/// Files with a short alphanumeric extension, like `photo.png`, aren't
/// notes. A dot in a note name (`v2.0 plans`) doesn't count.
fn has_other_extension(file_name: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && (1..=4).contains(&extension.len())
            && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
            && !extension.chars().all(|ch| ch.is_ascii_digit())
    })
}

/// The vault path of the note `link` names, seen from the note at
/// `from` (vault-relative): a path next to that note first, then the
/// index's own lookup by path and by name. `None` for a note that doesn't
/// exist; an empty note part is the note itself.
pub fn resolve(index: &VaultIndex, from: Option<&str>, link: &NoteLink) -> Option<String> {
    if link.note.is_empty() {
        return from.map(str::to_owned);
    }
    let folder = from
        .and_then(|from| from.rsplit_once('/'))
        .map(|(folder, _)| folder);
    let relative = folder.and_then(|folder| normalize(&format!("{folder}/{}", link.note)));
    relative
        .and_then(|path| index.find_note(&path))
        .or_else(|| index.find_note(link.note.trim_start_matches('/')))
        .map(|note| note.path.clone())
}

/// Resolves `.` and `..` in a `/`-separated path. `None` when it climbs
/// out of the vault.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// Decodes `%20` and friends, leaving malformed escapes as they are.
fn percent_decode(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault_index::NoteScan;

    fn index() -> VaultIndex {
        let mut index = VaultIndex::default();
        for path in [
            "Wave Packets.md",
            "Physics/Waves.md",
            "Physics/Deep/Notes.md",
            "Notes.md",
        ] {
            index.upsert(NoteScan {
                path: path.into(),
                tags: Vec::new(),
            });
        }
        index
    }

    fn found(from: Option<&str>, link: &str) -> Option<String> {
        resolve(&index(), from, &NoteLink::parse(link)?)
    }

    #[test]
    fn links_split_into_note_and_heading() {
        let link = NoteLink::parse("Wave%20Packets.md#Group velocity").unwrap();
        assert_eq!(link.note, "Wave Packets");
        assert_eq!(link.heading.as_deref(), Some("Group velocity"));
        assert_eq!(link.target(), "Wave Packets#Group velocity");
        assert_eq!(NoteLink::parse("#Intro").unwrap().note, "");
        assert_eq!(NoteLink::parse("v2.0 plans").unwrap().name(), "v2.0 plans");
        assert_eq!(NoteLink::parse("images/photo.png"), None);
        assert_eq!(NoteLink::parse("paper.pdf#page=3"), None);
    }

    #[test]
    fn notes_resolve_by_name_path_and_relative_path() {
        assert_eq!(
            found(None, "wave packets").as_deref(),
            Some("Wave Packets.md")
        );
        assert_eq!(found(None, "Waves").as_deref(), Some("Physics/Waves.md"));
        let from = Some("Physics/Deep/Notes.md");
        assert_eq!(
            found(from, "../Waves.md").as_deref(),
            Some("Physics/Waves.md")
        );
        assert_eq!(
            found(from, "./Notes.md").as_deref(),
            Some("Physics/Deep/Notes.md")
        );
        assert_eq!(found(None, "Notes").as_deref(), Some("Notes.md"));
        assert_eq!(found(from, "#Heading").as_deref(), from);
        assert_eq!(found(None, "Missing"), None);
    }
}
