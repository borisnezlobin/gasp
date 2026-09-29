//! The export fonts installed on this machine.
//!
//! The default body font, Iowan Old Style, and Courier New ship with macOS
//! but can't be embedded in the app, so they are read from the system font
//! folders. Parsing every installed font would take longer than the export
//! itself, so only files whose name could be a wanted family's are read,
//! and the family names inside them decide. File names are often short
//! forms, such as Windows' `cour.ttf` and `courbd.ttf` for Courier New,
//! so a file is a candidate when its name starts like the family's.
//! Each family is looked up once per process.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use typst::foundations::Bytes;
use typst::text::Font;

use super::world::{embedded_families, is_font_file};

/// Font files under the system font folders, listed once.
static FONT_FILES: LazyLock<Vec<PathBuf>> = LazyLock::new(|| {
    let mut files = Vec::new();
    for dir in font_dirs() {
        list_font_files(&dir, 0, &mut files);
    }
    files
});

/// Fonts found per normalised family name.
static FOUND: LazyLock<Mutex<HashMap<String, Vec<Font>>>> = LazyLock::new(Mutex::default);

/// Folder depth searched below each font folder; Linux distributions nest
/// fonts two or three levels deep.
const MAX_DEPTH: usize = 4;

/// The installed fonts of `families` that Typst doesn't already embed.
pub fn system_fonts(families: &[String]) -> Vec<Font> {
    let embedded = embedded_families();
    let mut fonts = Vec::new();
    for family in families {
        let key = normalise(family);
        if key.is_empty() || embedded.contains(&key) {
            continue;
        }
        let mut found = FOUND.lock().unwrap_or_else(|poison| poison.into_inner());
        let entry = found.entry(key).or_insert_with_key(|key| load_family(key));
        fonts.extend(entry.iter().cloned());
    }
    fonts
}

/// Lowercase letters and digits only, so "Iowan Old Style" matches both the
/// family name and the file `IowanOldStyle-Bold.otf`.
pub(crate) fn normalise(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

/// Letters a file name must share with a family's for its fonts to be
/// read: enough to skip nearly every file, few enough for `cour.ttf`.
const CANDIDATE_PREFIX: usize = 4;

/// Whether a font file named `stem` could hold the family `key` (both
/// normalised): its name starts like the family's.
fn is_candidate(stem: &str, key: &str) -> bool {
    let prefix = key
        .char_indices()
        .nth(CANDIDATE_PREFIX)
        .map_or(key, |(at, _)| &key[..at]);
    stem.starts_with(prefix)
}

fn load_family(key: &str) -> Vec<Font> {
    FONT_FILES
        .iter()
        .filter(|path| {
            path.file_stem()
                .is_some_and(|stem| is_candidate(&normalise(&stem.to_string_lossy()), key))
        })
        .filter_map(|path| std::fs::read(path).ok())
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .filter(|font| normalise(&font.info().family) == key)
        .collect()
}

fn list_font_files(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < MAX_DEPTH {
                list_font_files(&path, depth + 1, files);
            }
        } else if is_font_file(&path) {
            files.push(path);
        }
    }
}

#[cfg(not(target_os = "ios"))]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/Library/Fonts"),
    ];
    dirs.extend(home().map(|home| home.join("Library/Fonts")));
    dirs
}

#[cfg(target_os = "windows")]
fn font_dirs() -> Vec<PathBuf> {
    let windows =
        std::env::var_os("WINDIR").map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from);
    let mut dirs = vec![windows.join("Fonts")];
    dirs.extend(
        std::env::var_os("LOCALAPPDATA")
            .map(|local| PathBuf::from(local).join("Microsoft\\Windows\\Fonts")),
    );
    dirs
}

/// An iPhone app may read the system's own fonts, Charter and Iowan Old
/// Style among them.
#[cfg(target_os = "ios")]
fn font_dirs() -> Vec<PathBuf> {
    vec![PathBuf::from("/System/Library/Fonts")]
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "ios")))]
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Some(home) = home() {
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join(".fonts"));
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_family_names() {
        assert_eq!(normalise("Iowan Old Style"), "iowanoldstyle");
        assert_eq!(normalise("IowanOldStyle-Bold"), "iowanoldstylebold");
    }

    #[test]
    fn short_file_names_are_candidates_for_their_family() {
        let courier = normalise("Courier New");
        for stem in ["cour", "courbd", "couri", "courbi", "CourierNew-Bold"] {
            assert!(is_candidate(&normalise(stem), &courier), "{stem}");
        }
        assert!(!is_candidate("arial", &courier));
        assert!(is_candidate("times", &normalise("Times New Roman")));
        assert!(is_candidate("pt", "pt"), "short families match whole");
    }

    #[test]
    fn embedded_and_unknown_families_add_nothing() {
        assert!(system_fonts(&["Libertinus Serif".to_owned()]).is_empty());
        assert!(system_fonts(&["No Such Family 123".to_owned()]).is_empty());
    }
}
