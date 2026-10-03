//! Moving tabs between panes: dropping a dragged tab on a tab bar or a
//! note, opening notes dropped there from the file tree, the move-tab
//! commands, and closing tabs in bulk.

use std::path::PathBuf;

use gpui::{Context, Entity, Window};

use super::Workspace;
use super::pane::{DroppedItem, Pane, Tab, TabTarget};
use super::pane_tree::{Direction, DropZone};

impl Workspace {
    /// A tab or notes were dropped on `target`.
    pub(crate) fn drop_on_pane(
        &mut self,
        item: &DroppedItem,
        target: &Entity<Pane>,
        place: TabTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match item {
            DroppedItem::Tab { from, index } => {
                self.drop_tab(from, *index, target, place, window, cx)
            }
            DroppedItem::Notes(paths) => self.drop_notes(paths, target, place, window, cx),
        }
    }

    /// Notes from the file tree were dropped on `target`: they open as
    /// tabs where a tab dropped there would go.
    fn drop_notes(
        &mut self,
        paths: &[PathBuf],
        target: &Entity<Pane>,
        place: TabTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_tab_drops(cx);
        let landing = match place {
            TabTarget::Slot(slot) => Some((target.clone(), slot)),
            TabTarget::Zone(DropZone::Centre) => Some((target.clone(), after_active(target, cx))),
            TabTarget::Zone(DropZone::Side(side)) => self.split_for_notes(target, side, window, cx),
        };
        let Some((pane, slot)) = landing else {
            return;
        };
        self.open_notes_at(&pane, paths, slot, window, cx);
        if pane.read(cx).is_empty() {
            self.handle_empty_pane(&pane, window, cx);
        }
        self.refresh_status(cx);
        cx.notify();
    }

    /// A new, empty pane beside `target` toward `side`, and its first slot.
    fn split_for_notes(
        &mut self,
        target: &Entity<Pane>,
        side: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Entity<Pane>, usize)> {
        let pane = self.new_pane(window, cx);
        self.panes.split_toward(target, pane.clone(), side)?;
        Some((pane, 0))
    }

    /// Opens each note as a tab of `pane`, from `slot` on, in order.
    fn open_notes_at(
        &mut self,
        pane: &Entity<Pane>,
        paths: &[PathBuf],
        mut slot: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for path in paths {
            let path = self.resolve(path);
            match self.open_in_pane_at(pane, &path, slot, window, cx) {
                Ok(true) => slot += 1,
                Ok(false) => {}
                Err(error) => {
                    crate::notices::open_failed(&path, error, cx);
                }
            }
        }
    }

    /// A tab from `from` was dropped on `target`.
    pub(crate) fn drop_tab(
        &mut self,
        from: &Entity<Pane>,
        index: usize,
        target: &Entity<Pane>,
        place: TabTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_tab_drops(cx);
        match place {
            TabTarget::Slot(slot) => self.move_tab(from, index, target, slot, window, cx),
            TabTarget::Zone(DropZone::Centre) => {
                let slot = after_active(target, cx);
                self.move_tab(from, index, target, slot, window, cx);
            }
            TabTarget::Zone(DropZone::Side(side)) => {
                self.split_with_tab(from, index, target, side, window, cx);
            }
        }
    }

    /// Forgets every pane's drag marks, once a drag has ended.
    pub(crate) fn clear_tab_drops(&mut self, cx: &mut Context<Self>) {
        for pane in self.panes.panes() {
            pane.update(cx, |pane, cx| pane.clear_drop(cx));
        }
    }

    /// Moves tab `index` of `from` into `to`, before the tab at `slot`
    /// (or last), and shows it there. A pane left without tabs closes.
    pub fn move_tab(
        &mut self,
        from: &Entity<Pane>,
        index: usize,
        to: &Entity<Pane>,
        slot: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from == to {
            from.update(cx, |pane, cx| pane.move_tab(index, slot, cx));
        } else {
            let Some(tab) = from.update(cx, |pane, cx| pane.remove_tab(index, cx)) else {
                return;
            };
            self.put_moved_tab(to, tab, slot, cx);
            if from.read(cx).is_empty() {
                self.remove_pane(from, window, cx);
            }
        }
        self.activate_pane(to, window, cx);
        self.refresh_status(cx);
        cx.notify();
    }

    /// Puts a tab that came from another pane into `to`. When `to`
    /// already shows its note, that tab shows instead and this one goes.
    fn put_moved_tab(&mut self, to: &Entity<Pane>, tab: Tab, slot: usize, cx: &mut Context<Self>) {
        let existing = tab
            .path(cx)
            .and_then(|path| to.read(cx).index_of_path(path, cx));
        match existing {
            Some(index) => {
                to.update(cx, |pane, cx| pane.activate(index, cx));
                self.release_tab(tab, true, cx);
            }
            None => {
                to.update(cx, |pane, cx| pane.insert_tab(slot, tab, cx));
            }
        }
    }

    /// Splits `target` toward `side` and moves tab `index` of `from` into
    /// the new pane. A pane's only tab can't split away from it.
    pub fn split_with_tab(
        &mut self,
        from: &Entity<Pane>,
        index: usize,
        target: &Entity<Pane>,
        side: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from == target && from.read(cx).len() < 2 {
            return;
        }
        let pane = self.new_pane(window, cx);
        if self
            .panes
            .split_toward(target, pane.clone(), side)
            .is_none()
        {
            return;
        }
        self.move_tab(from, index, &pane, 0, window, cx);
    }

    /// Moves the active tab to the pane on `side`, or splits it off
    /// toward that side when there's no pane there.
    pub(crate) fn move_active_tab(
        &mut self,
        side: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = self.active_pane.clone();
        let index = pane.read(cx).active_index();
        if pane.read(cx).active_tab().is_none() {
            return;
        }
        match self.panes.beside(&pane, side) {
            Some(neighbor) => {
                let slot = after_active(&neighbor, cx);
                self.move_tab(&pane, index, &neighbor, slot, window, cx);
            }
            None => self.split_with_tab(&pane, index, &pane, side, window, cx),
        }
    }

    /// Closes every tab of `pane` but the one at `keep`.
    pub(crate) fn close_other_tabs(
        &mut self,
        pane: &Entity<Pane>,
        keep: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = pane.read(cx).len();
        for index in (0..count).rev().filter(|index| *index != keep) {
            self.close_tab(pane, index, window, cx);
        }
    }

    /// Closes the tabs of `pane` after the one at `index`.
    pub(crate) fn close_tabs_right(
        &mut self,
        pane: &Entity<Pane>,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = pane.read(cx).len();
        for index in (index + 1..count).rev() {
            self.close_tab(pane, index, window, cx);
        }
    }
}

/// The slot just after `pane`'s active tab, where a tab joining it goes.
fn after_active(pane: &Entity<Pane>, cx: &gpui::App) -> usize {
    let pane = pane.read(cx);
    if pane.is_empty() {
        0
    } else {
        pane.active_index() + 1
    }
}
