//! Where a popover hangs: under the control it belongs to, or over it
//! when there isn't room below, never cut off by the window.
//!
//! [`Popover`] fills the relative container of its control, so its own
//! bounds are the control's. When it's prepainted it knows where the
//! control is on screen and how tall the window is, so it picks a side
//! and caps the panel's height to the room there before laying the panel
//! out. The panel is drawn above everything else.

use std::panic::Location;

use gpui::{
    App, AvailableSpace, Bounds, Edges, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Length, Pixels, Position, Size, Style, Styled, Window, point, px, size,
};

/// Draws above the page and its other overlays.
const PRIORITY: usize = 1;

/// A panel hung from the right edge of the control before it.
pub struct Popover<E> {
    panel: Option<E>,
    /// The gap between the control and the panel.
    offset: Pixels,
    /// The room the panel keeps from the window's edges.
    margin: Pixels,
}

/// Hangs `panel` under the control in the same relative container, or
/// over it when it doesn't fit below and there's more room above. The
/// panel's height is capped to the room on the side it takes; a panel
/// that should scroll then needs a part that shrinks.
pub fn popover<E: IntoElement + Styled + 'static>(
    panel: E,
    offset: Pixels,
    margin: Pixels,
) -> Popover<E> {
    Popover {
        panel: Some(panel),
        offset,
        margin,
    }
}

/// The room a panel has below and above its control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Room {
    pub below: Pixels,
    pub above: Pixels,
}

impl Room {
    /// The room below and above `control` in a window `window` tall.
    pub fn around(control: Bounds<Pixels>, window: Pixels, offset: Pixels, margin: Pixels) -> Room {
        Room {
            below: (window - control.bottom() - offset - margin).max(px(0.)),
            above: (control.top() - offset - margin).max(px(0.)),
        }
    }

    /// The tallest a panel may be: the room on the larger side. A panel
    /// that fits below goes below, so this caps only a panel that
    /// doesn't, which goes to the larger side.
    pub fn max_height(&self) -> Pixels {
        self.below.max(self.above)
    }

    /// Whether a panel `height` tall goes below its control.
    pub fn fits_below(&self, height: Pixels) -> bool {
        height <= self.below || self.below >= self.above
    }
}

impl<E: IntoElement + Styled + 'static> IntoElement for Popover<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: IntoElement + Styled + 'static> Element for Popover<E> {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Covers the container, so its bounds are the control's.
        let style = Style {
            position: Position::Absolute,
            inset: Edges::all(Length::from(px(0.))),
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        control: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(panel) = self.panel.take() else {
            return;
        };
        let viewport = window.viewport_size();
        let room = Room::around(control, viewport.height, self.offset, self.margin);
        let mut panel = panel.max_h(room.max_height()).into_any_element();
        let content = size(AvailableSpace::MaxContent, AvailableSpace::MaxContent);
        let panel_size = panel.layout_as_root(content, window, cx);
        let origin = self.origin(control, panel_size, room, viewport);
        window.defer_draw(panel, origin, PRIORITY);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

impl<E> Popover<E> {
    /// Where the panel's top left corner goes: its right edge under or
    /// over the control's, kept inside the window's margins sideways.
    fn origin(
        &self,
        control: Bounds<Pixels>,
        panel: Size<Pixels>,
        room: Room,
        viewport: Size<Pixels>,
    ) -> gpui::Point<Pixels> {
        let rightmost = (viewport.width - self.margin - panel.width).max(self.margin);
        let x = (control.right() - panel.width).clamp(self.margin, rightmost);
        let y = if room.fits_below(panel.height) {
            control.bottom() + self.offset
        } else {
            control.top() - self.offset - panel.height
        };
        point(x, y)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};

    use super::Room;

    fn room(top: f32) -> Room {
        let control = Bounds::new(point(px(0.), px(top)), size(px(100.), px(30.)));
        Room::around(control, px(600.), px(4.), px(12.))
    }

    #[test]
    fn a_panel_that_fits_goes_below() {
        let room = room(400.);
        assert_eq!(room.below, px(154.));
        assert!(room.fits_below(px(150.)));
    }

    #[test]
    fn a_tall_panel_takes_the_larger_side() {
        let low = room(400.);
        assert!(!low.fits_below(px(300.)));
        assert_eq!(low.max_height(), low.above);
        let high = room(100.);
        assert!(high.fits_below(px(900.)));
        assert_eq!(high.max_height(), high.below);
    }
}
