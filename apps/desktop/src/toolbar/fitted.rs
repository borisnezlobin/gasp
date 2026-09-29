//! A docked bar's items laid along it, with the ones that don't fit left
//! for a trailing More button. The items are measured as they'd draw and
//! the fit is worked out against the bar's own bounds in the same frame,
//! so nothing flickers as the window is resized: [`super::fit`] decides.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, Size, Style, Window, point, px, size,
};

use super::fit::{Extent, fit_items, run_length, spacer_length};

/// Which way a bar runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarAxis {
    Row,
    Column,
}

impl BarAxis {
    fn along(self, size: Size<Pixels>) -> Pixels {
        match self {
            BarAxis::Row => size.width,
            BarAxis::Column => size.height,
        }
    }

    fn across(self, size: Size<Pixels>) -> Pixels {
        match self {
            BarAxis::Row => size.height,
            BarAxis::Column => size.width,
        }
    }

    fn along_at(self, at: Point<Pixels>) -> Pixels {
        match self {
            BarAxis::Row => at.x,
            BarAxis::Column => at.y,
        }
    }

    fn across_at(self, at: Point<Pixels>) -> Pixels {
        match self {
            BarAxis::Row => at.y,
            BarAxis::Column => at.x,
        }
    }

    fn size(self, along: Pixels, across: Pixels) -> Size<Pixels> {
        match self {
            BarAxis::Row => size(along, across),
            BarAxis::Column => size(across, along),
        }
    }

    fn point(self, along: Pixels, across: Pixels) -> Point<Pixels> {
        match self {
            BarAxis::Row => point(along, across),
            BarAxis::Column => point(across, along),
        }
    }
}

/// What kind of room an item takes, before it's measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Button,
    Separator,
    Widget,
    Spacer,
}

/// One of a bar's items: its index on the toolbar, and its element (a
/// spacer has none).
pub struct Slot {
    pub index: usize,
    pub kind: SlotKind,
    pub element: Option<AnyElement>,
}

/// The index of the first toolbar item that went into More, while some
/// did. The workspace reads it to fill More's menu and to keep the
/// keyboard on items that show.
pub type OverflowCell = Rc<Cell<Option<usize>>>;

/// A bar's items fitted to its length.
pub struct FittedItems {
    axis: BarAxis,
    gap: Pixels,
    /// Whether it grows to fill its bar, so spacers have room to push; a
    /// pill hugs its items instead.
    fill: bool,
    slots: Vec<Slot>,
    more: AnyElement,
    overflow: OverflowCell,
}

pub fn fitted_items(
    axis: BarAxis,
    gap: Pixels,
    slots: Vec<Slot>,
    more: AnyElement,
    overflow: OverflowCell,
) -> FittedItems {
    FittedItems {
        axis,
        gap,
        fill: true,
        slots,
        more,
        overflow,
    }
}

impl FittedItems {
    /// Takes only the length its items need, up to the room it has.
    pub fn hug(mut self) -> FittedItems {
        self.fill = false;
        self
    }
}

/// Every item's measured size, More's, and the extents they give.
pub struct Measured {
    sizes: Vec<Size<Pixels>>,
    extents: Vec<Extent>,
    more: Size<Pixels>,
}

fn extent(kind: SlotKind, length: Pixels) -> Extent {
    match kind {
        SlotKind::Button => Extent::Button(length),
        SlotKind::Separator => Extent::Separator(length),
        SlotKind::Widget => Extent::Widget(length),
        SlotKind::Spacer => Extent::Spacer,
    }
}

/// Where each shown item goes, and More when it shows.
pub struct Placed {
    shown: usize,
    more: bool,
}

impl FittedItems {
    fn measure(&mut self, window: &mut Window, cx: &mut App) -> Measured {
        let natural = size(AvailableSpace::MaxContent, AvailableSpace::MaxContent);
        let axis = self.axis;
        let sizes: Vec<Size<Pixels>> = self
            .slots
            .iter_mut()
            .map(|slot| match slot.element.as_mut() {
                Some(element) => element.layout_as_root(natural, window, cx),
                None => Size::default(),
            })
            .collect();
        let extents = self
            .slots
            .iter()
            .zip(&sizes)
            .map(|(slot, size)| extent(slot.kind, axis.along(*size)))
            .collect();
        let more = self.more.layout_as_root(natural, window, cx);
        Measured {
            sizes,
            extents,
            more,
        }
    }

    fn layout_style(&self) -> Style {
        let mut style = Style {
            flex_shrink: 1.,
            flex_grow: if self.fill { 1. } else { 0. },
            ..Style::default()
        };
        match self.axis {
            BarAxis::Row => style.min_size.width = px(0.).into(),
            BarAxis::Column => style.min_size.height = px(0.).into(),
        }
        style
    }

    /// Lays out each shown item after the one before, spacers taking an
    /// equal share of what's left, and More at the far end.
    fn place(
        &mut self,
        bounds: Bounds<Pixels>,
        measured: &Measured,
        window: &mut Window,
        cx: &mut App,
    ) -> Placed {
        let axis = self.axis;
        let room = axis.along(bounds.size);
        let more_length = axis.along(measured.more);
        let fit = fit_items(&measured.extents, self.gap, room, more_length);
        let shown = &measured.extents[..fit.shown];
        let spacer = if fit.overflows {
            Pixels::ZERO
        } else {
            spacer_length(shown, self.gap, room)
        };
        let start = axis.along_at(bounds.origin);
        let cross_start = axis.across_at(bounds.origin);
        let cross = axis.across(bounds.size);
        let mut at = start;
        for (slot, item_size) in self.slots[..fit.shown].iter_mut().zip(&measured.sizes) {
            let length = match slot.kind {
                SlotKind::Spacer => spacer,
                _ => axis.along(*item_size),
            };
            if let Some(element) = slot.element.as_mut() {
                let across = cross_start + (cross - axis.across(*item_size)) / 2.;
                element.prepaint_at(axis.point(at, across), window, cx);
            }
            at += length + self.gap;
        }
        if fit.overflows {
            let across = cross_start + (cross - axis.across(measured.more)) / 2.;
            let along = start + room - more_length;
            self.more.prepaint_at(axis.point(along, across), window, cx);
        }
        let first_hidden = fit
            .overflows
            .then(|| self.slots.get(fit.shown).map(|slot| slot.index))
            .flatten();
        self.overflow.set(first_hidden);
        Placed {
            shown: fit.shown,
            more: fit.overflows,
        }
    }
}

impl Element for FittedItems {
    type RequestLayoutState = Measured;
    type PrepaintState = Placed;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let measured = self.measure(window, cx);
        let axis = self.axis;
        let natural = run_length(&measured.extents, self.gap);
        let thickness = measured
            .sizes
            .iter()
            .chain([&measured.more])
            .map(|size| axis.across(*size))
            .fold(Pixels::ZERO, Pixels::max);
        let layout_id =
            window.request_measured_layout(self.layout_style(), move |known, available, _, _| {
                let room = match axis {
                    BarAxis::Row => (known.width, available.width),
                    BarAxis::Column => (known.height, available.height),
                };
                let along = room.0.unwrap_or(match room.1 {
                    AvailableSpace::Definite(room) => natural.min(room),
                    _ => natural,
                });
                axis.size(along, thickness)
            });
        (layout_id, measured)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        measured: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.place(bounds, measured, window, cx)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        placed: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for slot in &mut self.slots[..placed.shown] {
            if let Some(element) = slot.element.as_mut() {
                element.paint(window, cx);
            }
        }
        if placed.more {
            self.more.paint(window, cx);
        }
    }
}

impl IntoElement for FittedItems {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
