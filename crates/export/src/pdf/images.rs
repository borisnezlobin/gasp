//! Resolving embedded images to files on disk and virtual Typst paths.

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

/// Finds images relative to the note, its `images/` attachment folder and the
/// vault root, and assigns each one a virtual path.
#[derive(Debug, Default)]
pub(crate) struct ImageResolver {
    note_dir: Option<PathBuf>,
    vault_root: Option<PathBuf>,
    assets: Vec<(String, PathBuf)>,
}

impl ImageResolver {
    pub(crate) fn new(note_dir: Option<PathBuf>, vault_root: Option<PathBuf>) -> Self {
        Self {
            note_dir,
            vault_root,
            assets: Vec::new(),
        }
    }

    /// Virtual paths and the files they serve.
    pub(crate) fn into_assets(self) -> Vec<(String, PathBuf)> {
        self.assets
    }

    pub(crate) fn resolve(&mut self, target: &str) -> ResolvedImage {
        let target = clean_target(target);
        let Some(extension) = image_extension(&target) else {
            return ResolvedImage::NotAnImage;
        };
        let Some(path) = self.find(&target) else {
            return ResolvedImage::Missing;
        };
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
        candidates.into_iter().find(|candidate| candidate.is_file())
    }
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

fn image_extension(target: &str) -> Option<String> {
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
    fn parses_embed_sizes() {
        assert_eq!(embed_width("300"), Some(300.0));
        assert_eq!(embed_width("300x200"), Some(300.0));
        assert_eq!(embed_width("caption"), None);
    }
}
