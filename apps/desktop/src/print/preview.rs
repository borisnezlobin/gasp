//! Laying out the note for the print preview, off the main thread: the
//! PDF that's printed or saved, and each page drawn at the width the
//! preview shows it.

use std::path::PathBuf;
use std::sync::Arc;

use gasp_export::pdf::{
    PageImage, PdfError, compile_note, evict_memory, fonts_for, page_size_pt, render_pages,
    typst_source, write_pdf,
};
use gpui::RenderImage;

use super::settings::PrintSettings;

/// Memoized Typst results older than this many layouts are freed.
const TYPST_CACHE_LAYOUTS: usize = 4;

/// What to lay out.
#[derive(Clone)]
pub struct PreviewJob {
    pub text: Arc<str>,
    pub note_path: Option<PathBuf>,
    pub vault_root: Option<PathBuf>,
    pub settings: PrintSettings,
    /// How wide each page is drawn, in device pixels.
    pub page_width_px: u32,
}

/// One drawn page.
pub struct PreviewPage {
    pub image: Arc<RenderImage>,
    /// Height over width.
    pub aspect: f32,
}

/// A laid-out note: its PDF and its pages.
pub struct Preview {
    pub pdf: Arc<Vec<u8>>,
    pub pages: Vec<PreviewPage>,
    pub settings: PrintSettings,
}

/// Why the note couldn't be laid out, in words for the preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutFailure {
    pub messages: Vec<String>,
}

impl LayoutFailure {
    fn from_error(error: PdfError) -> LayoutFailure {
        let messages = match error {
            PdfError::Compile(errors) => errors
                .into_iter()
                .map(|error| sentence(&error.message))
                .collect(),
            PdfError::Pdf(message) => vec![sentence(&message)],
        };
        LayoutFailure { messages }
    }

    /// Everything on one line, for a status message.
    pub fn summary(&self) -> String {
        match self.messages.first() {
            Some(first) => first.clone(),
            None => "Typst stopped without saying why.".to_owned(),
        }
    }
}

/// `message` with a capital and a full stop, as the app's messages read.
pub fn sentence(message: &str) -> String {
    let message = message.trim();
    let mut chars = message.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(['.', '!', '?', '…']) {
        out.push('.');
    }
    out
}

/// Lays out the note, writes its PDF and draws its pages.
pub fn build_preview(job: PreviewJob) -> Result<Preview, LayoutFailure> {
    let options = job.settings.options();
    let fonts = fonts_for(&options);
    let note = typst_source(
        &job.text,
        job.note_path.as_deref(),
        job.vault_root.as_deref(),
        &options,
    );
    let compiled = compile_note(&note, &fonts);
    let result = compiled.and_then(|compiled| {
        let pdf = write_pdf(&compiled.document)?;
        let aspects: Vec<f32> = compiled
            .document
            .pages()
            .iter()
            .map(|page| {
                let (width, height) = page_size_pt(page);
                (height / width.max(1.)) as f32
            })
            .collect();
        let images = render_pages(&compiled.document, job.page_width_px);
        Ok((pdf, aspects, images))
    });
    evict_memory(TYPST_CACHE_LAYOUTS);
    let (pdf, aspects, images) = result.map_err(LayoutFailure::from_error)?;
    let pages = images
        .into_iter()
        .zip(aspects)
        .map(|(image, aspect)| PreviewPage {
            image: Arc::new(render_image(image)),
            aspect,
        })
        .collect();
    Ok(Preview {
        pdf: Arc::new(pdf),
        pages,
        settings: job.settings,
    })
}

fn render_image(page: PageImage) -> RenderImage {
    let pixels = image::RgbaImage::from_raw(page.width, page.height, page.rgba)
        .unwrap_or_else(|| image::RgbaImage::new(1, 1));
    crate::images::render_image(pixels)
}

/// The page the preview is showing: the one under the middle of the
/// visible area. `scrolled` is how far the column has scrolled down,
/// `stride` one page plus the gap after it, all in the same units.
pub fn current_page(scrolled: f32, visible: f32, stride: f32, pages: usize) -> usize {
    if pages == 0 {
        return 0;
    }
    if stride <= 0. {
        return 1;
    }
    let middle = scrolled.max(0.) + visible / 2.;
    let index = (middle / stride).floor().max(0.) as usize;
    index.min(pages - 1) + 1
}

/// "Page 2 of 5".
pub fn page_indicator(current: usize, pages: usize) -> String {
    format!("Page {current} of {pages}")
}

/// How tall the preview is: as tall as the window leaves room for,
/// within `min` and `max`.
pub fn preview_height(available: f32, min: f32, max: f32) -> f32 {
    available.clamp(min, max.max(min))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_middle_of_the_view_picks_the_page() {
        assert_eq!(current_page(0., 500., 600., 3), 1);
        assert_eq!(current_page(400., 500., 600., 3), 2);
        assert_eq!(current_page(5000., 500., 600., 3), 3);
        assert_eq!(current_page(0., 500., 600., 0), 0);
        assert_eq!(page_indicator(2, 3), "Page 2 of 3");
    }

    #[test]
    fn the_preview_fits_the_window() {
        assert_eq!(preview_height(900., 300., 640.), 640.);
        assert_eq!(preview_height(100., 300., 640.), 300.);
        assert_eq!(preview_height(500., 300., 640.), 500.);
    }

    #[test]
    fn typst_messages_read_as_sentences() {
        assert_eq!(sentence("unknown variable: foo"), "Unknown variable: foo.");
        assert_eq!(sentence("Done."), "Done.");
        assert_eq!(sentence("  "), "");
    }

    #[test]
    fn a_short_note_lays_out_as_pages() {
        let preview = build_preview(PreviewJob {
            text: "# Title\n\nSome text with $x^2$.".into(),
            note_path: None,
            vault_root: None,
            settings: PrintSettings::default(),
            page_width_px: 60,
        })
        .map_err(|failure| failure.summary())
        .unwrap();
        assert_eq!(preview.pages.len(), 1);
        assert!(preview.pdf.starts_with(b"%PDF"));
        assert!((preview.pages[0].aspect - 297. / 210.).abs() < 0.01);
        let size = preview.pages[0].image.size(0);
        assert_eq!(size.width.0, 60);
    }
}
