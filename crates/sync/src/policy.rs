use std::path::Path;

/// How the merge policy treats a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Notes and other text: merged line by line.
    Text,
    /// Images and other binaries: the local copy wins a conflict.
    Binary,
}

/// Extensions that are always binary, whatever their bytes look like.
const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif", "ico", "avif",
    "pdf", "zip", "gz", "tar", "mp3", "m4a", "wav", "ogg", "flac", "mp4", "mov", "webm", "mkv",
    "woff", "woff2", "ttf", "otf", "wasm", "exe", "dll", "so", "dylib", "bin", "sqlite", "db",
];

/// How many leading bytes are checked for a NUL, as git does.
const SNIFF_LEN: usize = 8000;

/// Classifies a file from its path and every version of its contents.
///
/// Known binary extensions are binary. Anything else is text only when every
/// version is valid UTF-8 with no NUL byte near the start.
pub fn classify(path: &Path, versions: &[&[u8]]) -> FileKind {
    if has_binary_extension(path) || versions.iter().any(|bytes| looks_binary(bytes)) {
        FileKind::Binary
    } else {
        FileKind::Text
    }
}

fn has_binary_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .is_some_and(|extension| BINARY_EXTENSIONS.contains(&extension.as_str()))
}

fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(SNIFF_LEN)];
    head.contains(&0) || std::str::from_utf8(bytes).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_text() {
        assert_eq!(
            classify(Path::new("a.md"), &[b"# hi\n", b""]),
            FileKind::Text
        );
    }

    #[test]
    fn images_are_binary_by_extension() {
        assert_eq!(
            classify(Path::new("img/A.PNG"), &[b"plain"]),
            FileKind::Binary
        );
    }

    #[test]
    fn nul_bytes_or_invalid_utf8_make_binary() {
        assert_eq!(classify(Path::new("data"), &[b"a\0b"]), FileKind::Binary);
        assert_eq!(
            classify(Path::new("x.md"), &[b"ok", &[0xff, 0xfe]]),
            FileKind::Binary
        );
    }
}
