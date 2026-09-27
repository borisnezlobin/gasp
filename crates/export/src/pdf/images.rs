//! Resolving embedded images to files on disk and virtual Typst paths.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};

const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp"];

/// How an embed target resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedImage {
    /// A file on disk, served to Typst under this virtual path.
    Found(String),
    /// An image that could not be found or read.
    Missing,
    /// An embed that is not an image (a note, a PDF, …).
    NotAnImage,
}

/// Where an embedded image lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Located {
    File(PathBuf),
    /// A web address, kept as it is.
    Remote(String),
    Missing,
    NotAnImage,
}

/// Finds images relative to the note, its `images/` attachment folder and the
/// vault root, then anywhere in the vault by file name, as Obsidian does,
/// and assigns each one a virtual path.
#[derive(Debug, Default)]
pub(crate) struct ImageResolver {
    note_dir: Option<PathBuf>,
    vault_root: Option<PathBuf>,
    /// Every visible file in the vault, vault-relative with `/`
    /// separators. Read on the first image the direct lookups miss, so an
    /// export whose images sit next to the note never walks the vault.
    vault_files: OnceCell<Vec<String>>,
    assets: Vec<(String, PathBuf)>,
}

impl ImageResolver {
    pub(crate) fn new(note_dir: Option<PathBuf>, vault_root: Option<PathBuf>) -> Self {
        Self {
            note_dir,
            vault_root,
            vault_files: OnceCell::new(),
            assets: Vec::new(),
        }
    }

    /// Virtual paths and the files they serve.
    pub(crate) fn into_assets(self) -> Vec<(String, PathBuf)> {
        self.assets
    }

    /// Where an embed target is, without registering it as an asset.
    pub(crate) fn locate(&self, target: &str) -> Located {
        let target = clean_target(target);
        if image_extension(&target).is_none() {
            return Located::NotAnImage;
        }
        if is_remote(&target) {
            return Located::Remote(target);
        }
        self.find(&target).map_or(Located::Missing, Located::File)
    }

    pub(crate) fn resolve(&mut self, target: &str) -> ResolvedImage {
        let path = match self.locate(target) {
            Located::File(path) => path,
            Located::NotAnImage => return ResolvedImage::NotAnImage,
            Located::Remote(_) | Located::Missing => return ResolvedImage::Missing,
        };
        let extension = image_extension(&path.to_string_lossy()).unwrap_or_default();
        if let Some((virtual_path, _)) = self.assets.iter().find(|(_, known)| *known == path) {
            return ResolvedImage::Found(virtual_path.clone());
        }
        let virtual_path = format!("/assets/{}.{extension}", self.assets.len());
        self.assets.push((virtual_path.clone(), path));
        ResolvedImage::Found(virtual_path)
    }

    fn find(&self, target: &str) -> Option<PathBuf> {
        if is_remote(target) {
            return None;
        }
        let relative = Path::new(target.trim_start_matches('/'));
        let mut candidates = Vec::new();
        if let Some(dir) = &self.note_dir {
            candidates.push(dir.join(relative));
            candidates.push(dir.join("images").join(relative));
        }
        if let Some(root) = &self.vault_root {
            candidates.push(root.join(relative));
        }
        candidates
            .into_iter()
            .find(|candidate| candidate.is_file())
            .or_else(|| self.find_in_vault(target.trim_start_matches('/')))
    }

    /// The vault file whose path ends with `target`, as Obsidian resolves
    /// a bare `![[name.png]]`: see [`closest_match`].
    fn find_in_vault(&self, target: &str) -> Option<PathBuf> {
        let root = self.vault_root.as_ref()?;
        let note_dir = self
            .note_dir
            .as_ref()
            .and_then(|dir| dir.strip_prefix(root).ok())
            .map(slash_path)
            .unwrap_or_default();
        let files = self.vault_files.get_or_init(|| visible_files(root));
        let found = closest_match(files, &note_dir, target)?;
        Some(root.join(found))
    }
}

/// The file in `files` whose path is `target` or ends with `/target`,
/// ignoring case: one in `note_dir` first, then the fewest folders deep,
/// then the first by path.
fn closest_match<'a>(files: &'a [String], note_dir: &str, target: &str) -> Option<&'a str> {
    let target = target.to_lowercase();
    let ending = format!("/{target}");
    files
        .iter()
        .filter(|file| {
            let file = file.to_lowercase();
            file == target || file.ends_with(&ending)
        })
        .min_by_key(|file| {
            let parent = file.rsplit_once('/').map_or("", |(parent, _)| parent);
            (parent != note_dir, file.matches('/').count(), file.as_str())
        })
        .map(String::as_str)
}

/// Every file under `root` outside hidden folders (`.git`, `.obsidian`,
/// the editor's own `.editor`), vault-relative.
fn visible_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(path),
                Ok(_) => files.extend(path.strip_prefix(root).ok().map(slash_path)),
                Err(_) => {}
            }
        }
    }
    files
}

/// A relative path with `/` separators on every platform.
fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_remote(target: &str) -> bool {
    target.contains("://") || target.starts_with("data:")
}

/// Drops a `#heading` or `|size` suffix and decodes `%20`-style escapes.
fn clean_target(target: &str) -> String {
    let target = target.split(['|', '#']).next().unwrap_or_default().trim();
    percent_decode(target)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escape = (bytes[index] == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escape {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| text.to_owned())
}

pub(crate) fn image_extension(target: &str) -> Option<String> {
    let extension = Path::new(target)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    IMAGE_EXTENSIONS
        .contains(&extension.as_str())
        .then_some(extension)
}

/// Parses an Obsidian embed size (`300` or `300x200`) into a width in pixels.
pub(crate) fn embed_width(size: &str) -> Option<f64> {
    let width = size.trim().split('x').next()?;
    width.parse::<f64>().ok().filter(|width| *width > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(clean_target("images/a%20b.png"), "images/a b.png");
        assert_eq!(clean_target("x.png|300"), "x.png");
    }

    #[test]
    fn classifies_targets() {
        let mut resolver = ImageResolver::default();
        assert_eq!(resolver.resolve("Other note"), ResolvedImage::NotAnImage);
        assert_eq!(resolver.resolve("gone.png"), ResolvedImage::Missing);
        assert_eq!(
            resolver.resolve("https://example.com/a.png"),
            ResolvedImage::Missing
        );
    }

    #[test]
    fn finds_images_in_attachment_folder() {
        let dir = std::env::temp_dir().join(format!("editor-export-images-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("images")).unwrap();
        std::fs::write(dir.join("images/pic.png"), b"png").unwrap();
        let mut resolver = ImageResolver::new(Some(dir.clone()), None);
        assert_eq!(
            resolver.resolve("pic.png"),
            ResolvedImage::Found("/assets/0.png".to_owned())
        );
        assert_eq!(
            resolver.resolve("pic.png|200"),
            ResolvedImage::Found("/assets/0.png".to_owned())
        );
        assert_eq!(resolver.into_assets().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_bare_names_anywhere_in_the_vault() {
        let root = std::env::temp_dir().join(format!("editor-export-vault-{}", std::process::id()));
        for dir in ["notes", "assets/deep", "zz", ".hidden"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "assets/deep/pic.png",
            "zz/pic.png",
            ".hidden/only.png",
            "assets/deep/Other.PNG",
        ] {
            std::fs::write(root.join(file), b"png").unwrap();
        }
        let resolver = ImageResolver::new(Some(root.join("notes")), Some(root.clone()));
        assert_eq!(
            resolver.locate("pic.png"),
            Located::File(root.join("zz/pic.png")),
            "the shallowest match wins"
        );
        assert_eq!(
            resolver.locate("deep/pic.png"),
            Located::File(root.join("assets/deep/pic.png")),
            "a partial path matches the end of one"
        );
        assert_eq!(
            resolver.locate("other.png"),
            Located::File(root.join("assets/deep/Other.PNG")),
            "names match without regard to case"
        );
        assert_eq!(resolver.locate("only.png"), Located::Missing, "hidden");
        assert_eq!(resolver.locate("eep/pic.png"), Located::Missing);
        let unrooted = ImageResolver::new(Some(root.join("notes")), None);
        assert_eq!(unrooted.locate("pic.png"), Located::Missing);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prefers_a_match_in_the_notes_own_folder() {
        let files = ["a/pic.png".to_owned(), "notes/sub/pic.png".to_owned()];
        assert_eq!(
            closest_match(&files, "notes/sub", "pic.png"),
            Some("notes/sub/pic.png")
        );
        assert_eq!(closest_match(&files, "", "pic.png"), Some("a/pic.png"));
        assert_eq!(closest_match(&files, "", "PIC.png"), Some("a/pic.png"));
    }

    #[test]
    fn parses_embed_sizes() {
        assert_eq!(embed_width("300"), Some(300.0));
        assert_eq!(embed_width("300x200"), Some(300.0));
        assert_eq!(embed_width("caption"), None);
    }
}
