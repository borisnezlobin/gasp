//! The tree's contents: folder listings read lazily from disk, which
//! folders are expanded, and the flat list of visible rows.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use super::entries::{Entry, EntryKind, SortOrder, is_hidden, sort_entries};

/// One visible line of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub entry: Entry,
    /// 0 for entries at the vault root.
    pub depth: usize,
    pub expanded: bool,
}

/// Folder listings and expansion state for one vault.
pub struct TreeModel {
    root: PathBuf,
    expanded: BTreeSet<PathBuf>,
    listings: HashMap<PathBuf, Vec<Entry>>,
    rows: Vec<Row>,
    sort: SortOrder,
}

impl TreeModel {
    /// Reads the vault root's listing. Folders load when they're expanded.
    pub fn new(root: impl Into<PathBuf>) -> TreeModel {
        let mut model = TreeModel {
            root: root.into(),
            expanded: BTreeSet::new(),
            listings: HashMap::new(),
            rows: Vec::new(),
            sort: SortOrder::default(),
        };
        model.refresh();
        model
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort
    }

    /// Re-sorts every folder.
    pub fn set_sort_order(&mut self, order: SortOrder) {
        if self.sort != order {
            self.sort = order;
            self.listings.clear();
            self.rebuild();
        }
    }

    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.rows.iter().position(|row| row.entry.path == path)
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    /// Expanded folders, relative to the vault root, in path order.
    pub fn expanded(&self) -> impl Iterator<Item = &Path> {
        self.expanded.iter().map(PathBuf::as_path)
    }

    /// Replaces the expanded set, such as one restored from a saved workspace.
    pub fn set_expanded(&mut self, folders: impl IntoIterator<Item = PathBuf>) {
        self.expanded = folders
            .into_iter()
            .filter(|folder| self.root.join(folder).is_dir())
            .collect();
        self.rebuild();
    }

    /// Expands a folder, reading its listing if needed. Returns whether
    /// anything changed.
    pub fn expand(&mut self, folder: &Path) -> bool {
        if !self.root.join(folder).is_dir() || !self.expanded.insert(folder.to_path_buf()) {
            return false;
        }
        self.rebuild();
        true
    }

    pub fn collapse(&mut self, folder: &Path) -> bool {
        if !self.expanded.remove(folder) {
            return false;
        }
        self.rebuild();
        true
    }

    pub fn toggle(&mut self, folder: &Path) {
        if !self.collapse(folder) {
            self.expand(folder);
        }
    }

    /// Expands every folder above `path` so its row shows.
    pub fn expand_ancestors(&mut self, path: &Path) {
        let ancestors: Vec<PathBuf> = path
            .ancestors()
            .skip(1)
            .filter(|folder| !folder.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .collect();
        let before = self.expanded.len();
        self.expanded.extend(ancestors);
        if self.expanded.len() != before {
            self.rebuild();
        }
    }

    /// The index of the row's parent folder, if it isn't at the root.
    pub fn parent_index(&self, index: usize) -> Option<usize> {
        let depth = self.rows.get(index)?.depth;
        (depth > 0).then(|| self.rows[..index].iter().rposition(|row| row.depth < depth))?
    }

    /// Re-reads every loaded listing from disk, forgetting folders that
    /// no longer exist.
    pub fn refresh(&mut self) {
        let root = self.root.clone();
        self.expanded.retain(|folder| root.join(folder).is_dir());
        self.listings.clear();
        self.rebuild();
    }

    /// Renames paths in the expanded set after a folder moves, so it stays open.
    pub fn follow_move(&mut self, from: &Path, to: &Path) {
        let moved: Vec<PathBuf> = self
            .expanded
            .iter()
            .filter(|folder| folder.starts_with(from))
            .cloned()
            .collect();
        for folder in moved {
            self.expanded.remove(&folder);
            if let Ok(rest) = folder.strip_prefix(from) {
                self.expanded.insert(to.join(rest));
            }
        }
        self.refresh();
    }

    fn rebuild(&mut self) {
        let mut rows = Vec::new();
        self.push_rows(Path::new(""), 0, &mut rows);
        self.rows = rows;
    }

    fn push_rows(&mut self, folder: &Path, depth: usize, rows: &mut Vec<Row>) {
        for entry in self.listing(folder) {
            let expanded = entry.is_folder() && self.expanded.contains(&entry.path);
            let path = entry.path.clone();
            rows.push(Row {
                entry,
                depth,
                expanded,
            });
            if expanded {
                self.push_rows(&path, depth + 1, rows);
            }
        }
    }

    fn listing(&mut self, folder: &Path) -> Vec<Entry> {
        let (root, sort) = (&self.root, self.sort);
        self.listings
            .entry(folder.to_path_buf())
            .or_insert_with(|| read_sorted_listing(root, folder, sort))
            .clone()
    }
}

/// The visible entries of one folder in `order`.
pub fn read_sorted_listing(root: &Path, folder: &Path, order: SortOrder) -> Vec<Entry> {
    let Ok(read) = fs::read_dir(root.join(folder)) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|item| {
            let name = item.file_name().into_string().ok()?;
            let kind = entry_kind(&item, &name)?;
            Some(Entry::new(folder.join(name), kind))
        })
        .collect();
    sort_entries(&mut entries, order, |entry| {
        fs::metadata(root.join(&entry.path))
            .and_then(|meta| meta.modified())
            .ok()
    });
    entries
}

fn entry_kind(item: &fs::DirEntry, name: &str) -> Option<EntryKind> {
    if is_hidden(name) {
        return None;
    }
    if item.path().is_dir() {
        return Some(EntryKind::Folder);
    }
    EntryKind::of_file(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for folder in [
            ".git/objects",
            ".obsidian",
            ".editor",
            ".trash",
            "Projects/Old",
            "Daily",
        ] {
            fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        for file in [
            "Note 10.md",
            "Note 2.md",
            "chart.png",
            "paper.pdf",
            "board.canvas",
            ".hidden.md",
            "Projects/Plan.md",
            "Projects/Old/Idea.md",
            "Daily/2024-01-01.md",
        ] {
            fs::write(dir.path().join(file), "").unwrap();
        }
        dir
    }

    fn labels(model: &TreeModel) -> Vec<String> {
        model
            .rows()
            .iter()
            .map(|row| format!("{}{}", "  ".repeat(row.depth), row.entry.label()))
            .collect()
    }

    #[test]
    fn root_lists_folders_then_files_and_hides_the_rest() {
        let dir = vault();
        let model = TreeModel::new(dir.path());
        assert_eq!(
            labels(&model),
            [
                "Daily",
                "Projects",
                "chart.png",
                "Note 2",
                "Note 10",
                "paper.pdf"
            ]
        );
    }

    #[test]
    fn expanding_shows_children_indented() {
        let dir = vault();
        let mut model = TreeModel::new(dir.path());
        assert!(model.expand(Path::new("Projects")));
        assert!(model.expand(Path::new("Projects/Old")));
        assert_eq!(
            labels(&model)[..5],
            ["Daily", "Projects", "  Old", "    Idea", "  Plan"]
        );
        assert!(model.collapse(Path::new("Projects")));
        assert_eq!(labels(&model).len(), 6);
        // The inner folder stays expanded for next time.
        assert!(model.is_expanded(Path::new("Projects/Old")));
    }

    #[test]
    fn files_cannot_be_expanded() {
        let dir = vault();
        let mut model = TreeModel::new(dir.path());
        assert!(!model.expand(Path::new("Note 2.md")));
        assert!(!model.expand(Path::new("Missing")));
    }

    #[test]
    fn parents_are_found_by_depth() {
        let dir = vault();
        let mut model = TreeModel::new(dir.path());
        model.expand(Path::new("Projects"));
        model.expand(Path::new("Projects/Old"));
        let idea = model.index_of(Path::new("Projects/Old/Idea.md")).unwrap();
        let old = model.parent_index(idea).unwrap();
        assert_eq!(
            model.row(old).unwrap().entry.path,
            Path::new("Projects/Old")
        );
        let projects = model.parent_index(old).unwrap();
        assert_eq!(model.parent_index(projects), None);
    }

    #[test]
    fn expand_ancestors_reveals_a_deep_file() {
        let dir = vault();
        let mut model = TreeModel::new(dir.path());
        model.expand_ancestors(Path::new("Projects/Old/Idea.md"));
        assert!(model.index_of(Path::new("Projects/Old/Idea.md")).is_some());
    }

    #[test]
    fn refresh_picks_up_changes_and_drops_missing_folders() {
        let dir = vault();
        let mut model = TreeModel::new(dir.path());
        model.expand(Path::new("Daily"));
        fs::write(dir.path().join("Daily/2024-01-02.md"), "").unwrap();
        fs::remove_dir_all(dir.path().join("Projects")).unwrap();
        model.refresh();
        assert!(model.index_of(Path::new("Daily/2024-01-02.md")).is_some());
        assert!(model.index_of(Path::new("Projects")).is_none());
    }

    #[test]
    fn restored_expansion_ignores_missing_folders() {
        let dir = vault();
        let mut model = TreeModel::new(dir.path());
        model.set_expanded([PathBuf::from("Daily"), PathBuf::from("Gone")]);
        let expanded: Vec<&Path> = model.expanded().collect();
        assert_eq!(expanded, [Path::new("Daily")]);
    }

    #[test]
    fn moved_folders_stay_expanded() {
        let dir = vault();
        let mut model = TreeModel::new(dir.path());
        model.expand(Path::new("Projects"));
        model.expand(Path::new("Projects/Old"));
        fs::rename(dir.path().join("Projects"), dir.path().join("Work")).unwrap();
        model.follow_move(Path::new("Projects"), Path::new("Work"));
        assert!(model.is_expanded(Path::new("Work/Old")));
        assert!(model.index_of(Path::new("Work/Old/Idea.md")).is_some());
    }
}
