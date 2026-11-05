//! The tree's context menu: its items and which row it belongs to.

use std::path::PathBuf;

use gpui::{Pixels, Point};

use crate::icons::IconName;

/// One action in the context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    NewNote,
    NewFolder,
    Rename,
    /// Encrypts the note, from a row that's a note without a lock.
    Lock,
    /// Takes the encryption off, from a row that's a locked note.
    RemoveLock,
    Reveal,
    CopyPath,
    Trash,
}

#[cfg(target_os = "macos")]
const REVEAL_LABEL: &str = "Reveal in Finder";
#[cfg(target_os = "windows")]
const REVEAL_LABEL: &str = "Reveal in Explorer";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const REVEAL_LABEL: &str = "Reveal in Files";

const ITEMS: [(MenuItem, &str, IconName); 8] = [
    (MenuItem::NewNote, "New note", IconName::FilePlus),
    (MenuItem::NewFolder, "New folder", IconName::FolderPlus),
    (MenuItem::Rename, "Rename", IconName::PencilSimple),
    (MenuItem::Lock, "Lock note", IconName::LockSimple),
    (
        MenuItem::RemoveLock,
        "Remove lock from note",
        IconName::LockSimpleOpen,
    ),
    (MenuItem::Reveal, REVEAL_LABEL, IconName::FolderOpen),
    (MenuItem::CopyPath, "Copy path", IconName::Copy),
    (MenuItem::Trash, "Move to trash", IconName::Trash),
];

impl MenuItem {
    pub fn label(self) -> &'static str {
        ITEMS
            .iter()
            .find(|(item, ..)| *item == self)
            .map_or("", |(_, label, _)| label)
    }

    pub fn icon(self) -> IconName {
        ITEMS
            .iter()
            .find(|(item, ..)| *item == self)
            .map_or(IconName::FileText, |(_, _, icon)| *icon)
    }

    /// The command the workspace runs on the note for a lock item.
    pub fn note_command(self) -> Option<&'static str> {
        match self {
            MenuItem::Lock => Some("encryption.encrypt-note"),
            MenuItem::RemoveLock => Some("encryption.decrypt-note"),
            _ => None,
        }
    }

    /// The items for a row, or for the empty space below the rows. `lock`
    /// is the lock item a note's row offers, if the row is a note.
    pub fn for_target(on_entry: bool, lock: Option<MenuItem>) -> Vec<MenuItem> {
        let entry_only = [MenuItem::Rename, MenuItem::Trash];
        ITEMS
            .iter()
            .map(|(item, ..)| *item)
            .filter(|item| on_entry || !entry_only.contains(item))
            .filter(|item| item.note_command().is_none() || Some(*item) == lock)
            .collect()
    }
}

/// An open context menu.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextMenu {
    /// The entry it acts on, relative to the root. `None` is the vault root.
    pub target: Option<PathBuf>,
    /// Where the pointer was; `None` when opened from the keyboard, which
    /// places it at the row.
    pub position: Option<Point<Pixels>>,
    pub items: Vec<MenuItem>,
    /// The row the keyboard is on. A menu the pointer opened has none
    /// until an arrow is pressed, so no row looks chosen before the
    /// pointer gets there.
    pub highlighted: Option<usize>,
}

impl ContextMenu {
    pub fn new(
        target: Option<PathBuf>,
        position: Option<Point<Pixels>>,
        lock: Option<MenuItem>,
    ) -> ContextMenu {
        let items = MenuItem::for_target(target.is_some(), lock);
        ContextMenu {
            target,
            position,
            items,
            highlighted: position.is_none().then_some(0),
        }
    }

    /// Moves the highlight by `delta`, wrapping; the first move from no
    /// highlight lands on the first row going down, the last going up.
    pub fn move_highlight(&mut self, delta: isize) {
        let count = self.items.len() as isize;
        let from = match self.highlighted {
            Some(index) => index as isize,
            None if delta > 0 => -1,
            None => count,
        };
        self.highlighted = Some((from + delta).rem_euclid(count) as usize);
    }

    pub fn highlighted_item(&self) -> Option<MenuItem> {
        self.items.get(self.highlighted?).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_get_every_item_and_the_background_gets_fewer() {
        assert_eq!(MenuItem::for_target(true, None).len(), 6);
        let background = MenuItem::for_target(false, None);
        assert!(!background.contains(&MenuItem::Rename));
        assert!(!background.contains(&MenuItem::Trash));
        assert!(background.contains(&MenuItem::NewNote));
    }

    #[test]
    fn a_note_offers_the_lock_that_fits_it() {
        let plain = MenuItem::for_target(true, Some(MenuItem::Lock));
        assert_eq!(plain[3], MenuItem::Lock);
        assert!(!plain.contains(&MenuItem::RemoveLock));
        let locked = MenuItem::for_target(true, Some(MenuItem::RemoveLock));
        assert_eq!(locked[3], MenuItem::RemoveLock);
        assert!(!locked.contains(&MenuItem::Lock));
    }

    #[test]
    fn the_highlight_wraps() {
        let mut menu = ContextMenu::new(Some(PathBuf::from("a.md")), None, None);
        menu.move_highlight(-1);
        assert_eq!(menu.highlighted_item(), Some(MenuItem::Trash));
        menu.move_highlight(1);
        assert_eq!(menu.highlighted_item(), Some(MenuItem::NewNote));
    }

    #[test]
    fn a_menu_the_pointer_opened_waits_for_an_arrow() {
        let at = gpui::point(gpui::px(10.), gpui::px(10.));
        let mut menu = ContextMenu::new(Some(PathBuf::from("a.md")), Some(at), None);
        assert_eq!(menu.highlighted_item(), None);
        menu.move_highlight(1);
        assert_eq!(menu.highlighted_item(), Some(MenuItem::NewNote));
        let mut menu = ContextMenu::new(Some(PathBuf::from("a.md")), Some(at), None);
        menu.move_highlight(-1);
        assert_eq!(menu.highlighted_item(), Some(MenuItem::Trash));
    }

    #[test]
    fn labels_are_sentence_case() {
        for (item, label, _) in ITEMS {
            assert_eq!(item.label(), label);
            let rest: String = label.chars().skip(1).collect();
            assert!(!rest.contains(char::is_uppercase) || label.starts_with("Reveal"));
        }
    }
}
