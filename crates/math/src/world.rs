//! A minimal in-memory Typst [`World`]: embedded fonts, the vendored mitex
//! package and one main source. It never touches the file system.

use std::sync::{LazyLock, Once};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

/// Path of the module that exports `mitex-scope` for evaluating converter
/// output.
pub const SCOPE_MODULE_PATH: &str = "/mitex/compat.typ";

const MAIN_PATH: &str = "/main.typ";

/// The mitex Typst scope and its compatibility layer, as (virtual path,
/// source) pairs. Other crates that typeset converted math, such as PDF
/// export, serve these in their own Typst world.
pub const MITEX_SOURCES: [(&str, &str); 4] = [
    (SCOPE_MODULE_PATH, include_str!("../assets/compat.typ")),
    ("/mitex/mod.typ", include_str!("../assets/mitex/mod.typ")),
    (
        "/mitex/prelude.typ",
        include_str!("../assets/mitex/prelude.typ"),
    ),
    (
        "/mitex/latex/standard.typ",
        include_str!("../assets/mitex/latex/standard.typ"),
    ),
];

/// The fonts Typst embeds, parsed once and shared with every other world
/// in the process.
struct EmbeddedFonts {
    fonts: Vec<Font>,
    book: LazyHash<FontBook>,
}

static FONTS: LazyLock<EmbeddedFonts> = LazyLock::new(|| {
    let fonts: Vec<Font> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
    let book = LazyHash::new(FontBook::from_fonts(&fonts));
    EmbeddedFonts { fonts, book }
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));

static SOURCES: LazyLock<Vec<Source>> = LazyLock::new(|| {
    MITEX_SOURCES
        .iter()
        .map(|(path, text)| Source::new(file_id(path), (*text).to_owned()))
        .collect()
});

/// The fonts Typst embeds (New Computer Modern, Libertinus Serif, DejaVu
/// Sans Mono), parsed on first use.
pub fn embedded_fonts() -> &'static [Font] {
    &FONTS.fonts
}

/// Starts parsing the fonts on another thread, once, so it overlaps with
/// evaluating the mitex scope, which needs no fonts. Laying out waits for
/// the fonts if they aren't ready.
fn load_fonts_in_background() {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("math fonts".into())
            .spawn(|| LazyLock::force(&FONTS));
        if spawned.is_err() {
            LazyLock::force(&FONTS);
        }
    });
}

/// Forces the one-time setup (font parsing, library construction).
pub(crate) fn warm_up() {
    load_fonts_in_background();
    LazyLock::force(&LIBRARY);
    LazyLock::force(&SOURCES);
    LazyLock::force(&FONTS);
}

fn file_id(path: &str) -> FileId {
    let vpath = VirtualPath::new(path).expect("embedded paths are valid virtual paths");
    RootedPath::new(VirtualRoot::Project, vpath).intern()
}

/// A world whose main file is `main_text`.
pub(crate) struct MathWorld {
    main: Source,
}

impl MathWorld {
    pub(crate) fn new(main_text: String) -> Self {
        Self {
            main: Source::new(file_id(MAIN_PATH), main_text),
        }
    }

    fn lookup_source(&self, id: FileId) -> Option<&Source> {
        if id == self.main.id() {
            return Some(&self.main);
        }
        SOURCES.iter().find(|source| source.id() == id)
    }
}

impl World for MathWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.lookup_source(id)
            .cloned()
            .ok_or(FileError::AccessDenied)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.lookup_source(id)
            .map(|source| Bytes::from_string(source.clone()))
            .ok_or(FileError::AccessDenied)
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Compiles `main_text` to a paged document, joining error messages on failure.
pub(crate) fn compile_document(main_text: String) -> Result<PagedDocument, String> {
    load_fonts_in_background();
    let world = MathWorld::new(main_text);
    typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| {
            errors
                .iter()
                .map(|error| error.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        })
}
