//! Several rows picked at once with Cmd- and Shift-click, so they move,
//! cut and go to the trash together, and which of them a move can take.

use std::path::{Path, PathBuf};

use gpui::Modifiers;

use super::entries::Entry;
use super::model::Row;

/// What a click on a row does to the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowClick {
    /// Selects just the row and opens it.
    Plain,
    /// Cmd-click: adds the row to the selection or takes it out.
    Toggle,
    /// Shift-click: selects every row from the anchor to this one.
    Extend,
}

impl RowClick {
    pub fn from_modifiers(modifiers: &Modifiers) -> RowClick {
        if modifiers.secondary() {
            RowClick::Toggle
        } else if modifiers.shift {
            RowClick::Extend
        } else {
            RowClick::Plain
        }
    }
}

/// The rows picked together, and the row Shift-click ranges start from.
#[derive(Debug, Default)]
pub(super) struct MultiSelection {
    entries: Vec<Entry>,
    anchor: Option<PathBuf>,
}

impl MultiSelection {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.entries.iter().any(|entry| entry.path == path)
    }

    /// Forgets the picked rows. Returns whether there were any.
    pub fn clear(&mut self) -> bool {
        let had_any = !self.entries.is_empty();
        self.entries.clear();
        had_any
    }

    pub fn set_anchor(&mut self, path: &Path) {
        self.anchor = Some(path.to_path_buf());
    }

    /// Adds `entry`, or takes it out when it's already in. The row chosen
    /// before (`current`) joins first, so it stays chosen.
    pub fn toggle(&mut self, entry: &Entry, current: Option<&Entry>) {
        if let Some(current) = current.filter(|current| self.is_empty() && *current != entry) {
            self.entries.push(current.clone());
        }
        match self.entries.iter().position(|picked| picked == entry) {
            Some(index) => {
                self.entries.remove(index);
            }
            None => self.entries.push(entry.clone()),
        }
        self.set_anchor(&entry.path);
    }

    /// Picks every row from the anchor to `to`. Without an anchor the
    /// range starts at `current`, or is just `to`.
    pub fn extend(&mut self, rows: &[Row], to: usize, current: Option<usize>) {
        let anchor = self
            .anchor
            .as_deref()
            .and_then(|anchor| rows.iter().position(|row| row.entry.path == anchor))
            .or(current)
            .unwrap_or(to);
        let range = anchor.min(to)..=anchor.max(to);
        self.entries = rows[range].iter().map(|row| row.entry.clone()).collect();
        if let Some(row) = rows.get(anchor) {
            self.set_anchor(&row.entry.path);
        }
    }

    /// The picked entries in the order the tree shows them. Rows inside
    /// collapsed folders come last.
    pub fn entries_in_tree_order(&self, rows: &[Row]) -> Vec<Entry> {
        let mut entries = self.entries.clone();
        entries.sort_by_key(|entry| {
            rows.iter()
                .position(|row| row.entry.path == entry.path)
                .unwrap_or(usize::MAX)
        });
        entries
    }

    /// Follows a rename or move of `from` to `to`.
    pub fn follow_move(&mut self, from: &Path, to: &Path) {
        for entry in &mut self.entries {
            entry.path = moved_path(&entry.path, from, to);
        }
        if let Some(anchor) = self.anchor.as_mut() {
            *anchor = moved_path(anchor, from, to);
        }
    }

    /// Drops picked entries that are gone from disk.
    pub fn retain_existing(&mut self, root: &Path) {
        self.entries.retain(|entry| root.join(&entry.path).exists());
    }
}

/// The moves that put `paths` into `folder`, as (from, to). An entry is
/// left where it is when it's already in `folder`, when `folder` is the
/// entry or inside it, or when a folder above it moves along with it.
pub(super) fn planned_moves(paths: &[PathBuf], folder: &Path) -> Vec<(PathBuf, PathBuf)> {
    paths
        .iter()
        .filter(|path| !has_ancestor_in(path, paths))
        .filter(|path| path.parent() != Some(folder) && !folder.starts_with(path))
        .filter_map(|path| Some((path.clone(), folder.join(path.file_name()?))))
        .collect()
}

/// The entries left after taking out those inside another of them.
pub(super) fn outermost(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|path| !has_ancestor_in(path, paths))
        .cloned()
        .collect()
}

fn has_ancestor_in(path: &Path, paths: &[PathBuf]) -> bool {
    paths
        .iter()
        .any(|other| other.as_path() != path && path.starts_with(other))
}

/// Where `path` is after `from` moved to `to`.
pub(super) fn moved_path(path: &Path, from: &Path, to: &Path) -> PathBuf {
    match path.strip_prefix(from) {
        Ok(rest) if rest.as_os_str().is_empty() => to.to_path_buf(),
        Ok(rest) => to.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::entries::EntryKind;
    use super::*;

    fn rows(paths: &[&str]) -> Vec<Row> {
        paths
            .iter()
            .map(|path| Row {
                entry: Entry::new(*path, EntryKind::Note),
                depth: 0,
                expanded: false,
            })
            .collect()
    }

    fn picked(selection: &MultiSelection, rows: &[Row]) -> Vec<PathBuf> {
        selection
            .entries_in_tree_order(rows)
            .into_iter()
            .map(|entry| entry.path)
            .collect()
    }

    fn paths(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn toggling_keeps_the_row_chosen_before_and_takes_rows_back_out() {
        let rows = rows(&["a.md", "b.md", "c.md"]);
        let mut selection = MultiSelection::default();
        selection.toggle(&rows[2].entry, Some(&rows[0].entry));
        assert_eq!(picked(&selection, &rows), paths(&["a.md", "c.md"]));
        selection.toggle(&rows[0].entry, Some(&rows[2].entry));
        assert_eq!(picked(&selection, &rows), paths(&["c.md"]));
    }

    #[test]
    fn shift_extends_from_the_anchor_either_way() {
        let rows = rows(&["a.md", "b.md", "c.md", "d.md"]);
        let mut selection = MultiSelection::default();
        selection.set_anchor(Path::new("b.md"));
        selection.extend(&rows, 3, None);
        assert_eq!(picked(&selection, &rows), paths(&["b.md", "c.md", "d.md"]));
        selection.extend(&rows, 0, None);
        assert_eq!(picked(&selection, &rows), paths(&["a.md", "b.md"]));
    }

    #[test]
    fn shift_without_an_anchor_starts_at_the_current_row() {
        let rows = rows(&["a.md", "b.md", "c.md"]);
        let mut selection = MultiSelection::default();
        selection.extend(&rows, 2, Some(1));
        assert_eq!(picked(&selection, &rows), paths(&["b.md", "c.md"]));
    }

    #[test]
    fn moves_skip_entries_already_there_or_moving_into_themselves() {
        let moving = paths(&[
            "Notes/a.md",
            "b.md",
            "Projects",
            "Projects/Plan.md",
            "Inbox",
        ]);
        let moves = planned_moves(&moving, Path::new("Projects/Old"));
        assert_eq!(
            moves,
            [
                ("Notes/a.md".into(), "Projects/Old/a.md".into()),
                ("b.md".into(), "Projects/Old/b.md".into()),
                ("Inbox".into(), "Projects/Old/Inbox".into()),
            ]
        );
        let to_root = planned_moves(&paths(&["b.md", "Notes/a.md"]), Path::new(""));
        assert_eq!(to_root, [("Notes/a.md".into(), "a.md".into())]);
    }

    #[test]
    fn moved_entries_stay_picked() {
        let rows = rows(&["Projects/a.md"]);
        let mut selection = MultiSelection::default();
        selection.toggle(&rows[0].entry, None);
        selection.follow_move(Path::new("Projects"), Path::new("Work"));
        assert!(selection.contains(Path::new("Work/a.md")));
    }
}
