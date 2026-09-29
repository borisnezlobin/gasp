//! Exporting a note as the desktop does: a web page in the website's
//! style, or a PDF laid out by Typst with footnotes at the foot of the page
//! that cites them. The phone shares or prints the file.

use std::path::Path;

use editor_export::html::{HtmlOptions, export_html, standalone_page};
use editor_export::pdf::{PdfOptions, export_pdf, fonts_for};

use crate::vault::{VaultError, VaultFolder};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ExportedFile {
    /// Such as `Waves.pdf`.
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[uniffi::export]
impl VaultFolder {
    /// `text`, the note at `path`, as a page in the website's style.
    pub fn export_html(&self, path: String, text: String) -> Result<ExportedFile, VaultError> {
        let note = self.note_path(&path)?;
        let export = export_html(
            &text,
            Some(&note),
            Some(&self.root),
            &HtmlOptions::default(),
        );
        Ok(ExportedFile {
            file_name: format!("{}.html", stem(&note)),
            bytes: standalone_page(&export).into_bytes(),
        })
    }

    /// `text`, the note at `path`, as a PDF with the owner's PDF settings.
    pub fn export_pdf(&self, path: String, text: String) -> Result<ExportedFile, VaultError> {
        let note = self.note_path(&path)?;
        let options = PdfOptions::default();
        let fonts = fonts_for(&options);
        let export = export_pdf(&text, Some(&note), Some(&self.root), &options, &fonts).map_err(
            |error| VaultError::Refused {
                message: error.to_string(),
            },
        )?;
        editor_export::pdf::evict_memory(0);
        Ok(ExportedFile {
            file_name: format!("{}.pdf", stem(&note)),
            bytes: export.pdf,
        })
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::vault::tests::vault_with;

    #[test]
    fn a_note_exports_as_a_page_and_a_pdf() {
        let (_dir, vault) = vault_with(&[("Waves.md", "")]);
        let text = "# Waves\n\nA note[^1].\n\n[^1]: Cited.\n";
        let page = vault.export_html("Waves.md".into(), text.into()).unwrap();
        assert_eq!(page.file_name, "Waves.html");
        assert!(String::from_utf8(page.bytes).unwrap().contains("<article"));
        let pdf = vault.export_pdf("Waves.md".into(), text.into()).unwrap();
        assert!(pdf.bytes.starts_with(b"%PDF"));
    }
}
