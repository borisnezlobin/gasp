//! PDF export through Typst.
//!
//! A note is converted to Typst markup ([`markdown_to_typst`]), wrapped in
//! the page template with the export settings ([`typst_source`]) and compiled
//! to PDF ([`export_pdf`]). Equations Typst cannot typeset are replaced by
//! their LaTeX source instead of failing the export.

mod compile;
mod convert;
mod escape;
mod images;
mod preprocess;
mod world;

use std::collections::BTreeSet;
use std::ops::Range;
use std::path::Path;

use typst::text::Font;
use typst_layout::PagedDocument;

pub use compile::CompileDiagnostic;
pub use convert::{ConvertOptions, MathSite, TypstBody, markdown_to_typst};
pub use world::{load_fonts, warm_up};

/// Page margins in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

/// Export settings. The defaults are the owner's PDF Export Plus settings.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfOptions {
    /// `A4`, `A3`, `A5`, `Letter`, `Legal` or `Tabloid`.
    pub page_size: String,
    pub margins: Margins,
    pub show_page_numbers: bool,
    pub print_background: bool,
    pub footnote_size_pt: f64,
    /// Adds the note title (or its frontmatter `title`) as a heading.
    pub include_title: bool,
    /// Lines spanned by the drop cap; 0 turns it off.
    pub drop_cap_lines: u32,
    /// Body font families in order of preference. The last entry should be
    /// an embedded font so output does not depend on the machine.
    pub font_family: Vec<String>,
    pub mono_font_family: Vec<String>,
    pub font_size_pt: f64,
    pub line_height: f64,
    /// First-line indent of paragraphs that follow a paragraph, in inches.
    pub paragraph_indent_in: f64,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            page_size: "A4".to_owned(),
            margins: Margins {
                top: 18.0,
                bottom: 16.0,
                left: 12.0,
                right: 12.0,
            },
            show_page_numbers: true,
            print_background: false,
            footnote_size_pt: 8.5,
            include_title: true,
            drop_cap_lines: 3,
            font_family: vec!["Iowan Old Style".to_owned(), "Libertinus Serif".to_owned()],
            mono_font_family: vec!["Courier New".to_owned(), "DejaVu Sans Mono".to_owned()],
            font_size_pt: 12.0,
            line_height: 2.0,
            paragraph_indent_in: 0.5,
        }
    }
}

/// Why an export failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    Compile(Vec<CompileDiagnostic>),
    Pdf(String),
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile(errors) => {
                let messages: Vec<&str> =
                    errors.iter().map(|error| error.message.as_str()).collect();
                write!(formatter, "typst: {}", messages.join("; "))
            }
            Self::Pdf(message) => write!(formatter, "pdf: {message}"),
        }
    }
}

impl std::error::Error for PdfError {}

/// A laid-out note.
pub struct CompiledNote {
    pub document: PagedDocument,
    /// Equations that failed to typeset and are shown as LaTeX source.
    pub replaced_math: Vec<MathSite>,
}

/// A converted note: the full Typst source and its images.
#[derive(Debug, Clone)]
pub struct TypstNote {
    pub preamble: String,
    pub body: TypstBody,
}

impl TypstNote {
    /// The complete Typst document.
    pub fn source(&self) -> String {
        format!("{}{}", self.preamble, self.body.markup)
    }
}

fn paper_name(page_size: &str) -> &'static str {
    match page_size.to_ascii_lowercase().as_str() {
        "a3" => "a3",
        "a5" => "a5",
        "letter" => "us-letter",
        "legal" => "us-legal",
        "tabloid" => "us-tabloid",
        _ => "a4",
    }
}

fn string_array(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| escape::string(item)).collect();
    format!("({},)", quoted.join(", "))
}

/// The template import and settings that precede a note's body.
pub fn typst_preamble(options: &PdfOptions) -> String {
    let margins = options.margins;
    format!(
        "#import \"{template}\": *\n\
         #show: note.with(\n  paper: \"{paper}\",\n  \
         margin: (top: {top}mm, bottom: {bottom}mm, left: {left}mm, right: {right}mm),\n  \
         font: {font},\n  mono-font: {mono},\n  size: {size}pt,\n  line-height: {line_height},\n  \
         footnote-size: {footnote}pt,\n  paragraph-indent: {indent}in,\n  \
         page-numbers: {numbers},\n  print-background: {background},\n)\n",
        template = world::TEMPLATE_PATH,
        paper = paper_name(&options.page_size),
        top = margins.top,
        bottom = margins.bottom,
        left = margins.left,
        right = margins.right,
        font = string_array(&options.font_family),
        mono = string_array(&options.mono_font_family),
        size = options.font_size_pt,
        line_height = options.line_height,
        footnote = options.footnote_size_pt,
        indent = options.paragraph_indent_in,
        numbers = options.show_page_numbers,
        background = options.print_background,
    )
}

/// Converts a note to a full Typst document. `note_path` locates images and
/// gives the title; `vault_root` is the last place images are looked up.
pub fn typst_source(
    markdown: &str,
    note_path: Option<&Path>,
    vault_root: Option<&Path>,
    options: &PdfOptions,
) -> TypstNote {
    let title = options.include_title.then(|| {
        note_path
            .and_then(Path::file_stem)
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let convert = ConvertOptions {
        note_dir: note_path.and_then(Path::parent).map(Path::to_path_buf),
        vault_root: vault_root.map(Path::to_path_buf),
        title,
        drop_cap_lines: Some(options.drop_cap_lines),
    };
    TypstNote {
        preamble: typst_preamble(options),
        body: markdown_to_typst(markdown, &convert),
    }
}

/// Rounds of replacing failing equations before giving up.
const MATH_RETRIES: usize = 4;

/// Lays out a converted note. Equations that fail to typeset are replaced by
/// their source and the note is compiled again.
pub fn compile_note(note: &TypstNote, fonts: &[Font]) -> Result<CompiledNote, PdfError> {
    let mut replaced = BTreeSet::new();
    for _ in 0..=MATH_RETRIES {
        let (body, ranges) = substitute_math(&note.body, &replaced);
        let source = format!("{}{body}", note.preamble);
        match compile::compile_document(source, &note.body.images, fonts.to_vec()) {
            Ok(document) => {
                let replaced_math = replaced
                    .iter()
                    .map(|index| note.body.math[*index].clone())
                    .collect();
                return Ok(CompiledNote {
                    document,
                    replaced_math,
                });
            }
            Err(errors) => {
                let failing = failing_math(&errors, &ranges, note.preamble.len());
                if !extend(&mut replaced, failing) {
                    return Err(PdfError::Compile(errors));
                }
            }
        }
    }
    Err(PdfError::Compile(Vec::new()))
}

/// Adds `failing` to `replaced`; false when nothing new was added.
fn extend(replaced: &mut BTreeSet<usize>, failing: BTreeSet<usize>) -> bool {
    let before = replaced.len();
    replaced.extend(failing);
    replaced.len() > before
}

/// Equations whose call contains an error location.
fn failing_math(
    errors: &[CompileDiagnostic],
    ranges: &[Range<usize>],
    offset: usize,
) -> BTreeSet<usize> {
    let mut failing = BTreeSet::new();
    for error in errors {
        for range in &error.ranges {
            let (start, end) = (
                range.start.saturating_sub(offset),
                range.end.saturating_sub(offset),
            );
            let hit = ranges
                .iter()
                .position(|site| site.start <= start && end <= site.end && range.start >= offset);
            failing.extend(hit);
        }
    }
    failing
}

/// The body with the equations in `replaced` shown as source, plus the range
/// of every equation call in the result.
fn substitute_math(body: &TypstBody, replaced: &BTreeSet<usize>) -> (String, Vec<Range<usize>>) {
    let mut markup = String::with_capacity(body.markup.len());
    let mut ranges = Vec::with_capacity(body.math.len());
    let mut copied = 0;
    for (index, site) in body.math.iter().enumerate() {
        markup.push_str(&body.markup[copied..site.range.start]);
        let start = markup.len();
        if replaced.contains(&index) {
            markup.push_str(&format!("#math-error({});", escape::string(&site.latex)));
        } else {
            markup.push_str(&body.markup[site.range.clone()]);
        }
        ranges.push(start..markup.len());
        copied = site.range.end;
    }
    markup.push_str(&body.markup[copied..]);
    (markup, ranges)
}

/// A finished export.
pub struct PdfExport {
    pub pdf: Vec<u8>,
    pub pages: usize,
    pub replaced_math: Vec<MathSite>,
}

/// Writes a laid-out note as PDF bytes.
pub fn write_pdf(document: &PagedDocument) -> Result<Vec<u8>, PdfError> {
    typst_pdf::pdf(document, &typst_pdf::PdfOptions::default()).map_err(|errors| {
        PdfError::Pdf(
            errors
                .iter()
                .map(|error| error.message.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })
}

/// Converts, lays out and writes a note as PDF.
pub fn export_pdf(
    markdown: &str,
    note_path: Option<&Path>,
    vault_root: Option<&Path>,
    options: &PdfOptions,
    fonts: &[Font],
) -> Result<PdfExport, PdfError> {
    let note = typst_source(markdown, note_path, vault_root, options);
    let compiled = compile_note(&note, fonts)?;
    Ok(PdfExport {
        pdf: write_pdf(&compiled.document)?,
        pages: compiled.document.pages().len(),
        replaced_math: compiled.replaced_math,
    })
}

/// Frees Typst's memoized results not used in the last `max_age` exports.
/// Call it after each export so memory does not grow without bound; `0`
/// clears everything.
pub fn evict_memory(max_age: usize) {
    typst::comemo::evict(max_age);
}
