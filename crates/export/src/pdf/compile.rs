//! Compiling a generated Typst document to pages and PDF bytes.

use std::ops::Range;
use std::path::PathBuf;

use typst::WorldExt;
use typst::diag::SourceDiagnostic;
use typst::text::Font;
use typst_layout::PagedDocument;

use super::world::{ExportWorld, MAIN_PATH, file_id};

/// A compile error with its location in the main source, when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileDiagnostic {
    pub message: String,
    /// Byte ranges in the main source of the error and its call trace.
    pub ranges: Vec<Range<usize>>,
}

impl std::fmt::Display for CompileDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// Compiles a full Typst document whose images are served from `images`.
pub(crate) fn compile_document(
    main_text: String,
    images: &[(String, PathBuf)],
    fonts: Vec<Font>,
) -> Result<PagedDocument, Vec<CompileDiagnostic>> {
    let world = ExportWorld::new(main_text, images, fonts);
    typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| {
            errors
                .iter()
                .map(|error| diagnostic(&world, error))
                .collect()
        })
}

fn diagnostic(world: &ExportWorld, error: &SourceDiagnostic) -> CompileDiagnostic {
    let main = file_id(MAIN_PATH);
    let spans =
        std::iter::once(error.span).chain(error.trace.iter().map(|point| point.span.into()));
    let ranges = spans
        .filter(|span: &typst::syntax::DiagSpan| span.id() == Some(main))
        .filter_map(|span| world.range(span))
        .collect();
    CompileDiagnostic {
        message: error.message.to_string(),
        ranges,
    }
}
