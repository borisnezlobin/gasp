//! Saving the split panes, with each pane's tabs and the space it has, to
//! the vault's device file, and building them again on the next start.

use gasp_config::device::{PaneLayout, SplitAxis};
use gpui::{App, Context, Entity, Window};

use super::Workspace;
use super::pane::Pane;
use super::pane_tree::{Axis, Direction, Node};

impl Workspace {
    /// The panes as they are now.
    pub(crate) fn pane_layout(&self, cx: &App) -> PaneLayout {
        self.node_layout(self.panes.root(), cx)
    }

    fn node_layout(&self, node: &Node<Entity<Pane>>, cx: &App) -> PaneLayout {
        match node {
            Node::Leaf(pane) => self.leaf_layout(pane, cx),
            Node::Split(split) => PaneLayout {
                split: Some(match split.axis {
                    Axis::Row => SplitAxis::Row,
                    Axis::Column => SplitAxis::Column,
                }),
                ratio: Some(split.ratio),
                sides: vec![
                    self.node_layout(&split.first, cx),
                    self.node_layout(&split.second, cx),
                ],
                ..PaneLayout::default()
            },
        }
    }

    /// A pane's note and image tabs. Empty tabs aren't kept.
    fn leaf_layout(&self, pane: &Entity<Pane>, cx: &App) -> PaneLayout {
        let read = pane.read(cx);
        let mut layout = PaneLayout {
            focused: *pane == self.active_pane,
            ..PaneLayout::default()
        };
        for (index, tab) in read.tabs().iter().enumerate() {
            let Some(path) = tab.path(cx) else {
                continue;
            };
            if index == read.active_index() {
                layout.active_tab = Some(layout.tabs.len());
            }
            layout.tabs.push(self.relative_name(path));
        }
        layout
    }

    /// Builds the saved panes in place of the single starting pane. Notes
    /// that are gone are skipped, and a pane left with none closes.
    pub(crate) fn restore_layout(
        &mut self,
        layout: &PaneLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.active_pane.clone();
        let mut focused = None;
        self.restore_node(layout, &root, &mut focused, window, cx);
        for pane in self.panes.panes() {
            if pane.read(cx).is_empty() {
                self.handle_empty_pane(&pane, window, cx);
            }
        }
        let focus = focused
            .filter(|pane| self.panes.contains(pane))
            .or_else(|| self.panes.panes().first().cloned());
        if let Some(pane) = focus {
            self.activate_pane(&pane, window, cx);
        }
    }

    fn restore_node(
        &mut self,
        layout: &PaneLayout,
        pane: &Entity<Pane>,
        focused: &mut Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(axis), [first, second]) = (layout.split, layout.sides.as_slice()) else {
            self.restore_tabs(layout, pane, focused, window, cx);
            return;
        };
        let other = self.new_pane(window, cx);
        let side = match axis {
            SplitAxis::Row => Direction::Right,
            SplitAxis::Column => Direction::Down,
        };
        let Some(id) = self.panes.split_toward(pane, other.clone(), side) else {
            return;
        };
        if let Some(ratio) = layout.ratio {
            self.panes.set_ratio(id, ratio);
        }
        self.restore_node(first, pane, focused, window, cx);
        self.restore_node(second, &other, focused, window, cx);
    }

    fn restore_tabs(
        &mut self,
        layout: &PaneLayout,
        pane: &Entity<Pane>,
        focused: &mut Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut opened = 0;
        for name in &layout.tabs {
            let path = self.vault.join(name);
            // The starting pane's empty tab gives way to the first note.
            let replace = opened == 0;
            if path.is_file()
                && matches!(
                    self.show_path_in_pane(pane, &path, replace, window, cx),
                    Ok(true)
                )
            {
                opened += 1;
            }
        }
        if opened == 0 {
            drop_empty_tabs(pane, cx);
        }
        let active = layout
            .active_tab
            .and_then(|index| layout.tabs.get(index))
            .map(|name| self.vault.join(name));
        if let Some(index) = active.and_then(|path| pane.read(cx).index_of_path(&path, cx)) {
            pane.update(cx, |pane, cx| pane.activate(index, cx));
        }
        if layout.focused {
            *focused = Some(pane.clone());
        }
    }
}

/// Clears a pane that shows only empty tabs, so a pane whose notes are all
/// gone closes rather than lingering.
fn drop_empty_tabs(pane: &Entity<Pane>, cx: &mut App) {
    let only_empty = pane.read(cx).tabs().iter().all(super::pane::Tab::is_blank);
    if !only_empty {
        return;
    }
    while let Some(tab) = pane.update(cx, |pane, cx| pane.remove_tab(0, cx)) {
        drop(tab);
    }
}
