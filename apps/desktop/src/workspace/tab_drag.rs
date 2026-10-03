//! Dragging tabs, on the pane's side: the tab that follows the pointer,
//! the bar between tabs where it would go, and the highlight over the
//! half of a note it would split off. Notes dragged from the file tree
//! land the same way. The workspace moves the tab or opens the notes when
//! they're dropped (see `tab_moves.rs`).
//!
//! Every pane hears every move of a drag, so each handler redraws its
//! pane only when the landing place changes.

use std::path::Path;

use gpui::{
    Animation, AnimationExt, AnyElement, Bounds, Context, DragMoveEvent, ElementId, Entity,
    MouseDownEvent, Pixels, Point, SharedString, Window, div, ease_out_quint, prelude::*, relative,
};

use super::pane::{DropState, DroppedItem, Pane, PaneEvent, PaneMenu, TabTarget};
use super::pane_tree::{DropZone, Rect};
use crate::file_tree::DraggedEntry;
use crate::ui::Selectable;
use crate::ui::{MenuAnchor, popover, ui_theme};

/// A tab being dragged: where it comes from, and what its stand-in shows.
#[derive(Clone)]
pub struct DraggedTab {
    pub pane: Entity<Pane>,
    pub index: usize,
    pub title: SharedString,
    pub dirty: bool,
    /// The tab's width, so the stand-in matches it.
    pub width: Pixels,
}

/// Something a pane takes when it's dropped on its tab strip or note.
pub(super) trait PaneDrop: 'static {
    /// The dragged tab's index, when it's one of `pane`'s own.
    fn own_tab(&self, _pane: &Entity<Pane>) -> Option<usize> {
        None
    }

    /// What lands in the pane, or nothing when it takes none of it.
    fn dropped_item(&self) -> Option<DroppedItem>;

    /// Whether the pane takes anything from it, asked on every move.
    fn droppable(&self) -> bool;
}

impl PaneDrop for DraggedTab {
    fn own_tab(&self, pane: &Entity<Pane>) -> Option<usize> {
        (self.pane == *pane).then_some(self.index)
    }

    fn dropped_item(&self) -> Option<DroppedItem> {
        Some(DroppedItem::Tab {
            from: self.pane.clone(),
            index: self.index,
        })
    }

    fn droppable(&self) -> bool {
        true
    }
}

/// Entries from the file tree: their notes open as tabs, and folders and
/// other files are left out.
impl PaneDrop for DraggedEntry {
    fn dropped_item(&self) -> Option<DroppedItem> {
        let notes: Vec<_> = self.notes().map(Path::to_path_buf).collect();
        (!notes.is_empty()).then_some(DroppedItem::Notes(notes))
    }

    fn droppable(&self) -> bool {
        self.notes().next().is_some()
    }
}

/// The tab's stand-in: a lifted copy of the tab, hanging just below and
/// right of the pointer so the bar between tabs and the highlight on the
/// note stay in sight.
struct TabPreview {
    title: SharedString,
    dirty: bool,
    width: Pixels,
    /// Where the pointer grabbed the tab, which GPUI keeps it at.
    grab: Point<Pixels>,
}

impl Render for TabPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let tab = popover(&ui)
            .flex_row()
            .items_center()
            .gap(ui.space_sm)
            .w(self.width)
            .h(ui.tab_height)
            .py_0()
            .px(ui.tab_padding_x)
            .rounded(ui.tab_radius)
            .child(crate::ui::truncated(self.title.clone()).grow())
            .when(self.dirty, |tab| {
                tab.child(
                    div()
                        .flex_none()
                        .size(ui.dirty_dot_size)
                        .rounded_full()
                        .bg(ui.text_muted),
                )
            });
        // The root's margin doesn't move a drag's stand-in; padding does.
        div()
            .pl(self.grab.x + ui.drag_preview_offset.x)
            .pt(self.grab.y + ui.drag_preview_offset.y)
            .child(tab)
    }
}

impl Pane {
    /// What dragging the tab at `index` carries.
    pub(super) fn dragged_tab(
        &self,
        index: usize,
        title: SharedString,
        dirty: bool,
        cx: &mut Context<Self>,
    ) -> DraggedTab {
        let ui = ui_theme(cx);
        let width = self
            .tab_scroll
            .bounds_for_item(index)
            .map_or(ui.tab_max_width, |bounds| bounds.size.width);
        DraggedTab {
            pane: cx.entity(),
            index,
            title,
            dirty,
            width,
        }
    }

    /// Starts showing a dragged tab's stand-in, and marks its place.
    pub(super) fn start_tab_drag(
        dragged: &DraggedTab,
        grab: Point<Pixels>,
        cx: &mut gpui::App,
    ) -> Entity<impl Render + use<>> {
        let index = dragged.index;
        dragged.pane.update(cx, |pane, cx| {
            pane.drop = DropState {
                dragged: Some(index),
                ..DropState::default()
            };
            cx.notify();
        });
        let preview = TabPreview {
            title: dragged.title.clone(),
            dirty: dragged.dirty,
            width: dragged.width,
            grab,
        };
        cx.new(|_| preview)
    }

    /// Forgets the last drag, once it has ended.
    pub fn clear_drop(&mut self, cx: &mut Context<Self>) {
        if self.drop != DropState::default() {
            self.drop = DropState::default();
            cx.notify();
        }
    }

    /// Whether this pane's tab at `index` is being dragged.
    pub(super) fn is_dragging(&self, index: usize, cx: &gpui::App) -> bool {
        cx.has_active_drag() && self.drop.dragged == Some(index)
    }

    /// The slot a drop into the tab bar would fill, while a tab is over
    /// it.
    pub(super) fn shown_slot(&self, cx: &gpui::App) -> Option<usize> {
        self.drop.slot.filter(|_| cx.has_active_drag())
    }

    /// Lets the tab bar take dragged tabs and notes.
    pub(super) fn catch_drops_on_tabs<E: InteractiveElement>(bar: E, cx: &mut Context<Self>) -> E {
        bar.on_drag_move(cx.listener(Self::on_drag_over_tabs::<DraggedTab>))
            .on_drag_move(cx.listener(Self::on_drag_over_tabs::<DraggedEntry>))
            .on_drop(cx.listener(Self::on_drop_on_tabs::<DraggedTab>))
            .on_drop(cx.listener(Self::on_drop_on_tabs::<DraggedEntry>))
    }

    /// Lets the note show where dragged tabs and notes would land.
    pub(super) fn follow_drags_over_note<E: InteractiveElement>(
        surface: E,
        cx: &mut Context<Self>,
    ) -> E {
        surface
            .on_drag_move(cx.listener(Self::on_drag_over_note::<DraggedTab>))
            .on_drag_move(cx.listener(Self::on_drag_over_note::<DraggedEntry>))
    }

    /// Follows a dragged tab or note over the tab bar.
    fn on_drag_over_tabs<D: PaneDrop>(
        &mut self,
        event: &DragMoveEvent<D>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = event.event.position;
        let dragged = event.drag(cx);
        let own = dragged.own_tab(&cx.entity());
        let over = dragged.droppable() && event.bounds.contains(&position);
        let slot = over
            .then(|| self.slot_at(position.x))
            .filter(|slot| !moves_nowhere(own, *slot));
        if self.drop.slot != slot {
            self.drop.slot = slot;
            cx.notify();
        }
    }

    /// The slot nearest `x`: before the first tab whose middle is past it.
    fn slot_at(&self, x: Pixels) -> usize {
        let scrolled = self.tab_scroll.offset().x;
        (0..self.len())
            .find(|&index| {
                self.tab_scroll
                    .bounds_for_item(index)
                    .is_some_and(|bounds| x < bounds.center().x + scrolled)
            })
            .unwrap_or(self.len())
    }

    /// A tab or note dropped on the tab bar.
    fn on_drop_on_tabs<D: PaneDrop>(
        &mut self,
        dragged: &D,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let own = dragged.own_tab(&cx.entity());
        let slot = self.drop.slot.unwrap_or(self.len());
        if !moves_nowhere(own, slot) {
            self.drop_item(dragged, TabTarget::Slot(slot), cx);
        }
    }

    /// Follows a dragged tab or note over the note.
    fn on_drag_over_note<D: PaneDrop>(
        &mut self,
        event: &DragMoveEvent<D>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dragged = event.drag(cx);
        let own = dragged.own_tab(&cx.entity()).is_some();
        let droppable = dragged.droppable();
        let zone = zone_at(event.bounds, event.event.position)
            .filter(|zone| droppable && self.takes_zone(*zone, own));
        if self.drop.zone != zone {
            self.drop.previous_zone = self.drop.zone;
            self.drop.zone = zone;
            cx.notify();
        }
    }

    /// Whether a drop in `zone` would change anything: a pane's own tab
    /// can't join it, and its only tab can't split away from it.
    fn takes_zone(&self, zone: DropZone, own: bool) -> bool {
        !own || (zone != DropZone::Centre && self.len() > 1)
    }

    fn drop_item<D: PaneDrop>(&mut self, dragged: &D, target: TabTarget, cx: &mut Context<Self>) {
        if let Some(item) = dragged.dropped_item() {
            cx.emit(PaneEvent::Drop { item, target });
        }
    }

    /// The highlight over where a tab dropped on the note would land. It
    /// glides from the last zone to the new one, and it's what catches
    /// the drop, above the editor.
    pub(super) fn render_drop_zone(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let zone = self.drop.zone.filter(|_| cx.has_active_drag())?;
        let ui = ui_theme(cx);
        let to = zone.landing();
        let from = self.drop.previous_zone.map_or(to, DropZone::landing);
        let motion = ElementId::Name(format!("drop-zone-{from:?}-{zone:?}").into());
        let highlight = div()
            .absolute()
            .rounded(ui.surface_radius)
            .bg(ui.drop_zone)
            .shadow(vec![ui.ring(ui.drop_zone_ring)])
            .with_animation(
                motion,
                Animation::new(ui.drop_zone_motion).with_easing(ease_out_quint()),
                move |highlight, delta| {
                    let rect = mix(from, to, delta);
                    highlight
                        .left(relative(rect.x))
                        .top(relative(rect.y))
                        .w(relative(rect.width))
                        .h(relative(rect.height))
                },
            );
        let catcher = div()
            .id("pane-drop-zone")
            .selector(|| "pane-drop-zone".to_owned())
            .absolute()
            .inset_0()
            .p(ui.space_sm)
            .on_drop(cx.listener(drop_in_zone::<DraggedTab>(zone)))
            .on_drop(cx.listener(drop_in_zone::<DraggedEntry>(zone)))
            .child(div().relative().size_full().child(highlight));
        Some(catcher.into_any_element())
    }

    /// A right-click on a tab opens its menu.
    pub(super) fn on_tab_right_click(
        index: usize,
    ) -> impl Fn(&mut Pane, &MouseDownEvent, &mut Window, &mut Context<Pane>) {
        move |_, event, _, cx| {
            cx.stop_propagation();
            cx.emit(PaneEvent::ActivateTab(index));
            cx.emit(PaneEvent::OpenMenu(
                PaneMenu::Tab(index),
                MenuAnchor::Pointer(event.position),
            ));
        }
    }
}

/// What a drop on the highlighted `zone` of a note does.
fn drop_in_zone<D: PaneDrop>(
    zone: DropZone,
) -> impl Fn(&mut Pane, &D, &mut Window, &mut Context<Pane>) {
    move |pane, dragged, _, cx| pane.drop_item(dragged, TabTarget::Zone(zone), cx)
}

/// Whether putting a pane's own tab `own` at `slot` leaves it where it is.
fn moves_nowhere(own: Option<usize>, slot: usize) -> bool {
    own.is_some_and(|index| slot == index || slot == index + 1)
}

/// The zone of a note with `bounds` under `position`, if it's over it.
fn zone_at(bounds: Bounds<Pixels>, position: Point<Pixels>) -> Option<DropZone> {
    if !bounds.contains(&position) {
        return None;
    }
    let x = (position.x - bounds.left()) / bounds.size.width.max(gpui::px(1.));
    let y = (position.y - bounds.top()) / bounds.size.height.max(gpui::px(1.));
    Some(DropZone::at(x, y))
}

fn mix(from: Rect, to: Rect, delta: f32) -> Rect {
    let lerp = |a: f32, b: f32| a + (b - a) * delta;
    Rect {
        x: lerp(from.x, to.x),
        y: lerp(from.y, to.y),
        width: lerp(from.width, to.width),
        height: lerp(from.height, to.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_dropped_beside_itself_stays() {
        assert!(moves_nowhere(Some(2), 2));
        assert!(moves_nowhere(Some(2), 3));
        assert!(!moves_nowhere(Some(2), 4));
        assert!(!moves_nowhere(None, 2));
    }
}
