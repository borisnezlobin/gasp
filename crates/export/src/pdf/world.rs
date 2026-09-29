//! The Typst [`World`] used for PDF export: embedded and user fonts, the
//! vendored mitex package, the page template, one main source and the note's
//! images read from disk on demand.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

/// Virtual path of the main document.
pub(crate) const MAIN_PATH: &str = "/main.typ";
/// Virtual path of the page template.
pub(crate) const TEMPLATE_PATH: &str = "/editor/template.typ";
/// The standard library with HTML output switched on, for typesetting
/// equations as MathML.
static HTML_LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| {
    LazyHash::new(
        Library::builder()
            .with_features([typst::Feature::Html].into_iter().collect())
            .build(),
    )
});

struct Embedded {
    library: LazyHash<Library>,
    /// Typst's embedded fonts, parsed once for math and export alike.
    fonts: &'static [Font],
    sources: Vec<Source>,
}

static EMBEDDED: LazyLock<Embedded> = LazyLock::new(|| Embedded {
    library: LazyHash::new(Library::default()),
    fonts: gasp_math::embedded_fonts(),
    sources: std::iter::once((TEMPLATE_PATH, include_str!("../../assets/template.typ")))
        .chain(gasp_math::MITEX_SOURCES)
        .map(|(path, text)| Source::new(file_id(path), text.to_owned()))
        .collect(),
});

/// Normalised family names of the embedded fonts.
static EMBEDDED_FAMILIES: LazyLock<HashSet<String>> = LazyLock::new(|| {
    EMBEDDED
        .fonts
        .iter()
        .map(|font| super::system_fonts::normalise(&font.info().family))
        .collect()
});

/// Parses the embedded fonts and sources.
pub(crate) fn load_embedded() {
    LazyLock::force(&EMBEDDED_FAMILIES);
}

pub(crate) fn embedded_families() -> &'static HashSet<String> {
    &EMBEDDED_FAMILIES
}

pub(crate) fn file_id(path: &str) -> FileId {
    let vpath = VirtualPath::new(path).expect("generated paths are valid virtual paths");
    RootedPath::new(VirtualRoot::Project, vpath).intern()
}

/// Loads every font file (`.ttf`, `.otf`, `.ttc`, `.otc`) in `paths`, which
/// may be files or directories (searched recursively).
pub fn load_fonts(paths: &[PathBuf]) -> Vec<Font> {
    let mut fonts = Vec::new();
    for path in paths {
        collect_fonts(path, &mut fonts);
    }
    fonts
}

fn collect_fonts(path: &Path, fonts: &mut Vec<Font>) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect_fonts(&entry.path(), fonts);
        }
        return;
    }
    if !is_font_file(path) {
        return;
    }
    if let Ok(data) = std::fs::read(path) {
        fonts.extend(Font::iter(Bytes::new(data)));
    }
}

pub(crate) fn is_font_file(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    matches!(extension.as_deref(), Some("ttf" | "otf" | "ttc" | "otc"))
}

/// A world for one export: the main source plus a map from virtual image
/// paths to files on disk.
pub(crate) struct ExportWorld {
    library: &'static LazyHash<Library>,
    main: Source,
    book: LazyHash<FontBook>,
    extra_fonts: Vec<Font>,
    images: HashMap<FileId, PathBuf>,
    loaded: Mutex<HashMap<FileId, Bytes>>,
}

impl ExportWorld {
    pub(crate) fn new(
        main_text: String,
        images: &[(String, PathBuf)],
        extra_fonts: Vec<Font>,
    ) -> Self {
        let mut book = FontBook::from_fonts(EMBEDDED.fonts);
        for font in &extra_fonts {
            book.push(font.info().clone());
        }
        Self {
            library: &EMBEDDED.library,
            main: Source::new(file_id(MAIN_PATH), main_text),
            book: LazyHash::new(book),
            extra_fonts,
            images: images
                .iter()
                .map(|(virtual_path, disk)| (file_id(virtual_path), disk.clone()))
                .collect(),
            loaded: Mutex::new(HashMap::new()),
        }
    }

    /// A world whose library has HTML output switched on.
    pub(crate) fn html(main_text: String) -> Self {
        Self {
            library: &HTML_LIBRARY,
            ..Self::new(main_text, &[], Vec::new())
        }
    }

    fn lookup_source(&self, id: FileId) -> Option<&Source> {
        if id == self.main.id() {
            return Some(&self.main);
        }
        EMBEDDED.sources.iter().find(|source| source.id() == id)
    }

    fn read_image(&self, id: FileId) -> FileResult<Bytes> {
        let path = self.images.get(&id).ok_or(FileError::AccessDenied)?;
        let mut loaded = self
            .loaded
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(bytes) = loaded.get(&id) {
            return Ok(bytes.clone());
        }
        let data = std::fs::read(path).map_err(|error| FileError::from_io(error, path))?;
        let bytes = Bytes::new(data);
        loaded.insert(id, bytes.clone());
        Ok(bytes)
    }
}

impl World for ExportWorld {
    fn library(&self) -> &LazyHash<Library> {
        self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
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
        match self.lookup_source(id) {
            Some(source) => Ok(Bytes::from_string(source.clone())),
            None => self.read_image(id),
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        let embedded = EMBEDDED.fonts.len();
        if index < embedded {
            return EMBEDDED.fonts.get(index).cloned();
        }
        self.extra_fonts.get(index - embedded).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}
