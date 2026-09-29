//! Where something shown on hover was last drawn, so whether the pointer
//! is over it is a question of geometry. GPUI's own hover ends whenever
//! anything is drawn over the element (a menu's backdrop, a popover), a
//! drag is under way, or the element misses the move that left it, so a
//! surface that hides when the pointer leaves asks this instead.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{Bounds, Pixels, Point, Styled, canvas};

/// The bounds a surface was last laid out at; empty while it isn't drawn.
/// Clones share one record.
#[derive(Clone, Debug, Default)]
pub struct DrawnArea(Rc<Cell<Option<Bounds<Pixels>>>>);

impl DrawnArea {
    pub fn set(&self, bounds: Option<Bounds<Pixels>>) {
        self.0.set(bounds);
    }

    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.0.get()
    }

    pub fn contains(&self, point: Point<Pixels>) -> bool {
        self.0.get().is_some_and(|bounds| bounds.contains(&point))
    }

    /// An empty element that records its own bounds here each frame. Lay
    /// it over the surface (it's absolutely placed; size it to cover what
    /// counts as the surface).
    pub fn probe(&self) -> gpui::Canvas<()> {
        let area = self.clone();
        canvas(move |bounds, _, _| area.set(Some(bounds)), |_, _, _, _| {}).absolute()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, size};

    use super::*;

    #[test]
    fn an_area_is_empty_until_drawn() {
        let area = DrawnArea::default();
        let inside = point(px(10.), px(10.));
        assert!(!area.contains(inside));
        area.set(Some(Bounds::new(
            point(px(0.), px(0.)),
            size(px(200.), px(600.)),
        )));
        assert!(area.contains(inside));
        assert!(!area.contains(point(px(250.), px(10.))));
        area.clone().set(None);
        assert!(!area.contains(inside), "clones share the record");
    }
}
