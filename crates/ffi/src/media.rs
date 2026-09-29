//! What the phone draws besides text: where an embedded image's file is,
//! a footnote's text for its popover, and colour tokens the palette
//! doesn't carry, such as the grammar underlines'.

use std::path::Path;

use editor_core::footnotes::{find_def, parse_footnotes};
use editor_vault::link_update::parent_dir;

use crate::document::NoteDocument;
use crate::theme::{ThemeColor, token_color};
use crate::vault::VaultFolder;

/// A colour token in both modes.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ThemedColor {
    pub light: ThemeColor,
    pub dark: ThemeColor,
}

#[uniffi::export]
impl VaultFolder {
    /// The file an image in the note at `note_path` shows, as a full path:
    /// `target` beside the note, in its `images` folder, or anywhere in
    /// the vault by its name, as Obsidian finds `![[name.png]]`. `None`
    /// for a web address or a file that isn't there.
    pub fn image_file(&self, note_path: String, target: String) -> Option<String> {
        if target.contains("://") {
            return None;
        }
        let target = percent_decoded(target.split('#').next().unwrap_or_default());
        let folder = self.root.join(parent_dir(&note_path));
        let file_name = Path::new(&target).file_name()?;
        let nearby = [folder.join(&target), folder.join("images").join(file_name)];
        if let Some(found) = nearby.into_iter().find(|path| path.is_file()) {
            return Some(found.to_string_lossy().into_owned());
        }
        let found = self
            .built_index()
            .as_ref()?
            .resolve_embed(parent_dir(&note_path), &target)?;
        let path = self.root.join(found);
        path.is_file().then(|| path.to_string_lossy().into_owned())
    }

    /// Where the vault is on disk, which names its caches on this device.
    pub fn location(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    /// The colour token `name` (such as `color.flag-spelling`) in light
    /// and dark mode, from the vault's theme.
    pub fn theme_color(&self, name: String) -> ThemedColor {
        let config = self.config();
        ThemedColor {
            light: token_color(&config, &name, false),
            dark: token_color(&config, &name, true),
        }
    }
}

#[uniffi::export]
impl NoteDocument {
    /// The text of the footnote labelled `label`, its lines joined, for
    /// showing when its reference is tapped.
    pub fn footnote_text(&self, label: String) -> Option<String> {
        let text = self.text();
        let parsed = parse_footnotes(&text);
        let definition = find_def(&parsed.defs, &label)?;
        let lines: Vec<&str> = definition
            .body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        Some(lines.join(" "))
    }
}

/// `Pasted%20image.png` as the file it names. Anything that isn't a valid
/// escape stays as written.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                at += 3;
            }
            None => {
                decoded.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests::vault_with;

    #[test]
    fn images_are_found_beside_the_note_and_anywhere_by_name() {
        let (dir, vault) = vault_with(&[
            ("Maths/Lemma.md", ""),
            ("Maths/images/plot one.png", "png"),
            ("Elsewhere/figure.png", "png"),
        ]);
        let find = |target: &str| vault.image_file("Maths/Lemma.md".into(), target.into());
        let beside = dir.path().join("Maths/images/plot one.png");
        assert_eq!(
            find("plot%20one.png"),
            Some(beside.to_string_lossy().into())
        );
        assert!(
            find("figure.png")
                .unwrap()
                .ends_with("Elsewhere/figure.png")
        );
        assert_eq!(find("missing.png"), None);
        assert_eq!(find("https://a.org/x.png"), None);
    }

    #[test]
    fn a_footnote_reads_as_one_line() {
        let document = NoteDocument::new("Text[^1].\n\n[^1]: First line\n    second.\n".into());
        assert_eq!(
            document.footnote_text("1".into()),
            Some("First line second.".into())
        );
        assert_eq!(document.footnote_text("2".into()), None);
    }

    #[test]
    fn theme_colours_come_in_both_modes() {
        let (_dir, vault) = vault_with(&[]);
        let spelling = vault.theme_color("color.flag-spelling".into());
        assert!(spelling.light.red > 0.7 && spelling.dark.red > 0.8);
        assert!(spelling.light.green < spelling.dark.green);
    }
}
