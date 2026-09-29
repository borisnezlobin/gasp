//! What the file tree lists: which files show, what they're called and in
//! what order.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const NOTE_EXTENSION: &str = "md";
const IMAGE_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "avif"];
const PDF_EXTENSION: &str = "pdf";

/// The kinds of entry the tree shows. Other files are hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    Folder,
    Note,
    Image,
    Pdf,
}

impl EntryKind {
    /// The kind of a file with this name, or `None` when the tree hides it.
    pub fn of_file(name: &str) -> Option<EntryKind> {
        let extension = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
        if extension == NOTE_EXTENSION {
            Some(EntryKind::Note)
        } else if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
            Some(EntryKind::Image)
        } else {
            (extension == PDF_EXTENSION).then_some(EntryKind::Pdf)
        }
    }
}

/// One file or folder, by its path inside the vault.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Entry {
    /// Relative to the vault root.
    pub path: PathBuf,
    pub kind: EntryKind,
}

impl Entry {
    pub fn new(path: impl Into<PathBuf>, kind: EntryKind) -> Entry {
        Entry {
            path: path.into(),
            kind,
        }
    }

    /// The file or folder name, with its extension.
    pub fn file_name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    }

    /// The name the tree shows: notes without `.md`, everything else as is.
    pub fn label(&self) -> &str {
        display_name(self.file_name(), self.kind)
    }

    pub fn is_folder(&self) -> bool {
        self.kind == EntryKind::Folder
    }

    /// The folder this entry is in, relative to the vault root.
    pub fn parent(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new(""))
    }
}

/// A file name as the tree shows it.
pub fn display_name(file_name: &str, kind: EntryKind) -> &str {
    if kind != EntryKind::Note {
        return file_name;
    }
    let cut = file_name.len().saturating_sub(NOTE_EXTENSION.len() + 1);
    match file_name.get(cut..) {
        Some(tail) if tail.eq_ignore_ascii_case(".md") => &file_name[..cut],
        _ => file_name,
    }
}

/// Dot-folders and dot-files (`.git`, `.obsidian`, `.gasp`, `.trash`)
/// never show.
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// How the tree orders each folder. Folders always come before files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SortOrder {
    /// By name, A to Z.
    #[default]
    NameAscending,
    /// By name, Z to A.
    NameDescending,
    /// Files changed most recently first; folders by name.
    ModifiedNewest,
    /// Files changed longest ago first; folders by name.
    ModifiedOldest,
}

/// Sorts one folder's entries. `modified` reads a file's change time.
pub fn sort_entries(
    entries: &mut [Entry],
    order: SortOrder,
    modified: impl Fn(&Entry) -> Option<std::time::SystemTime>,
) {
    match order {
        SortOrder::NameAscending => sort_by_name(entries, false),
        SortOrder::NameDescending => sort_by_name(entries, true),
        SortOrder::ModifiedNewest | SortOrder::ModifiedOldest => {
            let newest = order == SortOrder::ModifiedNewest;
            entries.sort_by_cached_key(|entry| {
                let time = (!entry.is_folder()).then(|| modified(entry)).flatten();
                (!entry.is_folder(), ModifiedKey { time, newest })
            });
            sort_runs_by_name(entries);
        }
    }
}

/// Orders files by change time, with folders (no time) all equal.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ModifiedKey {
    time: Option<std::time::SystemTime>,
    newest: bool,
}

impl Ord for ModifiedKey {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.newest {
            other.time.cmp(&self.time)
        } else {
            self.time.cmp(&other.time)
        }
    }
}

impl PartialOrd for ModifiedKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Folders, which sort as equals by time, go by name.
fn sort_runs_by_name(entries: &mut [Entry]) {
    let folders = entries.iter().take_while(|entry| entry.is_folder()).count();
    sort_by_name(&mut entries[..folders], false);
}

/// Sorts folders first, then files, each in natural order by name, or
/// with names reversed (folders still first).
/// Each entry's name is found in its path once, not on every comparison,
/// which makes a folder of thousands of notes sort several times faster.
fn sort_by_name(entries: &mut [Entry], descending: bool) {
    let keys: Vec<(bool, &str, &str)> = entries
        .iter()
        .map(|entry| (entry.is_folder(), entry.label(), entry.file_name()))
        .collect();
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (&keys[a], &keys[b]);
        let (first, second) = if descending { (b, a) } else { (a, b) };
        b.0.cmp(&a.0)
            .then_with(|| natural_cmp(first.1, second.1))
            .then_with(|| first.2.cmp(second.2))
    });
    let sorted: Vec<Entry> = order.iter().map(|&at| entries[at].clone()).collect();
    entries.clone_from_slice(&sorted);
}

/// Compares names the way people count: `Note 2` before `Note 10`, and
/// without regard to case.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = chunks(a);
    let mut right = chunks(b);
    loop {
        match (left.next(), right.next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match compare_chunks(x, y) {
                Ordering::Equal => continue,
                unequal => return unequal,
            },
        }
    }
}

/// Splits a name into runs of digits and runs of everything else.
fn chunks(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let digits = first.is_ascii_digit();
        let end = rest
            .find(|c: char| c.is_ascii_digit() != digits)
            .unwrap_or(rest.len());
        let (chunk, tail) = rest.split_at(end);
        rest = tail;
        Some(chunk)
    })
}

fn compare_chunks(a: &str, b: &str) -> Ordering {
    let numeric = |s: &str| s.starts_with(|c: char| c.is_ascii_digit());
    if numeric(a) && numeric(b) {
        return compare_numbers(a, b);
    }
    // Most names are ASCII, which compares without lowercased copies:
    // a big folder sorts about twice as fast.
    if a.is_ascii() && b.is_ascii() {
        fn lower(s: &str) -> impl Iterator<Item = u8> + '_ {
            s.bytes().map(|byte| byte.to_ascii_lowercase())
        }
        return lower(a).cmp(lower(b));
    }
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// Compares digit runs by value without parsing, so long runs can't overflow.
fn compare_numbers(a: &str, b: &str) -> Ordering {
    let a_value = a.trim_start_matches('0');
    let b_value = b.trim_start_matches('0');
    a_value
        .len()
        .cmp(&b_value.len())
        .then_with(|| a_value.cmp(b_value))
        .then_with(|| a.len().cmp(&b.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        names.sort_by(|a, b| natural_cmp(a, b));
        names
    }

    #[test]
    fn numbers_sort_by_value() {
        assert_eq!(
            sorted(&["Note 10", "Note 2", "Note 1", "Note 100"]),
            ["Note 1", "Note 2", "Note 10", "Note 100"]
        );
    }

    #[test]
    fn case_is_ignored() {
        assert_eq!(
            sorted(&["banana", "Apple", "cherry"]),
            ["Apple", "banana", "cherry"]
        );
    }

    #[test]
    fn dates_and_versions_sort_naturally() {
        assert_eq!(
            sorted(&["2024-10-01", "2024-9-30", "v1.10", "v1.9"]),
            ["2024-9-30", "2024-10-01", "v1.9", "v1.10"]
        );
    }

    #[test]
    fn leading_zeros_tie_break_after_value() {
        assert_eq!(sorted(&["a01", "a1", "a001"]), ["a1", "a01", "a001"]);
    }

    #[test]
    fn huge_numbers_do_not_overflow() {
        assert_eq!(
            sorted(&["x 99999999999999999999999", "x 100000000000000000000000"]),
            ["x 99999999999999999999999", "x 100000000000000000000000"]
        );
    }

    #[test]
    fn folders_come_before_files() {
        let mut entries = [
            Entry::new("b.md", EntryKind::Note),
            Entry::new("z", EntryKind::Folder),
            Entry::new("a.png", EntryKind::Image),
            Entry::new("c", EntryKind::Folder),
        ];
        sort_by_name(&mut entries, false);
        let names: Vec<&str> = entries.iter().map(Entry::label).collect();
        assert_eq!(names, ["c", "z", "a.png", "b"]);
    }

    #[test]
    fn sort_orders_keep_folders_first() {
        let entries = || {
            vec![
                Entry::new("b.md", EntryKind::Note),
                Entry::new("z", EntryKind::Folder),
                Entry::new("a.md", EntryKind::Note),
                Entry::new("c", EntryKind::Folder),
            ]
        };
        let names = |entries: &[Entry]| -> Vec<String> {
            entries.iter().map(|e| e.label().to_string()).collect()
        };
        let mut sorted = entries();
        sort_entries(&mut sorted, SortOrder::NameDescending, |_| None);
        assert_eq!(names(&sorted), ["z", "c", "b", "a"]);
        let time = |entry: &Entry| {
            let seconds = if entry.label() == "a" { 10 } else { 20 };
            Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
        };
        let mut sorted = entries();
        sort_entries(&mut sorted, SortOrder::ModifiedNewest, time);
        assert_eq!(names(&sorted), ["c", "z", "b", "a"]);
        let mut sorted = entries();
        sort_entries(&mut sorted, SortOrder::ModifiedOldest, time);
        assert_eq!(names(&sorted), ["c", "z", "a", "b"]);
    }

    #[test]
    fn notes_hide_their_extension_and_others_keep_it() {
        assert_eq!(Entry::new("a/Plan.md", EntryKind::Note).label(), "Plan");
        assert_eq!(Entry::new("Plan.MD", EntryKind::Note).label(), "Plan");
        assert_eq!(
            Entry::new("chart.png", EntryKind::Image).label(),
            "chart.png"
        );
        assert_eq!(Entry::new("paper.pdf", EntryKind::Pdf).label(), "paper.pdf");
    }

    #[test]
    fn only_notes_images_and_pdfs_show() {
        assert_eq!(EntryKind::of_file("a.md"), Some(EntryKind::Note));
        assert_eq!(EntryKind::of_file("a.JPG"), Some(EntryKind::Image));
        assert_eq!(EntryKind::of_file("a.pdf"), Some(EntryKind::Pdf));
        assert_eq!(EntryKind::of_file("a.canvas"), None);
        assert_eq!(EntryKind::of_file("README"), None);
    }

    #[test]
    fn dot_names_are_hidden() {
        for name in [
            ".git",
            ".obsidian",
            editor_config::CONFIG_DIR,
            ".trash",
            ".DS_Store",
        ] {
            assert!(is_hidden(name), "{name}");
        }
        assert!(!is_hidden("Notes"));
    }
}
